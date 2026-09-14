# When and where the mic listens: schedule and places

> Status: **Built, unverified on a device, 2026-09-14.** Written after a read
> of the audio plugin, the location probe, the ingest applet, and the
> day-summary dossier (the facts table below is that read), and built the
> same day in two commits: the box side (migration 0018, the flag through
> both place APIs and the place page's "Don't record here", `muted_by` into
> `data_audio_recording.metadata`, muted runs in the day dossier with a
> prompt rule, the plugin's Rust surface), then the phone half (the
> `muteReason` gate, schedule store with the quiet-hours migration, place
> cache, location→audio fix push, OS regions for the muted places, muted
> markers, the mobile screen's schedule editor and place search, `POST
> /api/entities/places` and `GET /api/places/details`). **Not done:** a
> native iOS build and the device walk in the slice gates below; the
> "sync on app open" hook (the muted places copy onto the phone when the
> device screen loads, not yet at launch — the app layout was mid-edit by
> another change); census counting muted minutes (census.rs was likewise
> claimed). Delete this file once the device gates pass; what survives is a
> page under `docs/` for the phone's recording settings and a record.

## The story, in one paragraph

The phone records continuously, and the only way to say "not now" is one
quiet-hours window that applies every day. There is no way to say "not on
Sundays", "only during the work week", or "never at the clinic". The second
of those is the one people actually ask for: the consent interstitial already
warns that the mic records other people, and a time window is a poor answer
to that worry because the people you should not record are at a *place*, not
at an hour. Both features are gates on the same line of code. Quiet hours
already established the one rule that makes them safe on iOS: **mute, never
release** — the capture graph stays armed and the tap stops writing chunks,
because iOS will not restart a background audio session for us when the
window ends. Everything below is a generalization of that one gate, plus
making the box able to tell a deliberate silence from a broken mic.

## Goal

1. **One gate, two reasons.** The tap asks a single question per buffer,
   "why am I muted right now", and the answer is nothing, a schedule, or a
   place. The session is never stopped by either.
2. **A schedule per weekday.** Quiet hours become the degenerate case of a
   list of muted windows per day of the week, in local wall-clock time.
3. **Places that mute are wiki places.** "Mute here" is a property of a
   `wiki_places` row, not a second list of coordinates on the phone. A person
   finds a place by name; the phone caches the muted subset so the gate runs
   offline. State changes only on evidence: a stale or coarse fix never
   flips it.
4. **The box can tell.** A muted stretch reaches the box as "covered, kept
   nothing, by choice", distinct from silence and distinct from a dead
   collector, and the day narrative says so instead of reporting a hole.

**Non-goals.** Battery: muting does not save the mic (it stays hot, as quiet
hours does today); the honest saving is the chunk encode and the drain, and
that is all this claims. Geofence-triggered *start* of recording (iOS cannot
give us that for audio). Mirroring the *schedule* to the box — nothing on
the box models a weekly schedule today, and the phone is the only thing that
evaluates it; slice 4 sketches it and it is deferred. A map picker. Android.

## What is true today

Read against the code on 2026-09-14.

| Where | What | Why it matters |
|---|---|---|
| `apps/web/plugins/audio/ios/Sources/Audio.swift:78-108` | Quiet hours: two ints in UserDefaults (`virtues.audio.quietStart`/`quietEnd`, minutes since local midnight, `-1` off, `start>end` wraps midnight), gate in `quietHoursActive(_:)` | The schedule generalizes this exact function; the storage keys must migrate |
| `Audio.swift:549-565` | The gate runs inside the realtime tap: when active, finalize the open chunk once and `return`; `lastBufferAt`/`lastGood` keep stamping so watchdog, gap nudge and location power-mode read "healthy, muted" | This is the mute-don't-release contract. Both new gates plug in here and nowhere else |
| `Audio.swift:48-51` | `chunkSeconds=300`, 16 kHz, 24 kbps AAC mono, hard constants | Not touched |
| `Audio.swift:637-692` | `finalizeAndEnqueue` builds the record: `id, audio_format, timestamp_start/end, duration_seconds, is_silent, average_db_level`, plus `audio_data` unless silent. Silent chunks ship metadata-only | The muted marker rides this exact path with one more key |
| `apps/web/plugins/reach/src/ffi.rs:45-58` | Silent records are deferred to a 30-minute wall-clock grid before draining | Muted markers inherit the deferral for free |
| `apps/web/plugins/audio/src/models.rs:15-40`, `AudioPlugin.swift:11-22` | `SetQuietHoursRequest{start,end}`; `AudioStatus` requires `authorized`/`recording`, quiet fields optional; one `fullStatus()` builder that every command resolves | New fields must be optional on the Rust side and must be added to `fullStatus()`, or the Svelte screen wipes them on the next command |
| `apps/web/plugins/audio/build.rs:6`, `src/lib.rs:49-56` | Command list is the ACL: `enable, disable, resume, status, set_notify, set_quiet_hours` | A new command needs a native build; the SPA can be OTA'd ahead of it, so Svelte feature-detects |
| `apps/web/plugins/location-probe/ios/Sources/LocationProbe.swift:166-167, 196-206` | `startMonitoringSignificantLocationChanges()` + `startUpdatingLocation()`, never stopped; every `didUpdateLocations` calls `virtues_ensure_recording()` | A continuous fix stream already exists, and there is already a C-ABI seam from location into audio |
| `LocationProbe.swift:72-96, 270-310` | Motion-adaptive power mode: `.coarse` = 3 km accuracy + 100 m filter when stationary; `.precise` = 10 m | In coarse mode a fix cannot resolve a 100 m place. The place gate must be accuracy-aware |
| `LocationProbe.swift:483-486`, `Audio.swift:16-17` | Audio pushes `0/1/2` health into location via `virtues_location_audio_state` | The mirror direction (location pushes a fix into audio) is the same shape |
| `apps/` (grep) | No `CLCircularRegion`, no `startMonitoringVisits`, no CoreMotion | Region monitoring is available and unused; slice 2 adds it for the muted places |
| `apps/web/src/lib/components/mobile/MobileDeviceScreen.svelte:332-354, 610-640` | The only audio settings UI: notify toggle, quiet toggle, two `<input type="time">`; default 22:00→07:00 | Grows into the schedule and places editor |
| `MobileDeviceScreen.svelte:582-611` | Consent interstitial about recording other people | Places is the answer to that copy; the copy should say so |
| `applets/ios_ingest/microphone.rs:161-228` | `ingest_one` inserts `data_audio_recording`; silent → `audio_url NULL`; **`metadata` is always `json!({})`** | The muted reason is dropped on the floor today; ingest must carry it |
| `virtues-core/migrations/0001_initial.sql:897-913` | `data_audio_recording(... is_silent, average_db_level, metadata jsonb ...)` | `metadata.muted_by` needs no migration. Migrations are append-only on live boxes; a column is not worth it |
| `virtues-core/src/api/day_summary.rs:1486` | The presence dossier already distinguishes `stale` ("COLLECTOR STOPPED; the gap after this is our blind spot") from a real gap | The audio dossier needs the same distinction for "muted by choice" |
| `0001_initial.sql:1869-1890`, `virtues-core/src/entity_resolution/places.rs:880-930` | `wiki_places(name, latitude, longitude, radius_m DEFAULT 100, metadata)` produced by clustering `data_location_point` → `data_location_visit`; a clustered place is born named `Location 30.2700, -97.7400` (`reverse_geocode_stub`) | The box already knows the person's places with a radius, but most of them have no name yet. Naming is the on-ramp to muting |
| `virtues-core/src/api/entities.rs:77-119, 164-219`, `server/mod.rs:574-584` | `GET/PUT/DELETE /api/entities/places[/:id]`; `list_places` returns only rows with `metadata.is_known_location = true`; `create_place` writes `is_known_location: true, source: "user"` into metadata and `radius_m 50` | `is_known_location` lives in metadata and costs a jsonb cast in the `WHERE`; the mute flag is a column instead. `radius_m 50` is too tight for GPS jitter |
| `virtues-core/src/api/home.rs:176-216`, `server/mod.rs:458` | `GET /api/places/unnamed`: clustered places still named `Location %`, ranked by visit count | The naming door exists; the phone can offer "your unnamed places" ranked by how often you are there |
| `virtues-core/src/api/places.rs`, `server/mod.rs:660-667` | `GET /api/places/autocomplete`: Google Places (New) via the virtues-api proxy; `get_place_details` resolves a prediction to coordinates | Search by name for a place you have never been to (next week's clinic) already exists |
| `virtues-core/src/api/wiki.rs:551-569`, `server/mod.rs:794` | `GET /api/wiki/places`: every wiki place ordered by ref count then name | The phone can filter a few hundred names client-side; no search endpoint is needed |
| `apps/web/plugins/reach/src/lib.rs:645` | The phone's webview talks to the box at `http://127.0.0.1:<port>` over iroh loopback | There *is* a downward channel, at the SPA layer, while the app is open. The native plugin has none |

## Design

### The gate

One function replaces `quietHoursActive`:

```swift
enum MuteReason: String { case schedule, place }
func muteReason(at now: Date) -> MuteReason?
```

evaluated once per tap buffer, cheap (a few comparisons and one haversine
against a list that is almost always shorter than five). Schedule is checked
first because it needs no location. The tap body at `Audio.swift:549` keeps
its exact shape; `if quietHoursActive(now)` becomes `if let why =
muteReason(at: now)`, and `why` is remembered so the marker (below) can name
it.

State transitions are logged with `NSLog("[Audio] mute=… reason=…")` and
pushed into the rolling log the location probe already writes, so the
device console and the Recent-activity list show them.

### Schedule

**Model.** A list of muted windows per weekday, local wall-clock, minutes
since midnight, the same wrap rule quiet hours has (`start>end` spans
midnight and the span belongs to the day it starts on):

```json
{"v":1,"default_muted":false,"days":{"mon":[[1320,420]],"tue":[[1320,420]],"sat":[[0,1440]]}}
```

Stored as one JSON string under `virtues.audio.schedule`. Quiet hours is the
case where all seven days carry the same single window, and the editor's
first view is exactly that: one window, "same every day", with a "customize
by day" disclosure below it.

**One default, and windows that invert it.** Some people want quiet hours
("record, except at night"); some want office hours ("don't record, except
9–5"). That is not two lists, it is one boolean: `default_muted` says what
happens outside every window, and a window flips it. `false` plus a
22:00→07:00 window is quiet hours. `true` plus a 09:00→17:00 window on
weekdays is work-only. A fresh install is `false` with no windows, which
records, so the empty case never surprises anyone. The editor shows it as
one sentence, "Outside these hours: record / don't record", above the
windows. Whitelist and blacklist fall out of the same store and the same
gate, and nobody has to maintain the complement by hand.

**Migration.** On first read, if the schedule key is absent and the old two
ints are set, synthesize the seven-day schedule from them and write it. The
old keys are left in place (an older native build reading them still gets
the window). `set_quiet_hours` stays as a command and writes the seven-day
form; `set_schedule` is new. `AudioStatus` grows an optional `schedule`
field; the Svelte screen shows the per-day editor only when the field is
present, which is how it survives an OTA'd SPA on an older native build.

**Time zones.** Local wall-clock, as today. A window follows the phone when
it travels. A muted marker carries the reason, not the window, so the box
never has to reason about zones.

### Places

**A muted place is a wiki place.** The person already has `wiki_places`:
clustered from their own location trail, named by them, with a radius the
box uses everywhere else. Keeping a second list of coordinates on the phone
would repeat that data and hand the user a lat/lon editor. So the mute is a
property of the place, stored where the place's other user-set flags live:

```sql
ALTER TABLE wiki_places ADD COLUMN is_audio_muted boolean NOT NULL DEFAULT false;
```

One append-only migration, claimed with `make migration`. A column rather
than a metadata key because the schema is shown to a model at runtime and
drives a table-driven UI, so a self-describing `is_` boolean is precisely the
convention, and because `list_places` already pays for one
`(metadata->>'…')::boolean` cast in its `WHERE` and should not grow a
second. Nothing indexes it; a few hundred rows are scanned in microseconds.

**The phone caches the muted subset.** The gate runs in the realtime tap
with no network, so the phone holds a cache under `virtues.audio.places`:

```json
{"v":1,"fetched_at":"…","places":[{"id":"place_…","name":"Clinic","lat":30.27,"lon":-97.74,"radius_m":150}]}
```

It is refreshed from `GET /api/entities/places` on app open and after any
edit made from the phone, and pushed into the native plugin with a new
`set_places` command. The cache is a copy, never an authority: the desktop
can flip `is_audio_muted` on a place's wiki page and the phone picks it up next
open. If the box is unreachable the cache stands.

**Finding a place by name.** One search field on the mobile screen, three
sources behind it, merged and ranked:

1. **Your named places** — `GET /api/wiki/places`, filtered client-side.
   Home, work, school: one tap each.
2. **Your unnamed places** — `GET /api/places/unnamed`, ranked by visits,
   shown as "somewhere you've been 14 times, last Tuesday". Choosing one
   names it *and* mutes it in one step, through the existing `PUT`. This is
   the door that turns the clustering's `Location 30.27, -97.74` rows into
   the record's vocabulary, which the wiki wanted anyway.
3. **Anywhere else** — `GET /api/places/autocomplete` for a place the
   person has never been, resolved to coordinates by `get_place_details`,
   then `create_place` with `is_audio_muted` set. Next week's appointment.

**"Mute here."** Still the one-tap door for someone standing in the waiting
room: `create_place` with the current fix, a name prompt, and `is_audio_muted`.
If there is already a wiki place within its radius, offer to mute that one
instead of minting a twin. Requires reach to the box; when the box cannot
be reached, say so and do not fake a local place.

**Radius.** `wiki_places.radius_m` is the radius, editable in the row via
the existing `PUT`. The gate uses `max(radius_m, 100)` for the enter test so
a user-created place's default 50 m does not lose to GPS jitter.

**The fix reaches audio the way health reaches location.** A C-ABI mirror
of `virtues_location_audio_state`: the probe calls
`virtues_audio_location(lat, lon, accuracy_m, age_s)` from
`didUpdateLocations`, right beside the existing `virtues_ensure_recording()`
call. Audio keeps the last fix and its timestamp; nothing else in the probe
changes.

**Sticky, accuracy-aware transitions.** The place state is `inside(place)`
or `outside`, and it changes only when a fix proves it:

- enter when `distance + accuracy < r`, with `r = max(radius_m, 100)`
- exit when `distance − accuracy > max(r × 1.5, r + 50)`

A coarse fix with 3 km accuracy proves neither and is ignored. A stale fix
(no update for 15 minutes) changes nothing. This is the answer to "what if
we don't know where the phone is": we keep the last thing we knew. Someone
who walked into a muted place and whose phone then went coarse because they
sat still stays muted, which is the safe direction for the feature's
purpose; someone who was outside stays recording. The hysteresis band stops
the writer flapping at the boundary.

**Regions: the OS watches the circles too.** A *place* is our row; a
*region* is that place's circle handed to iOS as a `CLCircularRegion`, so
the OS itself fires enter and exit callbacks for it. That covers the two
cases the fix stream cannot: a phone that has gone coarse (region crossings
are resolved by the system with whatever radios it has) and a cold relaunch
by significant-location-change, whose first fix may be coarse. Region
callbacks feed the same sticky state machine as fixes, with the OS's verdict
treated as a fix of accuracy zero. iOS caps regions at 20 per app, so the
probe registers the 20 muted places nearest the last fix and re-registers
on every significant-location change; a person with more than 20 muted
places is served by fixes for the rest. Regions never *start* anything;
they only inform the gate.

**The desktop gets it for free.** Once the column is on the row, the place's
wiki page grows one toggle, "don't record here", and the phone honors it
next open. That is the whole of what slice 4 used to be for places.

### The box can tell

Today a muted stretch produces no chunks at all, so the box sees the same
thing it sees when the app is dead. The fix is a **muted marker**: at each
rotation boundary while muted, `finalizeAndEnqueue` is bypassed and a
metadata-only record is enqueued on the same `microphone` stream:

```json
{"id":"…","audio_format":"m4a","timestamp_start":"…","timestamp_end":"…",
 "duration_seconds":300,"is_silent":true,"average_db_level":null,
 "muted_by":"place"}
```

It rides the silent path end to end: no bytes, 30-minute deferred drain,
`audio_url NULL` on insert. The one change on the box is `ingest_one`
copying `muted_by` into `metadata` instead of writing `{}`. `average_db_level`
is null because nothing was measured; readers that compute on it must treat
null as "no measurement", not as silence, and per CLAUDE.md that means `?`
and an explicit comment, not `unwrap_or`. The marker names the *reason*, not
the place, so it discloses nothing the location stream does not already
carry.

Then the readers:

- **Sessionize** (`sessionize/audio.rs`) skips markers when forming sessions
  (they are not silence-with-a-level; they are absence).
- **Day summary** renders a muted run in the dossier with the same
  discipline the presence dossier uses for `stale`: "mic muted by schedule
  (their choice) — not a blind spot, and not evidence of anything".
- **Census / device page** counts muted minutes beside silent minutes and
  recorded minutes so "why is today's audio short" has an answer.

One marker per five minutes is at most 288 rows a day, the same order as a
quiet home's silent chunks today.

### Slice 4, deferred: the schedule on the box

Places are on the box from day one (above). The schedule is not: nothing on
the box models a weekly schedule, and the phone is the only thing that
evaluates it. When desktop editing of the schedule is wanted, the shape is a
small document against the device (`app_device.device_info` already exists
as a jsonb bag; whether it is the right home is to be decided then, and an
append-only column is fine if not), pushed by the phone on change and pulled
on open. Not before slices 1–3 ship and are used.

## Slices

Each slice is a native iOS build. Verify on the review device before the
next; the audit history of this plugin says every "obviously fine" change to
the tap has surprised us once.

1. **Gate + schedule (phone only).** `muteReason`, schedule store and
   migration, `set_schedule` command through Rust/build.rs/permissions,
   `schedule` in `fullStatus()` and `AudioStatus`, the per-day editor in
   `MobileDeviceScreen.svelte`, `set_quiet_hours` kept as a wrapper.
   Gate: quiet hours behave exactly as before on the migrated store; a
   Saturday-only window mutes on Saturday and not Friday at the same hour;
   the watchdog stays quiet and location stays coarse through a window.
2. **Places (box + phone).** Box: migration for `is_audio_muted`, honored
   by `update_place`/`create_place`, returned by `list_places`. Phone: the
   location→audio push, `set_places` command and cache, sticky
   accuracy-aware transitions, region monitoring for the muted places, the
   one search field over named / unnamed /
   autocomplete, "Mute here", radius edit, one sentence added to the
   consent interstitial. Desktop: the toggle on the place's wiki page.
   Gate: walk into and out of a muted place with the console attached and
   see exactly one enter and one exit; sit still inside until coarse and
   confirm no exit; kill and relaunch inside and confirm muted on first
   precise fix; flip the toggle on the desktop and confirm the phone
   honors it after reopening.
3. **The box can tell.** Muted markers, `muted_by` through ingest into
   `metadata`, sessionize skips them, day summary and census read them.
   Gate: a day with a muted afternoon narrates "muted by place" and the
   coverage figure separates muted from silent from recorded.
4. **Schedule on the box.** Deferred; see above.

## Decisions

All decided 2026-09-14, none open:

- **Schedule:** one `default_muted` boolean with windows that invert it, so
  quiet hours and office-hours-only are the same store and gate.
- **Stale fix:** sticky. The gate keeps whatever state it last proved until
  a fix or a region callback proves otherwise. Never fail-open on silence
  from the location stack.
- **Markers:** yes. A metadata-only record every five minutes while muted,
  naming the reason and never the place.
- **Regions:** in v1, feeding the same state machine as fixes.
- **Flag:** a column, `wiki_places.is_audio_muted`, one append-only
  migration.
