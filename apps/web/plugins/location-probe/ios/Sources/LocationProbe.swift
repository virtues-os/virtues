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
  private var started = false

  /// Throttle: keep at most one fix per ~15s (matches the native app's sampling
  /// cadence — avoids flooding the log + outbox with near-identical points).
  private var lastFixAt: Date?
  private let minFixInterval: TimeInterval = 15

  /// Set by the AppDelegate: "user" for a normal launch, "location" when the
  /// app was relaunched by the OS for a location event (launchOptions carried
  /// `.location`). This is the flag that proves cold-relaunch collection.
  public var launchReason: String = "user"

  private override init() { super.init() }

  /// Idempotent — safe to call from both the Rust setup hook and a JS command.
  public func start() {
    if started { return }
    started = true

    // If the process came up straight into the background, the OS relaunched us
    // (e.g. for a significant-location change) with no UI — the exact case the
    // spike is proving. Record that as the launch reason.
    if launchReason == "user", appStateString() == "background" {
      launchReason = "background-launch"
    }

    manager.delegate = self
    manager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
    manager.allowsBackgroundLocationUpdates = true
    manager.pausesLocationUpdatesAutomatically = false
    manager.showsBackgroundLocationIndicator = true
    manager.requestAlwaysAuthorization()

    // Significant-location-change is the one service that relaunches a
    // terminated app into the background. startUpdatingLocation gives finer
    // callbacks while the process is alive.
    manager.startMonitoringSignificantLocationChanges()
    manager.startUpdatingLocation()

    writeMarker(source: "start(reason=\(launchReason))")
  }

  // MARK: - CLLocationManagerDelegate

  public func locationManager(_ m: CLLocationManager, didUpdateLocations locs: [CLLocation]) {
    guard let l = locs.last else { return }
    let now = Date()
    if let last = lastFixAt, now.timeIntervalSince(last) < minFixInterval { return }
    lastFixAt = now
    // Local rolling log (device-screen "recent activity" + background badge).
    write(lat: l.coordinate.latitude, lon: l.coordinate.longitude, source: "update")
    // Durable delivery: full-field record → shared outbox → box.
    enqueueFix(l)
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
      // Local-only breadcrumb so the device screen can badge background fixes;
      // the box keeps it in metadata (harmless).
      "app_state": appStateString(),
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

  public func locationManagerDidChangeAuthorization(_ m: CLLocationManager) {
    let raw: Int32
    if #available(iOS 14.0, *) {
      raw = m.authorizationStatus.rawValue
    } else {
      raw = CLLocationManager.authorizationStatus().rawValue
    }
    writeMarker(source: "auth=\(raw)")
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

  private func appStateString() -> String {
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
