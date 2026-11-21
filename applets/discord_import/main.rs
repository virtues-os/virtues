//! Discord import: one-time ingest of the messages in a Discord data package
//! into `data_communication_message`.
//!
//! Manual-trigger only (no cron). `POST /api/imports/discord/upload` stages
//! the upload and starts this applet with `payload = { "file_path": "..." }`.
//!
//! The file is the `package.zip` Discord emails after "Request all of my
//! data". It holds only the owner's own sent messages, so every row is from
//! the owner. `account/user.json` says who the owner is, and each channel's
//! folder under `messages/` (or `Messages/`) says what the channel is. Each
//! messages file is read one message at a time on a blocking thread and
//! written in batches, so memory stays flat.
//!
//! Rows dedup on `source_stream_id`, so importing a newer package later adds
//! only the new messages (`ON CONFLICT DO NOTHING`).

mod transform;

use std::collections::BTreeSet;
use std::io::BufReader;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use sqlx::PgPool;
use transform::{channel_folder, Channel, CsvLayout, Message, Owner};
use virtues_applets::import::{self, Batches, Csv, Tally, Upload};
use virtues_helpers::dedup::build_batch_insert_query;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const APPLET: &str = "discord_import";

const NOT_A_PACKAGE: &str = "This file has no Discord messages in it. Use the package.zip Discord emails you \
     after you request all of your data.";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let path = import::staged_file(&input, APPLET)?;
    let result = run(&path).await;
    import::remove_upload(&path, APPLET);
    let tally = result?;
    if tally.parsed == 0 {
        bail!("{NOT_A_PACKAGE}");
    }
    output_with_records(&tally.summary("Discord messages", "were"), &input.config, tally.written as i64)?;
    Ok(())
}

async fn run(path: &Path) -> Result<Tally> {
    let pool = connect_from_env("virtues-action-discord_import").await?;
    let (mut rx, reader) = import::spawn_reader(path, read_package);
    let mut tally = Tally::default();
    while let Some(batch) = rx.recv().await {
        for m in &batch {
            tally.saw(m.occurred_at);
        }
        tally.written += write_messages(&pool, &batch).await?;
    }
    import::join_reader(reader).await?;
    Ok(tally)
}

/// A small JSON file in the zip, found by its path ignoring case.
fn read_json(upload: &mut Upload, path: &str) -> Result<Option<Value>> {
    let Some(name) = upload.names(|n| n.eq_ignore_ascii_case(path)).into_iter().next() else {
        return Ok(None);
    };
    upload
        .with_entry(&name, |r| serde_json::from_reader(BufReader::new(r)).with_context(|| format!("{name} is not valid JSON")))
}

/// Read every message in the package, handing them to `sink` in batches.
fn read_package(path: &Path, sink: &mut dyn FnMut(Vec<Message>) -> Result<()>) -> Result<()> {
    let mut upload = Upload::open(path)?;
    if matches!(upload, Upload::Bare(_)) {
        bail!("{NOT_A_PACKAGE}");
    }
    let owner = Owner::from_user_json(read_json(&mut upload, "account/user.json")?.as_ref());
    let index = read_json(&mut upload, "messages/index.json")?;

    let all: BTreeSet<String> = upload.names(|_| true).into_iter().collect();
    let folders: Vec<(String, String)> = all.iter().filter_map(|n| channel_folder(n)).collect();
    if folders.is_empty() {
        bail!("{NOT_A_PACKAGE}");
    }

    let mut batches = Batches::new(sink);
    for (folder, folder_id) in &folders {
        let channel_json = read_json(&mut upload, &format!("{folder}channel.json"))?.unwrap_or(Value::Null);
        let index_name = index.as_ref().and_then(|i| i.get(folder_id)).and_then(Value::as_str);
        let channel = Channel::from_json(&channel_json, folder_id, index_name, &owner);

        let json_file = format!("{folder}messages.json");
        let csv_file = format!("{folder}messages.csv");
        let read = if all.contains(&json_file) {
            upload.with_entry(&json_file, |r| {
                let mut reader = BufReader::new(r);
                if !import::starts_with_array(&mut reader)? {
                    return Ok(());
                }
                import::for_each_element(reader, &mut |v| match transform::from_json(&v, &channel, &owner) {
                    Some(m) => batches.push(m),
                    None => Ok(()),
                })
            })
        } else if all.contains(&csv_file) {
            upload.with_entry(&csv_file, |r| {
                let mut csv = Csv::new(r);
                let Some(layout) = csv.next_record()?.as_deref().and_then(CsvLayout::from_header) else {
                    return Ok(());
                };
                while let Some(record) = csv.next_record()? {
                    if let Some(m) = transform::from_csv(&layout, &record, &channel, &owner) {
                        batches.push(m)?;
                    }
                }
                Ok(())
            })
        } else {
            // A voice channel, or a channel the owner never wrote in.
            continue;
        };
        // What came before a damaged file is kept.
        batches.flush()?;
        read.context("Part of this zip is damaged. Download the package again and retry; the messages before the damage were kept.")?;
    }
    batches.flush()
}

/// Insert messages, skipping any already here. Returns how many were new.
async fn write_messages(pool: &PgPool, rows: &[Message]) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_communication_message",
        &[
            "id",
            "message_id",
            "conversation_id",
            // The registry reads a message's source as "message:" || channel.
            "channel",
            "body",
            "from_identifier",
            "from_handle",
            "from_name",
            "to_identifiers",
            "is_read",
            "is_group_message",
            "has_attachments",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for m in rows {
        q = q
            .bind(import::row_id(&m.source_stream_id))
            .bind(&m.message_id)
            .bind(&m.conversation_id)
            .bind("discord")
            .bind(&m.body)
            .bind(&m.from_identifier)
            // Empty: the sender is the owner, so there is nobody to resolve.
            // A Discord id would otherwise read as a phone number.
            .bind("")
            .bind(&m.from_name)
            .bind(&m.to_identifiers)
            // Every message in the package is one the owner sent.
            .bind(true)
            .bind(m.is_group_message)
            .bind(m.has_attachments)
            .bind(m.occurred_at)
            .bind(&m.source_stream_id)
            .bind("discord_import")
            .bind("discord")
            .bind(&m.metadata);
    }
    Ok(q.execute(pool).await?.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("discord_import_test_{unique}_{name}"))
    }

    fn package(path: &Path) {
        let user = json!({"id": "100000000000000001", "username": "nick", "global_name": "Nick"}).to_string();
        let index = json!({"200": "Direct Message with davidokafor", "300": "general in Example Book Club"}).to_string();
        let dm = json!({"id": "200", "type": 1, "recipients": ["100000000000000001", "100000000000000003"]}).to_string();
        let dm_messages = json!([
            {"ID": "901", "Timestamp": "2024-04-01 18:30:00", "Contents": "See you at the night market", "Attachments": ""},
            {"ID": "902", "Timestamp": "2024-05-02 09:00:00", "Contents": "", "Attachments": ""}
        ])
        .to_string();
        let server = json!({"id": "300", "type": 0, "name": "general", "guild": {"id": "400", "name": "Example Book Club"}}).to_string();
        let server_csv = "ID,Timestamp,Contents,Attachments\n903,2020-01-02 03:04:05.000000+00:00,\"Chapter 3, anyone?\",\n";
        let voice = json!({"id": "600", "type": 2, "name": "lounge", "guild": {"id": "400", "name": "Example Book Club"}}).to_string();

        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("account/user.json", user.as_str()),
            ("messages/index.json", index.as_str()),
            ("messages/c200/channel.json", dm.as_str()),
            ("messages/c200/messages.json", dm_messages.as_str()),
            ("messages/c300/channel.json", server.as_str()),
            ("messages/c300/messages.csv", server_csv),
            ("messages/c600/channel.json", voice.as_str()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn collect(path: &Path) -> Result<Vec<Message>> {
        let mut out = Vec::new();
        read_package(path, &mut |b| {
            out.extend(b);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn a_package_reads_every_channel_in_both_layouts() {
        let path = temp_path("package.zip");
        package(&path);
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 2, "the empty message is passed over");
        let dm = rows.iter().find(|m| m.message_id == "901").unwrap();
        assert_eq!(dm.conversation_id, "200");
        assert_eq!(dm.metadata["channel_name"], "Direct Message with davidokafor");
        assert!(!dm.is_group_message);
        let server = rows.iter().find(|m| m.message_id == "903").unwrap();
        assert_eq!(server.body, "Chapter 3, anyone?");
        assert!(server.is_group_message);
        assert_eq!(server.metadata["guild_name"], "Example Book Club");
        assert_eq!(server.from_name.as_deref(), Some("Nick"));
    }

    #[test]
    fn a_bare_file_or_a_zip_without_messages_is_refused() {
        let path = temp_path("bare.upload");
        std::fs::write(&path, "[]").unwrap();
        assert!(collect(&path).unwrap_err().to_string().contains("no Discord messages"));
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("account/user.json", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();
        let err = collect(&path).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("no Discord messages"), "{err}");
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_second_import_of_the_same_package_adds_nothing(pool: PgPool) {
        let path = temp_path("again.zip");
        package(&path);
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(write_messages(&pool, &rows).await.unwrap(), 2);
        assert_eq!(write_messages(&pool, &rows).await.unwrap(), 0);
        let (n, from_me, handle): (i64, i64, String) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE metadata->>'is_from_me' = 'true'), min(from_handle) \
             FROM data_communication_message WHERE channel = 'discord'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((n, from_me, handle.as_str()), (2, 2, ""));
    }
}
