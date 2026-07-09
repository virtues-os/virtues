import BackgroundTasks
import Foundation
import UIKit

// Drain the outbox to the box (defined in the reach plugin's ffi.rs).
@_silgen_name("virtues_drain_blocking")
private func virtues_drain_blocking(_ timeoutSecs: Int32) -> Int32

/// Movement-independent background sync via `BGProcessingTask`.
///
/// Significant-location wakes only fire when you *move*; a stationary phone
/// would never sync while closed. `BGProcessingTask` is iOS's periodic "you're
/// idle (usually charging + on Wi-Fi), here's a longer window" — we use it to
/// collect fresh health samples and drain the whole outbox (all streams).
///
/// iOS requires the handler be registered during app launch, so `register()` is
/// called from `init_plugin_health` (which runs inside didFinishLaunching).
final class BackgroundSync {
  static let shared = BackgroundSync()
  /// Must match `BGTaskSchedulerPermittedIdentifiers` in the plist.
  private let taskId = "com.virtues.ios.sync"
  private var registered = false

  private init() {}

  func register() {
    if registered { return }
    registered = true
    BGTaskScheduler.shared.register(forTaskWithIdentifier: taskId, using: nil) { [weak self] task in
      self?.handle(task as! BGProcessingTask)
    }
  }

  /// Ask iOS to schedule the next run (~15 min out, at its discretion). Call on
  /// foreground/background transitions.
  func schedule() {
    let request = BGProcessingTaskRequest(identifier: taskId)
    request.requiresNetworkConnectivity = true
    request.requiresExternalPower = false
    request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
    try? BGTaskScheduler.shared.submit(request)
  }

  private func handle(_ task: BGProcessingTask) {
    schedule()  // chain the next one first

    // Collect fresh health samples (location keeps flowing via CLLocationManager),
    // then drain everything queued. Run off the main thread; end on completion.
    let work = DispatchWorkItem {
      HealthCollector.shared.collectAll()
      let rc = virtues_drain_blocking(25)
      NSLog("[BackgroundSync] BGProcessingTask drain rc/count=%d", rc)
      task.setTaskCompleted(success: rc >= 0)
    }
    task.expirationHandler = { work.cancel() }
    DispatchQueue.global(qos: .background).async(execute: work)
  }
}
