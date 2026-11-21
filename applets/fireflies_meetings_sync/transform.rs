//! A Fireflies.ai transcript (one item of the GraphQL `transcripts` query) as
//! a [`Meeting`].

use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues_applets::meeting::{parse_time, text_at, Attendee, Line, Meeting};

pub const PROVIDER: &str = "fireflies";

/// What became of one Fireflies transcript.
#[derive(Debug, PartialEq)]
pub enum Shaped {
    Meeting(Box<Meeting>),
    /// Still live, or Fireflies is still processing it.
    NotReady,
    /// No id or no date.
    Invalid,
}

pub fn shape(t: &Value) -> Shaped {
    let Some(id) = text_at(t, "/id") else { return Shaped::Invalid };
    let Some(started_at) = date(t) else { return Shaped::Invalid };

    let transcript: Vec<Line> = t
        .get("sentences")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(|l| {
                    Some(Line {
                        speaker: text_at(l, "/speaker_name").unwrap_or("").to_string(),
                        text: text_at(l, "/text")?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !is_ready(t, !transcript.is_empty()) {
        return Shaped::NotReady;
    }

    let ended_at = t
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|m| *m > 0.0)
        .map(|minutes| started_at + chrono::Duration::milliseconds((minutes * 60_000.0) as i64));

    Shaped::Meeting(Box::new(Meeting {
        provider: PROVIDER,
        vendor_id: id.to_string(),
        title: text_at(t, "/title").map(String::from),
        summary: text_at(t, "/summary/overview").or_else(|| text_at(t, "/summary/short_summary")).map(String::from),
        transcript,
        started_at,
        ended_at,
        attendees: t
            .get("meeting_attendees")
            .and_then(Value::as_array)
            .map(|people| {
                people
                    .iter()
                    .map(|p| Attendee {
                        name: text_at(p, "/name").or_else(|| text_at(p, "/displayName")).map(String::from),
                        email: text_at(p, "/email").map(String::from),
                    })
                    .filter(|a| a.name.is_some() || a.email.is_some())
                    .collect()
            })
            .unwrap_or_default(),
        action_items: text_at(t, "/summary/action_items").map(action_items).unwrap_or_default(),
        url: text_at(t, "/transcript_url").map(String::from),
    }))
}

/// `dateString`: when the meeting started, and the cursor the next run reads
/// from.
pub fn date(t: &Value) -> Option<DateTime<Utc>> {
    text_at(t, "/dateString").and_then(parse_time)
}

/// Fireflies has finished with a transcript. A live meeting's sentences are
/// rough captions, and `processing` means the transcript and summary are not
/// final. A `failed` or `skipped` summary will not change, so the sentences
/// are stored as they are. A transcript with no `meeting_info` counts as done
/// when it has sentences.
fn is_ready(t: &Value, has_sentences: bool) -> bool {
    if t.get("is_live").and_then(Value::as_bool).unwrap_or(false) {
        return false;
    }
    match t.pointer("/meeting_info/summary_status").and_then(Value::as_str) {
        Some("processed") | Some("skipped") => true,
        Some("failed") => has_sentences,
        Some(_) => false,
        None => has_sentences,
    }
}

/// Fireflies sends action items as one block of text: one item per line,
/// often under a `**Name**` line that names who the items below it are for.
/// Each item keeps that name as a prefix, "Nick: send the deck", the way the
/// Fathom applet writes an assignee.
pub fn action_items(block: &str) -> Vec<String> {
    let mut owner: Option<String> = None;
    let mut items = Vec::new();
    for raw in block.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("**").and_then(|l| l.strip_suffix("**")) {
            let name = name.trim().trim_end_matches(':').trim();
            owner = (!name.is_empty()).then(|| name.to_string());
            continue;
        }
        let item = line.trim_start_matches(|c: char| matches!(c, '-' | '*' | '•' | '–' | '—') || c.is_whitespace());
        if item.is_empty() {
            continue;
        }
        items.push(match &owner {
            Some(name) => format!("{name}: {item}"),
            None => item.to_string(),
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use virtues_applets::meeting::shape as row;

    fn transcript() -> Value {
        json!({
            "id": "01JEXAMPLE",
            "title": "Launch review",
            "dateString": "2026-11-18T15:00:00.000Z",
            "duration": 31.5,
            "transcript_url": "https://app.fireflies.example.com/view/01JEXAMPLE",
            "is_live": false,
            "meeting_attendees": [
                { "displayName": "Nick", "email": "nick@example.com", "name": null },
                { "displayName": "D. Okafor", "email": "david@example.com", "name": "David Okafor" }
            ],
            "summary": {
                "overview": "The team agreed to ship on Friday.",
                "short_summary": "Ship Friday.",
                "action_items": "**Nick**\nSend the deck (00:04)\n- Book the room\n\n**David Okafor**\n• Check the numbers\n"
            },
            "sentences": [
                { "text": "Shall we start?", "speaker_name": "Nick" },
                { "text": "Yes.", "speaker_name": "David Okafor" },
                { "text": "The numbers are in.", "speaker_name": "David Okafor" }
            ],
            "meeting_info": { "summary_status": "processed" }
        })
    }

    fn meeting(v: &Value) -> Meeting {
        match shape(v) {
            Shaped::Meeting(m) => *m,
            other => panic!("expected a meeting, got {other:?}"),
        }
    }

    #[test]
    fn a_fireflies_transcript_becomes_a_meeting() {
        let m = meeting(&transcript());
        assert_eq!(m.vendor_id, "01JEXAMPLE");
        assert_eq!(m.title.as_deref(), Some("Launch review"));
        assert_eq!(m.summary.as_deref(), Some("The team agreed to ship on Friday."));
        assert_eq!(m.started_at, "2026-11-18T15:00:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(m.ended_at, Some("2026-11-18T15:31:30Z".parse().unwrap()));
        assert_eq!(m.url.as_deref(), Some("https://app.fireflies.example.com/view/01JEXAMPLE"));
        assert_eq!(m.attendees[0].name.as_deref(), Some("Nick"));
        assert_eq!(m.attendees[1].name.as_deref(), Some("David Okafor"));
        assert_eq!(
            m.action_items,
            vec!["Nick: Send the deck (00:04)", "Nick: Book the room", "David Okafor: Check the numbers"]
        );

        let r = row(&m).unwrap();
        assert_eq!(r.source_stream_id, "fireflies:01JEXAMPLE");
        assert_eq!(r.text, "[Nick]: Shall we start?\n[David Okafor]: Yes. The numbers are in.");
        assert_eq!(r.duration_seconds, Some(1890.0));
    }

    #[test]
    fn the_short_summary_fills_in_and_plain_action_items_have_no_owner() {
        let mut v = transcript();
        v["summary"]["overview"] = Value::Null;
        v["summary"]["action_items"] = json!("- Send the deck\n* Book the room");
        let m = meeting(&v);
        assert_eq!(m.summary.as_deref(), Some("Ship Friday."));
        assert_eq!(m.action_items, vec!["Send the deck", "Book the room"]);
    }

    #[test]
    fn live_and_processing_transcripts_are_not_ready() {
        let mut live = transcript();
        live["is_live"] = json!(true);
        assert_eq!(shape(&live), Shaped::NotReady);

        let mut processing = transcript();
        processing["meeting_info"]["summary_status"] = json!("processing");
        assert_eq!(shape(&processing), Shaped::NotReady);

        // Skipped or failed summaries will not change; the sentences are kept.
        for status in ["skipped", "failed"] {
            let mut v = transcript();
            v["meeting_info"]["summary_status"] = json!(status);
            assert!(matches!(shape(&v), Shaped::Meeting(_)), "{status}");
        }
        let mut failed_empty = transcript();
        failed_empty["meeting_info"]["summary_status"] = json!("failed");
        failed_empty["sentences"] = json!([]);
        assert_eq!(shape(&failed_empty), Shaped::NotReady);

        // No status at all: done when there are sentences.
        let mut unknown = transcript();
        unknown["meeting_info"] = Value::Null;
        assert!(matches!(shape(&unknown), Shaped::Meeting(_)));
        unknown["sentences"] = Value::Null;
        assert_eq!(shape(&unknown), Shaped::NotReady);
    }

    #[test]
    fn a_transcript_without_an_id_or_date_is_invalid() {
        let mut v = transcript();
        v["dateString"] = Value::Null;
        assert_eq!(shape(&v), Shaped::Invalid);
        let mut v = transcript();
        v["id"] = Value::Null;
        assert_eq!(shape(&v), Shaped::Invalid);
    }
}
