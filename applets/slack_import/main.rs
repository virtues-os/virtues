//! Slack import: one-time ingest of a Slack workspace export into
//! `data_communication_message`.
//!
//! Manual-trigger only (no cron). `POST /api/imports/slack/upload` stages the
//! upload and starts this applet with
//! `payload = { "file_path": "...", "email": "..." }`, where `email` is the
//! address the owner uses in Slack.
//!
//! The file is the zip a workspace admin downloads from Slack's export page.
//! It holds every public channel in the workspace, and private channels and
//! direct messages when the workspace's plan allows it. The owner is the
//! person in `users.json` with that email, and only conversations they are a
//! member of are brought in. Each day file is read one message at a time on
//! a blocking thread and written in batches, so memory stays flat.
//!
//! Rows dedup on `source_stream_id`, so importing a newer export later adds
//! only the new messages (`ON CONFLICT DO NOTHING`).

mod transform;

use std::collections::{BTreeMap, HashMap};
use std::io::BufReader;
use std::path::Path;

use anyhow::{bail, Context as _, Result};
use serde_json::Value;
use sqlx::PgPool;
use transform::{Context, Conversation, Kind, Message, People};
use virtues_applets::import::{self, base_name, Batches, Tally, Upload};
use virtues_helpers::dedup::build_batch_insert_query;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const APPLET: &str = "slack_import";

const NOT_AN_EXPORT: &str = "This file isn't a Slack export. Use the zip a workspace admin downloads from \
     Slack's export page.";

const NOTHING_OF_YOURS: &str = "This export has no messages in conversations you're a member of.";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let path = import::staged_file(&input, APPLET)?;
    // Absent and empty both mean the owner is unknown, which the reader
    // refuses with the sentence the app shows.
    let email = input
        .payload
        .as_ref()
        .and_then(|p| p.get("email"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let result = run(&path, email).await;
    import::remove_upload(&path, APPLET);
    let tally = result?;
    if tally.parsed == 0 {
        bail!("{NOTHING_OF_YOURS}");
    }
    output_with_records(&tally.summary("Slack messages", "were"), &input.config, tally.written as i64)?;
    Ok(())
}

async fn run(path: &Path, email: String) -> Result<Tally> {
    let pool = connect_from_env("virtues-action-slack_import").await?;
    let (mut rx, reader) = import::spawn_reader(path, move |p, sink| read_export(p, &email, sink));
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

/// A small JSON file in the zip, or `None` when the export has no such file.
fn read_json(upload: &mut Upload, name: &str) -> Result<Option<Value>> {
    upload.with_entry(name, |r| serde_json::from_reader(BufReader::new(r)).with_context(|| format!("{name} is not valid JSON")))
}

/// Read every message of the owner's conversations, handing them to `sink`
/// in batches.
fn read_export(path: &Path, email: &str, sink: &mut dyn FnMut(Vec<Message>) -> Result<()>) -> Result<()> {
    let mut upload = Upload::open(path)?;
    if matches!(upload, Upload::Bare(_)) {
        bail!("{NOT_AN_EXPORT}");
    }
    let all = upload.names(|_| true);
    // Slack puts `users.json` at the top of the zip. Someone who unzipped
    // and zipped the export again has it one folder down, so the export's
    // root is wherever the shallowest `users.json` sits.
    let Some(users_path) = all.iter().filter(|n| base_name(n) == "users.json").min_by_key(|n| n.matches('/').count()) else {
        bail!("{NOT_AN_EXPORT}");
    };
    let root = users_path.strip_suffix("users.json").unwrap_or("").to_string();
    let users = read_json(&mut upload, users_path)?.unwrap_or(Value::Null);
    let people = People::from_json(&users);
    let owner = people.owner(email)?;

    let mut conversations = Vec::new();
    for (file, kind) in Kind::FILES {
        if let Some(list) = read_json(&mut upload, &format!("{root}{file}"))? {
            conversations.extend(Conversation::list(&list, kind));
        }
    }
    let channel_names: HashMap<String, String> =
        conversations.iter().filter_map(|c| Some((c.id.clone(), c.name.clone()?))).collect();

    // Day files by conversation folder. `all` is sorted, so each folder's
    // days are in date order and a re-import reads them the same way.
    let mut days: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for name in &all {
        let Some((folder, file)) = name.strip_prefix(root.as_str()).and_then(|r| r.split_once('/')) else {
            continue;
        };
        if transform::is_day_file(file) {
            days.entry(folder).or_default().push(name);
        }
    }

    let cx = Context { people: &people, channel_names: &channel_names, owner: &owner };
    let mut batches = Batches::new(sink);
    for conv in conversations.iter().filter(|c| c.includes(&owner)) {
        let Some(files) = days.get(conv.folder()) else {
            continue;
        };
        let label = conv.label(&people, &owner);
        for file in files {
            let read = upload.with_entry(file, |r| {
                let mut reader = BufReader::new(r);
                if !import::starts_with_array(&mut reader)? {
                    return Ok(());
                }
                import::for_each_element(reader, &mut |v| match transform::from_json(&v, conv, label.as_deref(), &cx) {
                    Some(m) => batches.push(m),
                    None => Ok(()),
                })
            });
            // What came before a damaged file is kept.
            batches.flush()?;
            read.context(
                "Part of this zip is damaged. Download the export again and retry; the messages before the damage were kept.",
            )?;
        }
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
            "reply_to_message_id",
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
            .bind("slack")
            .bind(&m.body)
            .bind(&m.from_identifier)
            .bind(&m.from_handle)
            .bind(&m.from_name)
            .bind(&m.to_identifiers)
            // An export is history the owner has already seen in Slack.
            .bind(true)
            .bind(m.is_group_message)
            .bind(&m.reply_to_message_id)
            .bind(m.has_attachments)
            .bind(m.occurred_at)
            .bind(&m.source_stream_id)
            .bind("slack_import")
            .bind("slack")
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
        std::env::temp_dir().join(format!("slack_import_test_{unique}_{name}"))
    }

    fn zip_with(path: &Path, entries: &[(&str, String)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// An invented workspace: Nick is in #launch-review and a DM with David
    /// Okafor, and not in #random.
    fn export(path: &Path, root: &str) {
        let users = json!([
            {"id": "U01NICK", "name": "nick", "profile": {"real_name": "Nick", "email": "nick@example.com"}},
            {"id": "U02DAVID", "name": "david", "profile": {"real_name": "David Okafor", "email": "david.okafor@example.com"}}
        ]);
        let channels = json!([
            {"id": "C01LAUNCH", "name": "launch-review", "members": ["U01NICK", "U02DAVID"]},
            {"id": "C02RANDOM", "name": "random", "members": ["U02DAVID"]}
        ]);
        let dms = json!([{"id": "D01", "members": ["U01NICK", "U02DAVID"]}]);
        let launch_day = json!([
            {"type": "message", "subtype": "channel_join", "ts": "1712300000.000100", "user": "U01NICK", "text": "<@U01NICK> has joined the channel"},
            {"type": "message", "ts": "1712345678.123456", "user": "U02DAVID", "text": "Draft is up for <#C01LAUNCH>"},
            {"type": "message", "ts": "1712345700.000200", "user": "U01NICK", "text": "Reading it now", "thread_ts": "1712345678.123456"}
        ]);
        let random_day = json!([{"type": "message", "ts": "1712345800.000100", "user": "U02DAVID", "text": "Anyone for the night market?"}]);
        let dm_day = json!([{"type": "message", "ts": "1714600000.000100", "user": "U01NICK", "text": "Lunch at noon?"}]);
        let entries = [
            ("users.json", users.to_string()),
            ("channels.json", channels.to_string()),
            ("dms.json", dms.to_string()),
            ("integration_logs.json", "[]".to_string()),
            ("launch-review/2024-04-05.json", launch_day.to_string()),
            ("launch-review/canvas_in_the_conversation.json", "{}".to_string()),
            ("random/2024-04-05.json", random_day.to_string()),
            ("D01/2024-05-01.json", dm_day.to_string()),
        ];
        let entries: Vec<(String, String)> = entries.into_iter().map(|(n, b)| (format!("{root}{n}"), b)).collect();
        let entries: Vec<(&str, String)> = entries.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
        zip_with(path, &entries);
    }

    fn collect(path: &Path, email: &str) -> Result<Vec<Message>> {
        let mut out = Vec::new();
        read_export(path, email, &mut |b| {
            out.extend(b);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn only_the_owners_conversations_are_read() {
        for root in ["", "Example Workspace Slack export/"] {
            let path = temp_path("export.zip");
            export(&path, root);
            let rows = collect(&path, "Nick@Example.com").unwrap();
            std::fs::remove_file(&path).unwrap();
            let mut ids: Vec<&str> = rows.iter().map(|m| m.source_stream_id.as_str()).collect();
            ids.sort();
            assert_eq!(
                ids,
                [
                    "slack:C01LAUNCH:1712345678.123456",
                    "slack:C01LAUNCH:1712345700.000200",
                    "slack:D01:1714600000.000100"
                ],
                "{root:?}: #random and the join event are left out"
            );
            let draft = rows.iter().find(|m| m.message_id == "1712345678.123456").unwrap();
            assert_eq!(draft.body, "Draft is up for #launch-review");
            let reply = rows.iter().find(|m| m.message_id == "1712345700.000200").unwrap();
            assert_eq!(reply.reply_to_message_id.as_deref(), Some("1712345678.123456"));
        }
    }

    #[test]
    fn an_unknown_owner_is_refused_with_one_sentence() {
        let path = temp_path("export.zip");
        export(&path, "");
        for email in ["", "someone@example.com"] {
            let err = collect(&path, email).unwrap_err().to_string();
            assert_eq!(err, transform::NOT_IN_EXPORT);
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_bare_file_or_a_zip_without_users_is_refused() {
        let path = temp_path("bare.upload");
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(collect(&path, "nick@example.com").unwrap_err().to_string(), NOT_AN_EXPORT);
        zip_with(&path, &[("channels.json", "[]".to_string())]);
        let err = collect(&path, "nick@example.com").unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(err, NOT_AN_EXPORT);
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_second_import_of_the_same_export_adds_nothing(pool: PgPool) {
        let path = temp_path("again.zip");
        export(&path, "");
        let rows = collect(&path, "nick@example.com").unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(write_messages(&pool, &rows).await.unwrap(), 3);
        assert_eq!(write_messages(&pool, &rows).await.unwrap(), 0);
        let (n, from_me, handles): (i64, i64, Vec<String>) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE metadata->>'is_from_me' = 'true'), \
                    array_agg(DISTINCT from_handle ORDER BY from_handle) \
             FROM data_communication_message WHERE channel = 'slack'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((n, from_me), (3, 2));
        assert_eq!(handles, ["", "david.okafor@example.com"]);
    }
}
