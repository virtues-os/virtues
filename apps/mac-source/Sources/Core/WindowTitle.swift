import ApplicationServices
import Foundation

/// Reads the focused window title of an app via the Accessibility API.
///
/// This is what turns "used Chrome for 40 minutes" into "read <page>" / "edited
/// <doc>". The box's `mac_ingest` applet ALREADY accepts `window_title` on
/// `app_events` and folds it into the session row (`data_activity_app_usage`) —
/// the collector just never sent it. No box change needed.
///
/// Requires the Accessibility permission. When untrusted we return nil, so the
/// collector degrades cleanly to app-name-only rather than failing — which is
/// exactly why a missing grant is so easy to miss: no error, no warning, just an
/// empty column.
enum WindowTitle {
  /// Whether the collector is trusted for Accessibility.
  static var isTrusted: Bool { AXIsProcessTrusted() }

  /// ASK for Accessibility, rather than only checking for it.
  ///
  /// This is not the same as `AXIsProcessTrusted()`, and the difference cost hours.
  /// For a bare (non-bundled) executable like this one, dragging the binary into
  /// System Settings → Accessibility creates an entry that LOOKS enabled while
  /// `AXIsProcessTrusted()` keeps returning false — the toggle is on, the grant is
  /// not. The process has to *request* the permission so TCC registers its identity
  /// against the entry; only then does the checkbox mean anything.
  ///
  /// Call once at startup. When already trusted it's a silent no-op; when not, it
  /// prompts (and the prompt is what makes the eventual grant actually bind).
  @discardableResult
  static func request() -> Bool {
    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
    return AXIsProcessTrustedWithOptions(options as CFDictionary)
  }

  /// The focused window title for a pid, or nil if untrusted/unavailable.
  static func focused(pid: pid_t) -> String? {
    guard AXIsProcessTrusted() else { return nil }

    let app = AXUIElementCreateApplication(pid)
    // An unresponsive (beachballing) app must never hang the monitor thread —
    // AX calls are synchronous IPC into the target process.
    AXUIElementSetMessagingTimeout(app, 0.4)

    var windowRef: CFTypeRef?
    guard
      AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &windowRef)
        == .success,
      let windowRef,
      CFGetTypeID(windowRef) == AXUIElementGetTypeID()
    else { return nil }
    let window = unsafeBitCast(windowRef, to: AXUIElement.self)

    var titleRef: CFTypeRef?
    guard
      AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &titleRef) == .success,
      let title = titleRef as? String
    else { return nil }

    let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}
