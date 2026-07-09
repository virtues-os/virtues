import Tauri
import UIKit

class HealthPlugin: Plugin {
  /// Explicit "Enable": prompt for HealthKit access, then backfill + collect.
  @objc public func enable(_ invoke: Invoke) throws {
    HealthCollector.shared.enable { ok in
      invoke.resolve(["authorized": ok, "collecting": HealthCollector.shared.isCollecting])
    }
  }

  /// Launch auto-resume: collect only if already opted in; never prompts.
  @objc public func resume(_ invoke: Invoke) throws {
    HealthCollector.shared.resume()
    invoke.resolve([
      "authorized": HealthCollector.shared.authorized(),
      "collecting": HealthCollector.shared.isCollecting,
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve([
      "authorized": HealthCollector.shared.authorized(),
      "collecting": HealthCollector.shared.isCollecting,
    ])
  }
}

@_cdecl("init_plugin_health")
func initPlugin() -> Plugin {
  return HealthPlugin()
}
