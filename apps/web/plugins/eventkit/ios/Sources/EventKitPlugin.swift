import Tauri
import UIKit

class EventKitPlugin: Plugin {
  @objc public func enable(_ invoke: Invoke) throws {
    EventKitCollector.shared.enable { ok in
      invoke.resolve(["authorized": ok, "collecting": EventKitCollector.shared.isCollecting])
    }
  }

  @objc public func resume(_ invoke: Invoke) throws {
    EventKitCollector.shared.resume()
    invoke.resolve([
      "authorized": EventKitCollector.shared.authorized(),
      "collecting": EventKitCollector.shared.isCollecting,
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve([
      "authorized": EventKitCollector.shared.authorized(),
      "collecting": EventKitCollector.shared.isCollecting,
    ])
  }

  @objc public func collect(_ invoke: Invoke) throws {
    EventKitCollector.shared.collectAll()
    invoke.resolve([
      "authorized": EventKitCollector.shared.authorized(),
      "collecting": EventKitCollector.shared.isCollecting,
    ])
  }
}

@_cdecl("init_plugin_eventkit")
func initPlugin() -> Plugin {
  return EventKitPlugin()
}
