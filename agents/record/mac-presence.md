# Mac app sessions and presence

Written 2026-09-29. Built 2026-07-13 (`a9c84904`, reshaped the same day by
`a47e6462` and `443535d8`). How the Mac collector's focus events become honest
app sessions, and why the design ended with one table rather than the two it
was planned with. The code is `applets/mac_ingest/sessionize.rs` (box) and
`apps/mac-source/Sources/Core/PresenceMonitor.swift` + `Monitor.swift`
(collector). Replaces `agents/plan/mac-presence-plan.md` (deleted).

## What was wrong

App usage was not merely inflated. It was close to an inversion of the truth:
real focused work was invisible, and artifacts were the headline numbers.

Measured on the box before the fix, 11,263 Mac sessions over about a month:

| Session length | Sessions | Hours |
|---|---:|---:|
| < 1 min | 8,845 | 32.7 |
| 1–5 min | 1,971 | 70.1 |
| 5–30 min | 541 | 96.6 |
| **> 30 min** | **85** | **229.6** |

**326 of 429 recorded hours came from sessions longer than the five-minute
upload interval — sessions steady-state collection cannot produce.** The box's
most-used application was the lock screen, at 211 hours.

## The failure class

Two structural causes, neither a tuning problem.

**Sessionization lived in a stateless per-batch transform.** The collector
emits events only on change: focus at 12:00, unfocus at 12:40. The box grouped
events into sessions *within one upload batch*. So a real 40-minute session
put its focus in one batch (start equals end, dropped as noise) and its unfocus
in a batch forty minutes later (also dropped). A deep-work session recorded
nothing. Meanwhile backlog batches — collector restarts, upload backoff,
sleep/wake — delivered hours of events at once, and the consecutive-run merge
fabricated enormous spans from them. That is where a 665-minute `loginwindow`
"session" came from.

Events that describe state transitions cross batch boundaries by nature. Any
transform that only sees one batch cannot sessionize them.

**Absence was never recorded.** No lock, no sleep, no idle. Walking away with
the editor focused was indistinguishable from forty minutes of concentration.
The only absence signal was `loginwindow` arriving as if it were an app.

## What was built

**State lives in Postgres, not in a batch.** A focus opens a session row with
`is_open = true`; the event that ends it — a switch, a quit, a lock, idle, the
lid closing — closes it and writes `closed_by`. A session still open at the end
of a batch stays open, and the next batch closes it. That is the whole fix. An
in-flight session reads as short for at most one upload cycle, which corrects
itself.

**The collector records absence from OS signals**, nothing inferred: lock and
unlock, screensaver (treated as lock), fast user switching, sleep (`suspend`),
and HID idle polled every 30 seconds. Idle onset is **back-dated** from the
system's "seconds since last input", so poll cadence costs latency, never
precision; without it every idle transition would credit up to a poll interval
of absence as work.

The shipped table is `data_activity_app_session`:

| Column | Values |
|---|---|
| `attention` | `active` \| `watching` |
| `closed_by` | `switch` (also a quit) \| `idle` \| `lock` \| `suspend` \| `stale` |
| `is_open`, `device_id` | open-row state, scoped per Mac |

`loginwindow`, the screensaver engine and `SecurityAgent` are never app
sessions. Focusing one closes the open session as a `lock`.

## Four things red-teaming caught before shipping

1. **Heartbeat.** Sessions close on the matching unfocus. If the collector dies
   mid-session — a crash, a power cut, or simply an update swapping the binary,
   which happened a dozen times that day — the unfocus never arrives and the
   orphan could only be clamped to its own start: zero duration, dropped. The
   original bug by another route. The collector emits a heartbeat every 60
   seconds that advances the open session's provisional end, so loss is bounded
   to one interval. A session open longer than 8 hours is reaped at its last
   heartbeat and marked `stale`.
2. **Watching.** A 40-minute video produces no input, so a naive idle check
   calls it "away" and deletes it. The signal is the focused app holding the
   display awake — scoped to the focused app's process, because builds, screen
   sharing and `caffeinate` hold that assertion too. On day one that alone still
   false-positived on an indexing editor, so `watching` also requires sound to
   be playing.
3. **Device scoping.** Sessions held open across batches meant two Macs would
   close each other's. The collector now sends its device identity and every
   open-row query is scoped by it.
4. **Idle is 10 minutes, not 5.** HID-idle assumes attention makes keystrokes;
   reading and thinking make none. Under-counting idle is far cheaper than
   deleting real attention, and the raw events are archived in the lake, so a
   wrong threshold is a re-derivation, not a re-collection.

## Two corrections to the ontology

The plan called for a second table, `data_activity_presence`, with states
`active | idle | locked | asleep`. Both halves changed during the build.

**`asleep` became `suspended`.** The state meant the *machine* slept. A Mac can
observe its lid closing; it cannot observe you. Human sleep already has
`data_health_sleep`, from a watch that can measure it. Close the lid at lunch
and a column saying `asleep 12:05–13:10` tells the narrative layer you took a
nap — and a column that has declared "asleep" can never be taken back. This is
the same error as `loginwindow`-as-an-app, one level up.

**The second table was dropped.** It was renamed `data_activity_device_state`
and then deleted the same day. It was never needed to close a session — the
events do that — and everything it would say is already on the session:

```
09:12–12:05  Cursor  closed_by = lock
13:10–13:48  Cursor  closed_by = switch
```

The gap explains itself. `closed_by = 'stale'` is the one honesty property
worth protecting: it distinguishes "we weren't watching" from "you walked
away". A full attention timeline (idle vs locked vs suspended) can be derived
from the raw events in the lake if anyone ever asks for one. A version that
tiled the timeline with an implicit `active` was rejected on the way: a gap is
also what a stopped collector looks like, so implicit-active renders downtime
as presence.

The resulting rule: `data_activity_app_session` holds only app sessions. Sum it
and you get app time, and only app time — never hours you were asleep.

## The old rows

The pre-fix Mac rows were deleted rather than migrated. They could not be
repaired: the raw focus events behind them had been aggregated away at ingest.
From this change on the raw events are archived in the lake, so a future
sessionization fix is a re-run rather than a loss.
