# Audio collector

Written 2026-09-29. How the phone records ambient audio continuously in the
background, and why it is shaped the way it is. Built 2026-07-09 onward on the
Tauri iOS app; the code is `apps/web/plugins/audio/ios/Sources/Audio.swift`,
and its comments carry the per-decision reasoning in more detail than this
page. Replaces `agents/plan/audio-collector-plan.md` (deleted).

Audio is the hardest collector, and not because of the recording API. It is
the only continuous, high-volume, binary stream, and it lives inside iOS's
audio-session lifecycle, which decides whether the app runs at all.

## The mechanic everything rests on

**A live recording session is itself the background keepalive.** With the
`audio` background mode and an active capture, iOS lets the app run
indefinitely, like a music player. The moment capture stops, that keepalive is
gone and the app is suspended within the normal grace period.

The first design assumed a stop could be repaired from outside: location wakes,
the processing task and a self-heal timer would each call an idempotent
`ensureRecording()` and bring the mic back. Research on 2026-07-10 refuted
that, and the refutation reshaped the collector:

1. **The mic can never be started from the background.** Apple's own answer
   (error `561145187`, `cannotStartRecording`; DTS: background audio only
   continues a session created in the foreground). Not under
   `beginBackgroundTask`, not while alive on continuous location, not on a
   significant-location cold relaunch. iOS gates on *why* the process is
   awake, and location is not a mic grant.
2. **Chunking by `AVAudioRecorder` stop→start dies at the first background
   boundary**, because each new recorder is a background start. There is also
   a known silent death around ninety minutes in the restart-loop pattern.

So the location-wake → `ensureRecording()` path is kept (the location plugin
calls `virtues_ensure_recording`), but it can only re-arm a graph that iOS
still lets us resume. It is not a resurrection mechanism for a dead mic.

## The capture graph: splice, don't restart

One `AVAudioEngine` is armed in the foreground and **never stopped**. A tap on
the input node converts to 16 kHz mono and writes AAC (~24 kbps) into an
`AVAudioFile`; at each five-minute boundary the tap swaps in a new file. No
stop, no gap, no background start. Each rotated file is a standalone `.m4a`,
so the box pipeline did not change.

Two traps inside that sentence:

- **The file is finalized by deallocation.** `AVAudioFile` writes its `moov`
  atom in its destructor. Holding any second strong reference past the swap
  ships an unfinalized chunk — `ftyp` plus raw AAC, undecodable. That shipped
  once, and the transcriber hallucinated over every chunk.
- **The tap can die silently.** On iOS 18.4+ an interruption can leave the
  engine "running" while no buffers arrive. Each buffer is stamped, and a
  five-second watchdog rebuilds the tap when the stamp goes stale. Re-arming
  always tears the tap down and reinstalls it, and refuses an input node that
  reports 0 Hz / 0 channels (installing a tap with that format throws).

Chunk length is five minutes because length trades only two things: shorter
loses less to a hard kill mid-chunk, longer gives the transcriber a coherent
scene. The keepalive makes mid-chunk kills rare, so coherence wins.

## Capability statement

Not true 24/7. Delivered: **continuous background recording for as long as the
session survives, once armed in the foreground; after a call, a kill or a
refused re-arm, capture resumes when the app is next foregrounded.** A gap
notification fires after five minutes of unintended silence (never during a
call or a CarPlay pause) so the owner knows to open the app. The products that
avoid this ceiling do it with pendant hardware and on-device buffering — a
hardware decision, not a software one.

## Session configuration and coexistence

`.playAndRecord`, mode `.default`, options `[.mixWithOthers,
.allowBluetoothA2DP]`, built-in mic pinned as preferred input. Tuned on-device;
the plan went through two wrong versions before this one.

- **`.mixWithOthers`** means we never interrupt and are never interrupted by
  other media, so the session stays live and the background-start wall is only
  met on real interruptions (calls, Siri, alarms). The mic hears music played
  through a speaker as ambience regardless, so yielding to media buys nothing.
  The old "stop when another app plays audio" gate was deleted: pointless, and
  its notification is only delivered to foreground apps anyway.
- **`.allowBluetoothA2DP`** is output-only, so iOS cannot route our input to
  AirPods; the user's audio stays high-quality on them. `.allowBluetooth`
  (HFP) is the poison pill: it grabs the AirPods mic in mono call mode.
- **`.defaultToSpeaker` is not in the category.** Re-activating in the
  background while another app owns the AirPods route would mean seizing it,
  which iOS refuses (`cannotInterruptOthers`) on every retry — recording wedged
  after a speaker→AirPods switch. The earpiece problem it solved is handled by
  an output override applied only when no external output is present.
- Residual: Continuity can still tug AirPods from a Mac to the phone on input
  activity alone. There is no programmatic opt-out.

**Interruptions and route changes.** A notified interruption (`.began`) sets a
hold: re-arming every five seconds against an alarm cut a wake-up alarm to a
snippet, so during the hold re-arms slow to one a minute and the real resume is
event-driven (`.ended`, foreground, route change, media-services reset). A
device switch deactivates the session and delivers `.began` with no `.ended`,
so route changes self-recover with staggered re-arms instead of waiting.
Engine configuration changes finalize the chunk and re-arm with the new
hardware format. Our own `.categoryChange` / `.override` notifications are
ignored, or reacting to them loops.

**CarPlay: release, don't coexist.** A record session on a CarPlay route evicts
the car's audio to the phone speaker, and every re-arm re-evicted it. So while
a car port is present the engine stops and the session is released, gating
every re-arm vector including foreground. Route reads are asymmetric: a
positive read is trusted anywhere, but an inactive session has been seen to
omit CarPlay, so the pause exits only on the system's own
`.oldDeviceUnavailable` naming the car, plus a rate-limited foreground
backstop.

## Silence and ambience

"No talking" is not silence. The earlier app discarded chunks below −50 dB and
threw away wind, traffic and the dog — the ambient signal that makes a day
narratable. Now every chunk is kept. Only a genuinely dead chunk (average below
−60 dBFS **and** peak below −50) ships metadata-only: the period stays covered
on the timeline, but the bytes never ride the radio. On the box a VAD pass
runs before any paid transcription call, so a quiet room is not billed.

## Mute: schedule and places, never release

Quiet hours, later generalized to a weekly schedule and muted places, **mute
without releasing**. The graph stays armed and only chunk writing stops,
because releasing the mic at 22:00 means an app nobody opens at 07:00 never
resumes. While muted, a metadata-only marker ships every chunk interval naming
the reason (never the place), so the box can tell a chosen silence from a dead
collector — otherwise both are the same thing: no rows.

## Back-pressure

Audio is the one stream that can fill a phone. The outbox refuses microphone
enqueues past ~1 GiB of live pages (about three days of non-silent backlog)
with its own return code. It is back-pressure, never eviction: once a row is
accepted the device deletes its `.m4a`, so the row is the only copy and must
not be dropped to make room. A refused chunk keeps its file; an orphan sweep
re-offers kept chunks on each arm, stops at the first capacity refusal, and
ages files out after seven days with a log line. Chunk files are excluded from
device backups — raw room audio must not reach iCloud.

The device never deletes a chunk before the outbox has taken it. An earlier
unconditional delete made a full disk destroy recordings with one log line as
the only trace.

## Wire contract

One record per chunk on stream `microphone`, through the ordinary reach drain
to `ios_ingest`: `id`, base64 `audio_data` (absent when silent or muted),
`audio_format`, `timestamp_start`/`timestamp_end`, `duration_seconds`,
`is_silent`, `average_db_level`, and `muted_by` on a mute marker. The box
writes the file to the lake and a `data_audio_recording` row; a device-agnostic
applet transcribes with the Omni slot model.

## App Store

An always-on ambient mic is a Guideline 2.5.4 rejection pattern. What review
needs: a visible in-app recording state and control, a usable app with audio
off, a strong usage string, and review notes saying the audio lands on the
owner's own box. The orange mic indicator is permanently on while recording;
it is a trust surface the UI has to own, not a bug.
