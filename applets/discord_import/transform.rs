//! Discord's data package shaped for `data_communication_message`.
//!
//! The package holds only the owner's own sent messages, one folder per
//! channel. Each folder has a `channel.json` saying what the channel is and
//! a `messages.json` (newer packages) or `messages.csv` (older ones). Each
//! message becomes a row from the owner, following the iMessage writer's
//! conventions: `metadata.is_from_me` says who sent it, `conversation_id` is
//! the channel, and `from_handle` is empty because the sender is the owner,
//! which is the value people resolution reads as "nobody to resolve".

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use virtues_applets::import::{field, parse_time, text, Columns};

/// The person whose package this is.
#[derive(Debug, Clone)]
pub struct Owner {
    pub id: String,
    pub name: Option<String>,
}

impl Owner {
    /// From `account/user.json`. A package without one still imports: the
    /// sender is then `me`, as the iMessage writer names the owner.
    pub fn from_user_json(v: Option<&Value>) -> Owner {
        let id = v.and_then(|v| text(v, "id")).unwrap_or_else(|| "me".to_string());
        let name = v.and_then(|v| text(v, "global_name").or_else(|| text(v, "username")));
        Owner { id, name }
    }
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub name: Option<String>,
    pub guild_name: Option<String>,
    pub is_group: bool,
    /// Everyone in a direct message or group DM but the owner. Empty for a
    /// server channel.
    pub recipients: Vec<String>,
}

impl Channel {
    /// From a folder's `channel.json`. `folder_id` is the id the folder name
    /// carries, used when the file has none; `index_name` is the package
    /// index's name for the channel ("Direct Message with nick"), used when
    /// the file names none.
    pub fn from_json(v: &Value, folder_id: &str, index_name: Option<&str>, owner: &Owner) -> Channel {
        let guild = v.get("guild").filter(|g| g.is_object());
        // Discord wrote the type as a number in older packages and as a name
        // in newer ones. 3 is a group DM.
        let group_dm = match v.get("type") {
            Some(Value::Number(n)) => n.as_i64() == Some(3),
            Some(Value::String(s)) => s.eq_ignore_ascii_case("GROUP_DM"),
            _ => false,
        };
        let recipients = if guild.is_some() {
            Vec::new()
        } else {
            v.get("recipients")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|r| match r {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .filter(|id| *id != owner.id)
                        .collect()
                })
                .unwrap_or_default()
        };
        Channel {
            id: text(v, "id").unwrap_or_else(|| folder_id.to_string()),
            name: text(v, "name").or_else(|| index_name.map(str::to_string)),
            guild_name: guild.and_then(|g| text(g, "name")),
            is_group: guild.is_some() || group_dm,
            recipients,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub message_id: String,
    pub conversation_id: String,
    pub body: String,
    pub from_identifier: String,
    pub from_name: Option<String>,
    pub to_identifiers: Value,
    pub is_group_message: bool,
    pub has_attachments: bool,
    pub occurred_at: DateTime<Utc>,
    pub source_stream_id: String,
    pub metadata: Value,
}

/// One element of a `messages.json` array.
pub fn from_json(v: &Value, channel: &Channel, owner: &Owner) -> Option<Message> {
    let id = text(v, "ID").or_else(|| text(v, "id"))?;
    let at = v.get("Timestamp").or_else(|| v.get("timestamp")).and_then(Value::as_str)?;
    let contents = v.get("Contents").or_else(|| v.get("contents")).and_then(Value::as_str).unwrap_or("");
    let attachments = match v.get("Attachments").or_else(|| v.get("attachments")) {
        Some(Value::String(s)) => s.split_whitespace().map(str::to_string).collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|a| a.as_str().map(str::to_string).or_else(|| text(a, "url")).or_else(|| text(a, "filename")))
            .collect(),
        _ => Vec::new(),
    };
    build(id, at, contents, attachments, channel, owner)
}

/// Where the columns of a `messages.csv` sit.
pub struct CsvLayout {
    id: usize,
    timestamp: usize,
    contents: Option<usize>,
    attachments: Option<usize>,
}

impl CsvLayout {
    pub fn from_header(header: &[String]) -> Option<CsvLayout> {
        let c = Columns::new(header);
        Some(CsvLayout {
            id: c.find(&["ID"])?,
            timestamp: c.find(&["Timestamp"])?,
            contents: c.find(&["Contents"]),
            attachments: c.find(&["Attachments"]),
        })
    }
}

/// One record of a `messages.csv`.
pub fn from_csv(layout: &CsvLayout, record: &[String], channel: &Channel, owner: &Owner) -> Option<Message> {
    let id = field(record, Some(layout.id))?.to_string();
    let at = field(record, Some(layout.timestamp))?;
    // Contents keep their own spacing; only an all-blank message is empty.
    let contents = layout.contents.and_then(|i| record.get(i)).map(String::as_str).unwrap_or("");
    let attachments = field(record, layout.attachments)
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    build(id, at, contents, attachments, channel, owner)
}

fn build(id: String, at: &str, contents: &str, attachments: Vec<String>, channel: &Channel, owner: &Owner) -> Option<Message> {
    // A message that cannot be placed in time is passed over: filing it at
    // import time would put it on the wrong day for good, since a later
    // import never corrects a row.
    let occurred_at = parse_time(at)?;
    let text = contents.trim();
    if text.is_empty() && attachments.is_empty() {
        return None;
    }
    // The package lists attachments as links that expire, so the body names
    // each one the way the iMessage writer does ("[Photo]"), which a person,
    // search and a model can all read.
    let labels: Vec<String> = attachments.iter().map(|a| attachment_label(a)).collect();
    let body = match (text.is_empty(), labels.is_empty()) {
        (true, _) => labels.join(" "),
        (false, true) => contents.to_string(),
        (false, false) => format!("{contents} {}", labels.join(" ")),
    };
    Some(Message {
        source_stream_id: format!("discord:{id}"),
        message_id: id,
        conversation_id: channel.id.clone(),
        body,
        from_identifier: owner.id.clone(),
        from_name: owner.name.clone(),
        to_identifiers: json!(channel.recipients),
        is_group_message: channel.is_group,
        has_attachments: !attachments.is_empty(),
        occurred_at,
        metadata: json!({
            "is_from_me": true,
            "channel_name": channel.name,
            "guild_name": channel.guild_name,
        }),
    })
}

/// "[Photo]", "[Video]", "[Audio]" or "[File: notes.pdf]" for an attachment
/// link, by the file name at its end.
fn attachment_label(link: &str) -> String {
    let path = link.split(['?', '#']).next().unwrap_or(link);
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "gif" => "[GIF]".to_string(),
        "png" | "jpg" | "jpeg" | "webp" | "heic" | "bmp" => "[Photo]".to_string(),
        "mp4" | "mov" | "webm" | "mkv" => "[Video]".to_string(),
        "mp3" | "ogg" | "m4a" | "wav" | "opus" => "[Audio]".to_string(),
        _ if !name.is_empty() => format!("[File: {name}]"),
        _ => "[Attachment]".to_string(),
    }
}

/// The channel folder a `channel.json` path names, as `(folder path, id)`:
/// `messages/c123/channel.json` gives `("messages/c123/", "123")`. Older
/// packages name the folder by the bare id. The top folder is `messages` or
/// `Messages` depending on the package's year.
pub fn channel_folder(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.split('/').collect();
    let [top, folder, file] = parts.as_slice() else {
        return None;
    };
    if !top.eq_ignore_ascii_case("messages") || *file != "channel.json" {
        return None;
    }
    let id = folder.strip_prefix('c').filter(|r| r.bytes().all(|b| b.is_ascii_digit())).unwrap_or(folder);
    Some((format!("{top}/{folder}/"), id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtues_applets::import::Csv;

    fn owner() -> Owner {
        Owner::from_user_json(Some(&json!({"id": "100000000000000001", "username": "nick", "global_name": "Nick"})))
    }

    fn dm() -> Channel {
        Channel::from_json(
            &json!({"id": "200000000000000002", "type": 1, "recipients": ["100000000000000001", "100000000000000003"]}),
            "200000000000000002",
            Some("Direct Message with davidokafor"),
            &owner(),
        )
    }

    #[test]
    fn a_dm_names_the_other_person_and_a_server_channel_is_a_group() {
        let c = dm();
        assert_eq!(c.recipients, vec!["100000000000000003"]);
        assert!(!c.is_group);
        assert_eq!(c.name.as_deref(), Some("Direct Message with davidokafor"));

        let server = Channel::from_json(
            &json!({"id": "300", "type": "GUILD_TEXT", "name": "general", "guild": {"id": "400", "name": "Example Book Club"}}),
            "300",
            None,
            &owner(),
        );
        assert!(server.is_group);
        assert!(server.recipients.is_empty());
        assert_eq!((server.name.as_deref(), server.guild_name.as_deref()), (Some("general"), Some("Example Book Club")));

        let group = Channel::from_json(&json!({"id": "500", "type": 3, "recipients": ["100000000000000001", "6", "7"]}), "500", None, &owner());
        assert!(group.is_group);
        assert_eq!(group.recipients, vec!["6", "7"]);
    }

    #[test]
    fn a_json_message_becomes_a_row_from_the_owner() {
        let m = from_json(
            &json!({"ID": 900000000000000009u64, "Timestamp": "2024-04-01 18:30:00", "Contents": "See you at the night market", "Attachments": ""}),
            &dm(),
            &owner(),
        )
        .unwrap();
        assert_eq!(m.message_id, "900000000000000009");
        assert_eq!(m.source_stream_id, "discord:900000000000000009");
        assert_eq!(m.conversation_id, "200000000000000002");
        assert_eq!(m.from_identifier, "100000000000000001");
        assert_eq!(m.from_name.as_deref(), Some("Nick"));
        assert_eq!(m.to_identifiers, json!(["100000000000000003"]));
        assert_eq!(m.occurred_at.to_rfc3339(), "2024-04-01T18:30:00+00:00");
        assert!(!m.has_attachments);
        assert_eq!(m.metadata["is_from_me"], true);
        assert_eq!(m.metadata["channel_name"], "Direct Message with davidokafor");
    }

    #[test]
    fn an_attachment_is_named_in_the_body_and_an_empty_message_is_passed_over() {
        let photo = from_json(
            &json!({"ID": "1", "Timestamp": "2024-04-01T18:30:00+00:00", "Contents": "", "Attachments": "https://cdn.example.com/a/lanterns.png?ex=1"}),
            &dm(),
            &owner(),
        )
        .unwrap();
        assert_eq!(photo.body, "[Photo]");
        assert!(photo.has_attachments);
        let both = from_json(
            &json!({"ID": "2", "Timestamp": "2024-04-01T18:30:00Z", "Contents": "the plan", "Attachments": "https://cdn.example.com/a/plan.pdf"}),
            &dm(),
            &owner(),
        )
        .unwrap();
        assert_eq!(both.body, "the plan [File: plan.pdf]");
        assert!(from_json(&json!({"ID": "3", "Timestamp": "2024-04-01 18:30:00", "Contents": "  ", "Attachments": ""}), &dm(), &owner()).is_none());
        assert!(from_json(&json!({"ID": "4", "Timestamp": "yesterday", "Contents": "hi"}), &dm(), &owner()).is_none());
    }

    #[test]
    fn an_older_csv_message_is_read() {
        let data = "ID,Timestamp,Contents,Attachments\n\
                    800,2019-06-01 12:00:00.123000+00:00,\"Lunch, then the lake?\",\n";
        let mut csv = Csv::new(data.as_bytes());
        let layout = CsvLayout::from_header(&csv.next_record().unwrap().unwrap()).unwrap();
        let m = from_csv(&layout, &csv.next_record().unwrap().unwrap(), &dm(), &owner()).unwrap();
        assert_eq!(m.body, "Lunch, then the lake?");
        assert_eq!(m.occurred_at.to_rfc3339(), "2019-06-01T12:00:00.123+00:00");
    }

    #[test]
    fn a_package_without_a_user_file_sends_as_me() {
        let o = Owner::from_user_json(None);
        assert_eq!((o.id.as_str(), o.name), ("me", None));
    }

    #[test]
    fn channel_folders_are_found_in_either_layout() {
        assert_eq!(channel_folder("messages/c123/channel.json"), Some(("messages/c123/".into(), "123".into())));
        assert_eq!(channel_folder("Messages/c123/channel.json"), Some(("Messages/c123/".into(), "123".into())));
        assert_eq!(channel_folder("messages/123/channel.json"), Some(("messages/123/".into(), "123".into())));
        assert_eq!(channel_folder("messages/index.json"), None);
        assert_eq!(channel_folder("servers/1/channel.json"), None);
    }
}
