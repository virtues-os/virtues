//! Delete and tombstone: removing ingested records the owner does not want
//! on their server, so that they stay removed.
//!
//! A deletion is proposed (by the assistant's `delete_data` tool, or made
//! directly in Query, the table viewer), the owner allows it, and this module does
//! the work. For each record it:
//!
//!   - writes a tombstone (`app_tombstones`: the table and the record's two
//!     ids, no content), which the `refuse_tombstoned` trigger on every
//!     `data_*` table checks before an insert, so no sync brings it back;
//!   - deletes the row;
//!   - clears its search index rows and its `wiki_refs` links;
//!   - deletes the files it owns: a message's attachments, a recording's
//!     audio.
//!
//! When the rows are gone it takes each record's raw copy out of the lake's
//! files (`storage::raw_copies`), matching by the tombstones it just wrote.
//!
//! The tombstone, the row, the index rows and the links change in one
//! transaction per batch, so a run that stops halfway leaves every record
//! either whole or gone, and allowing it again finishes the rest.
//!
//! What this does not reach, and the receipt says so (`KEPT`): backups made
//! earlier, and pages or chat replies that already quoted a record. A row
//! with no raw record of its own (a location visit, an app session) has no
//! raw copy to remove, and the receipt counts those too.
//!
//! Which rows a deletion covers may be a SQL predicate the assistant wrote.
//! That text only ever runs as the read-only reader login
//! (`database::restricted`), to list ids of the one named table. The deletes
//! themselves are this module's own statements, by id, on that table. So the
//! worst a hostile predicate can do is select rows of the table the card
//! names, which the owner sees counted and sampled before allowing it.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{Error, Result};

/// Rows handled per transaction. Large enough that a year of heart rate is a
/// few hundred round trips, small enough that one batch holds its locks
/// briefly while ingest keeps writing the same table.
const BATCH: i64 = 1_000;

/// The most ids one target may name outright. More than this is a predicate.
pub const MAX_IDS: usize = 5_000;

/// The most targets one deletion may carry.
const MAX_TARGETS: usize = 12;

/// The longest a predicate may run when it is counted or listed.
const PREDICATE_TIMEOUT: &str = "25s";

/// Sample rows the card shows per table, so the owner can check the scope.
const SAMPLES: i64 = 3;

/// What a deletion does not remove. The card words each of these.
const KEPT: &[&str] = &["backups", "pages_and_chats"];

// ── What a deletion covers ──────────────────────────────────────────────────

/// One table's share of a deletion: every row matching `where`, or the rows
/// named in `ids`. Exactly one of the two.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub table: String,
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
}

/// A data table as this module needs it.
#[derive(Debug, Clone)]
struct Table {
    name: String,
    /// `occurred_at` or `started_at`, when the table has one.
    time_column: Option<&'static str>,
    /// What a person calls its rows: "Messages".
    label: String,
    /// The `search_embeddings.ontology` names its rows are indexed under.
    ontologies: Vec<&'static str>,
    /// SQL for a sample row's label and preview, over the alias `t`.
    sample_sql: Option<(&'static str, &'static str)>,
}

/// Whether `name` is shaped like a data table. The catalog lookup in
/// [`table`] is what decides it exists; this keeps anything else out of the
/// SQL that names it.
fn is_data_table_name(name: &str) -> bool {
    name.starts_with("data_")
        && name.len() <= 63
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A column name that needs no quoting, which is all the catalog should
/// return for a data table.
fn is_plain_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Look a data table up. Refuses anything that is not a `data_*` table with
/// both ids a tombstone needs.
async fn table(pool: &PgPool, name: &str) -> Result<Table> {
    if !is_data_table_name(name) {
        return Err(Error::InvalidInput(format!(
            "`{name}` is not one of the data tables. Only rows in data_* tables can be deleted this way."
        )));
    }
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT a.attname::text FROM pg_attribute a \
         JOIN pg_class c ON c.oid = a.attrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname = $1 \
           AND a.attnum > 0 AND NOT a.attisdropped",
    )
    .bind(name)
    .fetch_all(pool)
    .await?;
    if columns.is_empty() {
        return Err(Error::NotFound(format!("There is no table named `{name}`.")));
    }
    let has = |c: &str| columns.iter().any(|x| x == c);
    if !has("id") || !has("source_stream_id") {
        return Err(Error::InvalidInput(match name {
            "data_audio_session" => "data_audio_session is rebuilt from recordings. Delete the recordings \
                 (data_audio_recording) or transcripts (data_communication_transcription) instead, and the \
                 sessions follow."
                .to_string(),
            _ => format!("`{name}` has no source id, so its rows cannot be kept from coming back."),
        }));
    }
    let time_column = ["occurred_at", "started_at"].into_iter().find(|c| has(c));

    let described: Vec<_> = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.table_name == name)
        .collect();
    let label = described.first().map(|o| o.display_name.to_string()).unwrap_or_else(|| {
        // A table the registry does not describe: its own name, read aloud.
        name.trim_start_matches("data_").replace('_', " ")
    });
    let sample_sql = described
        .iter()
        .find_map(|o| o.day_source.as_ref().map(|d| (d.label_sql, d.preview_sql)));
    Ok(Table {
        name: name.to_string(),
        time_column,
        label,
        ontologies: described.iter().map(|o| o.name).collect(),
        sample_sql,
    })
}

/// Check a target's shape and return it with its table.
async fn checked(pool: &PgPool, target: &Target) -> Result<Table> {
    let table = table(pool, &target.table).await?;
    match (&target.predicate, &target.ids) {
        (Some(p), None) if !p.trim().is_empty() => {}
        (None, Some(ids)) if !ids.is_empty() => {
            if ids.len() > MAX_IDS {
                return Err(Error::InvalidInput(format!(
                    "{} ids for {} is more than the {MAX_IDS} one target may name. Use `where` instead.",
                    ids.len(),
                    target.table
                )));
            }
        }
        _ => {
            return Err(Error::InvalidInput(format!(
                "The target for {} needs exactly one of `where` or `ids`.",
                target.table
            )))
        }
    }
    Ok(table)
}

/// The filter that selects a target's rows, and its ids when it has them.
/// With ids, they bind as `$1`.
///
/// A predicate sits alone between its parentheses, with the closing one on
/// its own line so a trailing `--` comment cannot swallow it.
fn filter(target: &Target) -> (String, Option<&Vec<String>>) {
    match (&target.predicate, &target.ids) {
        (Some(predicate), _) => (format!("(\n{predicate}\n)"), None),
        (None, ids) => ("id = ANY($1)".to_string(), ids.as_ref()),
    }
}

/// A read-only transaction as the reader login, in the owner's time zone, so
/// a date in a predicate means their day as it does in `sql_query`.
async fn reader(pool: &PgPool) -> Result<Transaction<'static, Postgres>> {
    use crate::database::restricted::{begin, Login};
    let zone = crate::timezone::resolve_timezone(pool, None).await?;
    let mut tx = begin(pool, Login::Reader)
        .await
        .map_err(|e| Error::Database(format!("Couldn't open a read-only connection: {e}")))?;
    sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
    crate::timezone::apply_timezone(&mut tx, &zone).await?;
    sqlx::query(&format!("SET LOCAL statement_timeout = '{PREDICATE_TIMEOUT}'"))
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// What a predicate's failure says to whoever wrote it.
fn predicate_error(table: &str, e: sqlx::Error) -> Error {
    Error::InvalidInput(format!("The filter for {table} did not run: {e}"))
}

// ── Preview ─────────────────────────────────────────────────────────────────

/// A sample row on the card.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub at: Option<DateTime<Utc>>,
    pub label: String,
    pub preview: String,
}

/// One table's share of a deletion, counted.
#[derive(Debug, Clone, Serialize)]
pub struct TargetPreview {
    pub table: String,
    pub label: String,
    pub count: i64,
    pub earliest: Option<DateTime<Utc>>,
    pub latest: Option<DateTime<Utc>>,
    /// A few rows, newest first. Empty for the tool, which must not carry
    /// content into the chat's stored tool result.
    pub samples: Vec<Sample>,
}

/// Count a target's rows and their span, with sample rows when asked.
async fn preview_target(pool: &PgPool, target: &Target, with_samples: bool) -> Result<TargetPreview> {
    let table = checked(pool, target).await?;
    let (filter, ids) = filter(target);
    let span = match table.time_column {
        Some(c) => format!("min({c}), max({c})"),
        None => "NULL::timestamptz, NULL::timestamptz".to_string(),
    };
    let mut tx = reader(pool).await?;
    let sql = format!("SELECT count(*)::bigint, {span} FROM public.{} WHERE {filter}", table.name);
    let mut query = sqlx::query_as::<_, (i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(&sql);
    if let Some(ids) = ids {
        query = query.bind(ids);
    }
    let (count, earliest, latest) =
        query.fetch_one(&mut *tx).await.map_err(|e| predicate_error(&table.name, e))?;

    let mut samples = Vec::new();
    if with_samples && count > 0 {
        if let Some((label_sql, preview_sql)) = table.sample_sql {
            let order = match table.time_column {
                Some(c) => format!("ORDER BY {c} DESC"),
                None => String::new(),
            };
            let sql = format!("SELECT id FROM public.{} WHERE {filter} {order} LIMIT {SAMPLES}", table.name);
            let mut query = sqlx::query_scalar::<_, String>(&sql);
            if let Some(ids) = ids {
                query = query.bind(ids);
            }
            let sample_ids = query.fetch_all(&mut *tx).await.map_err(|e| predicate_error(&table.name, e))?;
            samples = sample_rows(pool, &table, label_sql, preview_sql, &sample_ids).await;
        }
    }
    Ok(TargetPreview { table: table.name, label: table.label, count, earliest, latest, samples })
}

/// The registry's label and preview for a few rows. The card can do without
/// them, so a table whose preview SQL fails shows its count and dates alone.
async fn sample_rows(pool: &PgPool, table: &Table, label_sql: &str, preview_sql: &str, ids: &[String]) -> Vec<Sample> {
    let at = table.time_column.map(|c| format!("t.{c}")).unwrap_or_else(|| "NULL::timestamptz".to_string());
    let sql = format!(
        "SELECT {at} AS at, ({label_sql})::text AS label, left(({preview_sql})::text, 160) AS preview \
         FROM public.{} t WHERE t.id = ANY($1) ORDER BY 1 DESC NULLS LAST",
        table.name
    );
    match sqlx::query(&sql).bind(ids).fetch_all(pool).await {
        Ok(rows) => rows
            .iter()
            .map(|r| Sample {
                at: r.get("at"),
                // absent-ok: the registry's label or preview SQL is NULL for a row with no text; the card shows what it has
                label: r.get::<Option<String>, _>("label").unwrap_or_default(),
                // absent-ok: as the label above
                preview: r.get::<Option<String>, _>("preview").unwrap_or_default(),
            })
            .collect(),
        Err(e) => {
            tracing::warn!(table = %table.name, error = %e, "deletion preview: sample rows unavailable");
            Vec::new()
        }
    }
}

// ── The deletion row ────────────────────────────────────────────────────────

/// A deletion as the card and the receipt read it.
#[derive(Debug, Clone, Serialize)]
pub struct Deletion {
    pub id: String,
    pub chat_id: Option<String>,
    pub summary: String,
    /// proposed | running | done | failed | cancelled
    pub status: String,
    /// Each table's rows, counted now. Present while the deletion waits.
    pub targets: Vec<TargetPreview>,
    /// Why the preview could not be counted, when it could not.
    pub preview_error: Option<String>,
    /// What was removed, once it has run.
    pub receipt: Value,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

struct Stored {
    id: String,
    chat_id: Option<String>,
    summary: String,
    targets: Vec<Target>,
    status: String,
    receipt: Value,
    error: Option<String>,
    created_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

/// A deletion's chat, as `chat_id` over the alias `d`. A chat in Recently
/// deleted can't be opened, so while it waits there the deletion names none.
const CHAT_ID: &str = "CASE WHEN EXISTS (SELECT 1 FROM app_chats c WHERE c.id = d.chat_id AND c.deleted_at IS NULL) THEN d.chat_id END AS chat_id";

async fn stored(pool: &PgPool, id: &str) -> Result<Stored> {
    let row = sqlx::query(&format!(
        "SELECT d.id, {CHAT_ID}, d.summary, d.targets, d.status, d.receipt, d.error, \
                d.created_at, d.approved_at, d.finished_at \
         FROM app_deletions d WHERE d.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound("That deletion is not on your server.".to_string()))?;
    let targets: Value = row.get("targets");
    Ok(Stored {
        id: row.get("id"),
        chat_id: row.get("chat_id"),
        summary: row.get("summary"),
        targets: serde_json::from_value(targets)
            .map_err(|e| Error::Database(format!("deletion {id}: stored targets do not parse: {e}")))?,
        status: row.get("status"),
        receipt: row.get("receipt"),
        error: row.get("error"),
        created_at: row.get("created_at"),
        approved_at: row.get("approved_at"),
        finished_at: row.get("finished_at"),
    })
}

/// The deletions running in this process. A row that says `running` and is
/// not here was cut off by a restart.
fn running() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// What a deletion cut off by a restart says.
const RESTARTED: &str = "Your server restarted before this finished. Allow it again to remove the rest.";

fn is_running_here(id: &str) -> bool {
    running().lock().is_ok_and(|set| set.contains(id))
}

/// A run marked `running` that is not running in this process was cut off by
/// a restart: record it as failed, so the owner can allow it again. Every
/// batch committed whole, so a second run removes only what is left. Returns
/// whether it was.
async fn settle_cut_off(pool: &PgPool, id: &str, status: &str) -> Result<bool> {
    if status != "running" || is_running_here(id) {
        return Ok(false);
    }
    sqlx::query("UPDATE app_deletions SET status = 'failed', error = $2, finished_at = now() WHERE id = $1 AND status = 'running'")
        .bind(id)
        .bind(RESTARTED)
        .execute(pool)
        .await?;
    Ok(true)
}

/// A deletion as it stands. While it waits, its rows are counted and sampled
/// again, so the card shows what allowing it would remove now.
pub async fn get(pool: &PgPool, id: &str) -> Result<Deletion> {
    let mut row = stored(pool, id).await?;

    if settle_cut_off(pool, id, &row.status).await? {
        row.status = "failed".to_string();
        row.error = Some(RESTARTED.to_string());
    }

    let mut targets = Vec::new();
    let mut preview_error = None;
    if row.status == "proposed" || row.status == "failed" {
        for target in &row.targets {
            match preview_target(pool, target, true).await {
                Ok(p) => targets.push(p),
                Err(e) => {
                    preview_error = Some(e.to_string());
                    break;
                }
            }
        }
    }
    Ok(Deletion {
        id: row.id,
        chat_id: row.chat_id,
        summary: row.summary,
        status: row.status,
        targets,
        preview_error,
        receipt: row.receipt,
        error: row.error,
        created_at: row.created_at,
        approved_at: row.approved_at,
        finished_at: row.finished_at,
    })
}

/// Every deletion that ran or is running, newest first: Settings · Deleted
/// records. Every deletion the owner allowed is here, including one that
/// stopped and was then set aside: its tombstones still refuse what it had
/// deleted, and its page is where they can be lifted. A proposal never
/// allowed is not a deletion and is left out. Nothing is counted again; a
/// finished deletion's receipt says what went. One cut off by a restart is
/// recorded as failed, as `get` does.
pub async fn list(pool: &PgPool) -> Result<Vec<Deletion>> {
    let rows = sqlx::query(&format!(
        "SELECT d.id, {CHAT_ID}, d.summary, d.status, d.receipt, d.error, \
                d.created_at, d.approved_at, d.finished_at \
         FROM app_deletions d WHERE d.approved_at IS NOT NULL \
         ORDER BY d.approved_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get("id");
        let mut status: String = row.get("status");
        let mut error: Option<String> = row.get("error");
        if settle_cut_off(pool, &id, &status).await? {
            status = "failed".to_string();
            error = Some(RESTARTED.to_string());
        }
        out.push(Deletion {
            id,
            chat_id: row.get("chat_id"),
            summary: row.get("summary"),
            status,
            targets: Vec::new(),
            preview_error: None,
            receipt: row.get("receipt"),
            error,
            created_at: row.get("created_at"),
            approved_at: row.get("approved_at"),
            finished_at: row.get("finished_at"),
        });
    }
    Ok(out)
}

/// Record a deletion for the owner to allow. Nothing is deleted. Returns its
/// id and each table's count and span, with no row content.
pub async fn propose(
    pool: &PgPool,
    chat_id: Option<&str>,
    summary: &str,
    targets: &[Target],
) -> Result<(String, Vec<TargetPreview>)> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(Error::InvalidInput("Say what is being deleted in `summary`, in plain words.".to_string()));
    }
    if targets.is_empty() || targets.len() > MAX_TARGETS {
        return Err(Error::InvalidInput(format!("A deletion names between 1 and {MAX_TARGETS} tables.")));
    }
    let mut previews = Vec::new();
    for target in targets {
        previews.push(preview_target(pool, target, false).await?);
    }
    if previews.iter().all(|p| p.count == 0) {
        return Err(Error::InvalidInput(
            "Nothing matches, so there is nothing to delete. Check the filter with sql_query.".to_string(),
        ));
    }
    let id = format!("del_{}", uuid::Uuid::new_v4().simple());
    sqlx::query("INSERT INTO app_deletions (id, chat_id, summary, targets) VALUES ($1, $2, $3, $4)")
        .bind(&id)
        .bind(chat_id)
        .bind(summary)
        .bind(serde_json::to_value(targets).map_err(|e| Error::Other(e.to_string()))?)
        .execute(pool)
        .await?;
    Ok((id, previews))
}

/// The owner said no. Only a deletion that has not run can be set aside.
pub async fn cancel(pool: &PgPool, id: &str) -> Result<Deletion> {
    sqlx::query(
        "UPDATE app_deletions SET status = 'cancelled', finished_at = now(), targets = '[]'::jsonb \
         WHERE id = $1 AND status IN ('proposed', 'failed')",
    )
    .bind(id)
    .execute(pool)
    .await?;
    get(pool, id).await
}

/// The owner allowed it: mark it running and do the work on its own task.
/// The card follows it with [`get`].
pub async fn approve(pool: &PgPool, id: &str) -> Result<Deletion> {
    let claimed = sqlx::query(
        "UPDATE app_deletions SET status = 'running', error = NULL, approved_at = now(), finished_at = NULL \
         WHERE id = $1 AND status IN ('proposed', 'failed')",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if claimed == 0 {
        // Already running, done or cancelled: nothing to start, and the card
        // reads the same answer either way.
        return get(pool, id).await;
    }
    if let Ok(mut set) = running().lock() {
        set.insert(id.to_string());
    }
    let pool_for_task = pool.clone();
    let task_id = id.to_string();
    tokio::spawn(async move {
        let outcome = run(&pool_for_task, &task_id).await;
        let written = match &outcome {
            Ok(receipt) => {
                // The filters named what was deleted. Once it is gone, keep
                // only which tables it was.
                sqlx::query(
                    "UPDATE app_deletions SET status = 'done', receipt = $2, finished_at = now(), \
                     targets = (SELECT COALESCE(jsonb_agg(jsonb_build_object('table', t->>'table')), '[]'::jsonb) \
                                FROM jsonb_array_elements(targets) t) \
                     WHERE id = $1",
                )
                .bind(&task_id)
                .bind(receipt)
                .execute(&pool_for_task)
                .await
            }
            Err(e) => {
                tracing::error!(deletion = %task_id, error = %e, "deletion failed");
                sqlx::query("UPDATE app_deletions SET status = 'failed', error = $2, finished_at = now() WHERE id = $1")
                    .bind(&task_id)
                    .bind(e.to_string())
                    .execute(&pool_for_task)
                    .await
            }
        };
        if let Err(e) = written {
            tracing::error!(deletion = %task_id, error = %e, "deletion: could not record the outcome");
        }
        if let Ok(mut set) = running().lock() {
            set.remove(&task_id);
        }
    });
    get(pool, id).await
}

/// Let the records of a finished deletion be collected again: remove its
/// tombstones. The next sync brings back whatever the source still has.
pub async fn lift(pool: &PgPool, id: &str) -> Result<u64> {
    let lifted = sqlx::query("DELETE FROM app_tombstones WHERE deletion_id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    sqlx::query(
        "UPDATE app_deletions SET receipt = receipt || jsonb_build_object('tombstones_lifted', $2::bigint, 'lifted_at', now()) \
         WHERE id = $1",
    )
        .bind(id)
        .bind(lifted as i64)
        .execute(pool)
        .await?;
    Ok(lifted)
}

// ── Doing it ────────────────────────────────────────────────────────────────

/// What one call removed.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Removed {
    /// Rows deleted, the target's own and the rows that follow them.
    pub records: u64,
    /// `search_embeddings` rows.
    pub search_entries: u64,
    /// `wiki_refs` rows.
    pub links: u64,
    /// Attachment and audio files.
    pub files: u64,
    /// Tombstones written.
    pub tombstones: u64,
    /// Raw records taken out of the lake's files.
    pub raw_records: u64,
    /// Deleted records with no raw copy the lake could find.
    pub raw_missing: u64,
    /// Rows deleted per table, in the order they were first touched.
    pub tables: Vec<(String, String, u64)>,
    pub earliest: Option<DateTime<Utc>>,
    pub latest: Option<DateTime<Utc>>,
    /// Days whose audio sessions are rebuilt from what is left.
    #[serde(skip)]
    audio_span: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// The `source_provider` of every row deleted, which says whose raw files
    /// to read.
    #[serde(skip)]
    providers: HashSet<String>,
}

impl Removed {
    fn add_table(&mut self, table: &Table, n: u64) {
        if n == 0 {
            return;
        }
        match self.tables.iter_mut().find(|(name, _, _)| *name == table.name) {
            Some(entry) => entry.2 += n,
            None => self.tables.push((table.name.clone(), table.label.clone(), n)),
        }
    }

    fn widen(&mut self, earliest: Option<DateTime<Utc>>, latest: Option<DateTime<Utc>>) {
        if let Some(e) = earliest {
            self.earliest = Some(self.earliest.map_or(e, |x| x.min(e)));
        }
        if let Some(l) = latest {
            self.latest = Some(self.latest.map_or(l, |x| x.max(l)));
        }
    }

    /// What went, by place, as the chat card and Settings · Deleted records show it.
    pub fn receipt(&self) -> Value {
        json!({
            "records": self.records,
            "search_entries": self.search_entries,
            "links": self.links,
            "files": self.files,
            "tombstones": self.tombstones,
            "raw_records": self.raw_records,
            "raw_missing": self.raw_missing,
            "tables": self.tables.iter().map(|(table, label, records)| json!({
                "table": table, "label": label, "records": records,
            })).collect::<Vec<_>>(),
            "earliest": self.earliest,
            "latest": self.latest,
            "kept": KEPT,
        })
    }
}

/// Run an allowed deletion to the end and return its receipt.
async fn run(pool: &PgPool, id: &str) -> Result<Value> {
    let row = stored(pool, id).await?;
    let mut removed = Removed::default();
    for target in &row.targets {
        let table = checked(pool, target).await?;
        let (filter, ids) = filter(target);
        // Keyset pages by id. Each page is deleted before the next is read,
        // and the cursor still moves past it, so a row that will not delete
        // cannot hold the loop on one page.
        let mut after = String::new();
        loop {
            let sql = format!(
                "SELECT id FROM public.{} WHERE {filter} AND id > {} ORDER BY id LIMIT {BATCH}",
                table.name,
                if ids.is_some() { "$2" } else { "$1" },
            );
            let mut tx = reader(pool).await?;
            let mut query = sqlx::query_scalar::<_, String>(&sql);
            if let Some(ids) = ids {
                query = query.bind(ids);
            }
            let page = query
                .bind(&after)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| predicate_error(&table.name, e))?;
            drop(tx);
            let Some(last) = page.last() else { break };
            after = last.clone();
            delete_and_tombstone(pool, &table, &page, Some(id), &mut removed).await?;
        }
    }
    remove_raw_copies(pool, id, &mut removed).await?;
    rebuild_audio_sessions(pool, &removed).await;
    Ok(removed.receipt())
}

/// Take the raw copies of a deletion's records out of the lake's files.
///
/// Works from the tombstones the deletion wrote, not from the rows, which
/// are gone by now. So a run that stopped after the rows went, and is allowed
/// again, still finds what to remove.
async fn remove_raw_copies(pool: &PgPool, deletion_id: &str, removed: &mut Removed) -> Result<()> {
    use crate::storage::raw_copies::{scrub, Tombstoned};
    let ids: Vec<String> = sqlx::query_scalar("SELECT source_stream_id FROM app_tombstones WHERE deletion_id = $1")
        .bind(deletion_id)
        .fetch_all(pool)
        .await?;
    if ids.is_empty() {
        return Ok(());
    }
    let total = ids.len() as u64;
    // A phone's and a Mac's raw files are filed under the provider their rows
    // carry. A cloud sync's are not always, so a deletion that touched one,
    // or a second run that no longer knows, reads every raw file.
    let devices_only = !removed.providers.is_empty() && removed.providers.iter().all(|p| p == "ios" || p == "mac");
    let providers: Option<Vec<String>> = devices_only.then(|| removed.providers.iter().cloned().collect());

    let storage = crate::storage::lake::storage_from_env()?;
    let raw = scrub(pool, &storage, &Tombstoned::new(ids), providers.as_deref()).await?;
    removed.raw_records += raw.records;
    removed.raw_missing = total.saturating_sub(raw.matched.len() as u64);
    Ok(())
}

/// Delete rows named by id, from Query. Records the deletion so
/// its tombstones have one to point at, and returns what went.
pub async fn delete_rows(pool: &PgPool, table_name: &str, ids: &[String]) -> Result<Removed> {
    let target = Target { table: table_name.to_string(), predicate: None, ids: Some(ids.to_vec()) };
    let table = checked(pool, &target).await?;
    let id = format!("del_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO app_deletions (id, summary, targets, status, approved_at) \
         VALUES ($1, $2, $3, 'running', now())",
    )
    .bind(&id)
    .bind(format!("{}: rows you picked in Query", table.label))
    .bind(json!([{ "table": table.name, "ids": ids }]))
    .execute(pool)
    .await?;
    if let Ok(mut set) = running().lock() {
        set.insert(id.clone());
    }

    let mut removed = Removed::default();
    let mut outcome = Ok(());
    for page in ids.chunks(BATCH as usize) {
        outcome = delete_and_tombstone(pool, &table, page, Some(&id), &mut removed).await;
        if outcome.is_err() {
            break;
        }
    }
    if outcome.is_ok() {
        outcome = remove_raw_copies(pool, &id, &mut removed).await;
    }
    rebuild_audio_sessions(pool, &removed).await;

    let (status, error) = match &outcome {
        Ok(()) => ("done", None),
        Err(e) => ("failed", Some(e.to_string())),
    };
    // Done, it keeps only which table it was; failed, it keeps the ids, so
    // allowing it again deletes what is left.
    sqlx::query(
        "UPDATE app_deletions SET status = $2, error = $3, receipt = $4, finished_at = now(), \
         targets = CASE WHEN $2 = 'done' THEN jsonb_build_array(jsonb_build_object('table', targets->0->>'table')) \
                        ELSE targets END \
         WHERE id = $1",
    )
    .bind(&id)
    .bind(status)
        .bind(error)
        .bind(removed.receipt())
        .execute(pool)
        .await?;
    if let Ok(mut set) = running().lock() {
        set.remove(&id);
    }
    outcome.map(|()| removed)
}

/// Rows in other data tables that go with `ids` of `table`: rows a foreign
/// key would cascade away (a bank account's transactions), and a recording's
/// transcript or a transcript's recording, which share one audio file. They
/// are deleted and tombstoned in their own right, first, so nothing is
/// removed by a cascade that leaves its index rows and no tombstone.
async fn rows_that_follow(pool: &PgPool, table: &Table, ids: &[String]) -> Result<Vec<(Table, Vec<String>)>> {
    let mut out = Vec::new();

    let children: Vec<(String, String)> = sqlx::query_as(
        "SELECT child.relname::text, att.attname::text \
         FROM pg_constraint con \
         JOIN pg_class child ON child.oid = con.conrelid \
         JOIN pg_class parent ON parent.oid = con.confrelid \
         JOIN pg_namespace n ON n.oid = parent.relnamespace \
         JOIN pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = con.conkey[1] \
         JOIN pg_attribute ref ON ref.attrelid = con.confrelid AND ref.attnum = con.confkey[1] \
         WHERE con.contype = 'f' AND n.nspname = 'public' AND parent.relname = $1 \
           AND array_length(con.conkey, 1) = 1 AND ref.attname = 'id' \
           AND child.relname LIKE 'data\\_%' AND child.relname <> $1",
    )
    .bind(&table.name)
    .fetch_all(pool)
    .await?;
    for (child, column) in children {
        if !is_data_table_name(&child) || !is_plain_identifier(&column) {
            continue;
        }
        let child_ids: Vec<String> =
            sqlx::query_scalar(&format!("SELECT id FROM public.{child} WHERE {column} = ANY($1)"))
                .bind(ids)
                .fetch_all(pool)
                .await?;
        if !child_ids.is_empty() {
            out.push((self::table(pool, &child).await?, child_ids));
        }
    }

    let twin = match table.name.as_str() {
        "data_audio_recording" => Some("data_communication_transcription"),
        "data_communication_transcription" => Some("data_audio_recording"),
        _ => None,
    };
    if let Some(twin) = twin {
        let twin_ids: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT o.id FROM public.{twin} o \
             JOIN public.{} t ON t.source_stream_id = o.source_stream_id \
             WHERE t.id = ANY($1)",
            table.name
        ))
        .bind(ids)
        .fetch_all(pool)
        .await?;
        if !twin_ids.is_empty() {
            out.push((self::table(pool, twin).await?, twin_ids));
        }
    }
    Ok(out)
}

/// Delete and tombstone one batch of a table's rows, and the rows that
/// follow them.
async fn delete_and_tombstone(
    pool: &PgPool,
    table: &Table,
    ids: &[String],
    deletion_id: Option<&str>,
    removed: &mut Removed,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    for (follower, follower_ids) in rows_that_follow(pool, table, ids).await? {
        for page in follower_ids.chunks(BATCH as usize) {
            delete_batch(pool, &follower, page, deletion_id, removed).await?;
        }
    }
    delete_batch(pool, table, ids, deletion_id, removed).await
}

/// One batch of one table: files, then the tombstone, the index rows, the
/// links and the row in one transaction.
async fn delete_batch(
    pool: &PgPool,
    table: &Table,
    ids: &[String],
    deletion_id: Option<&str>,
    removed: &mut Removed,
) -> Result<()> {
    // Bytes first, rows second. If a file will not delete, the row still
    // names it and a second run finds it. The other order leaves bytes that
    // nothing points at.
    removed.files += delete_files(pool, table, ids).await?;

    let span: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) = match table.time_column {
        Some(c) => {
            sqlx::query_as(&format!("SELECT min({c}), max({c}) FROM public.{} WHERE id = ANY($1)", table.name))
                .bind(ids)
                .fetch_one(pool)
                .await?
        }
        None => (None, None),
    };

    let providers: Vec<String> =
        sqlx::query_scalar(&format!("SELECT DISTINCT source_provider FROM public.{} WHERE id = ANY($1)", table.name))
            .bind(ids)
            .fetch_all(pool)
            .await?;
    removed.providers.extend(providers);

    let mut tx = pool.begin().await?;
    let tombstones = sqlx::query(&format!(
        "INSERT INTO app_tombstones (source_table, source_stream_id, source_id, deletion_id) \
         SELECT $1, source_stream_id, id, $3 FROM public.{} WHERE id = ANY($2) \
         ON CONFLICT (source_table, source_stream_id) DO NOTHING",
        table.name
    ))
    .bind(&table.name)
    .bind(ids)
    .bind(deletion_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let search_entries = crate::search::evict::evict_records(&mut tx, &table.ontologies, ids).await?;
    let links = sqlx::query("DELETE FROM wiki_refs WHERE source_table = $1 AND source_id = ANY($2)")
        .bind(&table.name)
        .bind(ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let records = sqlx::query(&format!("DELETE FROM public.{} WHERE id = ANY($1)", table.name))
        .bind(ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;

    removed.tombstones += tombstones;
    removed.search_entries += search_entries;
    removed.links += links;
    removed.records += records;
    removed.add_table(table, records);
    if records > 0 {
        removed.widen(span.0, span.1);
        if matches!(table.name.as_str(), "data_audio_recording" | "data_communication_transcription") {
            if let (Some(e), Some(l)) = span {
                removed.audio_span = Some(match removed.audio_span {
                    Some((a, b)) => (a.min(e), b.max(l)),
                    None => (e, l),
                });
            }
        }
    }
    Ok(())
}

/// Delete the files a batch of rows owns. Returns how many went.
async fn delete_files(pool: &PgPool, table: &Table, ids: &[String]) -> Result<u64> {
    match table.name.as_str() {
        "data_communication_message" => {
            let rows: Vec<(Value,)> = sqlx::query_as(
                "SELECT metadata FROM data_communication_message \
                 WHERE id = ANY($1) AND metadata ? 'attachment_media'",
            )
            .bind(ids)
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok(0);
            }
            let storage = crate::storage::lake::storage_from_env()?;
            let mut files = 0;
            for (metadata,) in rows {
                files += crate::storage::message_media::delete_attachment_bytes(pool, &storage, &metadata).await?;
            }
            Ok(files)
        }
        "data_audio_recording" | "data_communication_transcription" => {
            let urls: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT DISTINCT audio_url FROM public.{} WHERE id = ANY($1) AND audio_url IS NOT NULL",
                table.name
            ))
            .bind(ids)
            .fetch_all(pool)
            .await?;
            let root = crate::storage::lake::lake_root();
            let mut files = 0;
            for url in urls {
                let Some(path) = crate::api::recordings::inside(&root, &url) else {
                    tracing::warn!(table = %table.name, "deletion: an audio path leaves the lake; left alone");
                    continue;
                };
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => files += 1,
                    // A recording and its transcript name one file, and the
                    // first of the two to be deleted takes it.
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(Error::Storage(format!(
                            "Couldn't delete a recording's audio file ({e}). Nothing was removed for it; allow the deletion again to retry."
                        )))
                    }
                }
                sqlx::query("DELETE FROM lake_objects WHERE kind = 'media' AND storage_key = $1")
                    .bind(&url)
                    .execute(pool)
                    .await?;
            }
            Ok(files)
        }
        "data_content_photo" => release_photo_pictures(pool, ids).await,
        _ => Ok(0),
    }
}

/// Delete the pictures of photos about to be deleted, except one another photo
/// shows (`api::photos`).
async fn release_photo_pictures(pool: &PgPool, ids: &[String]) -> Result<u64> {
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT asset_id FROM data_content_photo WHERE id = ANY($1) AND asset_id IS NOT NULL",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    if files.is_empty() {
        return Ok(0);
    }
    let config = crate::api::DriveConfig::new(std::sync::Arc::new(crate::storage::lake::storage_from_env()?));
    crate::api::photos::release_pictures(pool, &config, &files, ids).await
}

/// `data_audio_session` rows are written from a day's transcripts and carry
/// their text. Rebuild the sessions of every day that lost a recording or a
/// transcript, from what is left. A day that fails is logged and the nightly
/// sessionizer redoes it; the deletion itself has already committed.
async fn rebuild_audio_sessions(pool: &PgPool, removed: &Removed) {
    let Some((earliest, latest)) = removed.audio_span else { return };
    // A day either side, because a session's day is the owner's, not UTC's.
    let mut day = earliest.date_naive().pred_opt().unwrap_or(earliest.date_naive());
    let last = latest.date_naive().succ_opt().unwrap_or(latest.date_naive());
    while day <= last {
        if let Err(e) = crate::sessionize::audio::sessionize_day(pool, day).await {
            tracing::warn!(%day, error = %e, "deletion: audio sessions not rebuilt for this day");
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn heart_rate(pool: &PgPool, id: &str, stream_id: &str, at: &str) {
        sqlx::query(
            "INSERT INTO data_health_heart_rate (id, bpm, occurred_at, source_stream_id, source_table, source_provider) \
             VALUES ($1, 60, $2::timestamptz, $3, 'stream_ios_healthkit', 'ios') \
             ON CONFLICT (source_stream_id) DO UPDATE SET bpm = EXCLUDED.bpm",
        )
        .bind(id)
        .bind(at)
        .bind(stream_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn count(pool: &PgPool, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    /// What the server does at boot, which a fresh test database has not
    /// had: the reader login's SELECT on the data tables.
    async fn grant_reader(pool: &PgPool) {
        crate::server::faces::ensure_applet_db_grants(pool).await.unwrap();
    }

    /// A test that used the reader login ends with this: `#[sqlx::test]`
    /// cannot drop a database the login's pool still holds a session on.
    async fn close_reader(pool: &PgPool) {
        crate::database::restricted::close_pools(pool).await;
    }

    /// The trigger is created by a loop over the tables that existed when the
    /// migration ran. A data table added later must get it in its own
    /// migration, and this is what says so.
    #[sqlx::test]
    async fn every_data_table_refuses_tombstoned_rows(pool: PgPool) {
        let missing: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname LIKE 'data\\_%' \
               AND EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid \
                           AND a.attname = 'source_stream_id' AND NOT a.attisdropped) \
               AND NOT EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgrelid = c.oid AND t.tgname = 'refuse_tombstoned') \
             ORDER BY 1",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            missing.is_empty(),
            "these data tables have no refuse_tombstoned trigger, so a deleted row would come back: {missing:?}. \
             Add `CREATE TRIGGER refuse_tombstoned BEFORE INSERT ON <table> FOR EACH ROW EXECUTE FUNCTION \
             app_refuse_tombstoned()` to the migration that creates the table."
        );
    }

    #[sqlx::test]
    async fn a_deleted_row_does_not_come_back_until_its_tombstone_is_lifted(pool: PgPool) {
        heart_rate(&pool, "hr_a", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-03T10:05:00Z").await;

        let removed = delete_rows(&pool, "data_health_heart_rate", &["hr_b".to_string()]).await.unwrap();
        assert_eq!((removed.records, removed.tombstones), (1, 1));

        // The source sends both again, as a sync does.
        heart_rate(&pool, "hr_a", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-03T10:05:00Z").await;
        assert_eq!(count(&pool, "SELECT count(*) FROM data_health_heart_rate").await, 1);

        let deletion: String = sqlx::query_scalar("SELECT deletion_id FROM app_tombstones").fetch_one(&pool).await.unwrap();
        assert_eq!(lift(&pool, &deletion).await.unwrap(), 1);
        let receipt = get(&pool, &deletion).await.unwrap().receipt;
        assert_eq!(receipt["tombstones_lifted"], 1);
        assert!(receipt["lifted_at"].is_string(), "the receipt says when its records were let back: {receipt}");
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-03T10:05:00Z").await;
        assert_eq!(count(&pool, "SELECT count(*) FROM data_health_heart_rate").await, 2);
    }

    #[sqlx::test]
    async fn a_deletion_clears_the_search_index_and_the_links(pool: PgPool) {
        heart_rate(&pool, "hr_a", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        sqlx::query(
            "INSERT INTO search_embeddings (id, ontology, record_id, model, bm25_len) \
             VALUES ('e1', 'health_heart_rate', 'hr_a', 'm', 7), ('e2', 'health_heart_rate', 'hr_other', 'm', 5)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO search_index_meta (singleton, n_docs, sum_len) VALUES (TRUE, 2, 12) \
             ON CONFLICT (singleton) DO UPDATE SET n_docs = 2, sum_len = 12",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id) \
             VALUES ('r1', 'person', 'person_x', 'data_health_heart_rate', 'hr_a')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let removed = delete_rows(&pool, "data_health_heart_rate", &["hr_a".to_string()]).await.unwrap();
        assert_eq!((removed.search_entries, removed.links), (1, 1));
        assert_eq!(count(&pool, "SELECT count(*) FROM search_embeddings").await, 1);
        assert_eq!(count(&pool, "SELECT count(*) FROM wiki_refs").await, 0);
        let (docs, len): (i64, i64) =
            sqlx::query_as("SELECT n_docs, sum_len FROM search_index_meta").fetch_one(&pool).await.unwrap();
        assert_eq!((docs, len), (1, 5));
    }

    #[sqlx::test]
    async fn a_proposal_deletes_nothing_until_it_is_allowed(pool: PgPool) {
        grant_reader(&pool).await;
        heart_rate(&pool, "hr_a", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-04T10:00:00Z").await;
        let target = Target {
            table: "data_health_heart_rate".to_string(),
            predicate: Some("occurred_at < '2026-03-04T00:00:00Z'".to_string()),
            ids: None,
        };
        let (id, previews) = propose(&pool, None, "Heart rate from March 3", &[target]).await.unwrap();
        assert_eq!(previews[0].count, 1);
        assert!(previews[0].samples.is_empty(), "the tool's preview carries no row content");
        assert_eq!(count(&pool, "SELECT count(*) FROM data_health_heart_rate").await, 2);

        let receipt = run(&pool, &id).await.unwrap();
        assert_eq!(receipt["records"], 1);
        assert_eq!(count(&pool, "SELECT count(*) FROM data_health_heart_rate").await, 1);
        assert_eq!(count(&pool, "SELECT count(*) FROM app_tombstones").await, 1);
        close_reader(&pool).await;
    }

    #[sqlx::test]
    async fn only_data_tables_with_a_source_id_can_be_targets(pool: PgPool) {
        grant_reader(&pool).await;
        for table in ["app_chats", "credentials", "data_audio_session", "data_nope", "data_x; DROP TABLE app_chats"] {
            let target = Target { table: table.to_string(), predicate: Some("true".to_string()), ids: None };
            assert!(propose(&pool, None, "Everything", &[target]).await.is_err(), "{table}");
        }
        close_reader(&pool).await;
    }

    /// A predicate is the assistant's text. Whatever else it names, it can
    /// only choose rows of the table it is filed under, and it reads as the
    /// reader login.
    #[sqlx::test]
    async fn a_predicate_only_chooses_rows_of_its_own_table(pool: PgPool) {
        grant_reader(&pool).await;
        heart_rate(&pool, "shared_id", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-03T10:05:00Z").await;
        sqlx::query("INSERT INTO app_chats (id, title) VALUES ('shared_id', 'A chat whose id matches a heart rate row')")
            .execute(&pool)
            .await
            .unwrap();
        let heart_rate_where = |predicate: &str| Target {
            table: "data_health_heart_rate".to_string(),
            predicate: Some(predicate.to_string()),
            ids: None,
        };

        // A table the reader may not read: the filter fails, nothing is recorded.
        let hidden = heart_rate_where("EXISTS (SELECT 1 FROM credentials)");
        assert!(propose(&pool, None, "Everything", &[hidden]).await.is_err());
        // A second statement is not a condition.
        let second = heart_rate_where("true; DELETE FROM app_chats");
        assert!(propose(&pool, None, "Everything", &[second]).await.is_err());
        assert_eq!(count(&pool, "SELECT count(*) FROM app_deletions").await, 0);

        // A table the reader may read: the chat's id only picks the heart
        // rate row that shares it, and the chat stays.
        let through_chats = heart_rate_where("id IN (SELECT id FROM app_chats)");
        let (id, previews) = propose(&pool, None, "One reading", &[through_chats]).await.unwrap();
        assert_eq!(previews[0].count, 1);
        run(&pool, &id).await.unwrap();
        assert_eq!(count(&pool, "SELECT count(*) FROM data_health_heart_rate").await, 1);
        assert_eq!(count(&pool, "SELECT count(*) FROM app_chats").await, 1);
        close_reader(&pool).await;
    }

    /// A bank account's transactions go by foreign-key cascade. They are
    /// deleted in their own right first, so each leaves a tombstone and
    /// cannot come back without its account.
    #[sqlx::test]
    async fn rows_a_cascade_would_remove_get_their_own_tombstones(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_financial_account (id, account_name, account_type, source_stream_id, source_table, source_provider) \
             VALUES ('acct_1', 'Checking', 'depository', 'plaid:acct:1', 'stream_plaid_accounts', 'plaid')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO data_financial_transaction \
             (id, account_id, transaction_id, amount, occurred_at, source_stream_id, source_table, source_provider) \
             VALUES ('txn_1', 'acct_1', 't1', 1250, '2026-03-03T10:00:00Z', 'plaid:txn:1', 'stream_plaid_transactions', 'plaid')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let removed = delete_rows(&pool, "data_financial_account", &["acct_1".to_string()]).await.unwrap();
        assert_eq!((removed.records, removed.tombstones), (2, 2));
        let tombstoned: Vec<String> =
            sqlx::query_scalar("SELECT source_table FROM app_tombstones ORDER BY 1").fetch_all(&pool).await.unwrap();
        assert_eq!(tombstoned, ["data_financial_account", "data_financial_transaction"]);
    }

    /// A recording and its transcript are one moment: deleting either takes
    /// both, so no audio is left behind a deleted transcript.
    #[sqlx::test]
    async fn a_transcript_takes_its_recording_with_it(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_audio_recording (id, source_stream_id, started_at, source_table, source_provider) \
             VALUES ('rec_1', 'ios:mic:1', '2026-03-03T10:00:00Z', 'stream_ios_microphone', 'ios')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO data_communication_transcription (id, text, started_at, source_stream_id, source_table, source_provider) \
             VALUES ('tr_1', 'An invented sentence.', '2026-03-03T10:00:00Z', 'ios:mic:1', 'stream_ios_microphone', 'ios')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let removed = delete_rows(&pool, "data_communication_transcription", &["tr_1".to_string()]).await.unwrap();
        assert_eq!(removed.records, 2);
        assert_eq!(count(&pool, "SELECT count(*) FROM data_audio_recording").await, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM app_tombstones").await, 2);
    }

    async fn deletion(pool: &PgPool, id: &str, status: &str, approved: Option<&str>) {
        sqlx::query(
            "INSERT INTO app_deletions (id, summary, targets, status, approved_at) \
             VALUES ($1, 'Invented records', '[]', $2, $3::timestamptz)",
        )
        .bind(id)
        .bind(status)
        .bind(approved)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn the_list_holds_what_ran_newest_first_and_leaves_out_what_never_did(pool: PgPool) {
        deletion(&pool, "del_old", "done", Some("2026-10-03T10:00:00Z")).await;
        deletion(&pool, "del_new", "done", Some("2026-11-12T10:00:00Z")).await;
        deletion(&pool, "del_stopped", "failed", Some("2026-11-09T10:00:00Z")).await;
        // Marked running, but no run is going on in this process: a restart cut it off.
        deletion(&pool, "del_cut", "running", Some("2026-11-14T10:00:00Z")).await;
        // Stopped part way, then set aside: its tombstones still refuse what it deleted.
        deletion(&pool, "del_set_aside", "cancelled", Some("2026-11-10T10:00:00Z")).await;
        deletion(&pool, "del_waiting", "proposed", None).await;
        deletion(&pool, "del_not_now", "cancelled", None).await;

        let listed = list(&pool).await.unwrap();
        let ids: Vec<&str> = listed.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["del_cut", "del_new", "del_set_aside", "del_stopped", "del_old"]);
        assert_eq!(listed[0].status, "failed");
        assert_eq!(listed[0].error.as_deref(), Some(RESTARTED));
        // Recorded, so allowing it again finds it failed.
        let stored: String = sqlx::query_scalar("SELECT status FROM app_deletions WHERE id = 'del_cut'").fetch_one(&pool).await.unwrap();
        assert_eq!(stored, "failed");
    }

    #[sqlx::test]
    async fn a_query_deletion_keeps_its_ids_until_it_is_done(pool: PgPool) {
        grant_reader(&pool).await;
        heart_rate(&pool, "hr_a", "ios:hr:a", "2026-03-03T10:00:00Z").await;
        delete_rows(&pool, "data_health_heart_rate", &["hr_a".to_string()]).await.unwrap();
        let done: Value = sqlx::query_scalar("SELECT targets FROM app_deletions").fetch_one(&pool).await.unwrap();
        assert_eq!(done, json!([{ "table": "data_health_heart_rate" }]), "done, it keeps only its table");

        // One that stopped part way keeps the ids, so allowing it again deletes the rest.
        heart_rate(&pool, "hr_b", "ios:hr:b", "2026-03-03T10:05:00Z").await;
        sqlx::query(
            "INSERT INTO app_deletions (id, summary, targets, status, approved_at) \
             VALUES ('del_q', 'Heart rate: rows you picked in Query', $1, 'failed', now())",
        )
        .bind(json!([{ "table": "data_health_heart_rate", "ids": ["hr_b"] }]))
        .execute(&pool)
        .await
        .unwrap();
        let receipt = run(&pool, "del_q").await.unwrap();
        assert_eq!(receipt["records"], 1);
        close_reader(&pool).await;
    }

    #[sqlx::test]
    async fn a_deletion_stays_when_its_chat_is_deleted(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title) VALUES ('chat_d', 'Clearing out October')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_deletions (id, chat_id, summary, targets, status, approved_at) \
             VALUES ('del_d', 'chat_d', 'Invented records', '[]', 'done', now())",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO app_deletions (id, chat_id, summary, targets) VALUES ('del_never', 'chat_d', 'Invented records', '[]')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // In Recently deleted the chat can't be opened, so the deletion names none.
        sqlx::query("UPDATE app_chats SET deleted_at = now() WHERE id = 'chat_d'").execute(&pool).await.unwrap();
        assert_eq!(get(&pool, "del_d").await.unwrap().chat_id, None);

        sqlx::query("DELETE FROM app_chats WHERE id = 'chat_d'").execute(&pool).await.unwrap();
        let kept = get(&pool, "del_d").await.unwrap();
        assert_eq!(kept.chat_id, None, "the deletion stays, and only loses its chat");
        assert_eq!(
            count(&pool, "SELECT count(*) FROM app_deletions WHERE id = 'del_never'").await,
            0,
            "a proposal never allowed goes with its chat"
        );
    }
}
