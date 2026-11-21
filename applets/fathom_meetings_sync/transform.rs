//! A Fathom meeting (one item of `GET /external/v1/meetings`) as a
//! [`Meeting`].

use serde_json::Value;
use virtues_applets::meeting::{parse_time, text_at, Attendee, Line, Meeting};

pub const PROVIDER: &str = "fathom";

/// What became of one Fathom item.
#[derive(Debug, PartialEq)]
pub enum Shaped {
    Meeting(Box<Meeting>),
    /// The key also returns meetings shared with the person's team. Neither
    /// the recorder nor any invitee is the person, so it is not theirs.
    NotTheirs,
    /// Fathom has not finished it: there is no transcript and no summary yet.
    NotReady,
    /// No id or no start time.
    Invalid,
}

/// Shape one item for the person whose Fathom address is `email`.
pub fn shape(item: &Value, email: &str) -> Shaped {
    if !is_theirs(item, email) {
        return Shaped::NotTheirs;
    }
    let Some(id) = recording_id(item) else { return Shaped::Invalid };

    let transcript: Vec<Line> = item
        .get("transcript")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(|l| {
                    Some(Line {
                        speaker: text_at(l, "/speaker/display_name").unwrap_or("").to_string(),
                        text: text_at(l, "/text")?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let summary = text_at(item, "/default_summary/markdown_formatted").map(String::from);
    if transcript.is_empty() && summary.is_none() {
        return Shaped::NotReady;
    }

    let time = |a: &str, b: &str| text_at(item, a).or_else(|| text_at(item, b)).and_then(parse_time);
    let Some(started_at) = time("/recording_start_time", "/scheduled_start_time") else {
        return Shaped::Invalid;
    };

    Shaped::Meeting(Box::new(Meeting {
        provider: PROVIDER,
        vendor_id: id,
        title: text_at(item, "/title").or_else(|| text_at(item, "/meeting_title")).map(String::from),
        summary,
        transcript,
        started_at,
        ended_at: time("/recording_end_time", "/scheduled_end_time"),
        attendees: item
            .get("calendar_invitees")
            .and_then(Value::as_array)
            .map(|people| {
                people
                    .iter()
                    .map(|p| Attendee {
                        name: text_at(p, "/name").map(String::from),
                        email: text_at(p, "/email").map(String::from),
                    })
                    .filter(|a| a.name.is_some() || a.email.is_some())
                    .collect()
            })
            .unwrap_or_default(),
        action_items: item
            .get("action_items")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(action_item).collect())
            .unwrap_or_default(),
        url: text_at(item, "/share_url").or_else(|| text_at(item, "/url")).map(String::from),
    }))
}

/// `recording_id`, which Fathom sends as a number.
fn recording_id(item: &Value) -> Option<String> {
    match item.get("recording_id")? {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// The person recorded the meeting or was invited to it.
fn is_theirs(item: &Value, email: &str) -> bool {
    let email = email.trim();
    if email.is_empty() {
        return false;
    }
    let matches = |v: Option<&str>| v.is_some_and(|e| e.eq_ignore_ascii_case(email));
    matches(text_at(item, "/recorded_by/email"))
        || item
            .get("calendar_invitees")
            .and_then(Value::as_array)
            .is_some_and(|people| people.iter().any(|p| matches(text_at(p, "/email"))))
}

/// "Nick: send the deck" when the item has an assignee, else the description.
fn action_item(item: &Value) -> Option<String> {
    let description = text_at(item, "/description")?;
    Some(match text_at(item, "/assignee/name") {
        Some(name) => format!("{name}: {description}"),
        None => description.to_string(),
    })
}

/// When Fathom made the item: the cursor the next run reads from.
pub fn created_at(item: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    text_at(item, "/created_at").and_then(parse_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use virtues_applets::meeting::shape as row;

    fn item() -> Value {
        json!({
            "title": "Launch review",
            "meeting_title": "Weekly sync",
            "recording_id": 123456,
            "url": "https://fathom.example.com/calls/123456",
            "share_url": "https://fathom.example.com/share/abc",
            "created_at": "2026-11-18T15:40:00Z",
            "scheduled_start_time": "2026-11-18T15:00:00Z",
            "scheduled_end_time": "2026-11-18T15:30:00Z",
            "recording_start_time": "2026-11-18T15:02:00Z",
            "recording_end_time": "2026-11-18T15:32:00Z",
            "calendar_invitees": [
                { "name": "Nick", "email": "nick@example.com", "is_external": false },
                { "name": "David Okafor", "email": "david@example.com", "is_external": true }
            ],
            "recorded_by": { "name": "Nick", "email": "Nick@Example.com", "team": null },
            "transcript": [
                { "speaker": { "display_name": "Nick", "matched_calendar_invitee_email": "nick@example.com" }, "text": "Shall we start?", "timestamp": "00:00:04" },
                { "speaker": { "display_name": "David Okafor", "matched_calendar_invitee_email": null }, "text": "Yes, the numbers are in.", "timestamp": "00:00:07" },
                { "speaker": { "display_name": "David Okafor" }, "text": "Up on last week.", "timestamp": "00:00:10" }
            ],
            "default_summary": { "template_name": "General", "markdown_formatted": "## Summary\nShip on Friday." },
            "action_items": [
                { "description": "Send the deck", "assignee": { "name": "Nick", "email": "nick@example.com", "team": null }, "completed": false },
                { "description": "Book the room", "assignee": { "name": null, "email": null, "team": null } }
            ]
        })
    }

    fn meeting(v: &Value) -> Meeting {
        match shape(v, "nick@example.com") {
            Shaped::Meeting(m) => *m,
            other => panic!("expected a meeting, got {other:?}"),
        }
    }

    #[test]
    fn a_fathom_item_becomes_a_meeting() {
        let m = meeting(&item());
        assert_eq!(m.vendor_id, "123456");
        assert_eq!(m.title.as_deref(), Some("Launch review"));
        assert_eq!(m.summary.as_deref(), Some("## Summary\nShip on Friday."));
        assert_eq!(m.started_at, "2026-11-18T15:02:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap());
        assert_eq!(m.ended_at, Some("2026-11-18T15:32:00Z".parse().unwrap()));
        assert_eq!(m.url.as_deref(), Some("https://fathom.example.com/share/abc"));
        assert_eq!(m.action_items, vec!["Nick: Send the deck", "Book the room"]);
        assert_eq!(m.attendees.len(), 2);
        assert_eq!(m.attendees[1].email.as_deref(), Some("david@example.com"));

        let r = row(&m).unwrap();
        assert_eq!(r.source_stream_id, "fathom:123456");
        assert_eq!(r.text, "[Nick]: Shall we start?\n[David Okafor]: Yes, the numbers are in. Up on last week.");
        assert_eq!(r.speaker_count, Some(2));
    }

    #[test]
    fn scheduled_times_and_the_meeting_title_fill_in() {
        let mut v = item();
        for k in ["title", "recording_start_time", "recording_end_time", "share_url"] {
            v[k] = Value::Null;
        }
        let m = meeting(&v);
        assert_eq!(m.title.as_deref(), Some("Weekly sync"));
        assert_eq!(m.started_at, "2026-11-18T15:00:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap());
        assert_eq!(m.ended_at, Some("2026-11-18T15:30:00Z".parse().unwrap()));
        assert_eq!(m.url.as_deref(), Some("https://fathom.example.com/calls/123456"));
    }

    #[test]
    fn only_the_persons_own_meetings_are_kept() {
        // Recorded by them (any case), or they were invited.
        assert!(matches!(shape(&item(), "NICK@example.com"), Shaped::Meeting(_)));
        assert!(matches!(shape(&item(), "david@example.com"), Shaped::Meeting(_)));
        assert_eq!(shape(&item(), "ana@example.com"), Shaped::NotTheirs);
        assert_eq!(shape(&item(), ""), Shaped::NotTheirs);
    }

    #[test]
    fn a_meeting_without_a_transcript_or_summary_is_not_ready() {
        let mut v = item();
        v["transcript"] = Value::Null;
        v["default_summary"] = Value::Null;
        assert_eq!(shape(&v, "nick@example.com"), Shaped::NotReady);

        // A summary alone is enough.
        v["default_summary"] = json!({ "markdown_formatted": "Ship on Friday." });
        let m = meeting(&v);
        assert_eq!(row(&m).unwrap().text, "Ship on Friday.");
    }
}
