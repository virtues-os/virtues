import ApplicationServices
import Foundation

/// Reads the focused window title of an app via the Accessibility API.
///
/// This is what turns "used Chrome for 40 minutes" into "read <page>" / "edited
/// <doc>". The box's `mac_ingest` action ALREADY accepts `window_title` on
/// `app_events` and folds it into the session row (`data_activity_app_usage`) —
/// the collector just never sent it. No box change needed.
///
/// Requires the Accessibility permission (there is no prompt for it — the user
/// grants it in System Settings; the "This Mac" screen deep-links there and
/// truth-polls the daemon). When untrusted we return nil, so the collector
/// degrades cleanly to app-name-only rather than failing.
enum WindowTitle {
  /// Whether the collector is trusted for Accessibility.
  static var isTrusted: Bool { AXIsProcessTrusted() }

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
