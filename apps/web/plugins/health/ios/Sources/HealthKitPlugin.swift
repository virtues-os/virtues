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

  /// Fetch new samples now (the "Sync now" button; the drain is a separate call).
  @objc public func collect(_ invoke: Invoke) throws {
    HealthCollector.shared.collectAll()
    invoke.resolve([
      "authorized": HealthCollector.shared.authorized(),
      "collecting": HealthCollector.shared.isCollecting,
    ])
  }
}

@_cdecl("init_plugin_health")
func initPlugin() -> Plugin {
  // Register the BGProcessingTask handler during app launch (init_plugin_* runs
  // synchronously inside didFinishLaunching, the window iOS requires).
  BackgroundSync.shared.register()
  return HealthPlugin()
}
