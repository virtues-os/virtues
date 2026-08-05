//! Applet-owned tables: versioned, append-only, and honest about drift.
//!
//! `schema_sql` used to be applied as one idempotent file. That works exactly
//! once. Re-calling `setup_applet` is the edit path, so the second call
//! rewrote the file and re-applied it — and `CREATE TABLE IF NOT EXISTS`, on a
//! table that already exists, does nothing.
//!
//! The failure pointed the wrong way, which is what made it dangerous: the
//! apply *succeeded*, so a model adding a column believed the column was
//! there and wrote a prompt that used it. Every later `sql_write` naming that
//! column failed at runtime, nightly, forever.
//!
//! Two halves fix it, and both are needed:
//!
//! 1. **Mechanism** — each call's DDL is a numbered migration, applied once
//!    and recorded in `app_applet_schema_migrations`. Files live at
//!    `applets/<slug>/schema/NNNN_*.sql` so the folder stays the portable
//!    definition and a fresh box replays them to the same shape.
//!
//! 2. **A check that can see the mistake** — the mechanism alone does not
//!    help if the model resubmits the whole `CREATE TABLE IF NOT EXISTS` with
//!    an extra column, which is the natural thing to do. That version would be
//!    recorded as applied while changing nothing. So before anything is
//!    written, declared columns are compared against the live table, and a
//!    column that would not be created is a check failure carrying the exact
//!    `ALTER TABLE` to write instead.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// The legacy single-file name. Present in folders authored before versioning;
/// read as version 1 so those applets keep working and can be migrated on top.
pub const LEGACY_SCHEMA_FILE: &str = "schema.sql";

/// Subdirectory holding the numbered versions.
pub const SCHEMA_DIR: &str = "schema";

pub fn checksum(ddl: &str) -> String {
    let mut h = Sha256::new();
    h.update(ddl.as_bytes());
    format!("{:x}", h.finalize())
}

/// One recorded, applied version.
#[derive(Debug, Clone)]
pub struct AppliedMigration {
    pub version: i32,
    pub checksum: String,
}

/// Versions this box has already run for an applet, oldest first.
pub async fn applied(pool: &PgPool, applet_id: &str) -> Result<Vec<AppliedMigration>, String> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT version, checksum FROM app_applet_schema_migrations \
         WHERE applet_id = $1 ORDER BY version",
    )
    .bind(applet_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(version, checksum)| AppliedMigration { version, checksum })
        .collect())
}

/// Record a version as applied.
pub async fn record(
    pool: &PgPool,
    applet_id: &str,
    version: i32,
    name: &str,
    checksum: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO app_applet_schema_migrations (applet_id, version, name, checksum) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (applet_id, version) DO NOTHING",
    )
    .bind(applet_id)
    .bind(version)
    .bind(name)
    .bind(checksum)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// What to do with a freshly submitted `schema_sql`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// Byte-identical to a version already applied — the ordinary case when a
    /// re-setup changes the prompt and resubmits the same schema. Nothing to
    /// write, nothing to run.
    AlreadyApplied { version: i32 },
    /// New DDL. Write it as this version and apply it.
    NewVersion { version: i32 },
}

/// Decide how to treat submitted DDL against the applied set.
///
/// Only the *latest* version is compared, not every historical one. Matching
/// an older checksum would mean the model resubmitted a schema two edits stale,
/// which is a genuine change (a revert) and should be recorded as one rather
/// than silently treated as a no-op.
pub fn classify(ddl: &str, applied: &[AppliedMigration]) -> Submission {
    let sum = checksum(ddl);
    match applied.last() {
        Some(latest) if latest.checksum == sum => Submission::AlreadyApplied {
            version: latest.version,
        },
        Some(latest) => Submission::NewVersion {
            version: latest.version + 1,
        },
        None => Submission::NewVersion { version: 1 },
    }
}

/// Every version an applet folder carries, in order: the numbered files under
/// `schema/`, or a bare legacy `schema.sql` read as version 1.
///
/// The folder is the portable definition — this is what a box replays to reach
/// the shape the applet expects, and the reason versions are files rather than
/// only rows.
pub fn versions_on_disk(dir: &std::path::Path) -> Vec<(i32, String, String)> {
    let schema_dir = dir.join(SCHEMA_DIR);
    if schema_dir.is_dir() {
        let mut out: Vec<(i32, String, String)> = std::fs::read_dir(&schema_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("sql") {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_string();
                // Leading digits are the ordinal, matching how `make migration`
                // numbers the box's own migrations.
                let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
                let version: i32 = digits.parse().ok()?;
                let sql = std::fs::read_to_string(&path).ok()?;
                Some((version, name, sql))
            })
            .collect();
        out.sort_by_key(|(v, _, _)| *v);
        return out;
    }

    // Pre-versioning folder: one file, which is version 1 by definition.
    let legacy = dir.join(LEGACY_SCHEMA_FILE);
    match std::fs::read_to_string(&legacy) {
        Ok(sql) => vec![(1, LEGACY_SCHEMA_FILE.to_string(), sql)],
        Err(_) => Vec::new(),
    }
}

/// Apply any versions on disk that this box has not run yet, oldest first.
///
/// Idempotent and safe to call on every reconcile: a box that is current does
/// nothing. This is what makes a folder restored from git — or one that
/// travelled with a backup — actually produce its tables, rather than leaving
/// an applet whose prompt references a table nobody ever created.
pub async fn replay_pending(
    pool: &PgPool,
    applet_id: &str,
    slug: &str,
    dir: &std::path::Path,
) -> usize {
    let on_disk = versions_on_disk(dir);
    if on_disk.is_empty() {
        return 0;
    }
    let already = applied(pool, applet_id).await.unwrap_or_default();
    let mut ran = 0usize;

    for (version, name, sql) in on_disk {
        if let Some(prior) = already.iter().find(|a| a.version == version) {
            // A file whose contents no longer match what this box ran is a
            // divergence, not an update. Refusing loudly beats re-running DDL
            // against tables that were built from something else.
            if prior.checksum != checksum(&sql) {
                tracing::warn!(
                    applet_id,
                    version,
                    "applet schema version on disk differs from the one applied here; skipping"
                );
            }
            continue;
        }
        // The same guards the authoring path uses — a folder can arrive from a
        // git import, so its DDL is no more trusted than a model's.
        if let Err(e) = validate_text(&sql, slug) {
            tracing::warn!(applet_id, version, error = %e, "applet schema version rejected");
            continue;
        }
        if let Err(e) = apply(pool, &sql).await {
            tracing::warn!(applet_id, version, error = %e, "applet schema version failed to apply");
            break; // ordered migrations: do not run N+1 after N failed
        }
        if let Err(e) = record(pool, applet_id, version, &name, &checksum(&sql)).await {
            tracing::warn!(applet_id, version, error = %e, "failed to record applet schema version");
        }
        ran += 1;
    }
    ran
}

// ============================================================================
// Drift detection — the half that makes the silent failure loud
// ============================================================================

/// A table and the columns a `CREATE TABLE` statement declares for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredTable {
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
}

/// Pull `CREATE TABLE` declarations out of DDL.
///
/// Deliberately conservative: it recognizes the shape the authoring loop
/// actually writes and gives up on anything it cannot read cleanly. A missed
/// declaration costs one unhelpful check; a *wrongly* parsed one costs a false
/// finding that sends the model chasing a problem it does not have, which is
/// far worse in a retry loop.
pub fn declared_tables(ddl: &str) -> Vec<DeclaredTable> {
    let mut out = Vec::new();
    let lower = ddl.to_lowercase();
    let mut search_from = 0usize;

    while let Some(rel) = lower[search_from..].find("create table") {
        let start = search_from + rel;
        search_from = start + "create table".len();

        // Everything between "create table" and the opening paren is the
        // (optional) IF NOT EXISTS and the qualified name.
        let Some(paren_rel) = ddl[search_from..].find('(') else {
            break;
        };
        let paren = search_from + paren_rel;
        let head = ddl[search_from..paren]
            .replace("IF NOT EXISTS", "")
            .replace("if not exists", "");
        let name = head.trim().trim_matches('"').to_string();
        let Some((schema, table)) = name.split_once('.') else {
            continue; // unqualified — validate_schema_text rejects these anyway
        };

        // Match the closing paren of the column list, respecting nesting
        // (a CHECK constraint or a numeric(10,2) contains parens of its own).
        let body_start = paren + 1;
        let mut depth = 1i32;
        let mut end = None;
        for (i, c) in ddl[body_start..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(body_start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let body = &ddl[body_start..end];
        search_from = end;

        let columns = split_top_level(body)
            .into_iter()
            .filter_map(|item| {
                let item = item.trim();
                let first = item.split_whitespace().next()?.trim_matches('"');
                // Table-level constraints are not columns.
                if matches!(
                    first.to_lowercase().as_str(),
                    "primary" | "foreign" | "unique" | "check" | "constraint" | "exclude" | "like"
                ) {
                    return None;
                }
                if first.is_empty() {
                    return None;
                }
                Some(first.to_lowercase())
            })
            .collect();

        out.push(DeclaredTable {
            schema: schema.trim().trim_matches('"').to_lowercase(),
            table: table.trim().trim_matches('"').to_lowercase(),
            columns,
        });
    }
    out
}

/// Split a column list on commas that are not inside parentheses or quotes.
fn split_top_level(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut cur = String::new();
    for c in body.chars() {
        match c {
            '\'' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '(' if !in_quote => {
                depth += 1;
                cur.push(c);
            }
            ')' if !in_quote => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 && !in_quote => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Columns a live table actually has. Empty when the table does not exist.
async fn live_columns(pool: &PgPool, schema: &str, table: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = $1 AND table_name = $2",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// A column the DDL declares that the DDL will not actually create.
#[derive(Debug, Clone)]
pub struct Drift {
    pub schema: String,
    pub table: String,
    pub missing: Vec<String>,
}

impl Drift {
    /// The finding text — the exact statement to write instead. Error-message
    /// quality is the whole point here: the reader is a model in a retry loop,
    /// and "this does nothing" without "write this instead" costs a round trip.
    pub fn message(&self) -> String {
        let cols = self.missing.join(", ");
        format!(
            "table {}.{} already exists, so CREATE TABLE IF NOT EXISTS will not add {} — \
             the apply would succeed and change nothing",
            self.schema, self.table, cols
        )
    }

    pub fn suggestion(&self) -> String {
        let alters: Vec<String> = self
            .missing
            .iter()
            .map(|c| {
                format!(
                    "ALTER TABLE {}.{} ADD COLUMN IF NOT EXISTS {c} <type>;",
                    self.schema, self.table
                )
            })
            .collect();
        format!(
            "each setup_applet call is one MIGRATION, not the whole schema — \
             submit only what changed: {}",
            alters.join(" ")
        )
    }
}

/// Find columns the submitted DDL declares on tables that already exist.
///
/// This is the check that could not previously exist: the DDL is valid, the
/// apply succeeds, and the column is simply never created.
pub async fn detect_drift(pool: &PgPool, ddl: &str) -> Vec<Drift> {
    let mut out = Vec::new();
    for decl in declared_tables(ddl) {
        let Ok(live) = live_columns(pool, &decl.schema, &decl.table).await else {
            continue;
        };
        if live.is_empty() {
            continue; // table does not exist yet — the CREATE will do its job
        }
        let live: Vec<String> = live.into_iter().map(|c| c.to_lowercase()).collect();
        let missing: Vec<String> = decl
            .columns
            .iter()
            .filter(|c| !live.contains(c))
            .cloned()
            .collect();
        if !missing.is_empty() {
            out.push(Drift {
                schema: decl.schema,
                table: decl.table,
                missing,
            });
        }
    }
    out
}

// ============================================================================
// Applying DDL — the security guards, in one place
// ============================================================================

pub async fn check(pool: &PgPool, ddl: &str, slug: &str) -> Result<(), String> {
    validate_text(ddl, slug)?;
    run_schema_statements(pool, ddl, false).await
}

/// Apply DDL for real (post-check), committing it.
pub async fn apply(pool: &PgPool, ddl: &str) -> Result<(), String> {
    run_schema_statements(pool, ddl, true).await
}

/// Textual guards: no transaction control / role / grant statements, and every
/// schema-qualified identifier must live in the applet's own schema.
pub fn validate_text(ddl: &str, slug: &str) -> Result<(), String> {
    let lowered = ddl.to_lowercase();
    for kw in ["commit", "rollback", "savepoint", "grant", "revoke", "create role", "drop role"] {
        if lowered.contains(kw) {
            return Err(format!("schema_sql may not contain '{kw}'"));
        }
    }
    let expected = format!("applet_{slug}");
    for token in lowered.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')) {
        if let Some((schema, _)) = token.split_once('.') {
            if (schema.starts_with("applet_") && schema != expected)
                || schema == "public"
                || schema.starts_with("data_")
                || schema.starts_with("app_")
                || schema.starts_with("wiki_")
            {
                return Err(format!(
                    "schema_sql must only target schema {expected} (found '{token}')"
                ));
            }
        }
    }
    if !lowered.contains(&expected) {
        return Err(format!(
            "schema_sql must create tables in schema {expected} (start with CREATE SCHEMA IF NOT EXISTS {expected};)"
        ));
    }
    Ok(())
}

/// Run the DDL statement-by-statement over the extended protocol inside one
/// transaction (ROLLBACK for the dry-run check, COMMIT for apply). One
/// statement per query means a smuggled multi-statement string is a protocol
/// error, not an escape — and it avoids `raw_sql`, whose future trips
/// rustc's Send-generality inference inside the agent stream.
async fn run_schema_statements(pool: &PgPool, ddl: &str, commit: bool) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // Empty search_path is the real guard (the textual check is belt-and-
    // braces): every table name must be schema-qualified, so an unqualified
    // `DROP TABLE data_location_points` or `CREATE TABLE evil (...)` resolves
    // to no schema and errors instead of hitting `public`. Combined with
    // validate_text rejecting any qualified name outside applet_<slug>,
    // there is no path for this DDL to touch data_*/wiki_*/app_*/public.
    sqlx::query("SET LOCAL search_path = ''")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for stmt in ddl.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(stmt).execute(&mut *tx).await {
            tx.rollback().await.ok();
            return Err(format!("in statement '{}…': {e}", &stmt[..stmt.len().min(60)]));
        }
    }
    if commit {
        tx.commit().await.map_err(|e| e.to_string())
    } else {
        tx.rollback().await.map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognizes_an_unchanged_resubmit() {
        let ddl = "CREATE TABLE applet_x.a (id int);";
        let applied = vec![AppliedMigration {
            version: 1,
            checksum: checksum(ddl),
        }];
        assert_eq!(
            classify(ddl, &applied),
            Submission::AlreadyApplied { version: 1 },
            "re-setup that did not touch the schema must not append a version"
        );
    }

    #[test]
    fn classify_numbers_the_first_and_the_next() {
        assert_eq!(
            classify("CREATE TABLE applet_x.a (id int);", &[]),
            Submission::NewVersion { version: 1 }
        );
        let applied = vec![AppliedMigration {
            version: 1,
            checksum: "other".into(),
        }];
        assert_eq!(
            classify("ALTER TABLE applet_x.a ADD COLUMN b int;", &applied),
            Submission::NewVersion { version: 2 }
        );
    }

    #[test]
    fn parses_the_shape_the_authoring_loop_writes() {
        let ddl = "CREATE SCHEMA IF NOT EXISTS applet_calories;\n\
                   CREATE TABLE IF NOT EXISTS applet_calories.entries (\n\
                     id TEXT PRIMARY KEY,\n\
                     eaten_at TIMESTAMPTZ NOT NULL DEFAULT now(),\n\
                     kcal INTEGER NOT NULL\n\
                   );";
        let t = declared_tables(ddl);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].schema, "applet_calories");
        assert_eq!(t[0].table, "entries");
        assert_eq!(t[0].columns, vec!["id", "eaten_at", "kcal"]);
    }

    #[test]
    fn nested_parens_do_not_end_the_column_list() {
        let ddl = "CREATE TABLE applet_x.t (\
                     amount NUMERIC(10,2) NOT NULL,\
                     kind TEXT CHECK (kind IN ('a','b')),\
                     PRIMARY KEY (amount, kind)\
                   );";
        let t = declared_tables(ddl);
        assert_eq!(t.len(), 1);
        // numeric(10,2) must not split into two columns, the CHECK's inner
        // comma must not either, and PRIMARY KEY is a constraint not a column.
        assert_eq!(t[0].columns, vec!["amount", "kind"]);
    }

    #[test]
    fn table_level_constraints_are_not_columns() {
        let ddl = "CREATE TABLE applet_x.t (\
                     id TEXT,\
                     other_id TEXT,\
                     CONSTRAINT fk FOREIGN KEY (other_id) REFERENCES applet_x.u(id),\
                     UNIQUE (id)\
                   );";
        assert_eq!(declared_tables(ddl)[0].columns, vec!["id", "other_id"]);
    }

    #[test]
    fn several_tables_in_one_submission() {
        let ddl = "CREATE TABLE applet_x.a (id TEXT); CREATE TABLE applet_x.b (id TEXT, n INT);";
        let t = declared_tables(ddl);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].table, "b");
        assert_eq!(t[1].columns, vec!["id", "n"]);
    }

    #[test]
    fn unparseable_ddl_yields_nothing_rather_than_a_wrong_guess() {
        // No column list at all — better to skip the drift check than to
        // invent a finding the model would chase.
        assert!(declared_tables("CREATE TABLE applet_x.a").is_empty());
        assert!(declared_tables("ALTER TABLE applet_x.a ADD COLUMN b INT;").is_empty());
    }

    async fn insert_applet(pool: &PgPool, id: &str) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, agent) VALUES ($1, $1, 'ai', 'x')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("insert applet");
    }

    fn write_version(dir: &std::path::Path, name: &str, sql: &str) {
        let d = dir.join(SCHEMA_DIR);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(name), sql).unwrap();
    }

    /// The whole point: a version runs once. Replaying is what reconcile does
    /// on every pass, so a second call must be a no-op rather than re-running
    /// DDL against tables that already exist.
    #[sqlx::test]
    async fn replay_applies_each_version_exactly_once(pool: PgPool) {
        let id = "applet_user__t1";
        insert_applet(&pool, id).await;
        let tmp = tempfile::tempdir().unwrap();
        write_version(
            tmp.path(),
            "0001_schema.sql",
            "CREATE SCHEMA IF NOT EXISTS applet_t1; \
             CREATE TABLE IF NOT EXISTS applet_t1.entries (id TEXT PRIMARY KEY);",
        );

        assert_eq!(replay_pending(&pool, id, "t1", tmp.path()).await, 1);
        assert_eq!(
            replay_pending(&pool, id, "t1", tmp.path()).await,
            0,
            "a box already current must do nothing"
        );
        assert_eq!(applied(&pool, id).await.unwrap().len(), 1);
    }

    /// An edit adds a version, and only the new one runs.
    #[sqlx::test]
    async fn replay_runs_only_the_new_version(pool: PgPool) {
        let id = "applet_user__t2";
        insert_applet(&pool, id).await;
        let tmp = tempfile::tempdir().unwrap();
        write_version(
            tmp.path(),
            "0001_schema.sql",
            "CREATE SCHEMA IF NOT EXISTS applet_t2; \
             CREATE TABLE IF NOT EXISTS applet_t2.entries (id TEXT PRIMARY KEY);",
        );
        assert_eq!(replay_pending(&pool, id, "t2", tmp.path()).await, 1);

        // The shape an edit now produces: only the change.
        write_version(
            tmp.path(),
            "0002_schema.sql",
            "ALTER TABLE applet_t2.entries ADD COLUMN kcal INTEGER;",
        );
        assert_eq!(replay_pending(&pool, id, "t2", tmp.path()).await, 1);

        let cols = live_columns(&pool, "applet_t2", "entries").await.unwrap();
        assert!(cols.contains(&"kcal".to_string()), "the ALTER actually ran");
    }

    /// The regression this module exists for. Re-sending the CREATE with an
    /// extra column applies cleanly and adds nothing — so it must be caught
    /// before it is recorded as a successful migration.
    #[sqlx::test]
    async fn drift_catches_a_create_that_would_add_nothing(pool: PgPool) {
        apply(
            &pool,
            "CREATE SCHEMA IF NOT EXISTS applet_t3; \
             CREATE TABLE IF NOT EXISTS applet_t3.entries (id TEXT PRIMARY KEY);",
        )
        .await
        .unwrap();

        let resend = "CREATE SCHEMA IF NOT EXISTS applet_t3; \
                      CREATE TABLE IF NOT EXISTS applet_t3.entries (\
                        id TEXT PRIMARY KEY, protein_g INTEGER);";
        let drift = detect_drift(&pool, resend).await;
        assert_eq!(drift.len(), 1, "the silent no-op is seen");
        assert_eq!(drift[0].missing, vec!["protein_g"]);
        assert!(drift[0].suggestion().contains("ADD COLUMN IF NOT EXISTS protein_g"));
    }

    /// And it must not cry wolf: a first-time CREATE has no drift, and neither
    /// does an unchanged resubmit.
    #[sqlx::test]
    async fn drift_is_silent_when_nothing_is_wrong(pool: PgPool) {
        let ddl = "CREATE SCHEMA IF NOT EXISTS applet_t4; \
                   CREATE TABLE IF NOT EXISTS applet_t4.entries (id TEXT PRIMARY KEY);";
        assert!(
            detect_drift(&pool, ddl).await.is_empty(),
            "table does not exist yet — the CREATE will do its job"
        );
        apply(&pool, ddl).await.unwrap();
        assert!(
            detect_drift(&pool, ddl).await.is_empty(),
            "unchanged resubmit declares nothing new"
        );
    }

    #[test]
    fn the_suggestion_names_the_statement_to_write() {
        let d = Drift {
            schema: "applet_calories".into(),
            table: "entries".into(),
            missing: vec!["protein_g".into()],
        };
        assert!(d.message().contains("will not add protein_g"));
        assert!(d
            .suggestion()
            .contains("ALTER TABLE applet_calories.entries ADD COLUMN IF NOT EXISTS protein_g"));
    }
}
