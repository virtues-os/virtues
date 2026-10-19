import Tauri
import UIKit

class ContactsPlugin: Plugin {
  @objc public func enable(_ invoke: Invoke) throws {
    ContactsCollector.shared.enable { ok in
      invoke.resolve(["authorized": ok, "collecting": ContactsCollector.shared.isCollecting])
    }
  }

  @objc public func resume(_ invoke: Invoke) throws {
    ContactsCollector.shared.resume()
    invoke.resolve([
      "authorized": ContactsCollector.shared.authorized(),
      "collecting": ContactsCollector.shared.isCollecting,
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve([
      "authorized": ContactsCollector.shared.authorized(),
      "collecting": ContactsCollector.shared.isCollecting,
    ])
  }

  @objc public func collect(_ invoke: Invoke) throws {
    ContactsCollector.shared.collectAll()
    invoke.resolve([
      "authorized": ContactsCollector.shared.authorized(),
      "collecting": ContactsCollector.shared.isCollecting,
    ])
  }
}

@_cdecl("init_plugin_contacts")
func initPlugin() -> Plugin {
  return ContactsPlugin()
}
