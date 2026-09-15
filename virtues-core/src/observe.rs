//! The observability vocabulary: one spelling of every field, one subscriber,
//! and the spans that put a key on every line underneath them.
//!
//! # Why this exists
//!
//! The box had five tracing setups and four unrelated ledgers, and 535 log
//! call sites between them without a single `#[instrument]`. Every piece was
//! honest and none could name another: a failed applet run could not find its
//! own journal lines, because nothing on those lines said which run they
//! belonged to. Grep by time and hope was the whole story.
//!
//! So the point here is not "structured logging" as a style. It is that a
//! **key** — a run, a request, a chat turn — is stamped once, at the entry
//! point, and every line underneath inherits it. Then one `jq` filter is a
//! complete answer instead of the beginning of a search.
//!
//! # The vocabulary
//!
//! Field names are spelled here and nowhere else. Tracing's macros need
//! literal identifiers, so these cannot be `const`s used at the call site —
//! they are enforced by the span constructors below, which are the only
//! sanctioned way to open one of the five entry-point spans. If you find
//! yourself writing `run_id = …` by hand, use [`run_span`] instead.
//!
//! | Field | Meaning |
//! |---|---|
//! | `kind` | what happened, dotted, noun first: `box.crashed`, `applet.run.failed` |
//! | `source` | which process or device produced it: `box`, `applet:<id>`, `device:<id>` |
//! | `request_id` | one HTTP request; also returned as the `x-request-id` header |
//! | `run_id` | one applet run (`app_applet_runs.id`) |
//! | `applet_id` | which applet the run belongs to |
//! | `chat_id` / `turn_id` | a chat and one turn within it |
//! | `device_id` | the paired device acting |
//!
//! `severity` is not a field: it is the tracing level, and a second scale
//! beside it would immediately disagree with the first.
//!
//! # Format
//!
//! Text when stderr is a terminal, JSON when it is not. That is the whole
//! rule, and it falls out right in both places we care about: a developer
//! running `make dev` reads prose, and a box under systemd writes JSON into
//! journald, where `journalctl -o json` plus `jq` can filter on the keys
//! above. `VIRTUES_LOG_FORMAT=text|json` forces one when the guess is wrong
//! (piping dev output through `tee`, say).
//!
//! The `json` feature has been compiled into every tracing-subscriber
//! dependency in this repo since long before anything switched it on.

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::Span;

/// Initialize the process-wide subscriber. Call once, as early as possible.
///
/// `default_filter` applies only when `RUST_LOG` is unset — it is the caller's
/// judgment about its own noise floor (interactive CLI commands want `warn`,
/// daemons want `info`), not a global default this module can pick.
///
/// Safe to call when something else already initialized a subscriber: the
/// second call loses and is ignored rather than panicking. Tests and
/// `#[sqlx::test]` cases routinely bring their own.
pub fn init(default_filter: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));

    if want_json() {
        let _ = tracing_subscriber::fmt()
            .json()
            // Event fields at the top level, the enclosing span's fields under
            // `span`. This is the shape the runbooks assume:
            //   journalctl -u virtues -o json --output-fields=MESSAGE \
            //     | jq -r 'select(.MESSAGE|test("run_id")) | .MESSAGE | fromjson | .span.run_id'
            .flatten_event(true)
            .with_current_span(true)
            // The full ancestor list is the same keys again, one level in, and
            // triples the size of every line. The current span carries what a
            // reader filters on.
            .with_span_list(false)
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

/// JSON unless stderr is a terminal. `VIRTUES_LOG_FORMAT` overrides.
fn want_json() -> bool {
    match std::env::var("VIRTUES_LOG_FORMAT") {
        Ok(v) if v.trim().eq_ignore_ascii_case("json") => return true,
        Ok(v) if v.trim().eq_ignore_ascii_case("text") => return false,
        _ => {}
    }
    !console::Term::stderr().is_term()
}

/// One HTTP request. Opened by the `request_id` middleware; every handler,
/// query and downstream log line runs inside it.
pub fn request_span(request_id: &str, method: &str, path: &str) -> Span {
    tracing::info_span!("request", request_id, method, path)
}

/// One applet run. `run_id` is `app_applet_runs.id`, so a row in the run log
/// and the lines that produced it share a key.
pub fn run_span(run_id: &str, applet_id: &str) -> Span {
    tracing::info_span!("applet_run", run_id, applet_id)
}

/// One chat turn. `turn_id` is minted per request because nothing persists it
/// — two turns in the same chat are otherwise indistinguishable in the log,
/// which is precisely the case you are reading the log to tell apart.
pub fn turn_span(chat_id: &str, turn_id: &str) -> Span {
    tracing::info_span!("chat_turn", chat_id, turn_id)
}

/// A fresh turn id: `t_<process-unique counter>_<millis>`.
///
/// Not a UUID, deliberately — this is read by a person scanning a log, it
/// never leaves the process, and it only has to be unique among the turns a
/// reader is looking at. Short enough to eyeball in a filter.
pub fn new_turn_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ms = chrono::Utc::now().timestamp_millis();
    format!("t{n}_{ms}")
}

/// A fresh request id, same reasoning as [`new_turn_id`].
pub fn new_request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ms = chrono::Utc::now().timestamp_millis();
    format!("r{n}_{ms}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_short() {
        let a = new_turn_id();
        let b = new_turn_id();
        assert_ne!(a, b);
        // A reader has to be able to hold one of these in their eye while
        // typing a filter. If this ever grows past ~20 chars, the format
        // changed into something that wants to be a UUID and should just be
        // one.
        assert!(a.len() < 24, "turn id got long: {a}");
        assert!(new_request_id().starts_with('r'));
    }

    /// The shape the runbooks and `docs/operate/recovery.md` depend on:
    /// `.span.run_id` on a line emitted inside a run span, event fields at the
    /// top level. This is the contract, not an implementation detail — if
    /// `flatten_event`/`with_current_span` ever change, every saved `jq`
    /// filter silently returns null, which reads as "no such run" rather than
    /// as a broken query. That failure mode is why this is a test.
    #[test]
    fn a_line_inside_a_run_span_carries_the_run_id() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
            type Writer = Buf;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let buf = Buf::default();
        // Mirrors the JSON arm of `init`. Kept explicit rather than factored
        // out: the point is to assert the shape a reader actually gets, so a
        // test that shared a builder with the code under test could pass while
        // the real output was wrong.
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(buf.clone())
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = run_span("run_123", "day_summary");
            let _g = span.enter();
            tracing::warn!(kind = "applet.stderr", "applet stderr: boom");
        });

        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let line: serde_json::Value =
            serde_json::from_str(out.lines().next().expect("a line")).expect("valid JSON");

        assert_eq!(line["span"]["run_id"], "run_123");
        assert_eq!(line["span"]["applet_id"], "day_summary");
        // Event fields flattened to the top level, not nested under "fields".
        assert_eq!(line["kind"], "applet.stderr");
        assert_eq!(line["level"], "WARN");
    }

    #[test]
    fn log_format_env_overrides_the_tty_guess() {
        // Serial-unsafe by nature (process env), but these two are the only
        // readers and neither runs concurrently with the other.
        std::env::set_var("VIRTUES_LOG_FORMAT", "json");
        assert!(want_json());
        std::env::set_var("VIRTUES_LOG_FORMAT", "TEXT");
        assert!(!want_json());
        std::env::remove_var("VIRTUES_LOG_FORMAT");
    }
}
