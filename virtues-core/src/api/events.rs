//! `POST /api/events` — the one door a client reports through.
//!
//! # Why this exists
//!
//! The web app had 135 bare `console.*` calls and no way to reach the box with
//! any of them. So a phone that failed to pair, a Mac window that threw on
//! load, a request that died in the client after the server thought it had
//! answered — none of that existed anywhere the owner or we could see it. The
//! box's journal was a complete account of the box and a blank page about
//! every device talking to it.
//!
//! This makes a client failure a box fact. Events arrive here, get stamped
//! with the device the session belongs to, and are re-emitted into the box's
//! own journal, where they sit beside the server-side lines from the same
//! moment — and, when the client passes back the `x-request-id` it was handed,
//! on the same key.
//!
//! That last clause described an intention for one commit before it described
//! the code: the header was returned and nothing on the client ever read it.
//! It is wired now — `ApiError` carries the exact id of the request that
//! failed, the client keeps the last one it saw as a fallback for errors with
//! no request of their own, and a report says which of the two it is holding.
//! A hint that looked like a fact would be worse than no id at all, because it
//! would produce a confident join to the wrong request.
//!
//! # What this is NOT
//!
//! Not analytics, and not a second telemetry channel: nothing here leaves the
//! box. Not a log store either — these become journal lines, which journald
//! already rotates and bounds. Deliberately no table: see the deferred section
//! of `agents/record/observability.md` for what would justify one.
//!
//! # Trust
//!
//! The body is written by a client, so it is data, never authority:
//!
//! - `source` is stamped from the authenticated session's `device_id` and can
//!   NOT be set by the body. A device may not report as another device.
//! - `kind` and `message` are bounded and stripped of control characters
//!   before they reach a log line. A newline in a log field forges a second
//!   line, and `kind` is the field a reader filters on.
//! - `detail` is capped by serialized size. A client cannot spend the box's
//!   disk one 5 MB stack trace at a time.
//! - The batch is capped, and each device gets a bounded budget per window,
//!   so a client stuck in a render loop cannot push journald into rate-limiting
//!   the unit and dropping OTHER lines — which would make this feature a cause
//!   of blindness rather than a cure for it.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// No `State` extractor: this handler touches no database. `AuthUser` pulls the
// pool out of the router state itself, so taking one here would be an
// extractor that does nothing but look load-bearing.
use crate::middleware::auth::AuthUser;

/// Most events accepted in one request. A client batches; a client that has
/// more than this to say in one flush is looping, and the tail is not the
/// interesting part.
const MAX_BATCH: usize = 50;

/// Longest accepted `kind` and `message`. Generous for real values
/// (`chat.stream.failed`, a browser's error string) and far below anything
/// that would make a journal line unreadable.
const MAX_KIND: usize = 64;
const MAX_MESSAGE: usize = 512;

/// Cap on `detail` once serialized. Four kilobytes holds a stack trace worth
/// having; past that a client is shipping a document, not a diagnostic.
const MAX_DETAIL_BYTES: usize = 4 * 1024;

/// One reported event, as the client sends it.
#[derive(Debug, Deserialize)]
pub struct ClientEvent {
    /// Dotted, noun first: `chat.stream.failed`, `pair.ble.timeout`. Free text
    /// by design — the box does not keep a registry of client event names, and
    /// a client that invents one is describing something we had not thought of
    /// rather than making an error.
    pub kind: String,
    /// `info` | `warn` | `error`. Anything else is treated as `warn`, on the
    /// principle that an unparseable severity is itself mildly wrong.
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub message: String,
    /// Anything else the client thinks matters. Never parsed here.
    #[serde(default)]
    pub detail: Option<Value>,
    /// The box's own `x-request-id` for the request this is about, when the
    /// client knows it exactly (an API error carries the id of its own
    /// request). This is the join: the box already stamps every request with
    /// this id and every server line it logs inherits it, so a client report
    /// carrying it lands on the same key as the server's own account of the
    /// same failure.
    #[serde(default)]
    pub request_id: Option<String>,
    /// The most recent id the client saw, when it does not know the exact one
    /// — an uncaught error has no request of its own. A HINT, and named so it
    /// cannot be mistaken for the real thing when read back.
    #[serde(default)]
    pub last_request_id: Option<String>,
    /// When it happened on the client. Advisory only: a client's clock is its
    /// own, and the journal timestamps arrival regardless. Carried into the
    /// line so a delayed flush is distinguishable from a burst.
    #[serde(default)]
    pub occurred_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventBatch {
    pub events: Vec<ClientEvent>,
}

#[derive(Debug, Serialize)]
pub struct EventAck {
    /// How many were emitted. Less than what was sent means the batch was
    /// truncated — the client is not asked to do anything about it, but a
    /// human reading a response in devtools should not have to guess.
    pub accepted: usize,
}

/// `POST /api/events`
pub async fn report_handler(user: AuthUser, Json(batch): Json<EventBatch>) -> impl IntoResponse {
    if !crate::middleware::rate_limit::event_limiter().check_and_record(&user.device_id) {
        // 429 with no body: the client's queue drops these on any non-2xx, and
        // a client that is over budget is by definition not in a state where
        // reading an explanation helps. The box says so once, here, so the
        // silence is visible to us rather than to nobody.
        tracing::warn!(
            kind = "client.report.throttled",
            source = %format!("device:{}", user.device_id),
            "a device is over its event budget; dropping its reports for this window"
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(EventAck { accepted: 0 }),
        );
    }

    let source = format!("device:{}", user.device_id);
    let prepared = prepare(batch.events);

    for ev in &prepared {
        // The level has to be a literal at each call site, so this is a match
        // rather than a variable. `device_label` rides along because a reader
        // chasing "the phone" should not have to resolve an id by hand.
        match ev.severity {
            Severity::Error => tracing::error!(
                kind = %ev.kind, source = %source, device = %user.device_label,
                request_id = %ev.request_id, request_id_is_hint = ev.request_id_is_hint,
                occurred_at = %ev.occurred_at, detail = %ev.detail, "{}", ev.message
            ),
            Severity::Info => tracing::info!(
                kind = %ev.kind, source = %source, device = %user.device_label,
                request_id = %ev.request_id, request_id_is_hint = ev.request_id_is_hint,
                occurred_at = %ev.occurred_at, detail = %ev.detail, "{}", ev.message
            ),
            Severity::Warn => tracing::warn!(
                kind = %ev.kind, source = %source, device = %user.device_label,
                request_id = %ev.request_id, request_id_is_hint = ev.request_id_is_hint,
                occurred_at = %ev.occurred_at, detail = %ev.detail, "{}", ev.message
            ),
        }
    }

    (
        StatusCode::OK,
        Json(EventAck {
            accepted: prepared.len(),
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Info,
    Warn,
    Error,
}

/// One event, cleaned and ready to be a log line.
#[derive(Debug)]
struct PreparedEvent {
    kind: String,
    severity: Severity,
    message: String,
    detail: String,
    /// The request this is about, and whether the client knew it exactly.
    request_id: String,
    request_id_is_hint: bool,
    occurred_at: String,
}

/// Everything the handler does to a batch except emit it: cap, sanitize, drop
/// the empties.
///
/// Split out so the tests exercise the code that actually ships. A test that
/// re-implements this loop next to a router would pass forever while the real
/// handler drifted — which is the failure mode of "the test builds its own
/// version of the thing", and worth the small indirection to avoid.
fn prepare(events: Vec<ClientEvent>) -> Vec<PreparedEvent> {
    events
        .into_iter()
        .take(MAX_BATCH)
        .filter_map(|ev| {
            let kind = sanitize(&ev.kind, MAX_KIND);
            let message = sanitize(&ev.message, MAX_MESSAGE);
            // A client with nothing to say says nothing.
            if kind.is_empty() && message.is_empty() {
                return None;
            }
            Some(PreparedEvent {
                kind: if kind.is_empty() {
                    "client.unspecified".to_string()
                } else {
                    kind
                },
                severity: match ev.severity.trim().to_ascii_lowercase().as_str() {
                    "error" => Severity::Error,
                    "info" => Severity::Info,
                    _ => Severity::Warn,
                },
                message,
                detail: bound_detail(ev.detail),
                // An exact id always wins over the "last one I saw" hint, and
                // the flag travels with it: a reader who joins on a hint and
                // gets the wrong request should be able to see that they might
                // have, rather than trusting a key that was never a promise.
                request_id: ev
                    .request_id
                    .as_deref()
                    .or(ev.last_request_id.as_deref())
                    .map(|s| sanitize(s, 64))
                    .unwrap_or_default(),
                request_id_is_hint: ev.request_id.is_none() && ev.last_request_id.is_some(),
                occurred_at: ev
                    .occurred_at
                    .as_deref()
                    .map(|s| sanitize(s, 40))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Collapse a client string into something safe to put in a log field:
/// control characters out (a newline forges a log line), bounded length,
/// trimmed.
fn sanitize(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// Serialize `detail` and bound it. Oversized detail is replaced rather than
/// truncated mid-JSON: half an object in a log line is worse than an honest
/// note that it was too big, because the first looks like data.
fn bound_detail(detail: Option<Value>) -> String {
    let Some(v) = detail else {
        return String::new();
    };
    let s = v.to_string();
    if s.len() > MAX_DETAIL_BYTES {
        format!("<detail dropped: {} bytes, cap {MAX_DETAIL_BYTES}>", s.len())
    } else {
        // Control characters can live inside JSON string values; the whole
        // serialized blob goes into one field, so clean it the same way.
        s.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn control_characters_never_reach_a_log_field() {
        // The attack this exists for: a client value that closes the line and
        // opens a forged one.
        let forged = sanitize("boom\n2026-01-01 ERROR everything is fine", MAX_MESSAGE);
        assert!(!forged.contains('\n'));
        assert_eq!(forged, "boom 2026-01-01 ERROR everything is fine");
    }

    #[test]
    fn long_values_are_bounded() {
        assert_eq!(sanitize(&"k".repeat(1000), MAX_KIND).len(), MAX_KIND);
        assert_eq!(sanitize("  padded  ", MAX_KIND), "padded");
    }

    #[test]
    fn oversized_detail_is_replaced_not_truncated() {
        let big = json!({ "stack": "x".repeat(MAX_DETAIL_BYTES + 1) });
        let out = bound_detail(Some(big));
        assert!(out.starts_with("<detail dropped:"), "got: {out}");

        // A reasonable one survives intact and stays parseable.
        let small = json!({ "url": "/chat", "status": 500 });
        let out = bound_detail(Some(small));
        let back: Value = serde_json::from_str(&out).expect("still JSON");
        assert_eq!(back["status"], 500);
    }

    #[test]
    fn absent_detail_is_empty_not_the_string_null() {
        // `Value::Null.to_string()` is "null", which reads in a log line as a
        // client that sent the word null — which has happened elsewhere in
        // this codebase and cost an afternoon.
        assert_eq!(bound_detail(None), "");
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    fn ev(kind: &str, severity: &str, message: &str) -> ClientEvent {
        ClientEvent {
            kind: kind.into(),
            severity: severity.into(),
            message: message.into(),
            detail: None,
            request_id: None,
            last_request_id: None,
            occurred_at: None,
        }
    }

    #[test]
    fn severity_parses_and_anything_unknown_becomes_warn() {
        let out = prepare(vec![
            ev("client.a", "error", "m"),
            ev("client.b", "INFO", "m"),
            ev("client.c", "chatty", "m"),
            ev("client.d", "", "m"),
        ]);
        let got: Vec<_> = out.iter().map(|e| e.severity).collect();
        assert_eq!(
            got,
            vec![Severity::Error, Severity::Info, Severity::Warn, Severity::Warn]
        );
    }

    #[test]
    fn a_flood_is_truncated_not_refused() {
        // A client stuck in a render loop should not also get an error to
        // report about its reporting.
        let many: Vec<_> = (0..500).map(|_| ev("client.loop", "warn", "again")).collect();
        assert_eq!(prepare(many).len(), MAX_BATCH);
    }

    #[test]
    fn empty_events_are_skipped_and_a_missing_kind_still_reports() {
        assert_eq!(prepare(vec![ev("", "warn", "   ")]).len(), 0);
        let out = prepare(vec![ev("", "warn", "something broke")]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "client.unspecified");
    }

    #[test]
    fn an_exact_request_id_beats_the_hint_and_is_marked_as_such() {
        // The join between a client report and the server's own account of the
        // same failure. A hint that silently looked like a fact would be worse
        // than no id: it would produce a confident, wrong answer.
        let mut exact = ev("client.api", "error", "500 from the box");
        exact.request_id = Some("r7_123".into());
        exact.last_request_id = Some("r9_999".into());

        let mut hint = ev("client.uncaught", "error", "boom");
        hint.last_request_id = Some("r9_999".into());

        let out = prepare(vec![exact, hint]);
        assert_eq!(out[0].request_id, "r7_123");
        assert!(!out[0].request_id_is_hint);
        assert_eq!(out[1].request_id, "r9_999");
        assert!(out[1].request_id_is_hint, "a guess must say it is one");
    }

    #[test]
    fn a_forged_log_line_cannot_survive_the_batch() {
        let out = prepare(vec![ev(
            "client.x",
            "warn",
            "ok\n2026-01-01T00:00:00Z ERROR everything is fine",
        )]);
        assert!(!out[0].message.contains('\n'));
    }
}
