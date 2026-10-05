# Notifications — one way for the box to tell the owner anything

**Status:** Planned 2026-10-05. Nothing is built. The detection most producers
need already exists (see **Producers**); the table, the router and the
surfaces do not.

## The failure this exists to fix

A Mac collector has lost Full Disk Access to an update **three times**, and
each time iMessage went dark for days with nobody told: six days from
2026-07-30 ([record/mac-silent-outage.md](../record/mac-silent-outage.md)), three
days cited in `api/box_status.rs`, and two days from 2026-09-29, after the
collector binary was replaced and its grant stopped applying.

Detection was not the problem in the last two. The collector writes an honest
`health.json` and uploads it with every batch (`Uploader.swift`,
`collectorHealthPayload()`); `mac_ingest` records the denied grant on the
device row; `box_status.rs` computes a `degraded` list; `/api/streams/health`
marks streams `stalled` and `blocked`. The web app fetches `degraded` into
`setupState` **and renders it nowhere.** Every signal ended in a place nobody
watches.

The general shape: **the box can detect, but it has no way to reach the owner,
so each feature that needs to invents a partial one or none.** The Mac app has
its own update-failure banner (`record_update_failure`, `main.rs`). Backups have
"no Home warning" (`backup-plan.md`). iOS dead letters want a line on the
device screen (`ios-durability-plan.md`). Applets cannot reach anyone at all —
`applets-next-plan.md` marks Persona unauthorable for want of a channel.
Reminders were designing their own delivery.

## The thesis

**One notification object. Many producers write it. One router decides
delivery. Many destinations display it.**

A producer says *what happened, about what, and how to fix it*. It never says
where it goes, whether to push, or how loudly. That is how a stream failing
three hundred times becomes one notification, and how a new kind of producer
gets correct delivery for free.

## What a notification is

Three properties. They are deliberately few: every kind of thing on the
owner-facing list — error, warning, success, new feature, cost, reminder,
applet message — is a combination of them, not a category of its own. A
category per source is the toggle matrix that `reminders-plan.md` set out to
avoid, and it is still the thing to avoid.

| Property | Values | Decides |
|---|---|---|
| **kind** | `fix` · `fyi` | the lifecycle |
| **urgency** | `now` · `soon` · `whenever` | how loudly, how fast |
| **author** | `owner` · `system` | whose rule it is, and who may mute it |

How the familiar names map:

| Familiar name | Is |
|---|---|
| broken / error / needs you | `fix` · `now` or `soon` · system |
| warning | `fix` · `whenever` (disk 80% full) |
| success / info | `fyi` · `whenever` |
| new feature / update installed | `fyi` · `whenever` · system — **never pushed** |
| applet message | whatever the applet declares · owner |
| reminder | `fyi` · `now` · owner |
| card failed | `fix` · `soon` · system |
| a charge went through | `fyi` · `whenever` · system |
| "tell me when spend passes $50" | **a reminder the owner writes**, not a system category |

### The two lifecycles

- **`fix` is level-triggered.** It stays open until the producer observes the
  thing healthy again, then clears itself (`cleared_at`). It **cannot be marked
  read** — only fixed or snoozed. A notice that can be dismissed while the
  stream is still dead is how "Messages is broken" ends up looking handled.
- **`fyi` is edge-triggered.** It happened once; it is read and gone, and it
  expires on its own.

### The fields

- `subject` — a typed reference to what it is about: `stream:<registry name>`,
  `device:<id>`, `applet:<id>`, `box`, `account`. The subject is what the
  owner already knows by name; the notification is a fact about it.
- `producer` and `cause` — with `subject`, the **dedupe key**. One open row per
  key, ever. A reopened `fix` is a new raise, rate-limited against flapping.
- `body` — **consequence first, cause second** (rule 5 of the silent-outage
  record): *"Messages has not arrived since Tue · Full Disk Access is off on
  this Mac."* Owner-facing copy follows `agents/build/voice.md`.
- `observed_at` — when the producer saw it (rule 2: every claim carries when
  it was observed). "Unknown" is a real state and is reported as one (rule 1).
- `actions` — what can be done from the notification itself: open the fix
  (a deep link, or a shell command id such as `open_full_disk_access`),
  snooze, and for reminders *done* / *tomorrow*. Declared on the row so any
  destination that can carry a button, or later a reply, can offer it.

## Asks are not notifications

The resolution and maintenance items — name a salient new place, who is this
voice, are these the same person, add context to this day — are real and
valuable, and **they already have a home: `narrative-resolution-plan.md`'s
open-question queue.** "A place visited above N times with no label" is one
of its generators verbatim.

They stay there, and do not become a third kind here, because that plan's
rules are the right ones and a notification list would break them: one
question at a time, in the place where the answer pays off, no badge, no
count, never pushed. Its named risk is *"it becomes an inbox"*; folding asks
into notifications is exactly how that risk would land.

The one connection: the Home surface below may host that plan's daily slot
beside "Needs you". Same screen, separate objects.

## The router

Producers never decide delivery. The core does, by the three properties:

| kind · urgency | Delivery |
|---|---|
| `fix` · `now` | push + in the app |
| `fix` · `soon` | in the app; push once if still open after a threshold |
| `fix` · `whenever` | in the app only |
| `fyi` from the system | in the app only |
| `fyi` from the owner (reminders, applets) | wherever the rule says |

Invariants, all in the router and none in a producer or a prompt. The first
three are lifted from `reminders-plan.md`'s evaluator, so every producer gets
them rather than only reminders:

- **Push on the raise, never again while it stays open.**
- **Downtime collapse.** Box off for a day, six became true: deliver the most
  recent and say how many.
- **Never about something older than the rule** that raised it — a backfill
  must not produce forty notifications about last year.
- **Mute a rule or a subject, never a category.** An owner may silence one
  applet, or one flapping device. There is no "turn off warnings".
- **New features never push.** One unwanted push and the owner disables
  notifications, and then misses the one about the dead stream.

## Destinations

What a notification is stays separate from where it goes. Each destination
declares **how much a third party can read**, and that decides what it gets:

| Destination | Who can read it | Gets |
|---|---|---|
| The app (Home "Needs you", the full list, the subject's own page) | the owner | full text |
| Mac banner | the owner | full text — **only for a `fix` whose fix happens on that Mac** |
| APNs | Apple and our signer, in plaintext, until the v2 notification extension encrypts it | full text, per `reminders-plan.md` |
| Webhook (ntfy, Home Assistant, a script) | whoever runs the URL | pointer by default; full text if the owner says so |
| Email · Slack · Telegram | the vendor | pointer by default; full text only per rule, opted in |

A pointer is contentless: *"Something needs you on your Virtues."*

**Mac banner vs `reminders-plan.md`'s refusal.** That plan dropped
"Mac-first notifications" on 2026-09-22 as a weak surface for *reminders*, and
that stands. A Full Disk Access fix is different: the grant can only be given
on that Mac, so that Mac is the right place to say so. The update-failure
banner already works this way.

**The webhook comes first among external destinations.** It needs nothing
from us, works for a self-hosted box with no account, and covers "other apps"
generically. Email and Slack are each about a day once destinations exist.
iMessage through a vendor stays refused for the reason `reminders-plan.md`
gives: every message would cross a vendor's servers in the clear.

**Replies are the long-term payoff** — answering *done* to a reminder from the
lock screen, or a text reply. Not in scope; `actions` on the row is what keeps
the door open.

## Producers

| Producer | Notification | Reads from (exists) |
|---|---|---|
| Collector permissions | `fix` · `now` — "Messages has stopped · Full Disk Access is off on this Mac" | `degraded` in `box_status.rs`, via `mac_ingest` |
| Blocked applet | `fix` · `soon` | stream health `blocked` |
| Stalled stream, cause unknown | `fix` · `whenever`, **in-app only** until stream rhythm is per-stream | stream health `stalled` |
| Silent collector (stopped checking in) | `fix` · `soon` | device last-seen |
| Expired token / Plaid re-link | `fix` · `soon` | OAuth refresh failures, Plaid item errors |
| Phone unreachable for push | `fix` · `whenever` | `push_address` null (`reminders-plan.md`) |
| Backup stale or failed | `fix` · `soon` | `backup-plan.md` gaps |
| iOS dead letters | `fix` · `whenever` | `ios-durability-plan.md` |
| Mac update failed twice | `fix` · `soon` | `record_update_failure`, `main.rs` — moves here |
| Card failed / wallet empty | `fix` · `soon` | billing |
| Charge, top-up | `fyi` · `whenever` | billing |
| Update installed, new feature | `fyi` · `whenever` | release notes |
| Reminder fired | `fyi` · `now` · owner | `reminders-plan.md` phase 3 |
| Applet notify | as declared · owner | a new applet capability |

A stalled stream with no known cause stays in the app because a false
"Messages has stopped" on a quiet day costs more trust than the true one buys
— the same reason the nearest-place guess was cut. It earns a push when
stalled is measured against each stream's own rhythm rather than a flat 24h.

**Who notices when the reporter is dead.** A dead collector is noticed by the
box (the device went quiet). A dead box can only be noticed by a client that
fails to reach it, or by atlas missing a heartbeat — which would tell our
cloud when an owner's box is down. That is the owner's trade to make, not a
default; see Open.

## What changes elsewhere

- **`reminders-plan.md`.** Its "the owner learns one word: reminder; no
  notifications room" is **reversed in part.** A reminder stays the owner's
  object and the owner's word; what it sends is a notification, and its
  delivery invariants and the APNs signer (phase 2) become this plan's router
  and first push destination. Its phases 1 and 2 are unchanged in substance.
- **`applets-next-plan.md`.** Persona's missing channel-to-the-owner is the
  applet notify capability here. Send-as-me, to third parties, is unrelated and
  stays there.
- **`narrative-resolution-plan.md`.** Owns asks, unchanged.
- **`backup-plan.md`, `ios-durability-plan.md`.** Their warning lines become
  producers rather than bespoke surfaces.
- **`main.rs` update-failure banner.** Becomes a producer; the Mac banner
  becomes a destination.

## Build order

1. **The object and the surfaces.** Claim a migration, the table with the three
   properties and the dedupe key, Home "Needs you" (open `fix` rows only;
   empty means nothing shows), the full list, and the line on the subject's own
   page. Producers: **collector permissions and blocked applets**. No push.
   This alone would have caught all three outages within a sync.
2. **The rest of the system producers**, in the table's order. Stalled stays
   in-app.
3. **The Mac banner** for on-this-Mac fixes; `record_update_failure` moves onto
   it.
4. **Push**, when the reminders signer lands — the router's first external
   destination.
5. **The webhook destination**, then the applet notify capability, then release
   notes.
6. **Email and Slack**, if anyone asks.

## Open

- The `soon` → push threshold. Probably hours, not minutes; nobody has picked
  one.
- Quiet hours for pushes, and whether `now` overrides them.
- Whether a box-down heartbeat through atlas is offered at all, and how it is
  worded if so.
- Snooze length, and whether a snoozed `fix` re-raises when it gets worse
  (stalled → two days stalled).
- Whether the full list belongs in Settings, Home, or its own room. The bet is
  Home holds "Needs you" and nothing else needs a room.
