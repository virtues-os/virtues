# When and where the phone's mic records: schedule, places and pause

**Written 2026-10-20.** The schedule, muted places and muted markers were
built 2026-09-14 (migration 0018 and the box side, then the phone side); the
Control Center pause control and the recording widget were built 2026-10-07.
Device verification is still open and is tracked in
[audio-schedule-places-plan.md](../plan/audio-schedule-places-plan.md). How
the recorder itself works is in [audio-collector.md](audio-collector.md).

## Why

Before this, the only way to stop recording for a while was one quiet-hours
window that applied every day. People also want "not on Sundays", "only on
work days", and "never at the clinic". The consent screen already warns that
the mic records other people, and the people you should not record are at a
place, not at an hour, so places matter more than times.

Quiet hours had already established the rule that makes any of this safe on
iOS: mute the tap and never release the audio session. iOS will not restart a
background audio session for the app, so the capture graph stays armed and the
tap simply stops writing chunks. Everything here generalizes that one gate and
lets the box tell a chosen silence from a broken mic.

## What was built

### One gate

`muteReason(at:)` replaced `quietHoursActive`. The tap asks it once per buffer
and gets nothing, `schedule`, `place` or (since 2026-10-07) an override. The
schedule is checked first because it needs no location. Transitions are logged
as `[Audio] mute=… reason=…`. Muting does not save the mic's power (it stays
on, as with quiet hours); it saves the chunk encode and the upload.

### Schedule

A list of muted windows per weekday, in local wall-clock minutes since
midnight, with the quiet-hours wrap rule (`start>end` spans midnight and
belongs to the day it starts on), stored as one JSON string under
`virtues.audio.schedule`:

```json
{"v":1,"default_muted":false,"days":{"mon":[[1320,420]],"tue":[[1320,420]],"sat":[[0,1440]]}}
```

`default_muted` says what happens outside every window, and a window inverts
it. `false` plus a 22:00→07:00 window is quiet hours; `true` plus 09:00→17:00
on weekdays records only during work hours. A fresh install is `false` with no
windows, so it records. The old quiet-hours ints migrate into a seven-day
schedule on first read and are left in place for older native builds;
`set_quiet_hours` still works and writes the seven-day form, and
`set_schedule` is new. `AudioStatus.schedule` is optional, and the Svelte
editor shows the per-day view only when it is present, so an OTA'd SPA works
on an older native build. Windows follow the phone's local time when it
travels.

### Muted places are wiki places

A muted place is a `wiki_places` row with `is_audio_muted = true` (migration
0018). A column was chosen over a metadata key because the schema is shown to
a model and drives a table UI, and `list_places` already pays for one jsonb
cast in its `WHERE`. The phone caches only the muted subset under
`virtues.audio.places` and pushes it into the plugin with `set_places`
(`apps/web/src/lib/mobile/audioPlaces.ts`), so the gate runs with no network.
The cache is a copy: the place's wiki page has a "Don't record here" toggle,
and the phone picks it up the next time it refreshes.

The owner finds a place by name from one search field over three sources:
named places (`GET /api/wiki/places`, filtered on the phone), unnamed
clustered places (`GET /api/places/unnamed`, ranked by visits; choosing one
names and mutes it in one step), and anywhere else (`GET
/api/places/autocomplete`, then `POST /api/entities/places`). "Mute here"
creates a place at the current fix, offers to mute an existing place if one
is within its radius, and refuses when the box cannot be reached rather than
making a local-only place.

### Location reaches the gate

The gate uses `r = max(radius_m, 100)` so a user-created place's default 50 m
does not lose to GPS jitter. State changes only when a fix proves it:

- enter when `distance + accuracy < r`
- exit when `distance − accuracy > max(r × 1.5, r + 50)`

A coarse fix (3 km accuracy when stationary) proves neither and is ignored; a
fix older than 15 minutes changes nothing. So a phone that went coarse inside
a muted place stays muted, which is the safe direction. The probe also
registers the 20 muted places nearest the last fix as `CLCircularRegion`s (the
iOS limit), re-registered on each significant-location change. Region
callbacks feed the same state machine as a fix with accuracy zero; they cover
a phone that has gone coarse and a cold relaunch whose first fix is coarse.

Since 2026-10-13, location no longer re-arms audio and audio no longer pushes
its health into location (`virtues_ensure_recording` and
`virtues_location_audio_state` are gone); see "The background rule" in
[audio-collector.md](audio-collector.md).

### The box can tell a chosen silence

While muted, the phone enqueues a metadata-only record at each five-minute
rotation on the `microphone` stream, with `"is_silent":true`,
`"average_db_level":null` and `"muted_by":"schedule"|"place"|"pause"`. It
follows the silent-chunk path (no bytes, 30-minute deferred drain,
`audio_url NULL`). Ingest copies `muted_by` into
`data_audio_recording.metadata`. A null `average_db_level` means no
measurement, not silence. The marker names the reason, never the place. At
most 288 rows a day. The day summary's dossier renders a muted run as a
choice by the owner rather than a blind spot.

### Pause from Control Center, and the widget (2026-10-07)

A control that overrides the rules for a while and never stops the mic,
because a backgrounded app could switch a released mic off but not on again.
The orange dot stays lit, so it says "Paused", never "Mic off"; the in-app
Stop is still the real off, and Enable clears any override.

- A tap flips "is Virtues keeping audio right now?" for the length chosen when
  the control was added: 1 hour (default) or until ended. Off from recording
  is a silence; on while a schedule or place mutes is "record anyway".
  Every override ends by itself: a silence at its time or on resume, a record
  at its time or when the rules stop muting, whichever is first, so recording
  anyway at a muted place cannot outlast the visit.
- Recorder (`Audio.swift`, MARK: Override): `manual` behind `policyLock`;
  `gate(at:)` applies it ahead of the rules. It is read from the App Group's
  defaults at launch, on a Darwin notification the control posts, and on
  every event-driven `ensureRecording`.
- Plugin: `set_override {mode, minutes}`; `override`/`overrideUntil` in
  `fullStatus()`/`AudioStatus`; `mutedBy` gains `"pause"`.
- Extension (`gen/apple/VirtuesControls/`): an iOS 18 configurable
  `ControlWidget`, usable on the Lock Screen and the Action button too. The
  target was added to the Xcode project by hand (never `xcodegen generate`);
  a build phase stamps the app's version onto it and
  `tools/verify-ios-archive.py` checks it. App Group `group.com.virtues.app`
  is on `com.virtues.app`, `com.virtues.app.controls` and
  `com.virtues.app.share`.
- Widget (`RecordingWidget.swift`): one sentence ("Recording since 7:02", "No
  word since 3:40"), times instead of counts ("Up to date through 3:38"),
  today as a strip of kept / muted / gap, a countdown while paused. The Lock
  Screen ring is the share of today kept or muted by choice. It reads the
  recorder's heartbeat and today's stretches from the App Group, and
  `uploads.json` from the reach plugin (`reach/src/widget.rs`). Every timeline
  carries a "No word since" entry 45 minutes past the heartbeat, because a
  dead recorder cannot report itself. "Can't reach your server" needs 30
  minutes with no answer at all; a box that answers "skipped" (busy with an
  earlier batch) is reachable.

## Decisions (2026-09-14)

- Schedule: one `default_muted` boolean with windows that invert it.
- Stale fix: keep the last proven state until a fix or region callback proves
  otherwise. Never unmute because the location stack went quiet.
- Markers: yes, every five minutes while muted, naming the reason and never
  the place.
- Regions: in v1, feeding the same state machine as fixes.
- Flag: a column, `wiki_places.is_audio_muted`.
- Out of scope: starting recording on a geofence (iOS cannot do that for
  audio), a map picker, Android.

## Traps

- A patch that added three fields to `AudioStatus` in the plugin's Rust was a
  silent no-op on 2026-09-14; check the field actually landed. New fields must
  also go into `fullStatus()`, or the Svelte screen wipes them on the next
  command.
- A new plugin command needs a native build (the command list in `build.rs`
  is the ACL); the SPA can be OTA'd ahead of it, so Svelte feature-detects.
- TestFlight auto-update can silently replace a dev install.
