import SwiftRs
import Tauri
import UIKit
import WebKit

class RowsArgs: Decodable {
  let limit: Int?
}

class LocationProbePlugin: Plugin {
  /// Explicit user opt-in ("Enable"): prompt for permission if undetermined,
  /// then start collecting.
  @objc public func startProbe(_ invoke: Invoke) throws {
    DispatchQueue.main.async { LocationProbe.shared.start(prompt: true) }
    invoke.resolve(["started": true])
  }

  /// Launch auto-resume: start collecting only if already authorized; never
  /// prompts. Called on every launch (incl. cold background relaunch).
  @objc public func resumeProbe(_ invoke: Invoke) throws {
    DispatchQueue.main.async { LocationProbe.shared.start(prompt: false) }
    invoke.resolve(["started": true])
  }

  /// Return the rows the native side has written to SQLite.
  @objc public func readRows(_ invoke: Invoke) throws {
    let limit = (try? invoke.parseArgs(RowsArgs.self))?.limit ?? 200
    let rows = LocationProbe.shared.readRows(limit: limit)
    invoke.resolve(["rows": rows])
  }
}

@_cdecl("init_plugin_location_probe")
func initPlugin() -> Plugin {
  // Start the reach recovery watchdog at launch (runs inside didFinishLaunching):
  // heals the iroh socket on every network-path change + app foreground so the
  // box is reachable whenever it's up — no force-quit.
  ReachMonitor.shared.start()
  // Register for MetricKit's daily battery/network/location digests.
  Metrics.shared.start()
  return LocationProbePlugin()
}
