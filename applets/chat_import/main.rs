//! Chat import: one-time ingest of a Claude or ChatGPT conversation export
//! into the AI-chat ontology.
//!
//! Manual-trigger only (no cron). The `/api/chat-import/upload` route streams
//! the upload into its staging folder and starts this action with
//! `payload = { "file_path": "..." }`. Each conversation's shape says which
//! site it came from.
//!
//! The file is either the zip the site emails or a conversations JSON file
//! from inside it. ChatGPT splits a large export into numbered files
//! (`conversations-000.json`, ...), and every one in the zip is read. The JSON
//! is read one conversation at a time on a blocking thread and written in
//! batches as it goes, so memory stays flat however large the export is, and
//! a file cut off partway still keeps what came before the cut.
//!
//! Rows dedup on `source_stream_id`, so importing a fresher export later only
//! adds the new messages (`ON CONFLICT DO NOTHING`).

mod transform;

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::de::{Deserializer as _, SeqAccess, Visitor};
use serde_json::Value;
use virtues_helpers::conversations::{self, ConversationMsg};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

/// Messages per write. Large enough that inserts stay efficient, small
/// enough that the parser never holds much in memory.
const BATCH: usize = 2_000;

/// The folders the upload route stages into (`server::api::imports`): under
/// the state root on a box, under the temp folder in a dev checkout.
const STAGING_FOLDERS: [&str; 2] = ["chat-import", "virtues-chat-import"];

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let payload = input.payload.clone().unwrap_or_default();
    let file_path = PathBuf::from(
        payload
            .get("file_path")
            .and_then(|v| v.as_str())
            .context("chat_import: payload.file_path is required")?,
    );
    // This action removes the file it reads, so it reads only a staged
    // upload, never a path someone typed into a manual run.
    if !is_staged_upload(&file_path) {
        bail!("chat_import reads only files the upload route staged, not {}", file_path.display());
    }

    let pool = connect_from_env("virtues-action-chat_import").await?;
    let result = import(&file_path, &pool).await;
    // The upload is input, not a kept file: remove it however the import ends.
    if let Err(e) = std::fs::remove_file(&file_path) {
        tracing::warn!("chat_import: could not remove the upload {}: {e}", file_path.display());
    }
    let tally = result?;
    if tally.parsed == 0 {
        bail!(
            "This file has no chats in it. Use the zip that Claude or ChatGPT emails you, \
             or the conversations.json inside it."
        );
    }
    output_with_records(&tally.summary(), &input.config, tally.written as i64)?;
    Ok(())
}

/// A file inside one of the upload route's staging folders, ending `.upload`,
/// and a regular file rather than a link.
fn is_staged_upload(path: &Path) -> bool {
    let in_staging = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .is_some_and(|n| STAGING_FOLDERS.contains(&n));
    let named = path.extension().is_some_and(|e| e == "upload");
    let regular = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file());
    in_staging && named && regular
}

/// What one import read and wrote, for its summary.
#[derive(Default)]
struct Tally {
    parsed: usize,
    written: usize,
    providers: BTreeSet<String>,
    first: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
}

impl Tally {
    fn add(&mut self, batch: &[ConversationMsg]) {
        self.parsed += batch.len();
        for m in batch {
            if !self.providers.contains(&m.provider) {
                self.providers.insert(m.provider.clone());
            }
            self.first = Some(self.first.map_or(m.timestamp, |t| t.min(m.timestamp)));
            self.last = Some(self.last.map_or(m.timestamp, |t| t.max(m.timestamp)));
        }
    }

    /// "Brought in your ChatGPT chats from March 2023 to November 2026." The
    /// span says what arrived; `records_processed` carries the count.
    fn summary(&self) -> String {
        let names: Vec<&str> = self.providers.iter().map(|p| display_name(p)).collect();
        let who = join_names(&names);
        let span = match (self.first, self.last) {
            (Some(a), Some(b)) if month(a) == month(b) => format!(" from {}", month(a)),
            (Some(a), Some(b)) => format!(" from {} to {}", month(a), month(b)),
            _ => String::new(),
        };
        if self.written == 0 {
            format!("Your {who} chats{span} were already here.")
        } else {
            format!("Brought in your {who} chats{span}.")
        }
    }
}

/// Parse on a blocking thread and write each batch as it arrives.
async fn import(path: &Path, pool: &sqlx::PgPool) -> Result<Tally> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<ConversationMsg>>(2);
    let owned = path.to_path_buf();
    let parser = tokio::task::spawn_blocking(move || {
        read_export(&owned, &mut |batch| {
            tx.blocking_send(batch)
                .map_err(|_| anyhow::anyhow!("the import stopped writing"))
        })
    });

    let mut tally = Tally::default();
    while let Some(batch) = rx.recv().await {
        tally.add(&batch);
        tally.written += conversations::write_messages(pool, &batch).await?;
    }
    parser.await.context("the export reader stopped unexpectedly")??;
    Ok(tally)
}

/// Read every message in the export at `path`, a zip or a bare JSON array,
/// handing them to `sink` in batches.
fn read_export(path: &Path, sink: &mut dyn FnMut(Vec<ConversationMsg>) -> Result<()>) -> Result<()> {
    let mut file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut magic = [0u8; 4];
    let is_zip = file.read(&mut magic)? == 4 && magic == *b"PK\x03\x04";
    file.seek(SeekFrom::Start(0))?;

    if !is_zip {
        let mut reader = BufReader::new(file);
        if !starts_with_array(&mut reader)? {
            bail!("This file is not a chat export. Use the zip that Claude or ChatGPT emails you.");
        }
        return stream_elements(reader, sink).context(
            "This file stops partway through. Download the export again and retry; \
             the chats before the break were kept.",
        );
    }

    let mut archive = zip::ZipArchive::new(file).context(
        "This zip could not be opened. It may not have finished downloading. \
         Download it again and retry.",
    )?;
    let names: Vec<String> = archive
        .file_names()
        .filter(|n| is_conversations_file(n))
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        bail!(
            "This zip has no conversations.json in it. Use the export zip that \
             Claude or ChatGPT emails you."
        );
    }
    let mut read_any = false;
    for name in names {
        let entry = archive
            .by_name(&name)
            .context("Part of this zip is damaged. Download the export again and retry.")?;
        let mut reader = BufReader::new(entry);
        // A split export carries an index file beside its numbered parts;
        // anything that is not a list of conversations is passed over.
        if !starts_with_array(&mut reader)? {
            tracing::info!("chat_import: {name} is not a list of conversations; skipping it");
            continue;
        }
        stream_elements(reader, sink).context(
            "Part of this zip is damaged. Download the export again and retry; \
             the chats before the damage were kept.",
        )?;
        read_any = true;
    }
    if !read_any {
        bail!(
            "This zip has no conversations in it. Use the export zip that \
             Claude or ChatGPT emails you."
        );
    }
    Ok(())
}

/// `conversations.json`, or a numbered part of it (`conversations-000.json`),
/// in any folder of the zip.
fn is_conversations_file(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.starts_with("conversations") && base.ends_with(".json")
}

/// Whether the next non-blank character is `[`, without consuming it.
fn starts_with_array<R: Read>(reader: &mut BufReader<R>) -> Result<bool> {
    loop {
        let buf = reader.fill_buf()?;
        let Some(&first) = buf.first() else {
            return Ok(false);
        };
        let blank = buf.iter().take_while(|b| b.is_ascii_whitespace()).count();
        if blank == 0 {
            return Ok(first == b'[');
        }
        reader.consume(blank);
    }
}

/// Walk a top-level JSON array one element at a time, parsing each into
/// messages and handing them on in batches, so the whole document never sits
/// in memory as one value.
fn stream_elements<R: Read>(
    reader: R,
    sink: &mut dyn FnMut(Vec<ConversationMsg>) -> Result<()>,
) -> Result<()> {
    struct Each<'a> {
        sink: &'a mut dyn FnMut(Vec<ConversationMsg>) -> Result<()>,
        failed: &'a mut Option<anyhow::Error>,
    }
    impl<'de> Visitor<'de> for Each<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list of conversations")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            let mut batch = Vec::new();
            let mut flush = |batch: &mut Vec<ConversationMsg>| -> Result<(), A::Error> {
                if batch.is_empty() {
                    return Ok(());
                }
                if let Err(e) = (self.sink)(std::mem::take(batch)) {
                    *self.failed = Some(e);
                    return Err(serde::de::Error::custom("the import stopped writing"));
                }
                Ok(())
            };
            loop {
                // A bad element ends the walk, but what came before it is
                // handed on first.
                match seq.next_element::<Value>() {
                    Ok(Some(element)) => {
                        batch.extend(transform::parse_element(&element));
                        if batch.len() >= BATCH {
                            flush(&mut batch)?;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        flush(&mut batch)?;
                        return Err(e);
                    }
                }
            }
            flush(&mut batch)
        }
    }
    let mut failed = None;
    let mut de = serde_json::Deserializer::from_reader(reader);
    let walked = de.deserialize_seq(Each { sink, failed: &mut failed });
    if let Some(e) = failed {
        return Err(e);
    }
    walked?;
    de.end()?;
    Ok(())
}

fn month(t: DateTime<Utc>) -> String {
    t.format("%B %Y").to_string()
}

fn display_name(provider: &str) -> &str {
    match provider {
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        other => other,
    }
}

fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn chatgpt_json() -> String {
        serde_json::json!([{
            "id": "c-1",
            "current_node": "u1",
            "mapping": { "u1": { "parent": null, "message": {
                "id": "u1", "author": { "role": "user" }, "create_time": 1_700_000_000,
                "content": { "content_type": "text", "parts": ["What should I cook tonight?"] } } } }
        }])
        .to_string()
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("chat_import_test_{unique}_{name}"))
    }

    fn collect(path: &Path) -> Result<Vec<ConversationMsg>> {
        let mut out = Vec::new();
        read_export(path, &mut |batch| {
            out.extend(batch);
            Ok(())
        })?;
        Ok(out)
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

    #[test]
    fn reads_a_bare_json_export() {
        let path = temp_path("bare.json");
        std::fs::write(&path, format!("\n  {}", chatgpt_json())).unwrap();
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, "chatgpt");
    }

    #[test]
    fn reads_conversations_json_inside_a_zip() {
        let path = temp_path("export.zip");
        let json = chatgpt_json();
        zip_with(&path, &[("chat.html", b"<html></html>"), ("export/conversations.json", json.as_bytes())]);
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "What should I cook tonight?");
    }

    #[test]
    fn a_split_export_reads_every_part_and_passes_over_its_index() {
        let path = temp_path("split.zip");
        let part = chatgpt_json();
        let other = part.replace("c-1", "c-2");
        zip_with(
            &path,
            &[
                ("conversations-index.json", br#"{"parts":["conversations-000.json"]}"#),
                ("conversations-000.json", part.as_bytes()),
                ("conversations-001.json", other.as_bytes()),
            ],
        );
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_zip_without_conversations_fails_with_a_reason() {
        let path = temp_path("other.zip");
        zip_with(&path, &[("notes.txt", b"hello")]);
        let err = collect(&path).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("no conversations.json"), "{err}");
    }

    #[test]
    fn a_file_cut_off_partway_keeps_what_came_before() {
        let path = temp_path("cut.json");
        let two = serde_json::json!([
            serde_json::from_str::<Value>(&chatgpt_json()).unwrap()[0].clone(),
            serde_json::from_str::<Value>(&chatgpt_json().replace("c-1", "c-2")).unwrap()[0].clone()
        ])
        .to_string();
        let cut = &two[..two.len() - 40];
        std::fs::write(&path, cut).unwrap();
        let mut kept = Vec::new();
        let err = read_export(&path, &mut |batch| {
            kept.extend(batch);
            Ok(())
        })
        .unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(kept.len(), 1, "{err:#}");
        assert!(err.to_string().contains("stops partway"), "{err}");
    }

    #[test]
    fn only_a_staged_upload_is_read() {
        let dir = std::env::temp_dir().join(format!("virtues-chat-import-test-{}", std::process::id()));
        let staging = dir.join("virtues-chat-import");
        std::fs::create_dir_all(&staging).unwrap();
        let staged = staging.join("1.upload");
        std::fs::write(&staged, "[]").unwrap();
        let elsewhere = dir.join("notes.upload");
        std::fs::write(&elsewhere, "[]").unwrap();
        assert!(is_staged_upload(&staged));
        assert!(!is_staged_upload(&elsewhere));
        assert!(!is_staged_upload(&staging.join("missing.upload")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn summary_names_the_sites_and_the_span() {
        let mut tally = Tally::default();
        let mut rows = Vec::new();
        stream_elements(chatgpt_json().as_bytes(), &mut |b| {
            rows.extend(b);
            Ok(())
        })
        .unwrap();
        tally.add(&rows);
        tally.written = 1;
        assert_eq!(tally.summary(), "Brought in your ChatGPT chats from November 2023.");
        let mut later = rows.clone();
        later[0].provider = "claude".into();
        later[0].timestamp += chrono::Duration::days(400);
        tally.add(&later);
        tally.written = 0;
        assert_eq!(
            tally.summary(),
            "Your ChatGPT and Claude chats from November 2023 to December 2024 were already here."
        );
    }
}
