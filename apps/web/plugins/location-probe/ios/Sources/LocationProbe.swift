import CoreLocation
import Foundation
import SQLite3
import UIKit

// SQLite wants to copy bound strings, not borrow them.
private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

// The Rust outbox enqueue, bound by symbol name (no bridging header/modulemap).
// Defined in the reach plugin's `ffi.rs`; the whole app links one static lib.
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

// Drain the outbox to the box, blocking up to N seconds. Called from the
// background (sig-loc wake) while holding an OS background-task assertion.
@_silgen_name("virtues_drain_blocking")
private func virtues_drain_blocking(_ timeoutSecs: Int32) -> Int32

// Re-arm audio recording (audio plugin). The piggyback: location keeps the app
// alive + fires callbacks, giving audio a heartbeat to recover the mic after an
// interruption/kill. No-op unless audio is enabled+authorized. Whether iOS
// actually allows a background mic re-arm here is what we're testing.
@_silgen_name("virtues_ensure_recording")
private func virtues_ensure_recording()

/// ISO-8601 with fractional seconds so distinct fixes get distinct timestamps
/// (the outbox derives a per-record id from the record, incl. this).
private let isoMillis: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  return f
}()

/// The whole point of the spike.
///
/// A single, process-wide `CLLocationManager` owner that:
///   1. can be started from the app's AppDelegate on a COLD background relaunch
///      (i.e. before any webview exists), and again from a JS plugin command;
///   2. registers significant-location-change so iOS relaunches the terminated
///      app into the background on movement;
///   3. writes every callback straight to SQLite, recording the app state and
///      the launch reason — so you can later prove a row was written while the
///      app was not on screen.
///
/// Nothing here is Tauri-specific: it is the same CoreLocation you would write
/// in the native Swift app. The test is whether Tauri's launch lifecycle still
/// reaches `start()` on a background relaunch.
public final class LocationProbe: NSObject, CLLocationManagerDelegate {
  public static let shared = LocationProbe()

  private let manager = CLLocationManager()
  private var configured = false
  private var updating = false
  private var wantPrompt = false

  /// Throttle: keep at most one fix per ~15s (matches the native app's sampling
  /// cadence — avoids flooding the log + outbox with near-identical points).
  private var lastFixAt: Date?
  private let minFixInterval: TimeInterval = 15

  // MARK: - Power modes (motion-adaptive duty cycle)
  //
  // GPS at 10m accuracy 24/7 is the single biggest hardware burn and a phone is
  // stationary most of the day. `coarse` (cell/Wi-Fi positioning, GNSS chip off)
  // is only ever entered while audio records healthily — the audio session owns
  // process residency then, so location never carries the keepalive in coarse
  // mode. Movement, or audio going down (the mic re-arm piggyback needs a fast
  // heartbeat again), escalates straight back to `precise`. Updates are never
  // stopped — only accuracy/filter are modulated — so background residency and
  // the sig-loc relaunch path are untouched.

  private enum PowerMode { case precise, coarse }
  private var mode: PowerMode = .precise
  /// 0 = audio off, 1 = enabled but not recording (down), 2 = recording healthy.
  /// Pushed by the audio plugin on every state transition.
  private var audioState: Int32 = 0
  /// Last position that counted as movement; displacement is measured from here.
  private var moveAnchor: CLLocation?
  private var lastMovedAt = Date()
  private let stationaryAfter: TimeInterval = 300
  private let moveThreshold: CLLocationDistance = 60
  private let movingSpeed: CLLocationSpeed = 1.5

  /// Guards against overlapping background drains.
  private var isDraining = false

  /// Pace background drains at the audio-chunk cadence: with endpoint parking
  /// (reach plugin) every background drain is a fresh QUIC dial, and location
  /// fixes alone are too small to earn one every 15s while moving. The first
  /// drain after launch goes immediately — a cold sig-loc relaunch may not
  /// live long enough to wait out an interval.
  private var lastBgDrainAt: Date?
  private let minBgDrainInterval: TimeInterval = 300

  /// Set by the AppDelegate: "user" for a normal launch, "location" when the
  /// app was relaunched by the OS for a location event (launchOptions carried
  /// `.location`). This is the flag that proves cold-relaunch collection.
  public var launchReason: String = "user"

  private override init() { super.init() }

  /// Start collecting. `prompt = false` (launch auto-resume) starts only if
  /// already authorized and never shows a dialog; `prompt = true` (explicit
  /// "Enable" opt-in) requests permission when undetermined. Idempotent.
  public func start(prompt: Bool) {
    configure()
    wantPrompt = wantPrompt || prompt

    if launchReason == "user", appStateString() == "background" {
      launchReason = "background-launch"
    }

    switch currentStatus() {
    case .authorizedAlways, .authorizedWhenInUse:
      beginUpdates()
    case .notDetermined:
      if prompt { manager.requestWhenInUseAuthorization() }  // two-step continues in didChange
    default:
      break  // denied / restricted — nothing to do
    }
  }

  /// One-time manager configuration (safe to call before authorization).
  private func configure() {
    if configured { return }
    configured = true
    manager.delegate = self
    manager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
    manager.allowsBackgroundLocationUpdates = true
    manager.pausesLocationUpdatesAutomatically = false
    manager.showsBackgroundLocationIndicator = true
  }

  /// Begin location services. Idempotent. Significant-location-change is the one
  /// service that relaunches a terminated app into the background; continuous
  /// updates give finer callbacks while the process is alive.
  private func beginUpdates() {
    if updating { return }
    updating = true
    manager.startMonitoringSignificantLocationChanges()
    manager.startUpdatingLocation()
    writeMarker(source: "start(reason=\(launchReason))")
  }

  private func currentStatus() -> CLAuthorizationStatus {
    if #available(iOS 14.0, *) { return manager.authorizationStatus }
    return CLLocationManager.authorizationStatus()
  }

  // MARK: - CLLocationManagerDelegate

  public func locationManager(_ m: CLLocationManager, didUpdateLocations locs: [CLLocation]) {
    guard let l = locs.last else { return }
    updateMotion(l)  // mode logic sees every callback, before the log throttle
    let now = Date()
    if let last = lastFixAt, now.timeIntervalSince(last) < minFixInterval { return }
    lastFixAt = now
    // Local rolling log (device-screen "recent activity" + background badge).
    write(lat: l.coordinate.latitude, lon: l.coordinate.longitude, source: "update")
    // Durable delivery: full-field record → shared outbox → box.
    enqueueFix(l)
    // Piggyback: re-arm audio (no-op unless enabled). Location keeps the app
    // alive and fires here regularly, so this is audio's best shot at recovering
    // the mic in the background after a call/interruption without a foreground.
    virtues_ensure_recording()
    // If this fix arrived while backgrounded (incl. a cold sig-loc relaunch),
    // drain to the box now — the foreground loop won't run until next launch.
    maybeDrainInBackground()
  }

  /// On a background/sig-loc wake, hold an OS background-task assertion and run
  /// a bounded drain so queued fixes reach the box before iOS suspends us.
  /// No-op in the foreground (the plugin's 20s loop handles that).
  private func maybeDrainInBackground() {
    if appStateString() == "active" { return }
    if isDraining { return }
    if let last = lastBgDrainAt, Date().timeIntervalSince(last) < minBgDrainInterval { return }
    lastBgDrainAt = Date()
    isDraining = true

    var bg: UIBackgroundTaskIdentifier = .invalid
    bg = UIApplication.shared.beginBackgroundTask(withName: "virtues-drain") {
      // Expiration: iOS is reclaiming the process — end the assertion.
      if bg != .invalid { UIApplication.shared.endBackgroundTask(bg); bg = .invalid }
    }
    let budget = Int32(min(max(UIApplication.shared.backgroundTimeRemaining - 3, 5), 25))
    NSLog("[LocationProbe] bg drain start, budget=%ds", budget)

    DispatchQueue.global(qos: .utility).async { [weak self] in
      let rc = virtues_drain_blocking(budget)
      NSLog("[LocationProbe] bg drain done rc/count=%d", rc)
      DispatchQueue.main.async {
        self?.isDraining = false
        if bg != .invalid { UIApplication.shared.endBackgroundTask(bg); bg = .invalid }
      }
    }
  }

  /// Build a box-shaped location record and enqueue it into the Rust outbox.
  private func enqueueFix(_ l: CLLocation) {
    var rec: [String: Any] = [
      "timestamp": isoMillis.string(from: l.timestamp),
      "latitude": l.coordinate.latitude,
      "longitude": l.coordinate.longitude,
      "altitude": l.altitude,
      "horizontal_accuracy": l.horizontalAccuracy,
      "vertical_accuracy": l.verticalAccuracy,
      "speed": max(l.speed, 0),
      // Provenance: whether this fix was captured while the app was active or
      // running autonomously in the background. The box stores `raw_data` into
      // metadata.ios_raw, so this lands as a queryable signal (no box change).
      "raw_data": ["app_state": appStateString()],
    ]
    if l.course >= 0 { rec["course"] = l.course }
    if let floor = l.floor { rec["floor_level"] = floor.level }

    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }

    let rc = "location".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[LocationProbe] enqueue failed rc=%d", rc) }
  }

  /// Motion-adaptive mode switching. Runs on every delegate callback.
  private func updateMotion(_ l: CLLocation) {
    let moved: Bool
    if let a = moveAnchor {
      // Coarse fixes carry km-scale accuracy — demand displacement beyond the
      // error bar before calling it movement (spurious escalations self-correct
      // anyway: 5 min stationary in precise drops right back).
      let threshold = mode == .coarse ? max(200, l.horizontalAccuracy) : moveThreshold
      moved = l.distance(from: a) > threshold || l.speed > movingSpeed
    } else {
      moved = true
    }
    if moved {
      moveAnchor = l
      lastMovedAt = Date()
      if mode == .coarse { apply(.precise, reason: "movement") }
      return
    }
    if mode == .precise, audioState == 2,
      Date().timeIntervalSince(lastMovedAt) > stationaryAfter {
      apply(.coarse, reason: "stationary+audio")
    }
  }

  private func apply(_ new: PowerMode, reason: String) {
    if mode == new { return }
    mode = new
    switch new {
    case .precise:
      manager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
      manager.distanceFilter = kCLDistanceFilterNone
    case .coarse:
      manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
      manager.distanceFilter = 100
    }
    // Lands in the rolling log → visible under Recent activity, and greppable
    // in the device console for the battery A/B.
    writeMarker(source: "mode=\(new == .precise ? "precise" : "coarse") (\(reason))")
  }

  /// Audio plugin push (via C ABI): recording health drives how lazy location
  /// may be. Anything below healthy forces precise — location is the backup
  /// generator that keeps the process alive and the mic re-arm heartbeat fast
  /// while the audio session is down.
  public func setAudioState(_ state: Int32) {
    DispatchQueue.main.async { [weak self] in
      guard let self = self else { return }
      self.audioState = state
      if state < 2, self.mode == .coarse {
        self.apply(.precise, reason: "audio=\(state)")
      }
    }
  }

  public func locationManagerDidChangeAuthorization(_ m: CLLocationManager) {
    let status = currentStatus()
    writeMarker(source: "auth=\(status.rawValue)")
    switch status {
    case .authorizedWhenInUse:
      // Escalate to Always for background delivery (only after an explicit
      // opt-in that requested When-In-Use), then start.
      if wantPrompt { manager.requestAlwaysAuthorization() }
      beginUpdates()
    case .authorizedAlways:
      beginUpdates()
    default:
      break
    }
  }

  public func locationManager(_ m: CLLocationManager, didFailWithError error: Error) {
    writeMarker(source: "error=\(error.localizedDescription)")
  }

  // MARK: - Reads (for the JS viewer)

  public func readRows(limit: Int) -> [[String: Any]] {
    var out: [[String: Any]] = []
    withDB { db in
      var stmt: OpaquePointer?
      let sql =
        "SELECT ts, lat, lon, source, app_state, launch_reason FROM rows ORDER BY id DESC LIMIT ?"
      if sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK {
        sqlite3_bind_int(stmt, 1, Int32(limit))
        while sqlite3_step(stmt) == SQLITE_ROW {
          out.append([
            "ts": text(stmt, 0),
            "lat": sqlite3_column_double(stmt, 1),
            "lon": sqlite3_column_double(stmt, 2),
            "source": text(stmt, 3),
            "appState": text(stmt, 4),
            "launchReason": text(stmt, 5),
          ])
        }
      }
      sqlite3_finalize(stmt)
    }
    return out
  }

  // MARK: - Writes

  private func writeMarker(source: String) {
    write(lat: 0, lon: 0, source: source)
  }

  private func write(lat: Double, lon: Double, source: String) {
    let ts = ISO8601DateFormatter().string(from: Date())
    let state = appStateString()
    let reason = launchReason
    withDB { db in
      var stmt: OpaquePointer?
      let sql =
        "INSERT INTO rows (ts, lat, lon, source, app_state, launch_reason) VALUES (?,?,?,?,?,?)"
      if sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK {
        sqlite3_bind_text(stmt, 1, ts, -1, SQLITE_TRANSIENT)
        sqlite3_bind_double(stmt, 2, lat)
        sqlite3_bind_double(stmt, 3, lon)
        sqlite3_bind_text(stmt, 4, source, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(stmt, 5, state, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(stmt, 6, reason, -1, SQLITE_TRANSIENT)
        sqlite3_step(stmt)
      }
      sqlite3_finalize(stmt)
    }
    NSLog("[LocationProbe] %@ state=%@ reason=%@ (%f,%f)", source, state, reason, lat, lon)
  }

  // MARK: - SQLite plumbing

  private func dbPath() -> String {
    let dir = try! FileManager.default.url(
      for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    return dir.appendingPathComponent("location_probe.sqlite").path
  }

  private func withDB(_ body: (OpaquePointer) -> Void) {
    var db: OpaquePointer?
    if sqlite3_open(dbPath(), &db) == SQLITE_OK, let db = db {
      sqlite3_exec(
        db,
        """
        CREATE TABLE IF NOT EXISTS rows(
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          ts TEXT, lat REAL, lon REAL,
          source TEXT, app_state TEXT, launch_reason TEXT
        )
        """, nil, nil, nil)
      body(db)
    }
    if let db = db { sqlite3_close(db) }
  }

  private func text(_ stmt: OpaquePointer?, _ col: Int32) -> String {
    guard let c = sqlite3_column_text(stmt, col) else { return "" }
    return String(cString: c)
  }

  fileprivate func appStateString() -> String {
    let read: () -> String = {
      switch UIApplication.shared.applicationState {
      case .active: return "active"
      case .inactive: return "inactive"
      case .background: return "background"
      @unknown default: return "unknown"
      }
    }
    if Thread.isMainThread { return read() }
    return DispatchQueue.main.sync(execute: read)
  }
}

/// C-ABI push from the audio plugin (mirror of `virtues_ensure_recording`):
/// 0 = audio off, 1 = enabled but not recording, 2 = recording healthy.
@_cdecl("virtues_location_audio_state")
func virtues_location_audio_state(_ state: Int32) {
  LocationProbe.shared.setAudioState(state)
}
