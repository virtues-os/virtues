import Foundation
import Network
import UIKit

// Rust reach recovery (defined in the reach plugin's ffi.rs). Pokes iroh's
// `network_change`, and if a bounded probe still fails, rebuilds the whole
// endpoint — the escape from the iOS UDP-socket wedge (iroh#4289) that leaves the
// app fully offline (pages/chat/uploads dead) until force-quit.
@_silgen_name("virtues_recover_connection")
private func virtues_recover_connection() -> Int32

// App-state flag for the Rust side (reach plugin's ffi.rs). Backgrounded is
// what licenses endpoint parking after uploads — the only way to stop iroh's
// keepalive chatter so the cell radio can idle between drains.
@_silgen_name("virtues_app_background")
private func virtues_app_background(_ backgrounded: Int32)

/// Watches the two events that wedge iroh's UDP socket on iOS — **network path
/// changes** (Wi-Fi↔cellular/LAN) and **app foreground** (after a suspend that
/// killed the socket) — and kicks the Rust recovery. Lives in the always-on
/// location plugin because the Rust-only reach plugin has no iOS lifecycle hook.
///
/// Tailscale (whose netmon iroh's is derived from) proves this rebind-on-change/
/// foreground layer is necessary even *with* a Network Extension; we run in-app,
/// so it's mandatory. See docs/reach-reliability-plan.md.
final class ReachMonitor {
  static let shared = ReachMonitor()

  private let monitor = NWPathMonitor()
  private let queue = DispatchQueue(label: "com.virtues.reachmon", qos: .utility)
  private var started = false
  private var lastRun = Date.distantPast
  /// Coalesce bursts (AirPods/route flaps + foreground fire together). The Rust
  /// side also guards against overlapping recoveries.
  private let minInterval: TimeInterval = 3

  private init() {}

  func start() {
    if started { return }
    started = true
    monitor.pathUpdateHandler = { [weak self] _ in self?.kick("path") }
    monitor.start(queue: queue)
    NotificationCenter.default.addObserver(
      self, selector: #selector(onForeground),
      name: UIApplication.didBecomeActiveNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(onBackground),
      name: UIApplication.didEnterBackgroundNotification, object: nil)
    // Seed the flag with the LAUNCH state: a cold sig-loc relaunch starts in the
    // background with no didEnterBackground notification, and the flag must be
    // right before the first drain decides whether to park the endpoint.
    DispatchQueue.main.async {
      virtues_app_background(UIApplication.shared.applicationState == .background ? 1 : 0)
    }
    NSLog("[ReachMonitor] started (NWPathMonitor + fg/bg)")
  }

  @objc private func onForeground() {
    virtues_app_background(0)
    kick("foreground")
  }

  @objc private func onBackground() { virtues_app_background(1) }

  private func kick(_ reason: String) {
    queue.async { [weak self] in
      guard let self = self else { return }
      let now = Date()
      if now.timeIntervalSince(self.lastRun) < self.minInterval { return }
      self.lastRun = now
      // Blocks ~1–5s (poke → probe → maybe rebuild); we're on the utility queue.
      let rc = virtues_recover_connection()
      NSLog("[ReachMonitor] recover(%@) rc=%d", reason, rc)
    }
  }
}
