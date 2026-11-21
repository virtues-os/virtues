//! A Granola note (`GET /v1/notes/{id}?include=transcript`) as a [`Meeting`].

use serde_json::Value;
use virtues_applets::meeting::{parse_time, text_at, Attendee, Line, Meeting};

pub const PROVIDER: &str = "granola";

/// Fields of a note this server never stores. Granola returns the person's
/// private notes to a key they made themselves.
pub const PRIVATE_FIELDS: [&str; 2] = ["private_notes_text", "private_notes_markdown"];

/// What became of one Granola note.
#[derive(Debug, PartialEq)]
pub enum Shaped {
    Meeting(Box<Meeting>),
    /// No id, or no time to place it at.
    Invalid,
}

pub fn shape(note: &Value) -> Shaped {
    let Some(id) = text_at(note, "/id") else { return Shaped::Invalid };

    // A null transcript means a workspace admin turned transcripts off for
    // the API. The writer then stores the summary as the text.
    let items: &[Value] = note.get("transcript").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    let transcript: Vec<Line> = items
        .iter()
        .filter_map(|l| Some(Line { speaker: speaker(l).to_string(), text: text_at(l, "/text")?.to_string() }))
        .collect();

    let first_start = items.iter().find_map(|l| text_at(l, "/start_time").and_then(parse_time));
    let last_end = items.iter().rev().find_map(|l| text_at(l, "/end_time").and_then(parse_time));
    let Some(started_at) = first_start
        .or_else(|| text_at(note, "/calendar_event/scheduled_start_time").and_then(parse_time))
        .or_else(|| text_at(note, "/created_at").and_then(parse_time))
    else {
        return Shaped::Invalid;
    };
    let ended_at = last_end.or_else(|| text_at(note, "/calendar_event/scheduled_end_time").and_then(parse_time));

    Shaped::Meeting(Box::new(Meeting {
        provider: PROVIDER,
        vendor_id: id.to_string(),
        title: text_at(note, "/title").map(String::from),
        summary: text_at(note, "/summary_markdown").or_else(|| text_at(note, "/summary_text")).map(String::from),
        transcript,
        started_at,
        ended_at,
        attendees: note
            .get("attendees")
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
        // Granola has no action-item field; they are inside the summary.
        action_items: Vec::new(),
        url: text_at(note, "/web_url").map(String::from),
    }))
}

/// Who said a transcript line: the name Granola matched, else "You" for the
/// note's owner, else Granola's anonymous label ("Speaker A") from a phone
/// recording, else "Speaker".
fn speaker(line: &Value) -> &str {
    if let Some(name) = text_at(line, "/speaker/name") {
        return name;
    }
    if text_at(line, "/speaker/attribution") == Some("me") {
        return "You";
    }
    text_at(line, "/speaker/diarization_label").unwrap_or("Speaker")
}

/// When the note last changed: the cursor the next run reads from.
pub fn updated_at(note: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    text_at(note, "/updated_at").and_then(parse_time)
}

/// The note without the fields this server never stores.
pub fn without_private_notes(mut note: Value) -> Value {
    if let Some(o) = note.as_object_mut() {
        for k in PRIVATE_FIELDS {
            o.remove(k);
        }
    }
    note
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use virtues_applets::meeting::shape as row;

    fn note() -> Value {
        json!({
            "id": "not_a1b2c3d4e5f6g7",
            "object": "note",
            "title": "Launch review",
            "owner": { "name": "Nick", "email": "nick@example.com" },
            "created_at": "2026-11-18T15:35:00Z",
            "updated_at": "2026-11-18T15:40:00Z",
            "deleted_at": null,
            "web_url": "https://notes.granola.example.com/d/a1b2",
            "calendar_event": {
                "event_title": "Launch review",
                "scheduled_start_time": "2026-11-18T15:00:00Z",
                "scheduled_end_time": "2026-11-18T15:30:00Z"
            },
            "attendees": [
                { "name": "Nick", "email": "nick@example.com" },
                { "name": "David Okafor", "email": "david@example.com" }
            ],
            "summary_text": "Ship on Friday.",
            "summary_markdown": "### Decisions\n- Ship on Friday.",
            "private_notes_text": "invented private note",
            "private_notes_markdown": "invented private note",
            "transcript": [
                { "speaker": { "source": "microphone", "attribution": "me" }, "text": "Shall we start?",
                  "start_time": "2026-11-18T15:01:00Z", "end_time": "2026-11-18T15:01:03Z" },
                { "speaker": { "source": "speaker", "attribution": "them", "name": "David Okafor" }, "text": "Yes.",
                  "start_time": "2026-11-18T15:01:04Z", "end_time": "2026-11-18T15:01:05Z" },
                { "speaker": { "source": "speaker", "attribution": "them" }, "text": "Can you hear me?",
                  "start_time": "2026-11-18T15:01:06Z", "end_time": "2026-11-18T15:01:09Z" },
                { "speaker": { "source": "microphone", "diarization_label": "Speaker A" }, "text": "Loud and clear.",
                  "start_time": "2026-11-18T15:28:00Z", "end_time": "2026-11-18T15:28:02Z" }
            ]
        })
    }

    fn meeting(v: &Value) -> Meeting {
        match shape(v) {
            Shaped::Meeting(m) => *m,
            other => panic!("expected a meeting, got {other:?}"),
        }
    }

    #[test]
    fn a_granola_note_becomes_a_meeting() {
        let m = meeting(&note());
        assert_eq!(m.vendor_id, "not_a1b2c3d4e5f6g7");
        assert_eq!(m.title.as_deref(), Some("Launch review"));
        assert_eq!(m.summary.as_deref(), Some("### Decisions\n- Ship on Friday."));
        assert_eq!(m.url.as_deref(), Some("https://notes.granola.example.com/d/a1b2"));
        assert_eq!(m.started_at, "2026-11-18T15:01:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(m.ended_at, Some("2026-11-18T15:28:02Z".parse().unwrap()));
        assert_eq!(m.attendees.len(), 2);
        assert!(m.action_items.is_empty());

        let r = row(&m).unwrap();
        assert_eq!(r.source_stream_id, "granola:not_a1b2c3d4e5f6g7");
        assert_eq!(
            r.text,
            "[You]: Shall we start?\n[David Okafor]: Yes.\n[Speaker]: Can you hear me?\n[Speaker A]: Loud and clear."
        );
        assert!(!r.metadata.to_string().contains("private"), "private notes are never stored");
    }

    #[test]
    fn without_a_transcript_the_summary_and_the_calendar_times_are_used() {
        let mut v = note();
        v["transcript"] = Value::Null;
        v["summary_markdown"] = Value::Null;
        let m = meeting(&v);
        assert_eq!(m.started_at, "2026-11-18T15:00:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(m.ended_at, Some("2026-11-18T15:30:00Z".parse().unwrap()));
        let r = row(&m).unwrap();
        assert_eq!(r.text, "Ship on Friday.");
        assert_eq!(r.speaker_count, None);

        // No calendar event: the note's creation time, and no end.
        v["calendar_event"] = Value::Null;
        let m = meeting(&v);
        assert_eq!(m.started_at, "2026-11-18T15:35:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(m.ended_at, None);
    }

    #[test]
    fn private_notes_are_dropped_before_anything_is_kept() {
        let kept = without_private_notes(note());
        assert!(kept.get("private_notes_text").is_none());
        assert!(kept.get("private_notes_markdown").is_none());
        assert_eq!(kept["title"], "Launch review");
    }

    #[test]
    fn a_note_without_an_id_is_invalid() {
        let mut v = note();
        v["id"] = Value::Null;
        assert_eq!(shape(&v), Shaped::Invalid);
    }
}
