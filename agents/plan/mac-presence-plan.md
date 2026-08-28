# Mac presence + honest app usage

## Context

`data_activity_app_usage` is not merely inflated. It is close to an inversion of the
truth: **your real focused work is invisible, and artifacts are the headline numbers.**

Measured on the box (11,263 Mac sessions, ~1 month):

| Session length | Sessions | Hours |
|---|---:|---:|
| <1 min | 8,845 | 32.7 |
| 1–5 min | 1,971 | 70.1 |
| 5–30 min | 541 | 96.6 |
| **>30 min** | **85** | **229.6** |

**76% of all recorded hours (326 of 429) come from sessions longer than the 5-minute
upload interval — and such a session is structurally impossible to record.**

### Why

The collector emits events **only on change**: `focus(Cursor)` at 12:00,
`unfocus(Cursor)` at 12:40. The box (`actions/mac_ingest/transform.rs::write_app_events`)
sessionizes **within a single 5-minute batch**, grouping consecutive same-`bundle_id`
runs.

So for a real 40-minute session:

- Batch A holds only the `focus` event → `start == end` → duration 0 → **dropped** by the
  `< 1s` noise filter.
- Batch B, 40 minutes later, holds only the `unfocus` → `start == end` → **dropped**.

**A 40-minute deep-work session produces no row at all.** For a session to be recorded,
*both* of its endpoints must land in one batch — which steady-state collection cannot
do. The 626 sessions >5 min can therefore only have come from **backlog batches**:
collector restarts, upload backoff (which climbs to 16 min), sleep/wake. Hours of events
arrive at once, consecutive same-app runs get merged, and one fake span is born. That is
the origin of the 665-minute `loginwindow` "session" — and of the fact that the box
currently believes the user's most-used application is the lock screen (211 h of 429).

Two root causes, both structural:

1. **Sessionization happens in a stateless per-batch transform.** The events describe
   *state transitions* whose spans cross batch boundaries by nature. This is not a tuning
   problem; it is the wrong place to do the work.
2. **Absence is never recorded.** No lock, no sleep, no idle. Walking away with Cursor
   focused is indistinguishable from 40 minutes of concentration. `loginwindow` is the
   only absence signal we have, and it is a proxy, arriving as a fake "app".

## The model

Two distinct entities, which the current schema conflates:

- **`data_activity_app_usage` — attended, focused time.** Bounded by idle, lock, and
  sleep. Answers "what was I working on."
- **`data_activity_presence` — where the human was.** Spans of `active | idle | locked |
  asleep`. Answers "was I even here."

`loginwindow` time is **kept in full** — it is real signal, and the only record of being
away. It simply stops masquerading as app usage.

---

## 1. Collector — `PresenceMonitor`

New `apps/mac-source/Sources/Core/PresenceMonitor.swift`. Every signal comes from an OS
notification or API; nothing is inferred.

| Event | Source |
|---|---|
| `lock` / `unlock` | `DistributedNotificationCenter`: `com.apple.screenIsLocked` / `com.apple.screenIsUnlocked` |
| `sleep` / `wake` | `NSWorkspace.willSleepNotification` / `didWakeNotification` |
| `screensaver_start` / `_stop` | `com.apple.screensaver.didstart` / `.didstop` |
| `idle_start` / `idle_end` | Poll `CGEventSourceSecondsSinceLastEventType(.hidSystemState, kCGAnyInputEventType)` |

**Back-dating idle is the crux.** That API returns *how long* input has been absent, so
idle onset is `now − idleSeconds` — the exact instant input stopped, not the moment we
noticed. Without it, a 30s poll interval would silently credit up to 30s of idle as work,
on every idle transition, forever. Threshold: **5 minutes** of no HID input; poll every
**30s** (cheap, and the back-dating makes poll frequency irrelevant to accuracy).

`willSleepNotification` is delivered *before* the machine suspends and is a synchronous
window — enqueue the event there and do nothing else.

Reuses the existing spine: the `paused` flag-file gate, the SQLite queue, the 5-minute
uploader. New `presence_events` queue table (`event_type`, `timestamp`, `uploaded`), new
`presence_events` key in the webhook payload. `StartCommand` must actually **start** it —
the bug that left `MessageMonitor` built-but-dead for months.

## 2. Box — stateful sessionization

### Migration + registry

- New `data_activity_presence`: `id, state (CHECK IN active|idle|locked|asleep),
  started_at, ended_at, source_stream_id, source_table, source_provider, metadata, …`
- **Registry entry in `crates/virtues-registry/src/ontologies.rs`** — a migration alone is
  not enough. The registry is what makes a table queryable by the model (`sql_query`) and
  visible to day summaries. `data_activity_web_browsing` gets read by `day_summary.rs` and
  `dayline/context.rs` today *because* it is registered; presence needs the same to show up
  in "what did I do today."

### `write_app_events` becomes a sessionizer that consults the DB

`end_time` is `NOT NULL`, so an open session cannot be a NULL end. Instead:

- **`focus` / `launch`** → INSERT a session with `end_time = start_time` and
  `metadata.open = true`.
- **`unfocus` / `quit`** → UPDATE the open session for that bundle: set `end_time`, clear
  `open`.
- **`lock` / `sleep` / `idle_start` / `screensaver_start`** → close **all** open sessions at
  that instant.
- Sessions still open at the end of a batch **stay open**. The next batch closes them.

Cross-batch now works because the state lives in Postgres rather than in a 5-minute
window. An in-flight session reads as a short one for at most one upload cycle — an
acceptable and self-correcting lag, and far better than the current alternative of not
existing.

Note this needs a targeted `UPDATE … WHERE source_stream_id = …`, **not** the generic
upsert machinery we deferred. `ON CONFLICT DO NOTHING` in the batch-insert helper is
untouched.

### Guards (each maps to a failure we have actually seen)

- **Stale open session** — a hard power-off emits no `sleep`, so a session would stay open
  forever. On each batch, clamp any session open longer than **8h** to its last known
  activity and close it.
- **Max session** — hard ceiling; nothing legitimate runs for 12h of *attended* focus.
- **Idle back-dating > session start** — clamp to `start_time` (never a negative duration).
- **`loginwindow` / `ScreenSaverEngine` are never app sessions.** They are presence rows.
  This is the one place we translate rather than record.

## 3. Wipe the existing rows

The 11,263 Mac rows **cannot be repaired**: the raw focus events behind them no longer
exist (`mac_ingest` aggregated at ingest, and the lake only starts from today). 76% of
their hours are fabricated and the remainder is app-switch flurries.

`DELETE FROM data_activity_app_usage WHERE source_provider = 'mac'` and start honest.

Going forward this is no longer a one-way door: the raw `app_events` are archived in the
lake, so a future sessionization fix is a re-run rather than a loss.

## Verification

1. **The bug that motivated this**: focus an app for >10 min (longer than one upload
   cycle), switch away. It must produce **one session of the true length** — today it
   produces none.
2. **Lock**: work, lock the screen, wait, unlock. App session closes at the lock instant;
   a `locked` presence row covers the gap; no `loginwindow` app session exists.
3. **Idle**: focus an app, don't touch the keyboard for >5 min. Session closes at the
   moment input *stopped* (back-dated), not when the poll noticed. A subsequent keystroke
   opens a new session.
4. **Sleep**: close the lid, reopen. No session spans the sleep; an `asleep` presence row
   does.
5. **No fabrication**: after a collector restart with a large backlog, no session longer
   than the true focus duration appears (this is the 665-minute `loginwindow` test).
6. **Sum check**: `active + idle + locked + asleep` ≈ wall-clock, and app-usage hours ≤
   active hours.

## Out of scope

Now-playing (`data_activity_listening`), the `virtues replay` CLI, and the upsert /
scoped-reproject machinery. None are needed for this.
