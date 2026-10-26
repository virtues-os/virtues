# Chat sky plan

The chat sky is the field of dots above the composer on an empty new chat.
Each dot is part of the owner's day. Scrolling over it changes what it shows,
at four levels:

- **Moment:** one event, opened into its pieces.
- **Day:** today, last night's sleep, and the next few calendar items.
- **Context:** the projects, pages, chats and people today's events belong to.
- **Week:** those threads as lines across seven days.

Hover names a dot. Click opens its links and "Keep this moment". Dragging a
dot into the composer, or typing `@`, mentions it.

The design was settled on the "Final round" page of the New Chat Sky canvas
(Claude Design, 2026-10-25). This plan covers what has to exist in the server
and the web app for it, in the order it can ship.

## What exists today, and what does not

### Today's events are not in `wiki_events` until the day is over

`segment_day_events` returns nothing for a day that has not ended
(`virtues-core/src/api/day_summary.rs:421`). The nightly chain only cuts
yesterday and older (`applets/day_summary_eod/main.rs:36-60`). So the sky
cannot read today from `GET /api/wiki/day/:date/events` the way the removed
`NewChatSky.svelte` did. That component showed nothing for today on a real
box until the day was over.

The live read that does exist is `timeline::get_day`
(`GET /api/timeline/days/:date`, `virtues-core/src/api/timeline.rs:117`). It
returns, for any date:

- stays with place names
- nights with asleep minutes
- calendar entries
- steps in 10-minute bins

Its `sessions` are empty for today, because audio sessions are built nightly.

### What each level can read

| Need | Source | Status |
|---|---|---|
| Stays and places | `data_location_visit` + `wiki_places` through `wiki_refs`; resolved every 15 minutes | Exists, live |
| Calendar, next items | `data_calendar_event`; `GET /api/calendar/upcoming` | Exists, live |
| Last night | `dayline/sleep.rs` `night_for_day`; `data_health_sleep.sleep_stages` | Duration exists; deep minutes and wake count are not computed |
| Workouts, calls, messages | `data_*` tables with `started_at` or `occurred_at` | Exist, not yet grouped into moments |
| Weather | `GET /api/weather/current`, `data_environment_weather` | Exists |
| Past days' events, novelty | `wiki_events` (`novelty_z`, `local_novelty_z`, `entity_novelty`), nightly | Exists for finished days only |
| Novelty during the day | — | Nothing is computed intraday |
| Who matters to the owner | `wiki_people.bond`; narrative identity is one prose page | No structured list |
| Event → project, chat, page | `app_project_items` (indexed by url); chats and pages hold refs inline as markdown | No link from an event to anything; the project "magnet" is gone |
| A ref to one event or a time range | `refs.rs` resolves day, person, place, org, page, chat, project, drive | No event ref, no range ref; `/record/<ontology>/<id>` is produced elsewhere but does not resolve |
| Photos at a time | `app_drive_files` has no taken-at time | No photo table |
| Private places | `day_picture.rs` `PRIVATE_PLACE` regex; `wiki_places.category = 'health'`; `home_place_id` | No flag on a place; the regex is private to day pictures |
| A week in one call | `GET /api/wiki/activity` gives counts only | No range endpoint for events |
| Suggestion sources | `app_suggestions.refs` as `[{url, label}]`, with `/record/…` urls | Exists |

### What the web app already has

- The empty-chat hero mounts at `ChatView.svelte:2420`, inside `.init-hero`. It is `pointer-events: none` today.
- The composer accepts dropped mentions. `setMentionDrag(e, url, label)` takes a `/kind/id` url (`utils/projectActions.ts:277-308`). `MENTION_DROP_EVENT` lets a pointer-based drag drop without HTML5 drag-and-drop.
- `composer.insertRefHere` inserts a ref at a point.
- The `@` query is private to ChatInput (`ChatInput.svelte:197`). The sky cannot see it yet.
- Phone uses the same ChatView. `mobileLayout.isMobile` decides, and hover does not exist there.
- `TooltipLayer`, `RefPreview` and `contextMenu` exist. No radial menu component exists.

## The data contract

One server module, `virtues-core/src/api/sky.rs`, builds **moments**. A moment
is the unit every level draws:

```
Moment {
  id            // stable: "<source>:<row id>", e.g. "visit:vis_7f2a…"
  ref_url       // what a mention inserts (see Mentions)
  started_at, ended_at, kind   // stay | move | sleep | workout | call | thread | calendar
  label         // written by code: "Lunch with David Okafor, Café Rosa",
                // "Run, 5.1 km", "Call with Nick"
  place_id?, people[]          // entity ids, for Context and links
  planned       // true for calendar items not yet started
  private       // true at a private place (see Care)
  ink           // "plain" | "matters" | "unusual"
  reason?       // the hover's second line: "First lunch together in nine weeks"
  links[]       // up to four: {kind, url?, strong, rest}
}
```

There are three endpoints, all read-only, and all built from the same assembler:

- `GET /api/sky/day?date=&tz=`: moments for one day, plus `night`, `now` (local time, weather line, next item) and `upcoming[3]`.
- `GET /api/sky/week?start=&tz=`: seven days of moments and the threads that cross them.
- `GET /api/sky/moment?id=`: the pieces inside one moment.

**One assembler for every day.**

- A day that has been narrated reads `wiki_events`, with nightly novelty.
- Any other day is assembled live from `timeline::get_day`, plus workouts, calls and message threads. That covers today, and yesterday before the 4 AM run.
- Build on `timeline::get_day` rather than querying visits, nights and calendar a second time.

## What ships first

**Slice 1, the Day level, read-only.** It is useful on its own, and everything
later builds on its endpoint and component. Built on `wave` 2026-10-25, not
yet run on a real box: `api/sky.rs`, `GET /api/sky/day/:date`,
`components/chat/ChatSky.svelte`, and the switch in `SuggestionsSettings`.
It draws stays, calendar, workouts and last night; message threads wait on
the decision below. The switch is on unless set off. Until slice 3, a dot's
ink comes from its kind.

- Server: `sky.rs` with the live assembler and `GET /api/sky/day`.
  - Moments from stays, calendar, last night, workouts, calls, and message threads with 3 or more messages inside 30 minutes.
  - Labels written by code from place names, calendar titles and people's names.
  - `ink` is `plain` for everything in this slice.
  - Tests use invented fixtures only.
- Web: `lib/components/chat/ChatSky.svelte` replaces `NewChatMark` in `.init-hero` when there is at least one moment, and the mark stays as the fallback.
  - Dot size by duration, by area.
  - Each dot sits at the middle of its time.
  - A place visited twice keeps one height; one-off stops float.
  - Planned items are rings, three at most.
  - A short dotted now mark; its hover shows the time, the weather and the next item.
  - Two-line hover, using `RefPreview`-style placement.
  - Arrival in time order on the first open of the day only, kept in localStorage per date.
  - Breathing, dropped under reduced motion.
  - Theme tokens only, so dark themes work with no extra code.
- Setting: a "Chat sky" switch in Settings › Assistant, next to Suggested questions. It is stored in `app_assistant_profile.ui_preferences.chat_sky`, the way `day_pictures` is.
- Phone: the same sky. Tap shows the hover card; there is no drag yet.

**Check before it ships:** run it on dragon read-only against real days. That
tells us:
- whether labels written by code read well
- how many moments a busy day produces
- whether the cap of about 25 dots and the merging rule hold up

## Order of work after that

**Slice 2, mentions, links and the composer.**
- Mentions need a ref for a moment. Add a `record` arm to `refs.rs` for `/record/<ontology>/<id>`, which suggestions and citations already produce. Allow that 3-segment form in `mentionableUrl`. A time range is `/day/day_YYYY-MM-DD?from=12:00&to=15:30`. `split_ref` already strips the query for resolution, and the model reads the url as written.
- Drag: desktop calls `setMentionDrag`. Touch uses a pointer drag that dispatches `MENTION_DROP_EVENT`.
- Click: up to four links around the dot. They come from the moment's `links`: its place, its people, calendar attendees, and one fact such as a transaction total during a stay (`data_financial_transaction` in the window). Each link mentions in one click. Escape or a click outside closes them. This is a new small component, kept local to the sky.
- ChatInput gains `onMentionQuery(q)`, so `@lun` lights the matching dots. Typing anything else dims the sky to 40%. Chip hover lights its dot, and a mentioned dot keeps a ring.
- Suggestion sources: when the suggestion's `refs` match moments in the sky, focusing the composer draws the lines between them.

**Slice 3, ink during the day.** This needs no model calls.
- **Place:** the weighted share of your past visits to this place at this hour and weekday. It uses the same recency and phase weighting as `dayline/novelty.rs`, moved into one shared function. Low share means unusual.
- **Person:** the gap since you last saw this person, against your usual gap with them, from `wiki_refs.occurred_at`.
- A moment is scored once, when it ends. The dark-ink threshold is set from the owner's own history, so roughly 10–15% of a typical day is dark.
- The hover names the reason: "Unusual for a Tuesday morning", "First lunch together in nine weeks".
- "Matters" ink comes from `wiki_people.bond` for people, and from project membership for places and calendar items. A structured list taken from the narrative identity can replace both later.
- Narrated days keep their nightly scores. Only today is provisional.

**Slice 4, the Context level and its matcher.** Nothing links an event to a
project today. The matcher works in order of cost:
1. Projects whose `app_project_items` contain the moment's person, place or calendar event url. This is an indexed query.
2. Chats with messages inside the moment's window, or that mention its people or place. This is a `LIKE` scan bounded to the week, the way `get_subject_backlinks` works.
3. Pages that mention its people or place, through the existing backlinks functions.

Labels above the dots, with dotted lines down to them. Clicking a label mentions it.

**Slice 5, the Week level.**
- `GET /api/sky/week` runs the assembler for seven days and groups each day's moments by their first thread.
- Each thread becomes one line across the days. Lines bend to meet where a moment belongs to two threads.
- A group's size comes from its total minutes.
- Future days show only calendar rings.

**Slice 6, the Moment level.**
- `GET /api/sky/moment` returns what happened inside the window: arriving and leaving (from the visit), messages in the thread with its people, transcription lines, and transactions.
- Photos wait for a taken-at time. There is no photo table today, so the Moment level ships without them.
- The moment's dot grows into the band, and the pieces come out of it.

**Slice 7, care and edge cases.**
- **Private places:**
  - Move `PRIVATE_PLACE` out of `day_picture.rs` into one shared classifier, and add `wiki_places.category = 'health'`.
  - A private moment has no label anywhere: not on hover, at any zoom level, or in the share image. Its only link is "Talk about this privately".
  - A per-place switch in place review can follow.
- **The day ends when you sleep:** after midnight, today keeps going until the next night starts (`sleep.rs`).
- **Time zones:** the x axis is elapsed time, and a small fold marks where the clock jumped. `timeline::get_day` already carries `zone`.
- **Quiet days and hard days:** no unusual ink and a softer suggestion. The suggestions job decides the question, not the sky.
- **First days:** no unusual ink until there are 3 baseline days, the same rule as `novelty.rs`.
- **Two moments at once** (a call during a walk): a small offset, and a hover that names both.

**Slice 8, the parts that make it personal.**
- **Keep this moment:** a new table of kept moment urls with the time they were kept. Claim a migration number first with `make migration`. Kept moments show a glint at every level, and the chat can read them.
- **One year ago today:** a dashed ghost dot where a visit happened at the same place a year before, give or take 3 days.
- **Birthdays of people you named:** taken from the all-day birthday calendar entries that `/api/calendar/upcoming` currently filters out. They show as a small star in the week view.
- **Shooting star:** once a month at most, toward the rarest moment. It needs slice 3.
- **Share card:** drawn in the browser from the moment list, with the date and the mark only. Nothing is uploaded until the owner shares it.

## Decisions to make

- **The setting's default:** on or off. The suggestions switch defaulted to on through its applet.
- **Merging and the cap:** whether a busy day caps at about 25 dots by merging moments that are close together, or by hiding the plainest ones.
- **Message threads as moments:** whether they count at all, or only calls do. Threads are the noisiest source.
- **Old wiring:** whether `applets/dot_cloud`, which the owner called deprecated, is removed when the sky ships. Its query also reads "today" in the database's timezone, not the owner's.

## What done looks like

For each slice:
- the endpoint has tests on invented fixtures
- the sky has been looked at on dragon's real data, read-only
- the slice ships through a staging prerelease

The plan is deleted when slice 8 ships. A record of the design and a manual
section on the chat sky replace it.
