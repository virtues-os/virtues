# Audio collector — scoping

Porting continuous microphone capture from the old Swift app to the Tauri mobile stack.
Audio is the **hardest** collector, not because of the recording API, but because of iOS's
audio-session lifecycle and the fact that audio is the only *continuous, high-volume, binary*
stream. This doc scopes the port before any code.

## Phase B (EXPANDED 2026-07-10) — background continuity & resurrection

Background recording now *works* (the `audio` `UIBackgroundMode` was silently dropped by Tauri's
`Info.ios.plist` merge — fixed). Phase B makes it **bulletproof**: recording that survives, and
resurrects itself from, every interruption/suspension/kill — with no user action.

### B.0 Mental model — the two keepalives + one idempotent entry point
The app stays alive in the background via **audio recording OR location updates** (both are
`UIBackgroundModes`). Audio is fragile (any interruption stops it); **location is the durable
backbone** — CLLocationManager fires callbacks continuously while moving, and significant-location
**relaunches a *terminated* app**. iOS will NOT relaunch/wake an app *for audio*, but it will for
*location* — so location is how audio comes back from the dead.

Therefore the whole design collapses to: **one desire flag + one idempotent restart, fired by every
"we're awake" signal.**
- **Desire flag:** `shouldRecord = authorized && enabled` (the persisted `enabledKey`). Single truth.
- **`ensureRecording()`:** idempotent — start only if `shouldRecord && !recording`. Safe to spam.
- **Phase B = wire every wake/alive signal to `ensureRecording()`.**

### B.1 The resurrection/sustain vectors (the piggyback list)
Every one of these calls `ensureRecording()`:
| Vector | When it fires | Covers |
|---|---|---|
| Interruption `.ended` | call/Siri ends | resume after a call (the reported bug) |
| **Location `didUpdateLocations`** | continuously while moving (app alive) | the workhorse — restarts audio seconds after any stop, while alive |
| **Significant-location relaunch** | movement after the app was killed/suspended | resurrection from *terminated* (cold `setup()` → resume) |
| `BGProcessingTask` (`com.virtues.ios.sync`) | iOS periodic (charging/idle) | movement-independent resurrection |
| App foreground (`didBecomeActive`) | user reopens | guaranteed restart (has it) |
| Route change `.ended`/device change | headphones connect/disconnect | already rolls chunk; add ensure |
| Self-heal timer (~30s, while alive) | periodic while process alive | catches any silent stop |

Cross-plugin mechanism: audio exposes **`@_cdecl("virtues_ensure_recording")`**; location-probe +
health `BackgroundSync` call it via `@_silgen_name` (same pattern as reach's
`virtues_recover_connection`). This is the "any other hardening we can piggyback off of" — the
location + BGTask wakes already exist for the other collectors; audio just hooks the same events.

### B.2 Fix the interruption resume (the call bug)
- On `.ended`: **always** `ensureRecording()` — drop the `.shouldResume` gate (after a call iOS often
  omits it; an always-on recorder always wants back).
- **Wrap the restart in `beginBackgroundTask`** so we have execution time to reactivate the session +
  start the recorder in the background (see B.4 — this is likely what lets a bg restart succeed).
- On `.began`: finalize the current chunk (already do). Keep the desire flag set.

### B.3 mediaServicesWereReset + robustness
- Observe **`AVAudioSession.mediaServicesWereResetNotification`** → rebuild session + recorder + restart
  (full audio-subsystem reset; rare but total).
- **Wall-clock chunk rotation:** don't trust a 5-min `DispatchSourceTimer` in the background (jitter /
  coalescing). Reconcile the boundary against `Date` inside the handler.

### B.4 The two HARD iOS unknowns — RESOLVED by research (2026-07-10), and they reshape the plan
Both were answered definitively (sourced), and the answers are strict:

1. **You can NEVER start the mic from the background.** Confirmed by Apple (error `561145187
   cannotStartRecording`, DTS: background audio "only allows you to *continue* a session created in
   the foreground"). NOT enabled by `beginBackgroundTask`, NOT by being alive via continuous
   location, NOT by a significant-location cold relaunch. iOS gates on *why* the process is awake;
   "location" is not a mic grant. ⇒ **After a call or a kill, the mic resumes only on next
   FOREGROUND.** The location-wake→ensureRecording piggyback **does not work for audio** (it still
   helps the other collectors). Apple DTS on post-call: "don't resume in the background; resume when
   you return to foreground."
2. **`AVAudioRecorder` stop→start rotation DIES at the first background chunk boundary** — a
   boundary stop→start *is* a background start (same `cannotStartRecording`). Bonus: a known ~90-min
   silent-death in the restart-loop pattern. ⇒ **The current approach is unshippable for >5 min bg.**

### B.4′ DECIDED architecture: never-stopped capture graph (the "splice, don't restart" rewrite)
Replace `AVAudioRecorder` chunk-restart with a graph that is **armed once in the foreground and never
stopped**; rotate only the OUTPUT.
- **`AVAudioEngine` + `installTap(onBus:0)` on `inputNode` → rotating `AVAudioFile`.** At a boundary,
  release the old `AVAudioFile` (dealloc finalizes the moov atom — the #1 corruption bug is forgetting
  this) and point at a new one, **inside the tap block**. No stop, no gap, no background-start. Each
  rotated file is a **standalone `.m4a`** → box pipeline unchanged. (Alt: `AVAssetWriter` +
  `preferredOutputSegmentInterval` gives gapless segments as `Data`, but they're fMP4 — needs box
  changes; rejected for now.)
- **Format:** the tap runs at the hardware rate (~48 kHz). `AVAudioFile` encodes AAC directly but does
  NOT resample → either write **48 kHz mono AAC** (simplest; Gemini downsamples to 16 kHz anyway; ~3×
  storage) or add an `AVAudioConverter`/mixer for **16 kHz** (3× smaller — matters for the 4 GB cap).
  Decision pending; default 48 kHz for v1 simplicity, revisit if storage/bandwidth bites.
- **Residual fragility:** `AVAudioEngineConfigurationChangeNotification` (route change / calls /
  media-services-reset) **stops the engine** → restart + reinstall tap. Restarting in the background
  hits the same cannot-start wall → so a route change while backgrounded may pause until foreground.
  Net win: trades a *guaranteed every-5-min* start for a *rare, route-change-only* one. Handle
  interruption + config-change + `mediaServicesWereReset` together with guard flags; resume into a NEW
  file per resume.

### B.4″ Honest capability statement (set expectations)
Not true 24/7. Delivered: **continuous background recording for hours once armed in the foreground;
gaps only after a call or kill, until the app is next foregrounded.** The market leaders (Limitless,
Bee, Omi/Friend) avoid this ceiling entirely with **BLE pendant hardware + on-device buffering** —
the only real path to bulletproof 24/7, and a hardware decision, not software. Flag for product.

### B.5 Observability (so we can SEE it, per this session's pain)
This-device surfaces: recording live? last-chunk time, chunk count, queued audio bytes. Turns "is it
running?" from a guess into a readout. NSLog breadcrumbs already in place.

### B.6 The general "wake → do everything" coordinator (benefits ALL streams)
The location wake + BGProcessingTask already fan out to *some* collectors. Generalize: one background
wake → `ensureRecording()` + health collect + drain. Audio is just another subscriber; every stream
gets more resilient for free. (Don't over-fire: guard each with its own throttle/desire check.)

### B.7 App Store note
Always-on ambient mic is a Guideline **2.5.4** rejection pattern. Ship a clear user-visible recording
control + Review Notes justification before submission. (Tracked, not a Phase-B blocker.)

### Phase B build order (revised after B.4 research)
1. **The `AVAudioEngine` rotating-file rewrite (B.4′)** — the big one; replaces AVAudioRecorder.
   Never-stopped tap + rotating standalone `.m4a`. Kills the 5-min-boundary death. *(most of the work)*
2. **Interruption + config-change + mediaServicesWereReset handling** — re-arm on **foreground**
   (the only place the mic can start); new file per resume; guard flags.
3. **Test on-device** — 30-min background run (multiple boundaries), a call (expect resume on
   foreground), a route change (headphones), Spotify-mix, force-quit→reopen.
4. **B.5 observability** + honest UI copy ("records while the app's been active recently").
5. *(Note: B.1's location/BGTask piggyback stays for OTHER collectors; it CANNOT restart the mic.)*

---

## 0. The one mechanic that changes everything

**A recording audio session is itself the background-keepalive.** With `UIBackgroundModes: audio`
and an *active recording* session, iOS lets the app run **indefinitely** in the background — same
as a music app. The moment recording *stops* (call, other audio, silence-discard gap, error), that
keepalive is gone and iOS suspends the app within its normal ~30 s grace. To record again the app
must be **woken from outside**.

So audio has two distinct regimes:

- **Recording → alive.** While a chunk is recording, the app runs freely; location updates,
  drains, timers all work. This is the good state.
- **Stopped → suspended.** Once recording stops and can't immediately restart, the app dies. Only
  an external wake resurrects it.

Everything finicky about audio is about **surviving the stopped→suspended transition and getting
woken to restart.** This is exactly the behavior described: *"user plays audio / on the phone, it
naturally drops, but the next 5-min ping or a location piggyback restarts it."*

### The resurrection path (already proven in our stack)

We already have the one wake primitive that works after suspension **and** after force-quit/reboot:
**significant-location-change relaunch** (`LocationProbe` → `startMonitoringSignificantLocationChanges`,
proven to cold-relaunch the terminated app and run a bounded background task). Audio restart
**piggybacks on that**: every location wake (and the existing `com.virtues.ios.sync`
`BGProcessingTask`) calls `AudioRecorder.ensureRecording()`. If mic permission is granted and we're
not recording, it restarts — which *re-establishes the audio keepalive* and the app goes back to the
"alive" regime until the next stop.

```
recording (audio keepalive holds app alive)
   │  call / Siri / other-app audio / user / silence-discard / error
   ▼
stopped ──immediate self-restart possible?──► yes ─► recording   (stays alive)
   │ no (other audio still playing, or app suspended)
   ▼
suspended  ← app frozen, no keepalive
   │  external wake:  significant-location relaunch  |  BGProcessingTask  |  user foregrounds
   ▼
ensureRecording() → recording   (keepalive re-established)
```

## 1. State machine (all the drop/resume transitions)

Ported from the old `AudioManager`, these are the transitions to reproduce:

| Trigger | Drop behavior | Resume behavior |
|---|---|---|
| **Phone call / Siri / alarm** (`AVAudioSession.interruptionNotification`) | system stops the recorder (`.began`) | on `.ended` with `.shouldResume`: reactivate session + restart immediately. Else wait for wake/health-check |
| **Other app plays audio** (Spotify, video, call audio) — `silenceSecondaryAudioHintNotification` | `.begin` → we **stop** recording (we do **not** mix — see §4) | `.end` → if we stopped *for this reason*, restart |
| **Silence** (chunk avg < −50 dB) | discard chunk, immediately start next chunk | continuous (tiny gap only) |
| **Recorder error / encode failure** | chunk lost | health-check / next wake restarts |
| **App suspended** (recording stopped, keepalive gone) | app frozen | significant-location wake / BGProcessingTask → `ensureRecording()` |
| **User force-quit** | no keepalive, no relaunch-for-audio | next significant-location move relaunches app → `ensureRecording()` |
| **Reboot** | app dead | significant-location relaunch after unlock → `ensureRecording()` |
| **Foreground return** | — | `didBecomeActive` → `ensureRecording()` |

The old app had a **30 s `HealthCheckCoordinator`** that continuously re-asserted "should be
recording but isn't → restart." We reproduce that as a lightweight self-heal timer **while alive**,
but the *durable* healer is the location wake (works even when the timer's process is dead).

## 2. Architecture fit — what reuses vs what's new

**Good news: audio reuses the whole delivery spine.** The old app already sent audio as **base64
inside JSON** through the same `ios_ingest` webhook (stream `microphone`) — it is *not* a separate
blob-upload protocol on the device side. So:

### Reuses as-is
- **Outbox** (`virtues_enqueue(stream, json)`): a finished chunk → base64 → one JSON record →
  enqueue under stream `"microphone"`. One row per chunk.
- **Drain** (`upload.rs`): already generic/multi-stream, byte-bounded (2 MB/batch). A 5-min chunk is
  ~600 KB raw → ~800 KB base64 → one record, ~2 chunks per POST. Fits the 512 MB box body limit
  trivially.
- **Deterministic id**: chunk `UUID` (or `UUIDv5(device + start-instant)`) → idempotent, dedup-safe.
- **Location wake + BGProcessingTask**: the resurrection triggers already exist; we just add an
  `ensureRecording()` call to them.

### New work — device
- **`audio` Tauri plugin** (`apps/web/plugins/audio`): `AVAudioRecorder` (16 kHz mono AAC 16 kbps),
  5-min chunk timer (`ReliableTimer`/`DispatchSourceTimer`), dB metering + silence discard, all the
  §1 notification handlers, `enable()`/`ensureRecording()`/`status()` commands, FFI enqueue.
- **`UIBackgroundModes: audio`** + **`NSMicrophoneUsageDescription`** in `project.yml`
  (then `xcodegen generate`).
- **Piggyback hook**: `LocationProbe.didUpdateLocations` and the sync `BGProcessingTask` call the
  audio plugin's `ensureRecording()` (cross-plugin call via a shared FFI symbol or a small Rust
  coordinator, mirroring `virtues_enqueue`).
- **Queue pressure cap** (see §5).

### Box — ALREADY BUILT & PROVEN (verified 2026-07-09, no work needed)
The box side was built for the old Swift app and is still live/registered. **Nothing to build here.**
- `actions/ios_ingest/main.rs` — `"microphone"` case → `microphone::ingest_all` (`microphone.rs`):
  decodes base64 `audio_data`, writes `.m4a` to `data/lake/ios_microphone/{id}.{ext}`, inserts
  `data_audio_recording` (`audio_url` = that relative path), `ON CONFLICT (source_stream_id) DO NOTHING`.
- `applets/transcription_resolution/` — live cron applet (`0 */2 * * * *`): `LEFT JOIN` finds
  untranscribed `data_audio_recording`, calls **`google/gemini-2.5-flash`** (hardcoded, sanctioned
  exception), writes `data_communication_transcription` (text, title, summary, mood, entities, scene).
  Handles silent (skip Gemini, empty text), poison (no loop-bill), flash-lite-returns-empty.
- Billing: `BearerClient` chokepoint → `app_ai_calls` (feature "transcription"), wallet-metered.
- Downstream: `day_summary` already reads `data_communication_transcription`.

**Device→box record contract** (what the device must enqueue under stream `microphone`), one record
per chunk, matching `microphone.rs`:
```json
{
  "id": "<uuid — deterministic, dedup key>",
  "audio_data": "<base64 of the .m4a>",     // required
  "audio_format": "m4a",                     // default m4a
  "timestamp_start": "2026-07-09T…Z",        // (or "timestamp" fallback)
  "timestamp_end": "2026-07-09T…Z",
  "duration_seconds": 60.0,
  "is_silent": false,
  "average_db_level": -42.3
}
```
So the device just produces these and the existing pipeline does the rest.

## 3. Chunk format + length (DECIDED)
- 16 kHz, mono, AAC (`kAudioFormatMPEG4AAC`) → `.m4a`. (16 kHz mono = exactly what Gemini
  downsamples to; no fidelity wasted.)
- **Chunk length = 5 min.** Chunk length trades ONLY two things; all else (battery/cost/upload) is
  neutral: **shorter = less crash-loss** (a chunk is finalized to a valid `.m4a` only on stop; a hard
  kill *mid-chunk* loses it), **longer = better transcription** (box does one Gemini call per chunk;
  Gemini needs a coherent "scene"). Crash-loss is **rare** — the recording session is the bg keepalive
  so the app isn't suspended while recording, and every *natural* stop finalizes the chunk first — so
  we optimize for coherence. 5 min matches the old app + the 5-min cadence of other streams. Tunable.
- **2-second overlap** between chunks (cheap continuity insurance).
- Per-chunk metadata: `started_at`, `ended_at`, `duration`, `average_db_level`, **`peak_db_level`**.

## 3b. Silence vs ambiance — KEEP ambiance (fixes an old-app bug)
"No talking" ≠ "silence." The old app's `-50 dB` discard **threw away atmosphere** (wind, traffic,
elevator, a dog) — exactly the ambient signal that fills a life narrative. **Fix + simplification:**
- **Do NOT loudness-discard on the device.** Record continuously, send every chunk. Gemini natively
  describes non-speech sound, so ambiance *becomes* narrative.
- Only set `is_silent=true` (box then skips the Gemini call, no bill, still stores the recording +
  "quiet" marker) for **genuinely dead audio**: avg < −60 dB **and** no transient peaks. Safety net:
  the box already marks a recording silent if Gemini returns empty, so a mis-sent dead chunk can't
  loop-bill.
- This deletes the old "discard → restart → UUID-collision" churn entirely.

## 4. Session config + mix strategy — DECIDED (researched 2026-07-09)
The old app's two bad-UX bugs (AirPods → telephone quality; music/YouTube ducked/quieted) were
**config bugs**, not fundamental limits. Exact fix:

```
category: .playAndRecord    mode: .default
options:  [.mixWithOthers, .defaultToSpeaker, .allowBluetoothA2DP, .allowAirPlay]
NEVER:    .allowBluetooth (== .allowBluetoothHFP in the iOS 26 SDK)
then:     setPreferredInput(built-in mic)   // pin input; never setPreferredInput→AirPods
```
- **AirPods fix (CORRECTED after on-device test 2026-07-09):** we are a PURE recorder that plays
  nothing, so we allow **NO output-routing options** — `opts = [.mixWithOthers, .defaultToSpeaker]`,
  and NEVER `.allowBluetoothA2DP` / `.allowAirPlay` / `.allowBluetooth`. Reason: any BT/AirPlay
  *output* option makes iOS advertise for that route and **grab the user's AirPods** — an A2DP-output
  session literally pulled AirPods off the user's *Mac* onto the phone on first enable. (The WWDC
  A2DP-keeps-quality advice only applies when AirPods are on *the phone* and you're playing back; for a
  silent recorder it backfires cross-device.) Tradeoff: if the user is listening on the phone via
  AirPods, starting to record reroutes *their* output to the speaker — acceptable vs. hijacking their
  headphones. Input is the pinned built-in mic regardless of what's on Bluetooth.
- **Don't-degrade-media fix:** `.mixWithOthers` (never interrupt/duck other apps) + `.defaultToSpeaker`
  (kills the default earpiece/call-volume routing).
- **Mode `.default`** — NOT `.measurement` (drops gain + earpiece-routing side effects), `.voiceChat`
  (forces HFP + AGC), or `.videoRecording` (beamforming). `.default` = least interference.

### Strategy: MIX + hard-interruption-yield only (no voluntary yield gate)
Key reframe: **the mic hears the room — including speaker-played music — as ambient regardless.** So
recording *through* other audio buys nothing acoustically; `.mixWithOthers`'s only job is to not
*interrupt* the user. Therefore:
- **MIX** through everything mixable (music/YouTube/podcasts). Capture the ambient soundscape; never
  interrupt them.
- **Delete the old `silenceSecondaryAudioHint` stop-gate** — pointless (we hear the room anyway) AND
  broken in background (that hint is only delivered to foreground apps). Removing it deletes a whole
  finicky subsystem. (Optionally keep the hint as passive metadata "user was playing media at T.")
- **Yield only on hard interruptions (calls/Siri)** — OS-forced, not our choice. `interruptionNotification`:
  `.began` → finalize current chunk; `.ended` (+`.shouldResume`) → `setActive(true)` + start a NEW
  chunk (don't rely on `AVAudioRecorder.pause()`—known to drop pre-interruption audio). Idempotent
  resume, also run on `didBecomeActive`. Segmenting around calls **naturally excludes call audio**
  (which we legally want excluded).

### Route changes = the REAL "AirPods connecting broke it" fix
Observe `routeChangeNotification`: on **`.newDeviceAvailable`/`.oldDeviceUnavailable`** re-pin the
built-in mic + roll to a fresh chunk (input format can change); **ignore `.categoryChange`/`.override`**
(our own edits — reacting loops). **Debounce** the AirPods notification burst (~150 ms) and rebuild once
on the settled route; serialize session mutations on one queue. Using `AVAudioRecorder` (fixed 16 kHz,
resamples) — not a raw `AVAudioEngine` tap — avoids the format-mismatch crash class entirely.

### iOS 26 optional upgrade (not v1)
`.bluetoothHighQualityRecording` (iOS 26, H2 AirPods) records the AirPods *mic* in studio quality
instead of HFP (requires `.playAndRecord` + `.default`). Keep built-in-mic + A2DP as the zero-disruption
default; offer "record from AirPods" as an opt-in later. Permission already via `AVAudioApplication`
(iOS 17+); honor `inputMuteStateChangeHandler` (Control-Center mic mute) without tearing down.

## 5. Queue & storage pressure — DECIDED: keep the 4 GB cap, build it simple, files-on-disk
Audio dwarfs every other stream and accumulates when offline. **Keep the 4 GB cap** (necessary — it's
the one stream that can fill the disk). What the plan cut was the old app's *multi-tier cleanup
ceremony*, not the cap itself. In our stack the 4 GB cap is ~10 lines: **delete oldest audio when
total > 4 GB.**

**Refinement — store `.m4a` on disk, not base64 in SQLite.** Blobbing 4 GB of base64 into the outbox
DB = write amplification + VACUUM pain + 33% inflation held at rest. Instead:
- Audio chunk finalizes → `.m4a` written to an audio dir on disk.
- Outbox row for stream `microphone` holds **metadata + a `file_path` pointer** (tiny row).
- Drain reads the file at send time, base64s (or streams) it into the `ios_ingest` request.
- 4 GB cap = sum of on-disk `.m4a` sizes; over → delete oldest file + its row.
- `log()` what was dropped (no silent truncation). Silence-discard already trims most volume at source.

This is a small extension to the drain (handle file-backed records) and keeps the SQLite DB tiny.

## 5b. Per-stream toggles (all collectors, not just audio) — DECIDED
This-device exposes an **on/off toggle per stream** that actually stops/starts collection (location
updates, health background-delivery, audio recording, …), persisted, and honored on relaunch. Audio's
toggle doubles as its **pause-recording** control. Each plugin gains a `disable`/`stop` command +
persisted enabled-flag; `resume()` on launch respects it. Audio stays a **normal skippable collector
card** in onboarding (not a special default-off gate) — every stream is skippable already.

## 6. Privacy / App Store reality (plan for it, don't be surprised)
- Continuous background mic ⇒ the **orange mic dot is always on**, plus a Control-Center indicator.
  Users *will* see it. This is a **trust surface**, not a bug — the UI must own it (clear on/off,
  "recording" state visible in-app, easy disable).
- **App Store review** scrutinizes always-on mic hard. Needs a strong `NSMicrophoneUsageDescription`,
  a visible in-app recording state, and a usable app with audio **off** (our skip-everything
  onboarding already covers the "usable without granting" requirement).
- All audio lands on the user's **own box** (not our servers) — that's the core justification and
  should be stated plainly in the permission copy.

## 7. Decisions (RESOLVED 2026-07-09)
1. **Raw audio kept on the box** ✓ (`audio_url` blob retained). Transcript is derived, not a
   replacement.
2. **Transcription = Google Gemini 2.5 Flash (cloud), NOT local.** Chosen for real-life/ambient audio
   where we want the *essence and nature of the event*, not verbatim words — and it's cheap. Routes
   through the AI-passthrough (wallet-billed via virtues-api); documented exception to
   [[feedback_no_hardcoded_models]] (STT action may name a model). Raw audio → Gemini is transient;
   the blob stays on the box.
3. **Chunk length = 5 min** (§3). Trades only crash-loss (rare — keepalive keeps the app alive while
   recording) vs transcription coherence; optimize for coherence. Box transcribes per-chunk (does NOT
   re-window — verified), so chunk length *is* the Gemini context window.
3b. **Keep ambiance, don't loudness-discard** (§3b): record continuously; `is_silent` only for truly
   dead audio. Wind/cars/dog/elevator → Gemini describes them → narrative.
3c. **Per-stream on/off toggles for ALL collectors** (§5b); audio is a normal skippable card.
4. **Mix / stay continuous** (§4) — `.playAndRecord` + `.mixWithOthers`. Continuous through other-app
   audio; only calls force a (self-recovering) gap.
5. **Keep the 4 GB cap, simple + files-on-disk** (§5).
6. **Cross-plugin `ensureRecording()` wiring** — shared FFI symbol vs a small Rust coordinator that
   both location-probe and the sync task call. *(minor; decide at build.)*

Box-side transcription flow: `data_audio_recording` rows (raw blobs) → rolling-window transform →
Gemini 2.5 Flash → `transcript` records linked back to the recordings → retrieval.

## 8. Phasing (DEVICE-ONLY — box is done)
- **A — device capture loop (foreground first):** `audio` plugin, `AVAudioRecorder` (16 kHz mono AAC
  → `.m4a`), ~60 s chunk timer, dB meter + silence flag, `.m4a`→disk + base64 at drain, enqueue under
  stream `microphone` per the §2 contract, `enable/status/ensureRecording` commands. *Milestone: a
  spoken chunk lands in `data_audio_recording` AND `transcription_resolution` produces a transcript
  within ~2 min — full E2E on the existing pipeline.*
- **B — background continuity:** `UIBackgroundModes: audio` + `NSMicrophoneUsageDescription`
  (project.yml → xcodegen), all §1 interruption handlers, `.mixWithOthers`, `ensureRecording()`
  piggybacked on the proven location wake + `BGProcessingTask`, self-heal timer. *Milestone: survives
  a phone call (gap+resume), a Spotify play (keeps recording), an app suspension, and a force-quit
  (resurrected by walking ~500 m).*
- **C — pressure, control & polish:** 4 GB on-disk cap + drop logging (§5); **per-stream on/off toggles
  for ALL collectors** (§5b) with audio's toggle as its pause control; in-app recording-state UI +
  mic-dot ownership; normal skippable onboarding card (§6); observability (recording state, last-chunk
  time, queue depth) in This-device.

## Reuse map
- `apps/ios/.../Managers/Tracking/AudioManager.swift` — capture loop, chunking, interruption/other-
  audio/health-check logic to port (session config, −50 dB discard, 2 s overlap, UUID filenames).
- `apps/ios/.../Models/AudioStreamData.swift` — record shape.
- `apps/web/plugins/location-probe/ios/Sources/LocationProbe.swift` — wake hook to call
  `ensureRecording()`; the proven sig-loc relaunch + bounded bg-task pattern.
- `apps/web/plugins/reach/src/{outbox,ffi,upload}.rs` — enqueue + generic drain (audio = just another
  stream).
- Box: `data_audio_recording` (0007), `stream_ios_microphone` staging const, `transcript` id prefix —
  ingest + transcription transform to be built.
</content>
</invoke>
