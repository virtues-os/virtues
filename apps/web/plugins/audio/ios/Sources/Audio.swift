import AVFoundation
import Foundation
import UIKit

// Shared outbox enqueue (defined in the reach plugin's ffi.rs). The whole app
// links one static lib, so this resolves by symbol name — no bridging header.
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

private let isoMillis: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  return f
}()

/// Continuous ambient microphone recorder — **never-stopped capture graph**.
///
/// iOS forbids *starting* the mic from the background, and an `AVAudioRecorder`
/// chunk boundary (`stop()` → new recorder) IS a background start — so it dies at
/// the first ~5-min background boundary. The fix (researched): one `AVAudioEngine`
/// **armed once in the foreground and never stopped**; a continuous input tap
/// downsamples to 16 kHz mono and writes to an `AVAudioFile` that we **rotate**
/// (swap the output file) at chunk boundaries — no stop, no gap, no background
/// start. Each rotated file is a standalone `.m4a` → box pipeline unchanged.
///
/// Session: `.playAndRecord` + [.mixWithOthers, .allowBluetoothA2DP], built-in mic
/// pinned. The WWDC-lab coexistence recipe: A2DP is output-only so our mic uses the
/// built-in mic while the user's music stays high-quality on their AirPods, and
/// mixing means we're never interrupted → the session stays live 24/7 (only phone
/// calls gap us). No `.defaultToSpeaker`/`.allowBluetooth` — those were the poison
/// pills that forced the speaker / tugged AirPods in our earlier tests.
///
/// Resurrection (safety net for the rare hard interruptions — calls / Siri / media
/// reset): re-arm on interruption-end / foreground / location wake / config-change,
/// each wrapped in a background-task assertion.
public final class AudioRecorder: NSObject {
  public static let shared = AudioRecorder()

  private let chunkSeconds: Double = 300  // rotation interval (gapless — pure cut)
  private let enabledKey = "virtues.audio.enabled"
  private let targetSampleRate = 16000.0
  private let targetBitRate = 24000  // ~24 kbps mono AAC

  private let session = AVAudioSession.sharedInstance()
  private let engine = AVAudioEngine()
  private var converter: AVAudioConverter?
  private var hwFormat: AVAudioFormat?
  private let targetFormat = AVAudioFormat(
    commonFormat: .pcmFormatFloat32, sampleRate: 16000, channels: 1, interleaved: false)!

  private var outFile: AVAudioFile?
  private var chunkStart: Date?
  private var framesInChunk: AVAudioFrameCount = 0
  private var sumSq: Double = 0
  private var sampleCount: Double = 0
  private var peak: Float = 0
  private var tapInstalled = false

  // Liveness: `recording` can be true while the tap silently stops firing (iOS
  // 18.4+ interruption bug — start() succeeds but no buffers arrive). We stamp each
  // tap buffer and a watchdog forces a rebuild if the stamp goes stale.
  private var lastBufferAt: Date?
  private var watchdog: DispatchSourceTimer?
  private let livenessTimeout: TimeInterval = 5

  // Serialize engine lifecycle (start/stop/restart) off the realtime tap thread.
  private let q = DispatchQueue(label: "com.virtues.audio", qos: .userInitiated)

  private(set) var recording = false

  private override init() {
    super.init()
    let nc = NotificationCenter.default
    nc.addObserver(self, selector: #selector(handleInterruption),
      name: AVAudioSession.interruptionNotification, object: session)
    nc.addObserver(self, selector: #selector(handleRouteChange),
      name: AVAudioSession.routeChangeNotification, object: session)
    nc.addObserver(self, selector: #selector(handleMediaReset),
      name: AVAudioSession.mediaServicesWereResetNotification, object: session)
    nc.addObserver(self, selector: #selector(handleConfigChange),
      name: Notification.Name.AVAudioEngineConfigurationChange, object: engine)
    nc.addObserver(self, selector: #selector(handleForeground),
      name: UIApplication.didBecomeActiveNotification, object: nil)
  }

  // MARK: - Authorization

  public func authorized() -> Bool {
    if #available(iOS 17.0, *) {
      return AVAudioApplication.shared.recordPermission == .granted
    } else {
      return session.recordPermission == .granted
    }
  }

  private func requestPermission(_ completion: @escaping (Bool) -> Void) {
    if #available(iOS 17.0, *) {
      AVAudioApplication.requestRecordPermission { granted in completion(granted) }
    } else {
      session.requestRecordPermission { granted in completion(granted) }
    }
  }

  // MARK: - Public control (plugin commands)

  /// Explicit opt-in: prompt, persist enabled, arm the engine (must be foreground).
  public func enable(_ completion: @escaping (Bool) -> Void) {
    requestPermission { [weak self] granted in
      guard let self = self else { completion(false); return }
      if granted {
        UserDefaults.standard.set(true, forKey: self.enabledKey)
        self.armEngine(reason: "enable")
      }
      completion(granted)
    }
  }

  /// Toggle off / pause: clear the enabled flag, finalize, and stop the engine.
  public func disable() {
    UserDefaults.standard.set(false, forKey: enabledKey)
    stopWatchdog()
    q.async { [weak self] in self?.stopEngine(finalize: true) }
  }

  public func resume() { ensureRecording(reason: "resume") }

  /// Idempotent re-arm — start if we should be recording but the engine is down (or
  /// silently wedged). Safe to spam from interruption-end / foreground / location
  /// wake / config-change / watchdog.
  public func ensureRecording(reason: String = "ensure") {
    if !Thread.isMainThread {
      DispatchQueue.main.async { [weak self] in self?.ensureRecording(reason: reason) }
      return
    }
    guard authorized(), UserDefaults.standard.bool(forKey: enabledKey) else { return }
    // Liveness: if we think we're recording but no tap buffer has arrived within the
    // timeout, the tap silently died — drop the flag so armEngine does a full
    // rebuild instead of short-circuiting on `recording == true`.
    if recording, let last = lastBufferAt, Date().timeIntervalSince(last) > livenessTimeout {
      NSLog("[Audio] tap dead (no buffers %.1fs, reason=%@) — forcing rebuild",
            Date().timeIntervalSince(last), reason)
      recording = false
    }
    if recording { return }  // already live — don't churn a bg-task assertion (watchdog)
    armEngine(reason: reason)
  }

  /// Repeating liveness probe: fires even when foregrounded + stationary (when
  /// location isn't waking us), so a silently-dead tap self-heals within ~5s.
  private func startWatchdog() {
    if watchdog != nil { return }
    let t = DispatchSource.makeTimerSource(queue: q)
    t.schedule(deadline: .now() + livenessTimeout, repeating: livenessTimeout)
    t.setEventHandler { [weak self] in self?.ensureRecording(reason: "watchdog") }
    watchdog = t
    t.resume()
  }

  private func stopWatchdog() {
    watchdog?.cancel()
    watchdog = nil
  }

  // MARK: - Engine lifecycle

  /// Arm/re-arm the capture graph, wrapped in a background-task assertion so a
  /// background re-arm has execution time (the piggyback test — does iOS allow it?).
  private func armEngine(reason: String) {
    var bg = UIBackgroundTaskIdentifier.invalid
    bg = UIApplication.shared.beginBackgroundTask(withName: "virtues-audio-arm") {
      UIApplication.shared.endBackgroundTask(bg); bg = .invalid
    }
    q.async { [weak self] in
      guard let self = self else { return }
      if !self.recording {
        self.startEngine(reason: reason)
      }
      if bg != .invalid { UIApplication.shared.endBackgroundTask(bg); bg = .invalid }
    }
  }

  private func startEngine(reason: String) {
    do {
      try configureSession()
      let input = engine.inputNode
      // ALWAYS tear down and reinstall the tap. After an interruption iOS can leave
      // the engine "running" (start() succeeds) while the old tap silently stops
      // firing (iOS 18.4+ bug) — reusing it re-arms onto a corpse (recording=true,
      // no buffers, no dot). A fresh stop→removeTap→installTap→start is the
      // documented recovery (same as the manual disable/enable the user found).
      if engine.isRunning { engine.stop() }
      if tapInstalled { input.removeTap(onBus: 0); tapInstalled = false }
      let hw = input.inputFormat(forBus: 0)
      hwFormat = hw
      converter = AVAudioConverter(from: hw, to: targetFormat)
      input.installTap(onBus: 0, bufferSize: 4096, format: hw) { [weak self] buf, _ in
        self?.process(buf)
      }
      tapInstalled = true
      lastBufferAt = Date()  // reset heartbeat so the watchdog gives the new tap time
      try openChunk()
      engine.prepare()
      try engine.start()
      recording = true
      startWatchdog()
      NSLog("[Audio] engine armed (%@) hw=%.0fHz/%dch", reason, hw.sampleRate, hw.channelCount)
    } catch {
      NSLog("[Audio] arm failed (%@): %@ — will retry on next wake/foreground",
            reason, error.localizedDescription)
      recording = false
    }
  }

  private func stopEngine(finalize: Bool) {
    if finalize { rotate(restart: false) }
    if tapInstalled { engine.inputNode.removeTap(onBus: 0); tapInstalled = false }
    if engine.isRunning { engine.stop() }
    recording = false
    NSLog("[Audio] engine stopped")
  }

  private func configureSession() throws {
    // `.playAndRecord` + [.mixWithOthers, .allowBluetoothA2DP, .defaultToSpeaker],
    // mode `.default`. The coexistence recipe, tuned on-device. Why each option:
    //   • `.mixWithOthers` — never interrupt / never get interrupted → session stays
    //     live 24/7 → the "can't restart mic from background" wall is never hit
    //     (only phone calls gap us, and we auto-resume on interruption-end).
    //   • `.allowBluetoothA2DP` — A2DP is OUTPUT-ONLY, so iOS can't route our mic to
    //     the AirPods; it uses the built-in mic while the user's audio stays
    //     high-quality on their AirPods. (`.allowBluetooth`/HFP is the POISON PILL —
    //     it grabs the AirPods mic in mono call-mode; never add it.)
    // NOTE: we deliberately do NOT put `.defaultToSpeaker` in the category. It means
    // "force output to the phone speaker," so re-activating in the BACKGROUND while
    // another app owns the AirPods route would require seizing that route → iOS
    // rejects it with `.cannotInterruptOthers` (560557684) on EVERY retry, wedging
    // recording after a speaker→AirPods switch. The earpiece fix is instead done
    // dynamically below, only when it can't interrupt anyone.
    //
    // Residual risk: the AirPods TUG (Continuity moving them Mac→iPhone) has NO
    // programmatic opt-out — input activity alone can trigger it. This config
    // minimizes it (never requests an AirPods route); the hard fix is the user's
    // AirPods setting "Connect to This iPhone → When Last Connected to This iPhone".
    // We deliberately do NOT register MPNowPlayingInfoCenter (a second tug trigger).
    // Pin the built-in mic BEFORE activating (belt-and-suspenders).
    try session.setCategory(
      .playAndRecord, mode: .default, options: [.mixWithOthers, .allowBluetoothA2DP])
    if let builtIn = session.availableInputs?.first(where: { $0.portType == .builtInMic }) {
      try? session.setPreferredInput(builtIn)
    }
    try session.setActive(true, options: .notifyOthersOnDeactivation)

    // Earpiece fix, done safely: `.playAndRecord` defaults output to the RECEIVER
    // (quiet top earpiece), and our always-on session owns the shared route — so
    // other apps' playback would come out the earpiece. Bump it to the bottom
    // speaker, but ONLY when there's no external output (built-in receiver/speaker).
    // When AirPods/headphones are active we override to `.none` (leave their route
    // untouched) — never seizing it, so we never hit `.cannotInterruptOthers`. This
    // only affects OTHER apps' output; our capture is unaffected, so `try?` is safe.
    let outs = session.currentRoute.outputs.map { $0.portType }
    let hasExternal = outs.contains { $0 != .builtInReceiver && $0 != .builtInSpeaker }
    try? session.overrideOutputAudioPort(hasExternal ? .none : .speaker)
  }

  // MARK: - Chunk file (rotate without stopping the engine)

  private func openChunk() throws {
    let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    let url = dir.appendingPathComponent("mic_\(UUID().uuidString).m4a")
    let settings: [String: Any] = [
      AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
      AVSampleRateKey: targetSampleRate,
      AVNumberOfChannelsKey: 1,
      AVEncoderBitRateKey: targetBitRate,
    ]
    outFile = try AVAudioFile(forWriting: url, settings: settings,
                              commonFormat: .pcmFormatFloat32, interleaved: false)
    chunkStart = Date()
    framesInChunk = 0
    sumSq = 0; sampleCount = 0; peak = 0
  }

  /// Called on the realtime tap thread for every input buffer.
  private func process(_ input: AVAudioPCMBuffer) {
    lastBufferAt = Date()  // liveness heartbeat (watchdog reads this)
    guard let converter = converter else { return }
    let ratio = targetSampleRate / (hwFormat?.sampleRate ?? targetSampleRate)
    let cap = AVAudioFrameCount(Double(input.frameLength) * ratio) + 32
    guard let out = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: cap) else { return }
    var err: NSError?
    var fed = false
    converter.convert(to: out, error: &err) { _, status in
      if fed { status.pointee = .noDataNow; return nil }
      fed = true; status.pointee = .haveData; return input
    }
    if err != nil || out.frameLength == 0 { return }

    // Meter (dBFS) from the 16 kHz mono float samples.
    if let ch = out.floatChannelData?[0] {
      let n = Int(out.frameLength)
      for i in 0..<n {
        let s = ch[i]
        sumSq += Double(s * s)
        let a = abs(s)
        if a > peak { peak = a }
      }
      sampleCount += Double(n)
    }

    try? outFile?.write(from: out)
    framesInChunk += out.frameLength

    if Double(framesInChunk) >= chunkSeconds * targetSampleRate {
      rotate(restart: true)
    }
  }

  /// Close the current file (→ finalized on dealloc), enqueue it, open a fresh one.
  /// Runs on the tap thread at a boundary, or on `q` for stop/teardown.
  private func rotate(restart: Bool) {
    guard let done = outFile, let start = chunkStart, sampleCount > 0 else {
      if restart { try? openChunk() }
      return
    }
    let url = done.url
    let end = Date()
    let rms = sqrt(sumSq / sampleCount)
    let avgDb = rms > 0 ? 20 * log10(Float(rms)) : -160
    let peakDb = peak > 0 ? 20 * log10(peak) : -160
    // Reassign/close BEFORE reading so the moov atom is finalized (the classic
    // AVAudioFile "corrupt m4a" bug is forgetting to release the old file).
    outFile = nil
    if restart { try? openChunk() }

    let silent = avgDb < -60 && peakDb < -50
    q.async { [weak self] in
      self?.finalizeAndEnqueue(url: url, start: start, end: end, avgDb: avgDb, silent: silent)
    }
  }

  private func finalizeAndEnqueue(
    url: URL, start: Date, end: Date, avgDb: Float, silent: Bool
  ) {
    defer { try? FileManager.default.removeItem(at: url) }
    guard let data = try? Data(contentsOf: url), data.count > 1000 else {
      NSLog("[Audio] chunk too small / unreadable, dropping"); return
    }
    let rec: [String: Any] = [
      "id": UUID().uuidString,
      "audio_data": data.base64EncodedString(),
      "audio_format": "m4a",
      "timestamp_start": isoMillis.string(from: start),
      "timestamp_end": isoMillis.string(from: end),
      "duration_seconds": end.timeIntervalSince(start),
      "is_silent": silent,
      "average_db_level": Double(avgDb),
    ]
    guard let json = try? JSONSerialization.data(withJSONObject: rec),
          let str = String(data: json, encoding: .utf8) else { return }
    let rc = "microphone".withCString { s in str.withCString { j in virtues_enqueue(s, j) } }
    NSLog("[Audio] enqueued chunk %d bytes, avg=%.0fdB silent=%@ rc=%d",
          data.count, avgDb, silent ? "y" : "n", rc)
  }

  // MARK: - Interruptions / route / config / foreground (resurrection vectors)

  @objc private func handleInterruption(_ note: Notification) {
    guard let info = note.userInfo,
      let raw = info[AVAudioSessionInterruptionTypeKey] as? UInt,
      let type = AVAudioSession.InterruptionType(rawValue: raw) else { return }
    switch type {
    case .began:
      // Call/Siri deactivated our session + stopped the engine. Finalize the
      // partial chunk. Do NOT setActive(false) — keep it as recoverable as possible.
      NSLog("[Audio] interruption began")
      q.async { [weak self] in
        guard let self = self else { return }
        self.rotate(restart: false)
        self.recording = false
      }
    case .ended:
      // Always TRY to resume (drop the .shouldResume gate — an always-on recorder
      // always wants back). armEngine wraps it in a bg-task assertion so we probe
      // whether a background reactivation is allowed at all.
      NSLog("[Audio] interruption ended — attempting resume")
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
        self?.ensureRecording(reason: "interruption-end")
      }
    @unknown default: break
    }
  }

  @objc private func handleConfigChange(_ note: Notification) {
    // Route/format change stopped the engine. Finalize + re-arm (new hw format).
    NSLog("[Audio] engine config change — re-arming")
    q.async { [weak self] in
      guard let self = self else { return }
      self.rotate(restart: false)
      if self.tapInstalled { self.engine.inputNode.removeTap(onBus: 0); self.tapInstalled = false }
      if self.engine.isRunning { self.engine.stop() }
      self.recording = false
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
      self?.ensureRecording(reason: "config-change")
    }
  }

  @objc private func handleRouteChange(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
      let reason = AVAudioSession.RouteChangeReason(rawValue: raw) else { return }
    switch reason {
    case .newDeviceAvailable, .oldDeviceUnavailable:
      // A device switch (e.g. plugging into / out of AirPods) deactivates our
      // session and stops the engine, but — unlike a phone call — delivers an
      // interruption `.began` with NO matching `.ended`. So we can't wait passively:
      // re-pin the mic, then SELF-RECOVER with staggered re-arms (the session may
      // still be settling at the first attempt). ensureRecording is idempotent, so
      // once one succeeds the rest are no-ops; the location wake is the final
      // backstop. This turns a ~10s recovery (old: wait for the slow location poll)
      // into <1s.
      q.async { [weak self] in try? self?.configureSession() }
      for delay in [0.7, 2.5] {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
          self?.ensureRecording(reason: "route-change")
        }
      }
    default: break
    }
  }

  @objc private func handleMediaReset() {
    // Full audio-subsystem reset — rebuild everything.
    NSLog("[Audio] media services reset — rebuilding")
    q.async { [weak self] in
      guard let self = self else { return }
      if self.tapInstalled { self.engine.inputNode.removeTap(onBus: 0); self.tapInstalled = false }
      self.converter = nil
      self.recording = false
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
      self?.ensureRecording(reason: "media-reset")
    }
  }

  @objc private func handleForeground() { ensureRecording(reason: "foreground") }
}
