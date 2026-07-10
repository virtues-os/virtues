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

/// Continuous ambient microphone recorder.
///
/// Records fixed-length `.m4a` chunks (16 kHz mono AAC — exactly what the
/// box-side Gemini transcription downsamples to) and enqueues each as a
/// `microphone` record into the shared outbox. The box ingests it into
/// `data_audio_recording` and transcribes it.
///
/// Session strategy (researched): MIX through the user's other audio rather than
/// interrupt it — the mic hears the room (incl. speaker-played music) anyway.
///   • `.playAndRecord` + `.mixWithOthers` — never interrupt/duck other apps.
///   • `.defaultToSpeaker` — avoid the default earpiece / call-volume routing.
///   • `.allowBluetoothA2DP` and NEVER `.allowBluetooth` — keep AirPods on
///     high-quality A2DP *output*; record from the built-in mic. `.allowBluetooth`
///     would force AirPods into HFP (mono 16 kHz), wrecking the user's music.
///   • Yield only on hard interruptions (calls/Siri) — the OS forces those; we
///     finalize the current chunk and resume after.
public final class AudioRecorder: NSObject {
  public static let shared = AudioRecorder()

  // 5-minute chunks: trades (rare) crash-loss vs transcription coherence; the
  // recording session keeps the app alive so hard-kill mid-chunk is uncommon.
  private let chunkSeconds: TimeInterval = 300
  private let enabledKey = "virtues.audio.enabled"

  private let session = AVAudioSession.sharedInstance()
  private var recorder: AVAudioRecorder?
  private var chunkStart: Date?
  private var chunkTimer: DispatchSourceTimer?
  private var meterTimer: DispatchSourceTimer?
  private var routeDebounce: DispatchWorkItem?

  // Serialize all session/recorder mutation (route-change bursts race otherwise).
  private let q = DispatchQueue(label: "com.virtues.audio", qos: .userInitiated)

  // Metering accumulation for the current chunk (dBFS: -160 … 0).
  private var dbSamples: [Float] = []
  private var peakDb: Float = -160

  private(set) var recording = false

  private override init() {
    super.init()
    NotificationCenter.default.addObserver(
      self, selector: #selector(handleInterruption),
      name: AVAudioSession.interruptionNotification, object: session)
    NotificationCenter.default.addObserver(
      self, selector: #selector(handleRouteChange),
      name: AVAudioSession.routeChangeNotification, object: session)
    NotificationCenter.default.addObserver(
      self, selector: #selector(handleForeground),
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

  // MARK: - Public control (called from the plugin commands)

  /// Explicit opt-in: prompt, persist enabled, start recording.
  public func enable(_ completion: @escaping (Bool) -> Void) {
    requestPermission { [weak self] granted in
      guard let self = self else { completion(false); return }
      if granted {
        UserDefaults.standard.set(true, forKey: self.enabledKey)
        self.start()
      }
      completion(granted)
    }
  }

  /// Toggle off / pause: clear the enabled flag and stop (finalizing the chunk).
  public func disable() {
    UserDefaults.standard.set(false, forKey: enabledKey)
    stop()
  }

  /// Launch auto-resume / background wake: record only if authorized AND enabled.
  public func resume() { ensureRecording() }

  /// Idempotent: start recording if we should be but aren't. Safe to spam from
  /// interruption-end, route-change, foreground, and the location wake.
  public func ensureRecording() {
    guard authorized(), UserDefaults.standard.bool(forKey: enabledKey) else { return }
    start()
  }

  // MARK: - Recording lifecycle

  private func start() {
    q.async { [weak self] in
      guard let self = self, !self.recording else { return }
      do {
        try self.configureSession()
        try self.startChunk()
        self.recording = true
        NSLog("[Audio] recording started")
      } catch {
        NSLog("[Audio] start failed: %@", error.localizedDescription)
        self.recording = false
      }
    }
  }

  private func stop() {
    q.async { [weak self] in
      guard let self = self else { return }
      self.chunkTimer?.cancel(); self.chunkTimer = nil
      self.meterTimer?.cancel(); self.meterTimer = nil
      self.finalizeChunk(restart: false)  // flush the partial chunk
      self.recording = false
      NSLog("[Audio] recording stopped")
    }
  }

  private func configureSession() throws {
    // We are a PURE recorder — we play nothing. Any Bluetooth/AirPlay *output*
    // option makes iOS grab the user's headphones for output: activating an
    // A2DP-output session literally pulls AirPods off their Mac onto the phone.
    // So we allow NO output-routing options: mix with other apps, keep our
    // (silent) output on the built-in speaker, and never advertise for BT/AirPlay.
    // `.allowBluetooth` (HFP) is likewise omitted — it would force AirPods into
    // mono call quality. Input comes from the pinned built-in mic regardless of
    // what's on the user's Bluetooth.
    let opts: AVAudioSession.CategoryOptions = [.mixWithOthers, .defaultToSpeaker]
    try session.setCategory(.playAndRecord, mode: .default, options: opts)
    // Pin the built-in mic so a route flip can't steal input to a BT/HFP mic.
    if let builtIn = session.availableInputs?.first(where: { $0.portType == .builtInMic }) {
      try? session.setPreferredInput(builtIn)
    }
    try session.setActive(true, options: .notifyOthersOnDeactivation)
  }

  private func startChunk() throws {
    // UUID path so a discarded-then-restarted chunk can never collide while a
    // prior finalize is still reading its file.
    let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    let url = dir.appendingPathComponent("mic_\(UUID().uuidString).m4a")
    let settings: [String: Any] = [
      AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
      AVSampleRateKey: 16000.0,
      AVNumberOfChannelsKey: 1,
      AVEncoderAudioQualityKey: AVAudioQuality.low.rawValue,
      AVEncoderBitRateKey: 24000,  // ~24 kbps mono AAC — good for speech + ambiance
    ]
    let rec = try AVAudioRecorder(url: url, settings: settings)
    rec.isMeteringEnabled = true
    rec.record()
    recorder = rec
    chunkStart = Date()
    dbSamples.removeAll(); peakDb = -160

    // Chunk rotation (one-shot, re-armed each chunk).
    let ct = DispatchSource.makeTimerSource(queue: q)
    ct.schedule(deadline: .now() + chunkSeconds)
    ct.setEventHandler { [weak self] in self?.finalizeChunk(restart: true) }
    ct.resume()
    chunkTimer?.cancel(); chunkTimer = ct

    // Metering poll for avg/peak dB.
    let mt = DispatchSource.makeTimerSource(queue: q)
    mt.schedule(deadline: .now() + 0.5, repeating: 0.5)
    mt.setEventHandler { [weak self] in self?.sampleMeter() }
    mt.resume()
    meterTimer?.cancel(); meterTimer = mt
  }

  private func sampleMeter() {
    guard let rec = recorder, rec.isRecording else { return }
    rec.updateMeters()
    let db = rec.averagePower(forChannel: 0)
    dbSamples.append(db)
    if db > peakDb { peakDb = db }
  }

  /// Finalize the current chunk: stop the recorder, read the file, enqueue it,
  /// clean up, and (if still recording) start the next chunk.
  private func finalizeChunk(restart: Bool) {
    meterTimer?.cancel(); meterTimer = nil
    chunkTimer?.cancel(); chunkTimer = nil
    guard let rec = recorder, let start = chunkStart else {
      if restart, recording { try? startChunk() }
      return
    }
    let url = rec.url
    if rec.isRecording { rec.stop() }
    recorder = nil
    let end = Date()
    let avg = dbSamples.isEmpty ? peakDb : dbSamples.reduce(0, +) / Float(dbSamples.count)
    let peak = peakDb

    if let data = try? Data(contentsOf: url), data.count > 1000 {
      // Keep ambiance: only flag genuinely-dead audio silent (very conservative
      // — the box also catches an empty Gemini response). Everything with any
      // wind/traffic/dog/room-tone goes to Gemini and gets described.
      let silent = avg < -60 && peak < -50
      enqueue(
        data: data, start: start, end: end,
        duration: end.timeIntervalSince(start), avgDb: avg, silent: silent)
    } else {
      NSLog("[Audio] chunk too small / unreadable, dropping")
    }
    try? FileManager.default.removeItem(at: url)

    if restart, recording { try? startChunk() }
  }

  private func enqueue(
    data: Data, start: Date, end: Date, duration: Double, avgDb: Float, silent: Bool
  ) {
    let rec: [String: Any] = [
      "id": UUID().uuidString,
      "audio_data": data.base64EncodedString(),
      "audio_format": "m4a",
      "timestamp_start": isoMillis.string(from: start),
      "timestamp_end": isoMillis.string(from: end),
      "duration_seconds": duration,
      "is_silent": silent,
      "average_db_level": Double(avgDb),
    ]
    guard
      let json = try? JSONSerialization.data(withJSONObject: rec),
      let str = String(data: json, encoding: .utf8)
    else { return }
    let rc = "microphone".withCString { s in str.withCString { j in virtues_enqueue(s, j) } }
    NSLog("[Audio] enqueued chunk %.0fs, %d bytes, silent=%@, rc=%d",
          duration, data.count, silent ? "y" : "n", rc)
  }

  // MARK: - Interruptions / route changes / foreground

  @objc private func handleInterruption(_ note: Notification) {
    guard let info = note.userInfo,
      let raw = info[AVAudioSessionInterruptionTypeKey] as? UInt,
      let type = AVAudioSession.InterruptionType(rawValue: raw)
    else { return }
    switch type {
    case .began:
      // Call/Siri grabbed the session — finalize the chunk (excludes call audio).
      q.async { [weak self] in self?.finalizeChunk(restart: false) }
    case .ended:
      let shouldResume = (info[AVAudioSessionInterruptionOptionKey] as? UInt)
        .map { AVAudioSession.InterruptionOptions(rawValue: $0).contains(.shouldResume) } ?? true
      if shouldResume {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
          self?.ensureRecording()
        }
      }
    @unknown default: break
    }
  }

  @objc private func handleRouteChange(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
      let reason = AVAudioSession.RouteChangeReason(rawValue: raw)
    else { return }
    // React only to real device connect/disconnect; ignore our own category edits.
    switch reason {
    case .newDeviceAvailable, .oldDeviceUnavailable:
      // AirPods fire a burst — debounce and rebuild once on the settled route.
      routeDebounce?.cancel()
      let work = DispatchWorkItem { [weak self] in
        guard let self = self, self.recording else { return }
        self.q.async {
          try? self.configureSession()          // re-pin built-in mic
          self.finalizeChunk(restart: true)     // roll to a fresh chunk (format may change)
        }
      }
      routeDebounce = work
      q.asyncAfter(deadline: .now() + 0.2, execute: work)
    default: break
    }
  }

  @objc private func handleForeground() { ensureRecording() }
}
