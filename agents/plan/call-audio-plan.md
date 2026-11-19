# Call audio from the Mac

The iPhone records the owner's side of a Zoom, Meet, Teams or FaceTime call
and none of the other side, because the far side plays into headphones the
phone cannot hear. This plan has the Mac collector record both sides while a
call is running and send them to the box through the audio path that already
exists. Slice 1 (capture to local files) is built, in
`apps/mac-source/Sources/Core/CallAudioMonitor.swift`; the rest is not
built. Delete this plan when it ships; what remains
then is a record of the capture design and a manual page about call
recording.

## What the spike showed

A throwaway spike on 2026-10-26 (macOS 26.4, Apple silicon, AirPods as the
default input and output) checked the parts that could have ruled this out.
The code was not kept.

- **A LaunchAgent binary can get the permission.** The spike ran as a
  LaunchAgent, the same way `com.virtues.collector` runs, with an Info.plist
  embedded through `-sectcreate __TEXT __info_plist` that carried
  `NSAudioCaptureUsageDescription`, `NSMicrophoneUsageDescription` and a
  `CFBundleIdentifier`. macOS showed its own "System Audio Recording" and
  Microphone prompts for it and stored both grants in the user TCC database
  under that bundle id. Screen Recording was not needed.
- **A tap on one process works.** `CATapDescription(stereoMixdownOfProcesses:)`
  on one PID, in a private aggregate device that holds only the tap, delivered
  12.1 s of 48 kHz stereo in a 12.1 s run. Adding the output device to the
  aggregate as a clock changed nothing, so the tap-only shape is the one to
  use (see the screenpipe note below for why).
- **An unanswered prompt looks like a broken device.** While the first prompt
  was open, the IOProc delivered zero buffers and every call returned `noErr`.
  The spike first misread that as a clock problem. The collector must check
  the permission before it trusts silence.
- **An ad-hoc rebuild asks again.** Each unsigned rebuild re-prompted, which is
  the cdhash problem `CodeIdentity.swift` already describes. The shipped
  collector is Developer ID signed, so its grant should survive updates.
- **`tccutil reset` cannot find a bare binary.** `tccutil` resolves bundle ids
  through LaunchServices, which only knows registered bundles, so it returned
  OSStatus -10814 for the spike. A "reset this permission" path for the
  collector needs it registered as a bundle, or it has to send the owner to
  System Settings.
- **The AirPods mic changes the AirPods.** Opening the default input while
  AirPods are connected moves them to the Bluetooth call profile (mono, lower
  quality). During a call the meeting app has already done this, so it costs
  nothing then. Outside a call it degrades the owner's music, so the mic is
  opened only while a call is detected.

## What other projects do

A survey of the source of open-source projects that record calls on macOS.
File paths are in those repositories.

| Project | Capture | Call detection | Mic and attribution |
|---|---|---|---|
| Hyprnote (`fastrepl/anarlog`) | Global process tap through `cidre`, tap-only aggregate, macOS 15+ | Listener on the default input's `DeviceIsRunningSomewhere`, then a 1 s poll of each process's `IsRunningInput`; PID mapped to the outermost `.app` so a browser helper counts as the browser (`crates/detect/src/mic/macos/`) | Mic separate through cpal; two channels to speech-to-text, mic labeled "You" (`crates/transcript/src/speaker_context.rs`); a neural echo canceller that is off when every output is headphones |
| screenpipe | ScreenCaptureKit by default, process tap opt-in at 14.4+; a per-PID tap on the meeting app during a meeting | Snapshot of `IsRunningInput` per process with libproc names; ignores `corespeechd`, keeps `avconferenced` for FaceTime | One file per device, so input and output separate "you" and "them"; reads `kAudioProcessPropertyDevices` to follow the mic the meeting app uses; optional VoiceProcessingIO with fallback when it goes silent |
| Chromium (`media/audio/mac/catap_audio_input_stream.mm`) | Global tap excluding its own audio service, or one app; tap-only aggregate | None (a capture source only) | Matches browser helpers by bundle-id prefix; probes permission by reading and writing back `kAudioTapPropertyDescription` |
| AudioCap (insidegui) | One-process tap, macOS 14.4 | None | Checks permission with the private `TCCAccessPreflight` / `TCCAccessRequest` |
| Meetily | Hyprnote's tap code with the output device added to the aggregate | Manual start; watches output activity | Mixes mic and system into one track, so it cannot tell who spoke |

Most of them agree on these points: a Core Audio process tap in a private
aggregate, with ScreenCaptureKit only as a fallback; the mic captured
separately and kept as its own channel; call start detected from which
process has the input running; browser helpers mapped to their app by bundle
id; echo cancellation only when the output is speakers.

Problems they hit that apply here:

- screenpipe found that putting the real output device in the aggregate can
  stop Zoom and Slack from starting, and that excluding your own process from
  a global tap on macOS 26 makes Zoom retry its mic every ~14 s until it is
  relaunched.
- The tap's sample rate follows the output route. A route change (AirPods
  connecting mid-call) needs the format re-read and the tap rebuilt. macOS 14
  does not resample for you (Chromium comment).
- A tap can go silent mid-session with the IOProc still firing (Apple
  developer forum thread 825780, macOS 26.5 beta). Only rebuilding the tap and
  aggregate fixed it.
- `IsRunningInput` means a stream is registered, not that anyone is talking.
  Apps that keep the mic open all the time will look like a call.
- An unanswered first prompt can block Core Audio for about 60 s (Chromium).

## Design

### Where it runs

In the collector (`apps/mac-source`), as a new monitor beside
`PresenceMonitor` and `MessageMonitor`. The collector already runs all day as
a signed LaunchAgent, already holds TCC grants, and already uploads to
`mac_ingest`. Process taps need macOS 14.4; the package targets macOS 12, so
the monitor is behind an availability check and reports "not supported" on
older systems.

### Detecting a call

1. Listen for `kAudioDevicePropertyDeviceIsRunningSomewhere` on every device
   with an input, and re-register when the device list changes. Watching
   only the default input missed the first test call: with AirPods as the
   default input, Meet in Dia used the MacBook's built-in mic.
2. While it is running, poll the process list once a second for processes
   with `kAudioProcessPropertyIsRunningInput` set.
3. Map each one to an app by bundle-id prefix and check it against a list of
   call apps: Zoom, Teams, Slack, Discord, FaceTime and Continuity phone calls
   (`avconferenced`, `callservicesd`), and browsers (Chrome, Safari's WebKit
   GPU process, Arc and Dia, Firefox, Edge). Ignore `corespeechd` and the
   collector itself.
4. A call ends when no listed app has had the input running for 10 s, so a
   brief mute-and-unmute does not split one call into two.

Window titles (`WindowTitle.swift`, `BrowserMonitor.swift`) only label the
call, for example "Google Meet" for a browser call. They do not start or stop
it.

### What is recorded

- **The far side:** a process tap on every process of the detected app
  (all helpers for a browser), in a private, tap-only aggregate with drift
  compensation. A browser tap also hears other tabs' audio; that is accepted.
- **The owner:** the input device the meeting app is using, read from
  `kAudioProcessPropertyDevices`, not the system default.
- **The files:** two mono `.m4a` files per 300 s chunk, `NNN-owner.m4a` and
  `NNN-remote.m4a`, with one `NNN.json` sidecar naming both. Each side is
  resampled from the aggregate's rate (48 kHz in testing) to 16 kHz and
  encoded as AAC at 24 kbps, the phone's format. They are separate files
  because labels come from transcribing each side on its own (see
  "Transcription and attribution"). A chunk rotates without stopping
  capture.

The two channels are aligned by putting the mic and the tap in one aggregate
(the mic as main device, the tap drift-compensated to it), so one IOProc
delivers both on one clock. Nobody in the survey does this, and screenpipe's
Zoom finding is about adding a device to an aggregate, so it still has to be
checked with Zoom. It worked with Meet in Dia on 2026-10-26: 30 s with
signal on both channels. If Zoom objects, the fallback is running the mic on
its own and aligning the two by host time.

### Permission

- Embed an Info.plist in the collector binary with both usage strings and no
  `CFBundleIdentifier` (`apps/mac-source/Support/Info.plist`). The collector's
  Full Disk Access and Accessibility grants are stored by path, and a bundle
  id would make TCC key the binary by id instead, so every installed
  collector would lose them on upgrade. On macOS 27.0.1 a Developer ID binary
  with only the usage strings kept a path-keyed Accessibility grant and still
  got the System Audio Recording prompt.
- Ask during Setup or from Settings, never at the start of a call, because
  the first prompt blocks capture until it is answered.
- Check the grant with `TCCAccessPreflight` (private, used by AudioCap and
  Hyprnote) or with Chromium's description round-trip, and report it through
  `CollectorHealth` with the other permissions. That way a missing grant
  shows as a permission problem instead of quiet calls.
- Two other checks are planned: a far-side channel that is exactly zero for
  a whole chunk while the app's output is running, and a tap that stops
  delivering buffers. Either one should rebuild the tap. Slice 1 only logs
  the first; it does not rebuild or report it yet.

### Getting it to the box

- The Uploader sends chunks to `mac_ingest` under a new key, `call_audio`,
  with the same record shape the phone sends on the `microphone` stream:
  `id`, `audio_data`, `audio_format`, `timestamp_start`, `timestamp_end`,
  `duration_seconds`, `average_db_level`. The extra fields are `call_id`,
  `app`, `role` (`owner` or `remote`) and `output_route` (headphones or
  speakers). Each file is its own record, so the two sides of a chunk are two
  rows that share a `call_id` and the same times.
- `mac_ingest` stores the blob and writes `data_audio_recording` with
  `source_provider = 'mac'` and `source_table = 'stream_mac_call_audio'`, and
  puts the extra fields in `metadata`. The table needs no migration.
- `externalize_blobs` and `ingest_one` in `applets/ios_ingest/microphone.rs`
  move into a shared helper that both applets call, in the same change. A
  second copy is the pattern CLAUDE.md asks us to avoid.
- A 300 s chunk is two mono 16 kHz files, together a few MB at the current
  bit rate, small next to the attachment posts the Uploader already batches.

### Transcription and attribution

`transcription_resolution` already takes any `data_audio_recording` row. Call
rows get one prompt addition: the left channel is the owner, so their lines
start `[You]:` and other voices keep `[Speaker]:`.

That prompt does not work. On 2026-10-27, 24 calls to the Omni slot
(`google/gemini-3-flash`) used two invented 45 s scripts rendered with macOS
voices, with the owner on the left and then swapped to the right:

- The model labels whoever speaks first `[You]` and alternates from there.
  Swapping the channels gives the same labels, so 12/12 on one script and
  0/12 on the other.
- With the same voice on both sides it also misses voice changes, scoring
  4–7 of 12, about chance.
- It finds the turns well when the voices differ: 12 segments for 12 lines,
  with or without the channel prompt, at 16 kHz and 48 kHz alike.

So the labels come from the channels instead, by transcribing each channel
on its own. A second test the same day, 30 calls on mono 16 kHz channel files
of the same scripts plus one with a backchannel and a talk-over, settled how
to merge them:

- The model's own line times are not usable. Asked for `[m:ss.s]` per line,
  it times lines as if the silence were cut out: a channel whose first words
  come at 4 s gets `[0:00.0]`, and the median error is 7–10 s. Telling it to
  count silence changed nothing.
- Speech spans measured from the audio are accurate. A -40 dBFS energy gate
  with gaps under 0.3 s merged put every turn within 0.1 s of its true start.
  Extra spans come only from pauses inside a sentence.
- Matching each channel's lines to its spans in order, by how long each line
  should take to say (about 2.6 words/s), with a cost for a line stretching
  over a gap longer than 0.8 s, put every line in the right place: 10 of 10
  runs with every label right, no lines out of order, and every start within
  0.11 s, including the backchannel during the other side's line.

So the box transcribes each channel, places the lines on that channel's
spans, and labels by channel. The model needs no timestamps, only one line
per utterance. The cost is double the audio input, less whenever the
detector skips a silent channel. The chunk format follows: two mono 16 kHz
files per chunk rather than one stereo file, which also fits the detector.

These were synthetic voices with no room noise, echo or crosstalk. A real mic
has a noise floor, so the spans should come from the detector the box already
runs, not a fixed energy gate, and speaker calls put the far side into the
owner's channel (see "Echo on speakers").

This labels the owner in calls only. It does not reopen
[speaker-attribution-plan.md](speaker-attribution-plan.md), which stays
parked for ambient audio.

### Overlap with the phone

During a call the phone also records the owner's side, so the same speech
reaches the box twice. A phone chunk mostly covered by a call recording gets
`metadata.covered_by = <call_id>`. `sessionize/audio.rs` and the day article
read the call rows for that span and skip the covered phone chunks. Whether
covered chunks are still transcribed (cost against a fallback if the Mac
upload fails) is decided in slice 4. Nothing is deleted.

### Echo on speakers

When the output is the Mac's speakers, the mic also picks up the far side,
so the owner's channel holds both. The first version records `output_route`
and does nothing else. Add echo cancellation only if labeling on speaker
calls turns out wrong in the measurement. VoiceProcessingIO is the built-in
option, with the start-then-silent failure screenpipe documents.

## Decisions

- **On by default** (decided 2026-10-26). The macOS permission prompt during
  Setup is where the owner agrees to it, and Settings turns it off. Recording
  a call records other people, and some places require every party to agree,
  so the manual page says so plainly.
- **A fixed list of call apps** (decided 2026-10-26), the list under
  "Detecting a call". Any app that opens the mic would also catch dictation
  tools, Siri, Voice Memos and screen recorders, which hold the input without
  a call. A new call app is added to the list in a collector update.
- **Still open:** the wording of the manual page about recording other
  people.

## Slices

1. **Capture to local files.** Built (a94511455): `CallPolicy` (the app list
   and start/end rule, with tests), `CallAudioMonitor`, `CallAudioPermission`,
   the two grants in `CollectorHealth` and the health payload, and a
   `virtues-collector call-audio` command that runs only this monitor for
   testing. Chunks stay in `~/.virtues/call-audio/` under a 2 GB cap until
   slice 2 uploads them. Each chunk is two mono 16 kHz files since
   2026-10-27; a solo Meet call in Dia gave two files of equal length with the
   join sounds on the far side. Tested with Meet in Dia only. Still to test: Zoom
   (the aggregate question above), Meet in Chrome, FaceTime, a Continuity
   phone call, speakers instead of headphones, and an AirPods connect in the
   middle of a call.
2. **To the box.** One record per side per chunk, the shared ingest helper, the `call_audio` key in `mac_ingest`, and rows in
   `data_audio_recording`, transcribed per channel through the current
   prompt.
3. **Owner labels.** Detector spans per channel, lines placed on them by
   length, the merged transcript labeled by channel, then a test on real
   calls with room noise.
4. **Overlap.** `covered_by`, sessionize and the day article.
5. **Settings and manual.** The off switch, the permission row, and a manual
   page about call recording. The permission is asked in Setup from slice 1
   on, since recording is on by default.
