//! Chat import: one-time ingest of a Claude / ChatGPT / Gemini conversation
//! export into the AI-chat ontology.
//!
//! Manual-trigger only (no cron). The `/api/chat-import/upload` route streams
//! the upload to a local path and starts this action with
//! `payload = { "file_path": "...", "provider": "..." }`. `provider` is
//! optional: each conversation's shape says which site it came from.
//!
//! The file is either the zip the site emails (Claude and ChatGPT both send
//! one, with the chats in `conversations.json`) or that JSON file on its own.
//! The JSON is read one conversation at a time, so memory follows the text
//! kept rather than the size of the export, which can run to gigabytes.
//!
//! Rows dedup on `source_stream_id`, so importing a fresher export later only
//! adds the new messages (`ON CONFLICT DO NOTHING`).

mod transform;

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::de::{Deserializer as _, SeqAccess, Visitor};
use serde_json::Value;
use virtues_helpers::conversations::{self, ConversationMsg};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let payload = input.payload.clone().unwrap_or_default();
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .context("chat_import: payload.file_path is required")?
        .to_string();
    let hint = payload
        .get("provider")
        .and_then(|v| v.as_str())
        .filter(|p| matches!(*p, "claude" | "chatgpt" | "gemini"))
        .map(str::to_string);

    // The upload is input, not a kept file: remove it however parsing ends.
    let parsed = read_export(Path::new(&file_path), hint.as_deref());
    if let Err(e) = std::fs::remove_file(&file_path) {
        tracing::warn!("chat_import: could not remove the upload {file_path}: {e}");
    }
    let messages = parsed?;
    if messages.is_empty() {
        bail!(
            "This file has no chats in it. Use the zip that Claude or ChatGPT \
             emails you, or the conversations.json inside it."
        );
    }

    let pool = connect_from_env("virtues-action-chat_import").await?;
    let written = conversations::write_messages(&pool, &messages).await?;

    output_with_records(&summary(&messages, written), &input.config, written as i64)?;
    Ok(())
}

/// Read every message in the export at `path`, a zip or a bare JSON array.
fn read_export(path: &Path, hint: Option<&str>) -> Result<Vec<ConversationMsg>> {
    let mut file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut magic = [0u8; 4];
    let is_zip = file.read(&mut magic)? == 4 && magic == *b"PK\x03\x04";
    file.seek(SeekFrom::Start(0))?;

    let mut out = Vec::new();
    if !is_zip {
        stream_elements(BufReader::new(file), hint, &mut out)
            .context("This file is not a chat export. It should be a zip or a .json file.")?;
        return Ok(out);
    }

    let mut archive = zip::ZipArchive::new(file).context("could not open the zip")?;
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
    for name in names {
        let entry = archive
            .by_name(&name)
            .with_context(|| format!("could not read {name} in the zip"))?;
        stream_elements(BufReader::new(entry), hint, &mut out)
            .with_context(|| format!("{name} in the zip is not a chat export"))?;
    }
    Ok(out)
}

/// `conversations.json`, or a numbered part of it (`conversations-000.json`),
/// in any folder of the zip.
fn is_conversations_file(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.starts_with("conversations") && base.ends_with(".json")
}

/// Walk a top-level JSON array one element at a time, parsing each into
/// messages, so the whole document never sits in memory as one value.
fn stream_elements<R: Read>(
    reader: R,
    hint: Option<&str>,
    out: &mut Vec<ConversationMsg>,
) -> Result<()> {
    struct Each<'a> {
        hint: Option<&'a str>,
        out: &'a mut Vec<ConversationMsg>,
    }
    impl<'de> Visitor<'de> for Each<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list of conversations")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            let mut index = 0;
            while let Some(element) = seq.next_element::<Value>()? {
                self.out.extend(transform::parse_element(self.hint, index, &element));
                index += 1;
            }
            Ok(())
        }
    }
    let mut de = serde_json::Deserializer::from_reader(reader);
    de.deserialize_seq(Each { hint, out })?;
    de.end()?;
    Ok(())
}

/// "Brought in your ChatGPT chats from March 2023 to November 2026." The span
/// of time says what arrived; `records_processed` carries the count.
fn summary(messages: &[ConversationMsg], written: usize) -> String {
    let providers: BTreeSet<&str> = messages.iter().map(|m| display_name(&m.provider)).collect();
    let who = join_names(&providers.into_iter().collect::<Vec<_>>());
    let first = messages.iter().map(|m| m.timestamp).min();
    let last = messages.iter().map(|m| m.timestamp).max();
    let span = match (first, last) {
        (Some(a), Some(b)) if month(a) == month(b) => format!(" from {}", month(a)),
        (Some(a), Some(b)) => format!(" from {} to {}", month(a), month(b)),
        _ => String::new(),
    };
    if written == 0 {
        format!("Your {who} chats{span} were already here.")
    } else {
        format!("Brought in your {who} chats{span}.")
    }
}

fn month(t: DateTime<Utc>) -> String {
    t.format("%B %Y").to_string()
}

fn display_name(provider: &str) -> &str {
    match provider {
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        "gemini" => "Gemini",
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

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("chat_import_test_{unique}_{name}"))
    }

    #[test]
    fn reads_a_bare_json_export() {
        let path = temp_path("bare.json");
        std::fs::write(&path, chatgpt_json()).unwrap();
        let rows = read_export(&path, None).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, "chatgpt");
    }

    #[test]
    fn reads_conversations_json_inside_a_zip() {
        let path = temp_path("export.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("chat.html", opts).unwrap();
            zip.write_all(b"<html></html>").unwrap();
            zip.start_file("export/conversations.json", opts).unwrap();
            zip.write_all(chatgpt_json().as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let rows = read_export(&path, None).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "What should I cook tonight?");
    }

    #[test]
    fn a_zip_without_conversations_fails_with_a_reason() {
        let path = temp_path("other.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            zip.start_file("notes.txt", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"hello").unwrap();
            zip.finish().unwrap();
        }
        let err = read_export(&path, None).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("no conversations.json"), "{err}");
    }

    #[test]
    fn summary_names_the_sites_and_the_span() {
        let mut rows = read_export_str(&chatgpt_json());
        assert_eq!(summary(&rows, 1), "Brought in your ChatGPT chats from November 2023.");
        rows[0].provider = "claude".into();
        let mut more = read_export_str(&chatgpt_json());
        more[0].timestamp = more[0].timestamp + chrono::Duration::days(400);
        rows.extend(more);
        assert_eq!(
            summary(&rows, 0),
            "Your ChatGPT and Claude chats from November 2023 to December 2024 were already here."
        );
    }

    fn read_export_str(json: &str) -> Vec<ConversationMsg> {
        let mut out = Vec::new();
        stream_elements(json.as_bytes(), None, &mut out).unwrap();
        out
    }
}
