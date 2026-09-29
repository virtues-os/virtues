import Tauri
import UIKit

class HealthPlugin: Plugin {
  /// The one status payload every command resolves. `authorized` is the
  /// opt-in flag (see `optedIn`); `permission` is what iOS says about the sheet.
  private static func resolveStatus(_ invoke: Invoke) {
    let c = HealthCollector.shared
    c.permission { permission in
      invoke.resolve([
        "authorized": c.optedIn(),
        "collecting": c.isCollecting,
        "permission": permission,
      ])
    }
  }

  /// Explicit "Enable": show the HealthKit sheet, then backfill + collect.
  /// Resolves once the sheet closes; whether reads were allowed stays unknown.
  @objc public func enable(_ invoke: Invoke) throws {
    HealthCollector.shared.enable { _ in HealthPlugin.resolveStatus(invoke) }
  }

  /// Launch auto-resume: collect only if already opted in; never prompts.
  @objc public func resume(_ invoke: Invoke) throws {
    HealthCollector.shared.resume()
    HealthPlugin.resolveStatus(invoke)
  }

  @objc public func status(_ invoke: Invoke) throws {
    HealthPlugin.resolveStatus(invoke)
  }

  /// Fetch new samples now (the "Sync now" button; the drain is a separate call).
  @objc public func collect(_ invoke: Invoke) throws {
    HealthCollector.shared.collectAll()
    HealthPlugin.resolveStatus(invoke)
  }
}

@_cdecl("init_plugin_health")
func initPlugin() -> Plugin {
  // Register the BGProcessingTask handler during app launch (init_plugin_* runs
  // synchronously inside didFinishLaunching, the window iOS requires).
  BackgroundSync.shared.register()
  return HealthPlugin()
}
