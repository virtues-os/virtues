//! Removing a deleted record's raw copy from the lake.
//!
//! The lake keeps what every sync received, one JSON record per line, before
//! any transform ran (`storage::lake`). A deletion that removed only the
//! `data_*` row would leave the record's original on disk. This module takes
//! it out, in two places:
//!
//!   - [`scrub`] rewrites the raw files already on disk, when a deletion runs
//!     (`api::deletions`);
//!   - [`without_tombstoned`] drops tombstoned records from a batch before
//!     [`super::lake::archive`] writes it, so a record the source sends again
//!     does not land in a new raw file.
//!
//! # How a raw record is matched to a tombstone
//!
//! A tombstone is a row's `source_stream_id`. Each sync derives that id in its
//! own way, so a raw record is matched by what it says about itself:
//!
//!   - **By its own id fields.** A raw object names a tombstoned record when
//!     one of its [`IDENTITY_KEYS`] holds a value that is a tombstoned id, or
//!     that a tombstoned id ends with after a colon (`github:star:<node_id>`,
//!     `<calendar id>:<event id>`, `mac:<device>:<browser>:<guid>`). What goes
//!     is the nearest enclosing unit: the element of the array the object sits
//!     in, or the whole line. So a Plaid page loses one transaction from its
//!     `added` list, and an iMessage line goes whole.
//!   - **By the id its transform computes**, for the two cases where that is a
//!     hash and no field holds it: an iOS record with no `id`
//!     (`virtues_helpers::ios::stream_id_or_hash`) and a Mac browser visit
//!     (`virtues_helpers::mac::browser_stream_id`). The transform and this
//!     module call the same function.
//!
//! Reference keys are not identity keys: a transaction's `account_id` or a
//! reply's `reply_to_guid` does not make that record go.
//!
//! # What this does not reach
//!
//! A row with no raw record of its own has nothing here to remove: a location
//! visit and an app session are computed from other rows, and an iOS shared
//! bookmark is keyed by its URL. [`RawRemoved::matched`] says which tombstoned
//! ids had a raw copy, and the deletion's receipt reports the rest.
//!
//! Two tombstoned ids that end the same way are not told apart: an event that
//! sits in two Google calendars has one event id, so deleting it from one
//! calendar removes the raw copy under both. The other calendar's row stays.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::Storage;
use crate::{Error, Result};

/// Keys under which a sync's raw record carries its own id. A value under one
/// of these is compared with the tombstoned ids; a value under any other key
/// is not, so a reference to a deleted record does not remove the record that
/// makes it.
///
/// `message_guid` is the exception that is a reference on purpose: it is the
/// key of an iMessage attachment's record and of a deletion notice, which are
/// about that one message and go with it.
const IDENTITY_KEYS: &[&str] =
    &["id", "guid", "uuid", "node_id", "transaction_id", "message_guid", "rest_id", "id_str", "pk"];

/// The shortest value compared by its ending. A two-digit id would match the
/// end of unrelated ids; every provider id in use is far longer.
const MIN_TAIL_CHARS: usize = 6;

/// The text of an id after its last colon. Kept in step with the index in
/// migration 0090 and the query in [`tombstoned_among`].
fn tail(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

/// The tombstoned ids one pass matches against.
pub struct Tombstoned {
    ids: HashSet<String>,
    by_tail: HashMap<String, Vec<String>>,
}

impl Tombstoned {
    pub fn new(ids: impl IntoIterator<Item = String>) -> Self {
        let ids: HashSet<String> = ids.into_iter().collect();
        let mut by_tail: HashMap<String, Vec<String>> = HashMap::new();
        for id in &ids {
            by_tail.entry(tail(id).to_string()).or_default().push(id.clone());
        }
        Self { ids, by_tail }
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The tombstoned id that is exactly `id`.
    fn exact(&self, id: &str) -> Option<&str> {
        self.ids.get(id).map(String::as_str)
    }

    /// The tombstoned id a raw record's own id names: the id itself, or one
    /// that ends with a colon and the id.
    fn named_by(&self, value: &str) -> Option<&str> {
        if let Some(id) = self.exact(value) {
            return Some(id);
        }
        if value.chars().count() < MIN_TAIL_CHARS {
            return None;
        }
        self.by_tail.get(tail(value))?.iter().map(String::as_str).find(|id| {
            id.strip_suffix(value).is_some_and(|before| before.ends_with(':'))
        })
    }
}

/// A scalar as the id it would be: a string, or a whole number as its digits.
fn scalar_id(value: &Value) -> Option<Cow<'_, str>> {
    match value {
        Value::String(s) if !s.is_empty() => Some(Cow::Borrowed(s.as_str())),
        Value::Number(n) if n.is_i64() || n.is_u64() => Some(Cow::Owned(n.to_string())),
        _ => None,
    }
}

/// The id a record's transform computes when no field holds it, for a record
/// in memory: an iOS record of a hashed stream, a Mac browser visit.
fn computed_id(provider: &str, stream: &str, record: &Value) -> Option<String> {
    match provider {
        "ios" => virtues_helpers::ios::hashed_stream_table(stream)
            .map(|table| virtues_helpers::ios::stream_id_or_hash(record, table)),
        "mac" if stream == "browser_history" => virtues_helpers::mac::browser_visit_stream_id(record),
        _ => None,
    }
}

/// [`computed_id`] for a record read back from its archived line.
fn computed_id_of_line(provider: &str, stream: &str, line: &str, record: &Value) -> Option<String> {
    match provider {
        "ios" => virtues_helpers::ios::hashed_stream_table(stream)
            .map(|table| virtues_helpers::ios::stream_id_of_archived_line(line, record, table)),
        _ => computed_id(provider, stream, record),
    }
}

/// Every value in `value` that could be a record's own id, for the lookup
/// that fetches the tombstones a batch might match.
fn own_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if IDENTITY_KEYS.contains(&key.as_str()) {
                    if let Some(id) = scalar_id(child) {
                        out.push(id.into_owned());
                    }
                }
                own_ids(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| own_ids(item, out)),
        _ => {}
    }
}

/// Remove tombstoned records from inside `value`, and say whether `value`
/// itself names one. An array loses the elements that do; an object reports
/// it upward, to the array element or the line that holds it.
fn strip(value: &mut Value, tombstoned: &Tombstoned, matched: &mut HashSet<String>, removed: &mut u64) -> bool {
    match value {
        Value::Object(map) => {
            let mut names_one = false;
            for (key, child) in map.iter_mut() {
                if IDENTITY_KEYS.contains(&key.as_str()) {
                    if let Some(id) = scalar_id(child).and_then(|id| tombstoned.named_by(&id).map(str::to_string)) {
                        matched.insert(id);
                        names_one = true;
                    }
                }
                if child.is_object() {
                    names_one |= strip(child, tombstoned, matched, removed);
                } else if child.is_array() {
                    strip(child, tombstoned, matched, removed);
                }
            }
            names_one
        }
        Value::Array(items) => {
            let before = items.len();
            items.retain_mut(|item| !strip(item, tombstoned, matched, removed));
            *removed += (before - items.len()) as u64;
            false
        }
        _ => false,
    }
}

/// What became of one raw record.
enum Scrubbed {
    /// Nothing in it is tombstoned.
    Kept,
    /// Tombstoned records were taken out of it; this is what is left.
    Edited(Value),
    /// The record itself is tombstoned.
    Dropped,
}

/// Check one raw record. `computed` is the id its transform computes, when
/// its stream has one.
fn scrub_record(
    record: &Value,
    computed: Option<String>,
    tombstoned: &Tombstoned,
    matched: &mut HashSet<String>,
    removed: &mut u64,
) -> Scrubbed {
    if let Some(id) = computed.as_deref().and_then(|id| tombstoned.exact(id)) {
        matched.insert(id.to_string());
        *removed += 1;
        return Scrubbed::Dropped;
    }
    let mut edited = record.clone();
    let before = *removed;
    if strip(&mut edited, tombstoned, matched, removed) {
        *removed += 1;
        return Scrubbed::Dropped;
    }
    if *removed > before {
        Scrubbed::Edited(edited)
    } else {
        Scrubbed::Kept
    }
}

/// The tombstoned ids that any of `candidates` could name: one indexed lookup
/// by each candidate's tail. The result is a superset, which
/// [`Tombstoned::named_by`] narrows.
async fn tombstoned_among(pool: &PgPool, candidates: &[String]) -> Result<Tombstoned> {
    let tails: Vec<&str> = candidates.iter().map(|c| tail(c)).collect::<HashSet<_>>().into_iter().collect();
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT source_stream_id FROM app_tombstones \
         WHERE substring(source_stream_id from '[^:]*$') = ANY($1)",
    )
    .bind(&tails)
    .fetch_all(pool)
    .await?;
    Ok(Tombstoned::new(ids))
}

/// `records` without the ones the owner deleted, for a batch about to be
/// archived. Borrows the batch unchanged when nothing in it is tombstoned,
/// which on a server where nothing was ever deleted costs one small query.
pub async fn without_tombstoned<'a>(
    pool: &PgPool,
    provider: &str,
    stream: &str,
    records: &'a [Value],
) -> Result<Cow<'a, [Value]>> {
    let any: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM app_tombstones)").fetch_one(pool).await?;
    if !any {
        return Ok(Cow::Borrowed(records));
    }
    let mut candidates = Vec::new();
    for record in records {
        candidates.extend(computed_id(provider, stream, record));
        own_ids(record, &mut candidates);
    }
    if candidates.is_empty() {
        return Ok(Cow::Borrowed(records));
    }
    let tombstoned = tombstoned_among(pool, &candidates).await?;
    if tombstoned.is_empty() {
        return Ok(Cow::Borrowed(records));
    }

    let (mut matched, mut removed) = (HashSet::new(), 0);
    let mut kept = Vec::with_capacity(records.len());
    for record in records {
        match scrub_record(record, computed_id(provider, stream, record), &tombstoned, &mut matched, &mut removed) {
            Scrubbed::Kept => kept.push(record.clone()),
            Scrubbed::Edited(rest) => kept.push(rest),
            Scrubbed::Dropped => {}
        }
    }
    if removed == 0 {
        return Ok(Cow::Borrowed(records));
    }
    tracing::info!(provider, stream, removed, "lake: left tombstoned records out of a raw file");
    Ok(Cow::Owned(kept))
}

/// What a pass over the raw files removed.
#[derive(Debug, Default)]
pub struct RawRemoved {
    /// Raw records taken out: whole lines, and elements inside a line.
    pub records: u64,
    /// Raw files rewritten or deleted.
    pub files: u64,
    /// The tombstoned ids that had a raw copy.
    pub matched: HashSet<String>,
}

/// Take every raw copy of the `tombstoned` records out of the lake's files.
///
/// `providers` narrows the pass to those providers' files, for a deletion
/// that knows which sources its rows came from; `None` reads every raw file.
/// A file is rewritten under the name of its new content, its `lake_objects`
/// row follows, and then the old file goes, so a pass that stops halfway
/// leaves each file either as it was or already rewritten.
pub async fn scrub(
    pool: &PgPool,
    storage: &Storage,
    tombstoned: &Tombstoned,
    providers: Option<&[String]>,
) -> Result<RawRemoved> {
    let mut out = RawRemoved::default();
    if tombstoned.is_empty() {
        return Ok(out);
    }
    let objects: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, storage_key, provider, stream_name FROM lake_objects \
         WHERE kind = 'raw_stream' AND ($1::text[] IS NULL OR provider = ANY($1)) \
         ORDER BY created_at",
    )
    .bind(providers)
    .fetch_all(pool)
    .await?;

    for (id, key, provider, stream) in objects {
        let bytes = match storage.download(&key).await {
            Ok(bytes) => bytes,
            // The row outlived its file. There is nothing here to remove.
            Err(Error::NotFound(_)) => continue,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            tracing::warn!(key, "raw file is not UTF-8; left as it is");
            continue;
        };

        let mut removed = 0;
        let mut lines: Vec<Cow<'_, str>> = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            // A line that does not parse is not a record this can judge.
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                lines.push(Cow::Borrowed(line));
                continue;
            };
            let computed = computed_id_of_line(&provider, &stream, line, &record);
            match scrub_record(&record, computed, tombstoned, &mut out.matched, &mut removed) {
                // The line's own bytes, so an id-less iOS record still hashes
                // to the id it was written under.
                Scrubbed::Kept => lines.push(Cow::Borrowed(line)),
                Scrubbed::Edited(rest) => lines.push(Cow::Owned(
                    serde_json::to_string(&rest).map_err(|e| Error::Other(format!("raw file {key}: {e}")))?,
                )),
                Scrubbed::Dropped => {}
            }
        }
        if removed == 0 {
            continue;
        }
        out.records += removed;
        out.files += 1;

        if lines.is_empty() {
            sqlx::query("DELETE FROM lake_objects WHERE id = $1").bind(&id).execute(pool).await?;
            delete_file(storage, &key).await?;
            continue;
        }
        let mut rewritten = Vec::with_capacity(bytes.len());
        for line in &lines {
            rewritten.extend_from_slice(line.as_bytes());
            rewritten.push(b'\n');
        }
        let sha256 = format!("{:x}", Sha256::digest(&rewritten));
        // Content-addressed, as `lake::archive` names it: the same folder, the
        // new digest.
        let folder = key.rsplit_once('/').map_or("", |(folder, _)| folder);
        let new_key = format!("{folder}/records_{}.jsonl", &sha256[..16]);

        // Another raw file may already hold exactly what is left of this one.
        let twin: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM lake_objects WHERE kind = 'raw_stream' AND sha256 = $1 AND id <> $2)",
        )
        .bind(&sha256)
        .bind(&id)
        .fetch_one(pool)
        .await?;
        if twin {
            sqlx::query("DELETE FROM lake_objects WHERE id = $1").bind(&id).execute(pool).await?;
            delete_file(storage, &key).await?;
            continue;
        }

        storage.upload(&new_key, rewritten.clone()).await?;
        sqlx::query(
            "UPDATE lake_objects SET storage_key = $2, sha256 = $3, record_count = $4, size_bytes = $5 WHERE id = $1",
        )
        .bind(&id)
        .bind(&new_key)
        .bind(&sha256)
        .bind(lines.len() as i32)
        .bind(rewritten.len() as i64)
        .execute(pool)
        .await?;
        if new_key != key {
            delete_file(storage, &key).await?;
        }
    }
    Ok(out)
}

/// Delete a raw file. One that is already gone is not an error.
async fn delete_file(storage: &Storage, key: &str) -> Result<()> {
    match storage.delete(key).await {
        Ok(()) | Err(Error::NotFound(_)) => Ok(()),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tombstoned(ids: &[&str]) -> Tombstoned {
        Tombstoned::new(ids.iter().map(|s| s.to_string()))
    }

    /// Scrub one record and return what is left, how many records went and
    /// which tombstones matched.
    fn scrubbed(record: Value, tombstoned: &Tombstoned) -> (Option<Value>, u64, Vec<String>) {
        let (mut matched, mut removed) = (HashSet::new(), 0);
        let left = match scrub_record(&record, None, tombstoned, &mut matched, &mut removed) {
            Scrubbed::Kept => Some(record),
            Scrubbed::Edited(rest) => Some(rest),
            Scrubbed::Dropped => None,
        };
        let mut matched: Vec<String> = matched.into_iter().collect();
        matched.sort();
        (left, removed, matched)
    }

    #[test]
    fn a_record_goes_when_its_own_id_is_tombstoned() {
        // An iMessage line: the row's source id is the guid itself.
        let t = tombstoned(&["guid-0001"]);
        let (left, removed, matched) = scrubbed(json!({"guid": "guid-0001", "text": "An invented line."}), &t);
        assert_eq!((left, removed, matched), (None, 1, vec!["guid-0001".to_string()]));

        // A Strava activity: the id is a number on the wire and its digits in the row.
        let t = tombstoned(&["1234567890"]);
        assert_eq!(scrubbed(json!({"id": 1234567890u64, "name": "Morning run"}), &t).0, None);

        // A GitHub star: the row's id ends with the repo's node id, one level down.
        let t = tombstoned(&["github:star:R_kgDOexample"]);
        let star = json!({"starred_at": "2026-03-03T10:00:00Z", "repo": {"node_id": "R_kgDOexample", "name": "example"}});
        assert_eq!(scrubbed(star, &t).0, None);
    }

    #[test]
    fn a_page_loses_only_the_record_that_was_deleted() {
        // A Plaid page: two transactions, one tombstoned.
        let t = tombstoned(&["txn_aaaaaaaa"]);
        let page = json!({
            "added": [
                {"transaction_id": "txn_aaaaaaaa", "account_id": "acct_11111111", "amount": 12.5},
                {"transaction_id": "txn_bbbbbbbb", "account_id": "acct_11111111", "amount": 3.0},
            ],
            "next_cursor": "c1",
        });
        let (left, removed, _) = scrubbed(page, &t);
        assert_eq!(removed, 1);
        assert_eq!(
            left.unwrap(),
            json!({"added": [{"transaction_id": "txn_bbbbbbbb", "account_id": "acct_11111111", "amount": 3.0}], "next_cursor": "c1"})
        );

        // A calendar page: the row's id is `<calendar id>:<event id>`.
        let t = tombstoned(&["primary@example.com:evt_12345678"]);
        let page = json!({"items": [{"id": "evt_12345678", "summary": "Dentist"}, {"id": "evt_87654321", "summary": "Lunch"}]});
        assert_eq!(scrubbed(page, &t).0.unwrap(), json!({"items": [{"id": "evt_87654321", "summary": "Lunch"}]}));
    }

    #[test]
    fn a_reference_to_a_deleted_record_does_not_remove_the_record_that_makes_it() {
        // The account is tombstoned; a transaction only refers to it.
        let t = tombstoned(&["plaid:account:acct_11111111", "guid-0001"]);
        let page = json!({"added": [{"transaction_id": "txn_bbbbbbbb", "account_id": "acct_11111111"}]});
        assert_eq!(scrubbed(page.clone(), &t), (Some(page), 0, vec![]));
        // A reply to a deleted message is its own message.
        let reply = json!({"guid": "guid-0002", "reply_to_guid": "guid-0001", "associated_message_guid": "guid-0001"});
        assert_eq!(scrubbed(reply.clone(), &t), (Some(reply), 0, vec![]));
        // An attachment's record is about that one message, and goes with it.
        assert_eq!(scrubbed(json!({"message_guid": "guid-0001", "attachment_guid": "at_0_x"}), &t).0, None);
    }

    #[test]
    fn a_short_id_is_not_matched_by_its_ending() {
        // "42" must not match a tombstone that merely ends in ":42"…
        let t = tombstoned(&["calendar:42", "7"]);
        let record = json!({"id": "42"});
        assert_eq!(scrubbed(record.clone(), &t).0, Some(record));
        // …though an id that is exactly a tombstone always matches.
        assert_eq!(scrubbed(json!({"id": "7"}), &t).0, None);
        // And an ending must start at a colon.
        let t = tombstoned(&["x:bookmark:19000000000000001"]);
        let record = json!({"rest_id": "9000000000000001"});
        assert_eq!(scrubbed(record.clone(), &t).0, Some(record));
        assert_eq!(scrubbed(json!({"rest_id": "19000000000000001"}), &t).0, None);
    }

    #[test]
    fn a_hashed_ios_record_is_found_from_its_archived_line() {
        // What the transform writes for an id-less location record…
        let record = json!({"latitude": 30.267153000000002, "longitude": -97.74306079999999, "timestamp": "2026-03-03T10:00:00Z"});
        let written = virtues_helpers::ios::stream_id_or_hash(&record, virtues_helpers::ios::LOCATION_STREAM_TABLE);
        // …is what its line in the lake computes to, and nothing else is.
        let line = serde_json::to_string(&record).unwrap();
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(computed_id_of_line("ios", "location", &line, &parsed), Some(written.clone()));
        assert_eq!(computed_id("ios", "location", &record), Some(written));
        assert_eq!(computed_id("ios", "financekit", &record), None);
        assert_eq!(computed_id("plaid", "transactions", &record), None);
    }

    fn temp_storage(name: &str) -> (Storage, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("virtues-raw-copies-{name}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        (Storage::file(dir.to_string_lossy().into_owned()).unwrap(), dir)
    }

    /// Remove a test's folder. One that will not go is left for the system's
    /// own cleanup of its temp directory, and said so.
    fn remove(dir: std::path::PathBuf) {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            eprintln!("left {}: {e}", dir.display());
        }
    }

    async fn tombstone(pool: &PgPool, table: &str, stream_id: &str) {
        sqlx::query("INSERT INTO app_tombstones (source_table, source_stream_id, source_id) VALUES ($1, $2, $2)")
            .bind(table)
            .bind(stream_id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// A raw file on disk loses the deleted record, keeps its neighbours'
    /// bytes exactly, and its `lake_objects` row follows the new content.
    #[sqlx::test]
    async fn scrub_rewrites_a_raw_file_and_its_row(pool: PgPool) {
        use super::super::lake::{archive, Envelope};
        let (storage, dir) = temp_storage("scrub");
        let records = vec![
            json!({"guid": "guid-0001", "text": "An invented line.", "timestamp": "2026-03-03T10:00:00Z"}),
            json!({"guid": "guid-0002", "text": "Another invented line.", "timestamp": "2026-03-03T10:01:00Z"}),
        ];
        let landed = archive(&pool, &storage, "mac", "mac", Envelope::MacKey("imessages"), &records, json!({}))
            .await
            .unwrap()
            .expect("archived");

        let removed = scrub(&pool, &storage, &tombstoned(&["guid-0001"]), Some(&["mac".to_string()])).await.unwrap();
        assert_eq!((removed.records, removed.files), (1, 1));
        assert!(removed.matched.contains("guid-0001"));

        let (key, count, size): (String, i32, i64) =
            sqlx::query_as("SELECT storage_key, record_count, size_bytes FROM lake_objects WHERE id = $1")
                .bind(&landed.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(key, landed.storage_key, "the file is named for its new content");
        let left = storage.download(&key).await.unwrap();
        assert_eq!(String::from_utf8(left.clone()).unwrap(), format!("{}\n", serde_json::to_string(&records[1]).unwrap()));
        assert_eq!((count, size), (1, left.len() as i64));
        assert!(storage.download(&landed.storage_key).await.is_err(), "the old file is gone");

        // The last record too: the file and its row both go.
        let removed = scrub(&pool, &storage, &tombstoned(&["guid-0002"]), None).await.unwrap();
        assert_eq!(removed.records, 1);
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM lake_objects").fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 0);
        assert!(storage.download(&key).await.is_err());
        remove(dir);
    }

    /// A source that sends a deleted record again does not put it back on
    /// disk: the archive leaves it out, and a batch of nothing else writes no
    /// file at all.
    #[sqlx::test]
    async fn a_re_sent_record_does_not_land_in_a_new_raw_file(pool: PgPool) {
        use super::super::lake::{archive, Envelope};
        let (storage, dir) = temp_storage("resend");
        let deleted = json!({"latitude": 30.267153000000002, "longitude": -97.74306079999999, "timestamp": "2026-03-03T10:00:00Z"});
        let kept = json!({"latitude": 30.3, "longitude": -97.7, "timestamp": "2026-03-03T10:05:00Z"});
        let id = virtues_helpers::ios::stream_id_or_hash(&deleted, virtues_helpers::ios::LOCATION_STREAM_TABLE);
        tombstone(&pool, "data_location_point", &id).await;

        let landed = archive(&pool, &storage, "ios", "ios", Envelope::IosStream("location"), &[deleted.clone(), kept.clone()], json!({}))
            .await
            .unwrap()
            .expect("the record that was not deleted is archived");
        assert_eq!(landed.record_count, 1);
        let text = String::from_utf8(storage.download(&landed.storage_key).await.unwrap()).unwrap();
        assert_eq!(text, format!("{}\n", serde_json::to_string(&kept).unwrap()));

        let nothing = archive(&pool, &storage, "ios", "ios", Envelope::IosStream("location"), &[deleted], json!({}))
            .await
            .unwrap();
        assert!(nothing.is_none());
        remove(dir);
    }
}
