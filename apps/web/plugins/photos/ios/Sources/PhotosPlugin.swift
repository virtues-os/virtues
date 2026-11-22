import Tauri
import UIKit

class PhotosPlugin: Plugin {
  @objc public func enable(_ invoke: Invoke) throws {
    PhotosCollector.shared.enable { _ in
      invoke.resolve(PhotosCollector.shared.statusDict())
    }
  }

  @objc public func resume(_ invoke: Invoke) throws {
    PhotosCollector.shared.resume()
    invoke.resolve(PhotosCollector.shared.statusDict())
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve(PhotosCollector.shared.statusDict())
  }

  @objc public func collect(_ invoke: Invoke) throws {
    PhotosCollector.shared.collect()
    invoke.resolve(PhotosCollector.shared.statusDict())
  }
}

@_cdecl("init_plugin_photos")
func initPlugin() -> Plugin {
  return PhotosPlugin()
}
