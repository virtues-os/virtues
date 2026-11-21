//! Spotify import: one-time ingest of a Spotify data download into
//! `data_activity_listening`.
//!
//! Manual-trigger only (no cron). `POST /api/imports/spotify/upload` stages
//! the upload and starts this applet with `payload = { "file_path": "..." }`.
//!
//! The file is the zip Spotify emails, or one JSON file from inside it. Both
//! downloads are read: the extended streaming history and the account data
//! download. When one upload holds both, only the extended history is read,
//! because it covers the same plays with seconds and URIs and the two cannot
//! be matched play for play. Each JSON file is read one play at a time on a
//! blocking thread and written in batches, so memory stays flat.
//!
//! Rows dedup on `source_stream_id`, so importing a newer download later adds
//! only the new plays. A play `spotify_recent_sync` already added has the
//! same key, and the download's values replace the sync's estimate
//! (`virtues_applets::listening`).

mod transform;

use std::io::BufReader;
use std::path::Path;

use anyhow::{bail, Context, Result};
use sqlx::PgPool;
use transform::{is_account_file, is_extended_file, parse_play};
use virtues_applets::import::{self, base_name, Batches, Tally, Upload};
use virtues_applets::listening::{self, Play, Writer};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const APPLET: &str = "spotify_import";

const NOT_A_DOWNLOAD: &str = "This file has no Spotify listening history in it. Use the zip Spotify emails you \
     after you request your data, or a StreamingHistory JSON file from inside it.";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let path = import::staged_file(&input, APPLET)?;
    let result = run(&path).await;
    import::remove_upload(&path, APPLET);
    let tally = result?;
    if tally.parsed == 0 {
        bail!("{NOT_A_DOWNLOAD}");
    }
    output_with_records(&tally.summary("Spotify listening", "was"), &input.config, tally.written as i64)?;
    Ok(())
}

async fn run(path: &Path) -> Result<Tally> {
    let pool = connect_from_env("virtues-action-spotify_import").await?;
    let (mut rx, reader) = import::spawn_reader(path, read_download);
    let mut tally = Tally::default();
    while let Some(batch) = rx.recv().await {
        for p in &batch {
            tally.saw(p.ended_at);
        }
        tally.written += write_plays(&pool, &batch).await?;
    }
    import::join_reader(reader).await?;
    Ok(tally)
}

/// Read every play in the upload, a zip or a bare JSON file, handing them to
/// `sink` in batches.
fn read_download(path: &Path, sink: &mut dyn FnMut(Vec<Play>) -> Result<()>) -> Result<()> {
    let mut upload = Upload::open(path)?;
    let mut batches = Batches::new(sink);
    if let Upload::Bare(file) = &mut upload {
        let mut reader = BufReader::new(file);
        if !import::starts_with_array(&mut reader)? {
            bail!("{NOT_A_DOWNLOAD}");
        }
        return read_plays(reader, &mut batches).context(
            "This file stops partway through. Download it again and retry; the plays before the break were kept.",
        );
    }

    let mut names = upload.names(|n| is_extended_file(base_name(n)));
    if names.is_empty() {
        names = upload.names(|n| is_account_file(base_name(n)));
    }
    if names.is_empty() {
        bail!("{NOT_A_DOWNLOAD}");
    }
    for name in &names {
        // `names` came from this zip's own listing, so every entry is there.
        upload
            .with_entry(name, |entry| {
                let mut reader = BufReader::new(entry);
                if !import::starts_with_array(&mut reader)? {
                    tracing::info!("{APPLET}: {name} is not a list of plays; skipping it");
                    return Ok(());
                }
                read_plays(reader, &mut batches)
            })
            .context("Part of this zip is damaged. Download it again and retry; the plays before the damage were kept.")?;
    }
    batches.flush()
}

/// Parse each element of one JSON array of plays. What came before a bad
/// element is handed on before the error returns.
fn read_plays<R: std::io::Read>(reader: R, batches: &mut Batches<'_, Play>) -> Result<()> {
    let walked = import::for_each_element(reader, &mut |element| match parse_play(&element) {
        Some(play) => batches.push(play),
        None => Ok(()),
    });
    batches.flush()?;
    walked
}

/// Write plays. A play already here from this import or an earlier one is
/// left alone; one the recently-played sync added is replaced with the
/// download's values. Returns how many rows were added or replaced.
async fn write_plays(pool: &PgPool, plays: &[Play]) -> Result<u64> {
    Ok(listening::write_plays(pool, plays, Writer::Import).await?.len() as u64)
}

// The recently-played sync's shaping, so the tests below can show that the
// two applets meet on one row.
#[cfg(test)]
#[path = "../spotify_recent_sync/transform.rs"]
mod recent_transform;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("spotify_import_test_{unique}_{name}"))
    }

    fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    fn collect(path: &Path) -> Result<Vec<Play>> {
        let mut out = Vec::new();
        read_download(path, &mut |b| {
            out.extend(b);
            Ok(())
        })?;
        Ok(out)
    }

    fn extended() -> String {
        json!([
            {"ts": "2024-03-05T08:15:30Z", "ms_played": 183000, "master_metadata_track_name": "Paper Lanterns",
             "master_metadata_album_artist_name": "The Example Band", "spotify_track_uri": "spotify:track:0000000000000000000001"},
            {"ts": "2024-03-05T08:20:00Z", "ms_played": 0}
        ])
        .to_string()
    }

    fn account() -> String {
        json!([{"endTime": "2022-07-01 21:04", "artistName": "The Example Band", "trackName": "Paper Lanterns", "msPlayed": 60000}])
            .to_string()
    }

    #[test]
    fn a_zip_with_both_downloads_reads_only_the_extended_history() {
        let path = temp_path("both.zip");
        let ext = extended();
        let acc = account();
        zip_with(
            &path,
            &[
                ("Spotify Extended Streaming History/Streaming_History_Audio_2024.json", ext.as_bytes()),
                ("Spotify Account Data/StreamingHistory_music_0.json", acc.as_bytes()),
                ("Spotify Extended Streaming History/ReadMeFirst.pdf", b"%PDF"),
            ],
        );
        let plays = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(plays.len(), 1);
        assert!(plays[0].uri.is_some());
    }

    #[test]
    fn a_zip_of_account_data_alone_is_read() {
        let path = temp_path("account.zip");
        let acc = account();
        zip_with(&path, &[("Spotify Account Data/StreamingHistory_music_0.json", acc.as_bytes())]);
        let plays = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].uri, None);
    }

    #[test]
    fn a_bare_json_file_is_read_and_anything_else_refused() {
        let path = temp_path("bare.upload");
        std::fs::write(&path, extended()).unwrap();
        assert_eq!(collect(&path).unwrap().len(), 1);
        std::fs::write(&path, "{\"not\": \"a list\"}").unwrap();
        let err = collect(&path).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("no Spotify listening history"), "{err}");
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_second_import_of_the_same_plays_adds_nothing(pool: PgPool) {
        let plays: Vec<Play> = serde_json::from_str::<serde_json::Value>(&extended())
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .chain(serde_json::from_str::<serde_json::Value>(&account()).unwrap().as_array().unwrap())
            .filter_map(parse_play)
            .collect();
        assert_eq!(plays.len(), 2);
        assert_eq!(write_plays(&pool, &plays).await.unwrap(), 2);
        assert_eq!(write_plays(&pool, &plays).await.unwrap(), 0);
        let (n, ip): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE metadata::text LIKE '%ip_addr%') FROM data_activity_listening",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((n, ip), (2, 0));
    }

    /// The extended history's play of "Paper Lanterns" and the same play
    /// from the recently-played list, which stamps it with milliseconds.
    fn the_same_play_twice() -> (Play, Play) {
        let download = json!({
            "ts": "2026-03-05T22:15:30Z", "ms_played": 120_000, "platform": "ios",
            "master_metadata_track_name": "Paper Lanterns", "master_metadata_album_artist_name": "The Example Band",
            "master_metadata_album_album_name": "Night Market", "spotify_track_uri": "spotify:track:0000000000000000000001",
            "reason_start": "clickrow", "reason_end": "fwdbtn", "shuffle": false, "skipped": true
        });
        let recent = json!({
            "played_at": "2026-03-05T22:15:30.874Z",
            "context": { "uri": "spotify:playlist:0000000000000000000009" },
            "track": { "uri": "spotify:track:0000000000000000000001", "name": "Paper Lanterns", "duration_ms": 183_000,
                       "artists": [{ "name": "The Example Band" }], "album": { "name": "Night Market" } }
        });
        (parse_play(&download).unwrap(), recent_transform::shape(&recent).unwrap())
    }

    #[test]
    fn a_download_and_the_recent_list_key_one_play_alike() {
        let (download, recent) = the_same_play_twice();
        assert_eq!(download.source_stream_id, recent.source_stream_id);
        assert_eq!(import::row_id(&download.source_stream_id), import::row_id(&recent.source_stream_id));

        let mut a_second_later = recent.clone();
        a_second_later.ended_at += chrono::Duration::seconds(1);
        let key = listening::stream_id(a_second_later.ended_at, a_second_later.uri.as_deref(), &[]);
        assert_ne!(key, download.source_stream_id, "the times must agree to the second");
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_download_replaces_the_recent_estimate_and_nothing_replaces_a_download(pool: PgPool) {
        let (download, recent) = the_same_play_twice();
        let row = |pool: PgPool| async move {
            let r: (String, i64, Option<bool>, Option<String>, serde_json::Value, i64) = sqlx::query_as(
                "SELECT source_table, played_ms, is_skipped, reason_end, metadata, count(*) OVER () \
                 FROM data_activity_listening",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            r
        };

        // (a) The sync adds the play once; a second run adds nothing.
        assert_eq!(listening::write_plays(&pool, &[recent.clone()], Writer::Recent).await.unwrap().len(), 1);
        assert!(listening::write_plays(&pool, &[recent.clone()], Writer::Recent).await.unwrap().is_empty());
        let (table, ms, skipped, _, meta, n) = row(pool.clone()).await;
        assert_eq!((table.as_str(), ms, skipped, n), ("spotify_recent", 183_000, None, 1));
        assert_eq!(meta["played_ms_estimated"], true);

        // (b) A download over the sync's row replaces it, and counts as written.
        assert_eq!(write_plays(&pool, &[download.clone(), download.clone()]).await.unwrap(), 1);
        let (table, ms, skipped, reason_end, meta, n) = row(pool.clone()).await;
        assert_eq!((table.as_str(), ms, skipped, reason_end.as_deref(), n), ("spotify_import", 120_000, Some(true), Some("fwdbtn"), 1));
        assert_eq!(meta, download.metadata, "metadata is replaced, not merged");
        let (moved,): (bool,) = sqlx::query_as("SELECT updated_at > created_at FROM data_activity_listening")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(moved);

        // (c) Neither the sync nor a second download changes the download's row.
        assert!(listening::write_plays(&pool, &[recent], Writer::Recent).await.unwrap().is_empty());
        assert_eq!(write_plays(&pool, &[download]).await.unwrap(), 0);
        let (table, ms, ..) = row(pool.clone()).await;
        assert_eq!((table.as_str(), ms), ("spotify_import", 120_000));
    }
}
