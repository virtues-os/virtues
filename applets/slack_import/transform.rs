//! A Slack workspace export shaped for `data_communication_message`.
//!
//! A workspace admin downloads the export as a zip. `users.json` lists the
//! workspace's people; `channels.json`, `groups.json`, `dms.json` and
//! `mpims.json` list its conversations (a standard export has only the
//! first); and each conversation has a folder of one JSON file per day.
//!
//! Rows follow the iMessage and Discord writers' conventions:
//! `conversation_id` is the channel, `metadata.is_from_me` says who sent it,
//! and `from_handle` is what people resolution matches against a person's
//! handles. Resolution keys on lowercased email addresses, so a message from
//! someone else carries their Slack email there. The owner's own messages
//! and senders with no email carry an empty handle, which resolution reads
//! as "nobody to resolve". The handle is never left NULL, because the
//! resolver's backfill would then try to read a Slack user id as a phone
//! number.

use std::collections::HashMap;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use virtues_applets::import::text;
use virtues_helpers::handles::normalize_handle;

/// The sentence the app shows when the owner cannot be found in the export.
pub const NOT_IN_EXPORT: &str =
    "Your email isn't in this export's list of people. Enter the email you use in Slack and import it again.";

/// One entry of `users.json`.
#[derive(Debug, Clone)]
pub struct Person {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Everyone in `users.json`, by Slack user id.
#[derive(Debug, Default)]
pub struct People(HashMap<String, Person>);

impl People {
    pub fn from_json(v: &Value) -> People {
        let mut people = HashMap::new();
        for u in v.as_array().into_iter().flatten() {
            let Some(id) = text(u, "id") else { continue };
            let profile = u.get("profile").unwrap_or(&Value::Null);
            let name = text(profile, "real_name")
                .or_else(|| text(u, "real_name"))
                .or_else(|| text(u, "name"))
                .map(|n| n.trim().to_string());
            let email = text(profile, "email").map(|e| e.trim().to_string());
            people.insert(id, Person { name, email });
        }
        People(people)
    }

    pub fn name(&self, id: &str) -> Option<&str> {
        self.0.get(id).and_then(|p| p.name.as_deref())
    }

    /// The sender's handle for people resolution: their email lowercased,
    /// or empty when the export has none for them.
    pub fn handle(&self, id: &str) -> String {
        self.0
            .get(id)
            .and_then(|p| p.email.as_deref())
            .filter(|e| e.contains('@'))
            .and_then(normalize_handle)
            .unwrap_or_default()
    }

    /// The user id whose profile email is `email`, ignoring case. An export
    /// holds every public channel in the workspace, so without the owner
    /// there is no way to tell their conversations from everyone else's, and
    /// the import stops rather than bringing in all of them.
    pub fn owner(&self, email: &str) -> Result<String> {
        let email = email.trim();
        if email.is_empty() {
            bail!("{NOT_IN_EXPORT}");
        }
        let found = self
            .0
            .iter()
            .filter(|(_, p)| p.email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(email)))
            .map(|(id, _)| id)
            .min();
        match found {
            Some(id) => Ok(id.clone()),
            None => bail!("{NOT_IN_EXPORT}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Channel,
    PrivateChannel,
    Dm,
    GroupDm,
}

impl Kind {
    /// The conversation list file for each kind, in the order they are read.
    pub const FILES: [(&'static str, Kind); 4] = [
        ("channels.json", Kind::Channel),
        ("groups.json", Kind::PrivateChannel),
        ("dms.json", Kind::Dm),
        ("mpims.json", Kind::GroupDm),
    ];

    fn as_str(self) -> &'static str {
        match self {
            Kind::Channel => "channel",
            Kind::PrivateChannel => "private_channel",
            Kind::Dm => "dm",
            Kind::GroupDm => "group_dm",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub name: Option<String>,
    pub kind: Kind,
    pub members: Vec<String>,
}

impl Conversation {
    /// Every conversation in one of the list files.
    pub fn list(v: &Value, kind: Kind) -> Vec<Conversation> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                let id = text(c, "id")?;
                let name = text(c, "name");
                // A channel's folder is its name, so one without a name has no
                // messages to find.
                if kind != Kind::Dm && name.is_none() {
                    return None;
                }
                let members = c
                    .get("members")
                    .and_then(Value::as_array)
                    .map(|m| m.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                Some(Conversation { id, name, kind, members })
            })
            .collect()
    }

    /// The folder holding this conversation's day files: the name for a
    /// channel or group DM, the id for a DM.
    pub fn folder(&self) -> &str {
        match self.kind {
            Kind::Dm => &self.id,
            _ => self.name.as_deref().unwrap_or(&self.id),
        }
    }

    pub fn includes(&self, user: &str) -> bool {
        self.members.iter().any(|m| m == user)
    }

    /// The name stored with each message: the channel's name, or for a DM
    /// the other person's name.
    pub fn label(&self, people: &People, owner: &str) -> Option<String> {
        match self.kind {
            Kind::Dm => self
                .members
                .iter()
                .find(|m| *m != owner)
                .and_then(|m| people.name(m))
                .map(str::to_string),
            _ => self.name.clone(),
        }
    }
}

/// Everything shaping a message needs to know about the export.
pub struct Context<'a> {
    pub people: &'a People,
    /// Channel names by id, for `<#C123>` links that carry no name.
    pub channel_names: &'a HashMap<String, String>,
    pub owner: &'a str,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub message_id: String,
    pub conversation_id: String,
    pub body: String,
    pub from_identifier: String,
    pub from_handle: String,
    pub from_name: Option<String>,
    pub to_identifiers: Value,
    pub is_group_message: bool,
    pub reply_to_message_id: Option<String>,
    pub has_attachments: bool,
    pub occurred_at: DateTime<Utc>,
    pub source_stream_id: String,
    pub metadata: Value,
}

/// Subtypes that carry something a person said. Every other subtype is an
/// event Slack logs in the channel (a join, a topic change, a pin, a
/// deleted-message placeholder) and is passed over. A bot's message is kept
/// only when a person's user id is on it.
const SPOKEN: [&str; 5] = ["file_share", "me_message", "thread_broadcast", "reply_broadcast", "bot_message"];

/// One element of a day file, or `None` when it is not something someone
/// said.
pub fn from_json(v: &Value, conv: &Conversation, label: Option<&str>, cx: &Context) -> Option<Message> {
    if v.get("type").and_then(Value::as_str).is_some_and(|t| t != "message") {
        return None;
    }
    let subtype = text(v, "subtype");
    if subtype.as_deref().is_some_and(|s| !SPOKEN.contains(&s)) {
        return None;
    }
    // A bot posting on its own has no user id, and there is nobody to file
    // the message under.
    let user = text(v, "user")?;
    let ts = text(v, "ts")?;
    // A message that cannot be placed in time is passed over: filing it at
    // import time would put it on the wrong day for good, since a later
    // import never corrects a row.
    let occurred_at = parse_ts(&ts)?;

    let said = readable(v.get("text").and_then(Value::as_str).unwrap_or(""), cx);
    let said = said.trim();
    // File links need a Slack sign-in and expire, so the body names each
    // file the way the Discord writer names attachments and keeps no link.
    let files: Vec<String> = v
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| match text(f, "name").or_else(|| text(f, "title")) {
            Some(name) => format!("[File: {name}]"),
            None => "[File]".to_string(),
        })
        .collect();
    if said.is_empty() && files.is_empty() {
        return None;
    }
    let body = match (said.is_empty(), files.is_empty()) {
        (true, _) => files.join(" "),
        (false, true) => said.to_string(),
        (false, false) => format!("{said} {}", files.join(" ")),
    };

    let profile = v.get("user_profile").unwrap_or(&Value::Null);
    let from_name = cx
        .people
        .name(&user)
        .map(str::to_string)
        .or_else(|| text(profile, "real_name"))
        .or_else(|| text(profile, "display_name"));
    let is_from_me = user == cx.owner;
    let to: Vec<&String> = match conv.kind {
        Kind::Dm | Kind::GroupDm => conv.members.iter().filter(|m| **m != user).collect(),
        Kind::Channel | Kind::PrivateChannel => Vec::new(),
    };
    let thread_ts = text(v, "thread_ts");
    let mut metadata = json!({
        "is_from_me": is_from_me,
        "channel_name": label,
        "conversation_kind": conv.kind.as_str(),
    });
    if let Some(s) = &subtype {
        metadata["subtype"] = json!(s);
    }
    if let Some(t) = &thread_ts {
        metadata["thread_ts"] = json!(t);
    }

    Some(Message {
        source_stream_id: format!("slack:{}:{ts}", conv.id),
        reply_to_message_id: thread_ts.filter(|t| *t != ts),
        message_id: ts,
        conversation_id: conv.id.clone(),
        body,
        from_handle: if is_from_me { String::new() } else { cx.people.handle(&user) },
        from_identifier: user,
        from_name,
        to_identifiers: json!(to),
        is_group_message: conv.kind != Kind::Dm,
        has_attachments: !files.is_empty(),
        occurred_at,
        metadata,
    })
}

/// A Slack `ts` ("1712345678.123456", seconds and microseconds since the
/// epoch) as a UTC time.
pub fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    let ts = ts.trim();
    let (secs, frac) = ts.split_once('.').unwrap_or((ts, ""));
    let secs: i64 = secs.parse().ok()?;
    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let nanos: u32 = if frac.is_empty() { 0 } else { format!("{frac:0<9}").parse().ok()? };
    DateTime::from_timestamp(secs, nanos)
}

/// Whether a file in a conversation folder is a day file, `2024-04-01.json`.
pub fn is_day_file(name: &str) -> bool {
    let Some(date) = name.strip_suffix(".json") else {
        return false;
    };
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
}

/// Slack's message markup as plain text: `<@U123>` becomes `@Nick`,
/// `<#C123|general>` becomes `#general`, `<https://x|label>` becomes
/// `label (https://x)`, and `&amp;`, `&lt;` and `&gt;` are unescaped. Slack
/// escapes every `<` a person types, so a bare `<` always opens a link.
pub fn readable(raw: &str, cx: &Context) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        out.push_str(&unescape(&rest[..start]));
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            break;
        };
        out.push_str(&link(&after[..end], cx));
        rest = &after[end + 1..];
    }
    out.push_str(&unescape(rest));
    out
}

fn link(inner: &str, cx: &Context) -> String {
    let (target, label) = match inner.split_once('|') {
        Some((t, l)) => (t, Some(unescape(l)).filter(|l| !l.is_empty())),
        None => (inner, None),
    };
    if let Some(id) = target.strip_prefix('@') {
        let name = cx
            .people
            .name(id)
            .map(str::to_string)
            .or_else(|| label.map(|l| l.trim_start_matches('@').to_string()))
            .unwrap_or_else(|| id.to_string());
        return format!("@{name}");
    }
    if let Some(id) = target.strip_prefix('#') {
        let name = label.or_else(|| cx.channel_names.get(id).cloned()).unwrap_or_else(|| id.to_string());
        return format!("#{name}");
    }
    if let Some(command) = target.strip_prefix('!') {
        // `<!here>`, `<!channel>` and `<!everyone>` are the group mentions;
        // anything else (`<!subteam^S1|@design>`, `<!date^…|Jan 1>`) carries
        // its own readable text after the bar.
        let word = command.split('^').next().unwrap_or(command);
        return match (word, label) {
            ("here" | "channel" | "everyone", _) => format!("@{word}"),
            (_, Some(l)) => l,
            (_, None) => format!("@{word}"),
        };
    }
    let url = unescape(target);
    let bare = url.split_once(':').map(|(_, r)| r.trim_start_matches('/')).unwrap_or(&url);
    match label {
        // Slack links a typed address or email itself, labelled with what
        // was typed, so the label alone says it.
        Some(l) if l == url || l == bare => l,
        Some(l) => format!("{l} ({url})"),
        None if url.starts_with("mailto:") => bare.to_string(),
        None => url,
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> People {
        People::from_json(&json!([
            {"id": "U01NICK", "name": "nick", "profile": {"real_name": "Nick", "display_name": "nick", "email": "Nick@Example.com"}},
            {"id": "U02DAVID", "name": "david", "real_name": "David Okafor", "profile": {"real_name": "", "email": "david.okafor@example.com"}},
            {"id": "U03SAM", "name": "sam", "deleted": true, "profile": {}},
            {"id": "B01BOT", "name": "deploybot", "is_bot": true, "profile": {"real_name": "Deploy Bot"}}
        ]))
    }

    fn channel_names() -> HashMap<String, String> {
        HashMap::from([("C01LAUNCH".to_string(), "launch-review".to_string())])
    }

    fn channel() -> Conversation {
        Conversation {
            id: "C01LAUNCH".into(),
            name: Some("launch-review".into()),
            kind: Kind::Channel,
            members: vec!["U01NICK".into(), "U02DAVID".into()],
        }
    }

    fn dm() -> Conversation {
        Conversation::list(&json!([{"id": "D01", "members": ["U01NICK", "U02DAVID"]}]), Kind::Dm).remove(0)
    }

    fn shape(v: Value, conv: &Conversation) -> Option<Message> {
        let p = people();
        let names = channel_names();
        let cx = Context { people: &p, channel_names: &names, owner: "U01NICK" };
        let label = conv.label(&p, "U01NICK");
        from_json(&v, conv, label.as_deref(), &cx)
    }

    #[test]
    fn markup_becomes_plain_text() {
        let p = people();
        let names = channel_names();
        let cx = Context { people: &p, channel_names: &names, owner: "U01NICK" };
        let r = |s: &str| readable(s, &cx);
        assert_eq!(r("<@U02DAVID> can you look?"), "@David Okafor can you look?");
        assert_eq!(r("<@U09GONE|gone>"), "@gone");
        assert_eq!(r("see <#C02XYZ|general> and <#C01LAUNCH>"), "see #general and #launch-review");
        assert_eq!(r("<https://example.com/plan|the plan>"), "the plan (https://example.com/plan)");
        assert_eq!(r("<https://example.com/a?b=1&amp;c=2>"), "https://example.com/a?b=1&c=2");
        assert_eq!(r("<http://example.com|example.com>"), "example.com");
        assert_eq!(r("<mailto:nick@example.com|nick@example.com>"), "nick@example.com");
        assert_eq!(r("<!here> <!subteam^S01|@design> ship it"), "@here @design ship it");
        assert_eq!(r("1 &lt; 2 &amp;&amp; 3 &gt; 2"), "1 < 2 && 3 > 2");
        assert_eq!(r("&amp;lt; stays literal"), "&lt; stays literal");
    }

    #[test]
    fn the_owner_is_found_by_email_ignoring_case() {
        let p = people();
        assert_eq!(p.owner(" nick@example.com ").unwrap(), "U01NICK");
        for email in ["", "  ", "someone@example.com"] {
            assert_eq!(p.owner(email).unwrap_err().to_string(), NOT_IN_EXPORT, "{email:?}");
        }
        assert_eq!(p.name("U02DAVID"), Some("David Okafor"), "an empty profile name falls back");
        assert_eq!(p.name("U03SAM"), Some("sam"));
        assert_eq!(p.handle("U01NICK"), "nick@example.com");
        assert_eq!(p.handle("U03SAM"), "");
    }

    #[test]
    fn conversations_list_their_folder_and_members() {
        let channels = Conversation::list(
            &json!([
                {"id": "C01LAUNCH", "name": "launch-review", "members": ["U01NICK", "U02DAVID"]},
                {"id": "C02", "name": "random", "members": ["U02DAVID"]},
                {"id": "C03", "members": ["U01NICK"]}
            ]),
            Kind::Channel,
        );
        assert_eq!(channels.len(), 2, "a channel without a name has no folder");
        let mine: Vec<&str> = channels.iter().filter(|c| c.includes("U01NICK")).map(|c| c.folder()).collect();
        assert_eq!(mine, ["launch-review"]);
        assert_eq!(dm().folder(), "D01");
        assert_eq!(dm().label(&people(), "U01NICK").as_deref(), Some("David Okafor"));
    }

    #[test]
    fn a_channel_message_becomes_a_row() {
        let m = shape(
            json!({"type": "message", "ts": "1712345678.123456", "user": "U02DAVID",
                   "text": "Draft is up, <@U01NICK>", "thread_ts": "1712340000.000100", "parent_user_id": "U01NICK",
                   "files": [{"id": "F1", "name": "launch-plan.pdf", "url_private": "https://files.example.com/F1"}]}),
            &channel(),
        )
        .unwrap();
        assert_eq!(m.message_id, "1712345678.123456");
        assert_eq!(m.source_stream_id, "slack:C01LAUNCH:1712345678.123456");
        assert_eq!(m.conversation_id, "C01LAUNCH");
        assert_eq!(m.occurred_at.to_rfc3339(), "2024-04-05T19:34:38.123456+00:00");
        assert_eq!(m.body, "Draft is up, @Nick [File: launch-plan.pdf]");
        assert!(!m.body.contains("https://"), "no file link is kept");
        assert!(m.has_attachments);
        assert_eq!(m.from_identifier, "U02DAVID");
        assert_eq!(m.from_handle, "david.okafor@example.com");
        assert_eq!(m.from_name.as_deref(), Some("David Okafor"));
        assert_eq!(m.to_identifiers, json!([]));
        assert!(m.is_group_message);
        assert_eq!(m.reply_to_message_id.as_deref(), Some("1712340000.000100"));
        assert_eq!(m.metadata["is_from_me"], false);
        assert_eq!(m.metadata["channel_name"], "launch-review");
        assert_eq!(m.metadata["conversation_kind"], "channel");
        assert_eq!(m.metadata["thread_ts"], "1712340000.000100");
        assert!(m.metadata.get("subtype").is_none());
    }

    #[test]
    fn the_owners_dm_names_the_other_person_and_has_no_handle() {
        let m = shape(json!({"type": "message", "ts": "1712345678.000200", "user": "U01NICK", "text": "Lunch at noon?",
                             "thread_ts": "1712345678.000200"}), &dm())
        .unwrap();
        assert_eq!(m.from_handle, "");
        assert_eq!(m.to_identifiers, json!(["U02DAVID"]));
        assert!(!m.is_group_message);
        assert_eq!(m.reply_to_message_id, None, "a thread's first message replies to nothing");
        assert_eq!(m.metadata["is_from_me"], true);
        assert_eq!(m.metadata["channel_name"], "David Okafor");
        assert_eq!(m.metadata["conversation_kind"], "dm");
    }

    #[test]
    fn events_bots_and_empty_messages_are_passed_over() {
        let at = |subtype: &str, user: Option<&str>| {
            let mut v = json!({"type": "message", "ts": "1712345678.000300", "text": "something", "subtype": subtype});
            if let Some(u) = user {
                v["user"] = json!(u);
            }
            shape(v, &channel())
        };
        for skipped in [
            "channel_join", "channel_leave", "channel_topic", "channel_purpose", "channel_name", "channel_archive",
            "channel_unarchive", "group_join", "group_leave", "pinned_item", "unpinned_item", "bot_add", "bot_remove",
            "tombstone",
        ] {
            assert!(at(skipped, Some("U02DAVID")).is_none(), "{skipped}");
        }
        for kept in ["file_share", "me_message", "thread_broadcast"] {
            let m = at(kept, Some("U02DAVID")).unwrap_or_else(|| panic!("{kept}"));
            assert_eq!(m.metadata["subtype"], kept);
        }
        assert!(at("bot_message", None).is_none());
        assert!(at("bot_message", Some("U02DAVID")).is_some());
        assert!(shape(json!({"type": "message", "ts": "1.0", "bot_id": "B01BOT", "text": "Deployed"}), &channel()).is_none());
        assert!(shape(json!({"type": "message", "ts": "1.0", "user": "U02DAVID", "text": "  "}), &channel()).is_none());
        let file_only =
            shape(json!({"type": "message", "ts": "1.5", "user": "U02DAVID", "text": "", "files": [{"mode": "hidden_by_limit"}]}), &channel())
                .unwrap();
        assert_eq!(file_only.body, "[File]");
    }

    #[test]
    fn ts_and_day_files_parse() {
        assert_eq!(parse_ts("1712345678.5").unwrap().to_rfc3339(), "2024-04-05T19:34:38.500+00:00");
        assert_eq!(parse_ts("1712345678").unwrap().to_rfc3339(), "2024-04-05T19:34:38+00:00");
        assert!(parse_ts("yesterday").is_none());
        assert!(parse_ts("1712345678.12x").is_none());
        assert!(is_day_file("2024-04-01.json"));
        assert!(!is_day_file("canvas_in_the_conversation.json"));
        assert!(!is_day_file("2024-04-01.txt"));
    }
}
