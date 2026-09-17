# Observability

**Built 2026-09-14/16, unreleased.** How a box explains itself: one vocabulary,
a key on every log line, and one door a client reports its own failures
through. Supersedes `agents/plan/observability-plan.md` (deleted).

---

## What was wrong

The box had four ledgers and five logging setups. Every one of them was
honest, and no one of them could name another.

| Where | What it held |
|---|---|
| the journal | every line the daemon wrote, as prose |
| `app_system_samples` | the machine, once a minute |
| `app_ai_calls` | one row per model call, with cost |
| `app_applet_runs` | one row per applet run, collapsed for reading |
| `app_auth_event` | pairings, revocations, sudo |

Answering "why did the box do that at 3am" meant four tools and a guess.
Nothing tied them together: there were 535 logging call sites and **zero**
spans, so a failed applet run could not find its own log lines — nothing on
those lines said which run they belonged to. Grep by timestamp and hope was
the whole procedure.

Two things were worse than merely disconnected.

**The web app could not reach the box at all.** 135 bare `console.*` calls, no
wrapper, nothing posted anywhere. A phone that failed to pair, a screen that
threw on load — none of it existed outside a devtools console nobody was
looking at, on a device that is usually a phone.

**The one thing that did leave the box was undisclosed.** When the daemon dies
abnormally, systemd runs a hook that posts the exit status and the last 50
journal lines to us. It is on by default. Meanwhile a comment in `main.rs`
said Virtues collects no central telemetry, the module that implements the
opt-out described an install-time notice that has never existed, and the
opt-out appeared in no document. The crash was **never recorded locally at
all** — a box whose owner had opted out, or whose network was down when it
died, kept no evidence that it had crashed. The owner's own machine was the
one place the fact did not exist.

## What was built

**One vocabulary.** `virtues-core/src/observe.rs` spells the field names once:
`kind`, `source`, `request_id`, `run_id`, `applet_id`, `chat_id`/`turn_id`,
`device_id`. Severity is deliberately not a field — it is the logging level,
and a second scale beside it would immediately disagree with the first.

**One subscriber.** Text when stderr is a terminal, JSON when it is not. That
single rule lands correctly in both places that matter: a developer reads
prose, and a box under systemd writes structured lines into journald where a
filter can find them.

**Keys, stamped at the entry point.** Three spans — HTTP request, applet run,
chat turn — plus a keyed event on each model call, which needs no span of its
own because it has no children. The scheduler tick deliberately got nothing:
every cron fire already reaches the run span, so a tick span would key the same
work twice and be empty on the ticks that do nothing.

**A request id that the client can quote.** Minted per request, put on the
span so server lines inherit it, returned as `x-request-id`, and carried back
by the client on an error report. That last half was claimed in a comment for
one commit before it was true, which is its own lesson.

**A client door.** `POST /api/events` takes a batch from a paired device and
re-emits each entry into the box's journal. The device identity is stamped
from the session and cannot be set by the body. There is no table: these are
journal lines, which journald already rotates.

**A crash a box knows about.** The local record is written first and
unconditionally; the beacon is a second, optional step on top of it. The
beacon now sends only panics and our own errors, not the last 50 lines
whatever they were.

**Applet events that can be found.** An applet is a subprocess, so everything
it logs reaches the journal through the runner. The runner unwraps a
structured line rather than quoting it, so the applet's `kind` and its own
level become the box's fields.

## What was deliberately not built

A general `app_events` table and a Logbook view in the System page. Both are
designed and both are deferred, with a named trigger: **build them when
someone other than the operator must answer "what happened" without a shell.**
Until then the journal is the log and `jq` is the reader.

Also deferred: moving the 135 existing `console.*` calls onto the wrapper.
Nearly all sit in `catch` blocks, which means the code already handled the
failure. The errors that actually break a screen are the uncaught ones, and
those are covered by global handlers. Do the rest opportunistically.

## What deploying taught

Everything above was tested, and then hand-installed on a real box and tested
again. Four defects survived the first pass and were found only by reading
what the box actually wrote.

1. **Color inside a structured field.** The text formatter colors by default,
   and "text" is not only a terminal: an applet's output is a pipe, so escape
   codes ended up inside the runner's JSON, where they break grep. Color is
   now tied to stderr actually being a terminal.
2. **A crash record that named the wrong build.** It reported the version
   string from `Cargo.toml`, pinned at `0.1.0`, so every crash claimed to be
   `0.1.0` regardless of what was running. The first question about a crash is
   which build died.
3. **A drop-notice that went missing when it mattered.** The client tells the
   box how many reports it had to discard. On a full queue that notice was the
   entry the server truncated away — absent exactly when there was loss to
   report.
4. **Two forever-retry loops.** A client posting into a box that predates the
   route, and an unpaired client posting into a 401, each retried every ten
   seconds for the life of the page. The first is the *normal* state after a
   client release, since phones update themselves and boxes update when
   someone types a command. The second was worst on the pairing screens, which
   are unpaired by definition and where a failure is most likely.

## Traps worth knowing

- **`journalctl -p err` does not work for this.** systemd stamps every line a
  service writes to stderr as informational, absent a syslog prefix the
  formatter does not write. Filtering by journal priority drops our own error
  lines. The level lives inside the message.
- **Bare `jq` aborts on the journal stream.** journald carries systemd's own
  plain-text lines, and `jq` dies on the first one and prints nothing — which
  reads exactly like "the keys are missing". Use `-o cat` and
  `jq -R 'fromjson? | …'`. This cost twenty minutes of believing a working
  feature was broken.
- **Entering a span inside an async function makes the future non-Send**, and
  axum reports that as "not a Handler", which points nowhere near the cause.
  Instrument the future instead.
- **`sendBeacon` is the wrong tool here** despite looking purpose-built. The
  mobile app reaches the box through a `fetch` proxy that `sendBeacon` does
  not go through, so it would post to an origin serving no API and fail
  silently. Use `fetch` with `keepalive`.
- **`cargo build -p a --bin a -p b` builds no binaries from `b`** — the
  `--bin` filter applies across packages. This shipped yesterday's applets
  against today's daemon. Check binary timestamps before deploying.
- **A successful applet run logs nothing**, so it has no keyed lines. The span
  only appears on lines that exist.

## Measured

On a real box, of the last 400 journal lines the crash beacon would send
**two**: the crash itself and a deliberately planted error. It previously sent
the last fifty, whatever they happened to be.

## Open

- None of this reaches a user until a release ships.
- The client half is verified in a browser, not on a box: the web bundle was
  not part of the hand-install.
- `with_current_span` serializes only the innermost span, so a line logged
  inside a nested span shows that span's fields rather than the request's. No
  line checked on the box was affected. If one ever is, the fix is to emit the
  full span list and accept a longer line.
