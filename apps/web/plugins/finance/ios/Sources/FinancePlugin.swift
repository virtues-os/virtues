import Tauri
import UIKit

class FinancePlugin: Plugin {
  @objc public func enable(_ invoke: Invoke) throws {
    FinanceCollector.shared.enable { ok in
      invoke.resolve(["authorized": ok, "collecting": FinanceCollector.shared.isCollecting])
    }
  }

  @objc public func resume(_ invoke: Invoke) throws {
    FinanceCollector.shared.resume()
    invoke.resolve([
      "authorized": FinanceCollector.shared.authorized(),
      "collecting": FinanceCollector.shared.isCollecting,
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve([
      "authorized": FinanceCollector.shared.authorized(),
      "collecting": FinanceCollector.shared.isCollecting,
    ])
  }

  @objc public func collect(_ invoke: Invoke) throws {
    FinanceCollector.shared.collectAll()
    invoke.resolve([
      "authorized": FinanceCollector.shared.authorized(),
      "collecting": FinanceCollector.shared.isCollecting,
    ])
  }
}

@_cdecl("init_plugin_finance")
func initPlugin() -> Plugin {
  return FinancePlugin()
}
