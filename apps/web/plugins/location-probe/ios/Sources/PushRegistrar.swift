import Foundation
import ObjectiveC
import UIKit
import UserNotifications

// Rust (reach plugin's ffi.rs): POST this device's push address to the box over
// the warm iroh client. NULL reports "unreachable". Dials, and blocks up to
// ~10s — call off the main thread, and only while the app is active.
@_silgen_name("virtues_report_push_address")
private func virtues_report_push_address(_ address: UnsafePointer<CChar>?) -> Int32

/// Keeps the box told where it can reach this phone.
///
/// The phone already has a KEY on the box (its iroh EndpointId). This keeps
/// the other half current: the ADDRESS the box reaches it at, which is the APNs
/// token. See agents/plan/reminders-plan.md.
///
/// Three rules, each of which is a way this silently stops working:
///
/// 1. **Re-register on every foreground, not once at pair time.** APNs rotates
///    tokens on restore-from-backup, reinstall and some OS updates. A box holding
///    a stale token pushes into nothing and no error reaches anyone.
/// 2. **Report "unreachable" explicitly.** If the owner turns notifications off,
///    APNs still accepts pushes for the token and returns success — the phone
///    just drops them. Only the phone knows, so the phone has to say.
/// 3. **Never dial from a background wake.** This plugin is relaunched cold by
///    significant-location changes; a report there would spend a radio wake on
///    something the next foreground does for free.
///
/// Never prompts on its own. The OS permission sheet appears only when the owner
/// presses a button (`request`) — an app that asks cold, at launch, with nothing
/// yet to say, is how it earns a permanent "Don't Allow".
final class PushRegistrar {
  static let shared = PushRegistrar()

  /// Serializes reports and owns the dedupe state below. Every read and write of
  /// `accepted`/`acceptedAt`/`waiters` happens on this queue.
  private let queue = DispatchQueue(label: "com.virtues.push", qos: .utility)
  private var hooksInstalled = false

  /// What the box last ACCEPTED. Outer nil = never; `.some(nil)` = the box
  /// accepted "unreachable". **Persisted**, because the box keeps the address
  /// across app restarts — and without it every cold open would ask for status
  /// before this launch's report lands and wrongly tell the owner the server
  /// does not have this phone's address.
  private var accepted: String?? = PushRegistrar.loadAccepted()
  private var acceptedAt = Date.distantPast
  /// Whether the most recent report failed (not paired, box unreachable, box
  /// refused). While true, the screen must not claim the server can reach this
  /// phone, whatever was accepted before — an unpaired phone reports -1 here.
  private var lastFailed = false
  /// The first report of every launch always goes out, so the box's
  /// `push_address_at` reflects this launch and a newly paired box learns the
  /// address without waiting out the coalesce window.
  private var reportedThisLaunch = false
  /// Foreground flaps (Control Center, a call) fire didBecomeActive in bursts
  /// seconds apart. The same value inside this window is already on the box.
  private let coalesce: TimeInterval = 60

  private static let acceptedKey = "virtues.push.accepted"
  /// Stored as the token, or "" for an accepted "unreachable".
  private static func loadAccepted() -> String?? {
    guard let v = UserDefaults.standard.string(forKey: acceptedKey) else { return nil }
    return .some(v.isEmpty ? nil : v)
  }

  /// `request` callers waiting for the report that their grant set off.
  private var waiters: [() -> Void] = []

  private init() {}

  /// Called from plugin init, which runs inside didFinishLaunching — the app
  /// delegate exists, and the token callback is installed before anything asks
  /// iOS to register.
  func start() {
    installDelegateHooks()
    NotificationCenter.default.addObserver(
      self, selector: #selector(onActive),
      name: UIApplication.didBecomeActiveNotification, object: nil)
    // Rule 3: a cold background relaunch reads .background here and does nothing.
    if UIApplication.shared.applicationState == .active { refresh() }
  }

  @objc private func onActive() { refresh() }

  // MARK: - Status for the UI

  /// `authorized` means the box has this phone's address, not merely that iOS
  /// said yes: without the `aps-environment` entitlement, or before the token
  /// arrives, iOS can say yes while the box still cannot reach the phone.
  /// `unregistered` is that gap, and the screen must not claim otherwise.
  func status(_ done: @escaping (String) -> Void) {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      self.queue.async {
        // `.ephemeral` (iOS 14+, App Clips only) is deliberately absent: the
        // package targets iOS 13, and this app is never an App Clip.
        switch settings.authorizationStatus {
        case .authorized, .provisional:
          if !self.lastFailed, case .some(.some(_)) = self.accepted {
            done("authorized")
          } else {
            done("unregistered")
          }
        case .notDetermined:
          done("not_determined")
        default:
          done("denied")
        }
      }
    }
  }

  /// The owner pressed "Allow". Shows the OS sheet if it has never been shown
  /// (after that iOS answers from Settings without asking), registers, and
  /// resolves once the box has the answer — or after 10s, so a slow link cannot
  /// hang the button.
  func request(_ done: @escaping (String) -> Void) {
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in
      var fired = false
      let finish = {
        // Always on `queue`: both the waiter and the timeout run there.
        if fired { return }
        fired = true
        self.status(done)
      }
      self.queue.async { self.waiters.append(finish) }
      self.queue.asyncAfter(deadline: .now() + 10, execute: finish)
      self.refresh(force: true)
    }
  }

  // MARK: - Registration

  private func refresh(force: Bool = false) {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      switch settings.authorizationStatus {
      case .authorized, .provisional:
        // The answer arrives on the delegate hooks below, which report it.
        DispatchQueue.main.async { UIApplication.shared.registerForRemoteNotifications() }
      default:
        self.report(nil, force: force)  // rule 2
      }
    }
  }

  fileprivate func didRegister(_ token: Data) {
    report(token.map { String(format: "%02x", $0) }.joined(), force: false)
  }

  /// Usually a missing `aps-environment` entitlement, or the simulator. Either
  /// way the box cannot reach this phone, and saying so is the whole point.
  fileprivate func didFail(_ error: Error) {
    NSLog("[virtues push] registration failed: %@", error.localizedDescription)
    report(nil, force: false)
  }

  private func report(_ address: String?, force: Bool) {
    queue.async {
      if !force, self.reportedThisLaunch, !self.lastFailed,
        case .some(let last) = self.accepted, last == address,
        Date().timeIntervalSince(self.acceptedAt) < self.coalesce
      {
        self.drainWaiters()
        return
      }
      let rc: Int32
      if let address {
        rc = address.withCString { virtues_report_push_address($0) }
      } else {
        rc = virtues_report_push_address(nil)
      }
      self.reportedThisLaunch = true
      if rc == 0 {
        self.accepted = .some(address)
        self.acceptedAt = Date()
        self.lastFailed = false
        UserDefaults.standard.set(address ?? "", forKey: Self.acceptedKey)
      } else {
        self.lastFailed = true
        // Not retried here: the next foreground reports again, and iOS re-issues
        // the token on every registerForRemoteNotifications.
        NSLog("[virtues push] box did not accept the report (rc=%d)", rc)
      }
      self.drainWaiters()
    }
  }

  private func drainWaiters() {
    let pending = waiters
    waiters.removeAll()
    pending.forEach { $0() }
  }

  // MARK: - Delegate hooks

  /// The app delegate belongs to tao (Rust), and Tauri forwards no remote-
  /// notification callbacks to plugins, so the two methods iOS calls with the
  /// token are attached to the delegate's class at runtime — the same move
  /// main.mm makes for the keyboard bar, and what every push SDK does.
  ///
  /// `class_addMethod` first: it only succeeds if the class does not already
  /// define the method, and then there is nothing to preserve. If it does (a
  /// future tao, another SDK), wrap it so ours runs and theirs still does.
  private func installDelegateHooks() {
    guard !hooksInstalled, let delegate = UIApplication.shared.delegate,
      let cls = object_getClass(delegate)
    else { return }
    hooksInstalled = true

    Self.hook(
      cls,
      #selector(UIApplicationDelegate.application(_:didRegisterForRemoteNotificationsWithDeviceToken:))
    ) { arg in
      if let token = arg as? Data { PushRegistrar.shared.didRegister(token) }
    }
    Self.hook(
      cls,
      #selector(UIApplicationDelegate.application(_:didFailToRegisterForRemoteNotificationsWithError:))
    ) { arg in
      if let error = arg as? Error { PushRegistrar.shared.didFail(error) }
    }
  }

  /// Both callbacks share one shape: `-(void)application:(UIApplication *)app
  /// <verb>:(id)arg`, type encoding `v@:@@`.
  private typealias Callback = @convention(c) (AnyObject, Selector, UIApplication, AnyObject) -> Void

  private static func hook(_ cls: AnyClass, _ sel: Selector, _ body: @escaping (AnyObject) -> Void) {
    let ours: @convention(block) (AnyObject, UIApplication, AnyObject) -> Void = { _, _, arg in
      body(arg)
    }
    if class_addMethod(cls, sel, imp_implementationWithBlock(ours), "v@:@@") { return }

    guard let method = class_getInstanceMethod(cls, sel) else { return }
    let original = unsafeBitCast(method_getImplementation(method), to: Callback.self)
    let chained: @convention(block) (AnyObject, UIApplication, AnyObject) -> Void = { me, app, arg in
      body(arg)
      original(me, sel, app, arg)
    }
    method_setImplementation(method, imp_implementationWithBlock(chained))
  }
}
