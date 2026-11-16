// The iOS half of the reach plugin: setup that has to happen in Swift when
// the plugin loads (the outbox kept out of backups, the App Group path for
// the widget, the share inbox). The plugin's commands all run in Rust.

import Foundation
import Tauri

class ReachPlugin: Plugin {}

/// `<AppSupport>/virtues` holds the outbox — gigabytes of base64 audio churn
/// when the box is unreachable — and none of it belongs in iCloud/device
/// backups: the queue is transient by design, and the pairing that must
/// survive lives in the Keychain, not here. Creates the dir if the Rust side
/// has not yet (both creators tolerate the other), then flags it excluded.
private func excludeVirtuesDirFromBackup() {
  let fm = FileManager.default
  guard let base = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
    return
  }
  var dir = base.appendingPathComponent("virtues", isDirectory: true)
  try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
  var vals = URLResourceValues()
  vals.isExcludedFromBackup = true
  do {
    try dir.setResourceValues(vals)
  } catch {
    NSLog("[Reach] backup exclusion failed: %@", error.localizedDescription)
  }
}

// Tell Rust where the App Group container is, so it can write the upload
// snapshot the Virtues widget reads (reach/src/widget.rs).
@_silgen_name("virtues_set_shared_dir")
private func virtues_set_shared_dir(_ path: UnsafePointer<CChar>)

@_cdecl("init_plugin_reach")
func initPlugin() -> Plugin {
  excludeVirtuesDirFromBackup()
  if let shared = FileManager.default
    .containerURL(forSecurityApplicationGroupIdentifier: ShareInbox.appGroup)
  {
    shared.path.withCString { virtues_set_shared_dir($0) }
  }
  // Shares made from the share sheet while the app was closed leave on the
  // next foreground; see ShareInbox.swift.
  ShareInbox.observeForeground()
  return ReachPlugin()
}
