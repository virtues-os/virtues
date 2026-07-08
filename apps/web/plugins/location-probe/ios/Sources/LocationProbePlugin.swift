import SwiftRs
import Tauri
import UIKit
import WebKit

class RowsArgs: Decodable {
  let limit: Int?
}

class LocationProbePlugin: Plugin {
  /// Start (or re-affirm) background location collection from JS. On a normal
  /// launch this is how the delegate gets installed; on a cold background
  /// relaunch the AppDelegate has already called `LocationProbe.shared.start()`
  /// before any webview — this call is then a harmless no-op (idempotent).
  @objc public func startProbe(_ invoke: Invoke) throws {
    DispatchQueue.main.async { LocationProbe.shared.start() }
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
  return LocationProbePlugin()
}
