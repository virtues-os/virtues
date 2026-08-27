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
/// Two jobs:
///
/// 1. **Programmatic focus raises the keyboard.** WKWebView refuses to
///    present the keyboard for a `focus()` the user didn't tap for, so the
///    chat-first shell's auto-focused empty composer showed a caret and no
///    keys. The only lever is the one Cordova and Capacitor have shipped for
///    years: swizzle WKContentView's focus method so `userIsInteracting`
///    arrives true. Private selector, industry-standard workaround, App
///    Store-tolerated at enormous scale.
///
/// 2. **The scroll view stays parked.** The SPA pins its shell
///    (`position:fixed; inset:0`) and scrolls only inner elements, so the
///    WKScrollView's content offset is rightfully always zero — but when the
///    keyboard rises, WKWebView "helps": it insets the scroll view and pans
///    to reveal the focused field, on top of the SPA's own `--keyboard-inset`
///    compensation. The double accommodation is the visible jolt (and the
///    phantom scroll indicator) on input focus. So: no automatic insets, and
///    any inset/offset iOS applies for the keyboard is put back.
enum KeyboardShell {
  private static var attached = false

  static func attach(_ webview: WKWebView) {
    guard !attached else { return }
    attached = true

    allowProgrammaticFocus()

    let scroll = webview.scrollView
    scroll.contentInsetAdjustmentBehavior = .never

    let reset: (Notification) -> Void = { [weak webview] _ in
      DispatchQueue.main.async {
        guard let sv = webview?.scrollView else { return }
        if sv.contentInset != .zero { sv.contentInset = .zero }
        if sv.contentOffset != .zero { sv.setContentOffset(.zero, animated: false) }
        if sv.verticalScrollIndicatorInsets != .zero { sv.verticalScrollIndicatorInsets = .zero }
      }
    }
    let center = NotificationCenter.default
    for name in [
      UIResponder.keyboardWillShowNotification,
      UIResponder.keyboardDidShowNotification,
      UIResponder.keyboardWillChangeFrameNotification,
      UIResponder.keyboardWillHideNotification,
    ] {
      // Observers live for the app's lifetime, like the webview they guard.
      _ = center.addObserver(forName: name, object: nil, queue: .main, using: reset)
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
