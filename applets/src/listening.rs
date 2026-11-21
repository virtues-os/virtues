//! Spotify plays in `data_activity_listening`, shared by the two applets
//! that write them: `spotify_import` (a data download) and
//! `spotify_recent_sync` (the recently-played API).
//!
//! Both key a play on its stop time to the second and its Spotify URI, so a
//! play the sync added lands on the same row when a download later covers
//! it. A download knows more than the sync does: how long the play actually
//! ran, skips, shuffle, and podcasts. So an import replaces a row the sync
//! wrote, and neither applet ever changes a row an import wrote.

use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use sqlx::PgPool;
use virtues_helpers::dedup::build_batch_insert_query;

use crate::import::{content_key, row_id};

/// `source_table` for rows from a Spotify data download.
pub const FROM_IMPORT: &str = "spotify_import";
/// `source_table` for rows from the recently-played API.
pub const FROM_RECENT: &str = "spotify_recent";

/// One play, shaped for `data_activity_listening`.
#[derive(Debug, Clone)]
pub struct Play {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub played_ms: i64,
    pub kind: &'static str,
    pub title: Option<String>,
    pub artist_name: Option<String>,
    pub album_name: Option<String>,
    pub uri: Option<String>,
    pub is_skipped: Option<bool>,
    pub is_shuffle: Option<bool>,
    pub reason_start: Option<String>,
    pub reason_end: Option<String>,
    pub source_stream_id: String,
    pub metadata: Value,
}

/// `spotify:{stop time}:{uri}`, or a key from the play's text when there is
/// no URI (a local file, or the account data download). The stop time is cut
/// to the second because the extended history records whole seconds and the
/// recently-played API records milliseconds.
pub fn stream_id(ended_at: DateTime<Utc>, uri: Option<&str>, text: &[&str]) -> String {
    let at = ended_at.to_rfc3339_opts(SecondsFormat::Secs, true);
    match uri {
        Some(u) => format!("spotify:{at}:{u}"),
        None => format!("spotify:{at}:{}", content_key(text)),
    }
}

/// Which applet is writing, which decides what happens when a play's key is
/// already here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// A data download. Replaces a row the recently-played sync wrote, and
    /// leaves its own earlier rows alone.
    Import,
    /// The recently-played sync. Never changes a row that is already here.
    Recent,
}

impl Writer {
    fn source_table(self) -> &'static str {
        match self {
            Writer::Import => FROM_IMPORT,
            Writer::Recent => FROM_RECENT,
        }
    }
}

const COLUMNS: [&str; 17] = [
    "id",
    "started_at",
    "ended_at",
    "played_ms",
    "kind",
    "title",
    "artist_name",
    "album_name",
    "uri",
    "is_skipped",
    "is_shuffle",
    "reason_start",
    "reason_end",
    "source_stream_id",
    "source_table",
    "source_provider",
    "metadata",
];

/// The columns an import's values replace on a row the sync wrote. `id`
/// and `source_stream_id` are the same on both by design, and `created_at`,
/// `is_archived` and `deleted_at_source` are not the download's to set.
const REPLACED: [&str; 15] = [
    "started_at",
    "ended_at",
    "played_ms",
    "kind",
    "title",
    "artist_name",
    "album_name",
    "uri",
    "is_skipped",
    "is_shuffle",
    "reason_start",
    "reason_end",
    "source_table",
    "source_provider",
    "metadata",
];

fn insert_sql(writer: Writer, rows: usize) -> String {
    let sql = build_batch_insert_query("data_activity_listening", &COLUMNS, "source_stream_id", rows);
    let sql = match writer {
        Writer::Recent => sql,
        Writer::Import => {
            let head = sql
                .strip_suffix(" ON CONFLICT (source_stream_id) DO NOTHING")
                .expect("build_batch_insert_query ends with its ON CONFLICT clause");
            let set: Vec<String> = REPLACED.iter().map(|c| format!("{c} = EXCLUDED.{c}")).collect();
            format!(
                "{head} ON CONFLICT (source_stream_id) DO UPDATE SET {}, updated_at = now() \
                 WHERE data_activity_listening.source_table = '{FROM_RECENT}'",
                set.join(", ")
            )
        }
    };
    format!("{sql} RETURNING ended_at")
}

/// Write plays and return the stop time of each row inserted or replaced,
/// so a caller can count them or name the span they cover.
///
/// A batch holds each key once. Postgres refuses an `ON CONFLICT DO UPDATE`
/// that meets one key twice in a single statement, and a download can list
/// the same play twice; the first copy is kept, as `DO NOTHING` would.
pub async fn write_plays(pool: &PgPool, plays: &[Play], writer: Writer) -> Result<Vec<DateTime<Utc>>> {
    let mut seen = HashSet::with_capacity(plays.len());
    let plays: Vec<&Play> = plays.iter().filter(|p| seen.insert(p.source_stream_id.as_str())).collect();
    if plays.is_empty() {
        return Ok(Vec::new());
    }
    let sql = insert_sql(writer, plays.len());
    let mut q = sqlx::query_scalar::<_, DateTime<Utc>>(&sql);
    for p in plays {
        q = q
            .bind(row_id(&p.source_stream_id))
            .bind(p.started_at)
            .bind(p.ended_at)
            .bind(p.played_ms)
            .bind(p.kind)
            .bind(&p.title)
            .bind(&p.artist_name)
            .bind(&p.album_name)
            .bind(&p.uri)
            .bind(p.is_skipped)
            .bind(p.is_shuffle)
            .bind(&p.reason_start)
            .bind(&p.reason_end)
            .bind(&p.source_stream_id)
            .bind(writer.source_table())
            .bind("spotify")
            .bind(&p.metadata);
    }
    Ok(q.fetch_all(pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_cut_to_the_second() {
        let whole: DateTime<Utc> = "2024-03-05T08:15:30Z".parse().unwrap();
        let with_ms: DateTime<Utc> = "2024-03-05T08:15:30.874Z".parse().unwrap();
        let uri = Some("spotify:track:0000000000000000000001");
        assert_eq!(stream_id(whole, uri, &[]), "spotify:2024-03-05T08:15:30Z:spotify:track:0000000000000000000001");
        assert_eq!(stream_id(whole, uri, &[]), stream_id(with_ms, uri, &[]));
    }

    #[test]
    fn only_an_import_replaces_and_only_a_recent_row() {
        let recent = insert_sql(Writer::Recent, 1);
        assert!(recent.ends_with("ON CONFLICT (source_stream_id) DO NOTHING RETURNING ended_at"), "{recent}");
        let import = insert_sql(Writer::Import, 2);
        assert!(import.contains("DO UPDATE SET started_at = EXCLUDED.started_at"), "{import}");
        assert!(import.contains("metadata = EXCLUDED.metadata, updated_at = now()"), "{import}");
        assert!(import.ends_with("WHERE data_activity_listening.source_table = 'spotify_recent' RETURNING ended_at"), "{import}");
        assert!(!import.contains("id = EXCLUDED.id") && !import.contains("created_at"), "{import}");
    }
}
