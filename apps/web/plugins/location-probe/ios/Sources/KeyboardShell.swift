import Foundation
import UIKit
import WebKit

/// Shell keyboard behavior for the WKWebView the whole app lives in.
///
/// Hosted in this plugin only because it is the one plugin guaranteed loaded
/// at launch on iOS and `load(webview:)` is the sole place Tauri hands a
/// plugin the webview — this is shell behavior, not location behavior, and it
/// moves the day a dedicated shell plugin exists.
///
/// Three jobs:
///
/// 1. **Programmatic focus raises the keyboard.** WKWebView refuses to
///    present the keyboard for a `focus()` the user didn't tap for, so the
///    chat-first shell's auto-focused empty composer showed a caret and no
///    keys. The only lever is the one Cordova and Capacitor have shipped for
///    years: swizzle WKContentView's focus method so `userIsInteracting`
///    arrives true. Private selector, industry-standard workaround, App
///    Store-tolerated at enormous scale.
///
/// 2. **The scroll view stays parked — by prevention, not repair.** The SPA
///    pins its shell (`position:fixed; inset:0`) and scrolls only inner
///    elements, so the WKScrollView's offset is rightfully always zero. When
///    the keyboard rises, WKWebView "helps": it animates a pan to reveal the
///    focused field. The first version of this file reset the pan on
///    keyboard notifications — after iOS had animated it — and the visible
///    result was the composer riding up and snapping back. The pan is now
///    clamped continuously (KVO on contentOffset), so it never gets a frame
///    on screen; automatic insets stay off and any keyboard inset is zeroed.
///
/// 3. **The frame bridge.** The web side needs to know how much of the
///    window the keyboard covers, and it needs to know BEFORE the keyboard
///    moves so its own layout can animate on the same clock.
///    `keyboardWillChangeFrame` carries the final frame and UIKit's
///    duration; both are handed to `window.__virtuesKeyboardInset(px, ms)`
///    (see stores/keyboard.svelte.ts). visualViewport — the web side's only
///    other source — fires mid-animation with intermediate values, which is
///    exactly the staircase this bridge exists to replace.
enum KeyboardShell {
  private static var attached = false
  private static var offsetClamp: NSKeyValueObservation?

  static func attach(_ webview: WKWebView) {
    guard !attached else { return }
    attached = true

    allowProgrammaticFocus()

    let scroll = webview.scrollView
    scroll.contentInsetAdjustmentBehavior = .never

    // Job 2: the clamp. Any non-zero document offset is put back within the
    // same runloop turn, so WKWebView's focus-reveal pan never renders.
    // Zoomed pages are exempt — a pinch-zoomed page genuinely needs to pan,
    // and clamping it would freeze the viewport in place.
    offsetClamp = scroll.observe(\.contentOffset, options: [.new]) { sv, change in
      guard sv.zoomScale == 1 else { return }
      guard let offset = change.newValue, offset != .zero else { return }
      sv.contentOffset = .zero
    }

    let center = NotificationCenter.default

    // Insets aren't part of the offset clamp: WKWebView applies a bottom
    // contentInset for the keyboard (the phantom scrollbar), and zeroing it
    // on the notifications that create it is sufficient.
    let resetInsets: (Notification) -> Void = { [weak webview] _ in
      DispatchQueue.main.async {
        guard let sv = webview?.scrollView else { return }
        if sv.contentInset != .zero { sv.contentInset = .zero }
        if sv.verticalScrollIndicatorInsets != .zero { sv.verticalScrollIndicatorInsets = .zero }
      }
    }
    for name in [
      UIResponder.keyboardWillShowNotification,
      UIResponder.keyboardDidShowNotification,
      UIResponder.keyboardWillHideNotification,
    ] {
      // Observers live for the app's lifetime, like the webview they guard.
      _ = center.addObserver(forName: name, object: nil, queue: .main, using: resetInsets)
    }

    // Job 3: the frame bridge. willChangeFrame fires for show, hide, and
    // height changes (emoji keyboard, accessory bars) with the END frame —
    // a hide reports an offscreen frame, so overlap lands on 0 by itself.
    _ = center.addObserver(
      forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: .main
    ) { [weak webview] note in
      guard let webview, let window = webview.window,
        let end = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
      else { return }
      let duration =
        (note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double) ?? 0.25
      let frameInWindow = window.convert(end, from: nil)
      let overlap = max(0, window.bounds.maxY - frameInWindow.minY)
      let js =
        "window.__virtuesKeyboardInset && window.__virtuesKeyboardInset(\(Int(overlap.rounded())), \(Int((duration * 1000).rounded())))"
      webview.evaluateJavaScript(js, completionHandler: nil)
    }
  }

  /// The Capacitor/Cordova swizzle: force `userIsInteracting: true` into
  /// WKContentView's element-focus call so the keyboard presents for
  /// programmatic focus. Both selector generations are tried; whichever the
  /// running OS implements gets wrapped.
  private static func allowProgrammaticFocus() {
    guard let contentView: AnyClass = NSClassFromString("WKContentView") else { return }

    // iOS 12.2+ (still current): activityStateChanges is an NSInteger.
    typealias NewFocus = @convention(c) (Any, Selector, UnsafeRawPointer, Bool, Bool, Int, Any?) -> Void
    let newSel = sel_getUid("_elementDidFocus:userIsInteracting:blurPreviousNode:activityStateChanges:userObject:")
    if let method = class_getInstanceMethod(contentView, newSel) {
      let original = unsafeBitCast(method_getImplementation(method), to: NewFocus.self)
      let block: @convention(block) (Any, UnsafeRawPointer, Bool, Bool, Int, Any?) -> Void = {
        me, arg0, _, arg2, arg3, arg4 in
        original(me, newSel, arg0, true, arg2, arg3, arg4)
      }
      method_setImplementation(method, imp_implementationWithBlock(block))
      return
    }

    // iOS 11.3–12.1: changingActivityState is a Bool.
    typealias OldFocus = @convention(c) (Any, Selector, UnsafeRawPointer, Bool, Bool, Bool, Any?) -> Void
    let oldSel = sel_getUid("_elementDidFocus:userIsInteracting:blurPreviousNode:changingActivityState:userObject:")
    if let method = class_getInstanceMethod(contentView, oldSel) {
      let original = unsafeBitCast(method_getImplementation(method), to: OldFocus.self)
      let block: @convention(block) (Any, UnsafeRawPointer, Bool, Bool, Bool, Any?) -> Void = {
        me, arg0, _, arg2, arg3, arg4 in
        original(me, oldSel, arg0, true, arg2, arg3, arg4)
      }
      method_setImplementation(method, imp_implementationWithBlock(block))
    }
  }
}
