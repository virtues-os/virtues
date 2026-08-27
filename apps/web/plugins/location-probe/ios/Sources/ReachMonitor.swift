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

// Radio-cost flag for the Rust side (reach plugin's ffi.rs): 1 while the radio
// is expensive (cellular / Low Power Mode, not charging) so background drains
// batch ~3 chunks per dial instead of dialing per chunk.
@_silgen_name("virtues_radio_constrained")
private func virtues_radio_constrained(_ constrained: Int32)

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
    // Seed the flag with the LAUNCH state BEFORE the path monitor starts:
    // NWPathMonitor delivers its initial path immediately on the utility queue,
    // and the recovery it kicks reads this flag — an async seed loses that race
    // and lets a cold background relaunch cold-build an endpoint it shouldn't.
    // start() runs inside didFinishLaunching on the main thread, so the read is
    // synchronous; the async branch is a defensive fallback only.
    if Thread.isMainThread {
      virtues_app_background(UIApplication.shared.applicationState == .background ? 1 : 0)
    } else {
      DispatchQueue.main.async {
        virtues_app_background(UIApplication.shared.applicationState == .background ? 1 : 0)
      }
    }
    monitor.pathUpdateHandler = { [weak self] path in
      self?.pathExpensive = path.isExpensive || path.usesInterfaceType(.cellular)
      self?.pushRadioPolicy("path")
      self?.kick("path")
    }
    monitor.start(queue: queue)
    NotificationCenter.default.addObserver(
      self, selector: #selector(onForeground),
      name: UIApplication.didBecomeActiveNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(onBackground),
      name: UIApplication.didEnterBackgroundNotification, object: nil)
    // The other two radio-policy inputs. Battery monitoring must be explicitly
    // enabled or batteryState always reads .unknown.
    UIDevice.current.isBatteryMonitoringEnabled = true
    NotificationCenter.default.addObserver(
      self, selector: #selector(onPowerChange),
      name: UIDevice.batteryStateDidChangeNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(onPowerChange),
      name: .NSProcessInfoPowerStateDidChange, object: nil)
    pushRadioPolicy("launch")
    NSLog("[ReachMonitor] started (NWPathMonitor + fg/bg + radio policy)")
  }

  // MARK: - Radio policy (drain batching on expensive radio)

  /// Latest path verdict from NWPathMonitor (written on its utility queue).
  private var pathExpensive = false
  private var lastPushedConstrained: Int32 = -1

  /// Constrained = expensive path (cellular/hotspot) or Low Power Mode, unless
  /// charging (on power, batching buys nothing — drain freely). Pushed to Rust
  /// only on change; safe to call from any thread (hops to main for the UIKit
  /// battery read).
  private func pushRadioPolicy(_ reason: String) {
    if !Thread.isMainThread {
      DispatchQueue.main.async { [weak self] in self?.pushRadioPolicy(reason) }
      return
    }
    let charging: Bool = {
      switch UIDevice.current.batteryState {
      case .charging, .full: return true
      default: return false
      }
    }()
    let lowPower = ProcessInfo.processInfo.isLowPowerModeEnabled
    let constrained: Int32 = ((pathExpensive || lowPower) && !charging) ? 1 : 0
    if constrained == lastPushedConstrained { return }
    lastPushedConstrained = constrained
    virtues_radio_constrained(constrained)
    NSLog("[ReachMonitor] radio %@ (%@)", constrained == 1 ? "constrained" : "cheap", reason)
  }

  @objc private func onPowerChange() { pushRadioPolicy("power") }

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
