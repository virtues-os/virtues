import Foundation

struct Event: Codable {
    let id: Int64?
    let timestamp: Date
    let eventType: String
    let appName: String
    let bundleId: String?
    /// Focused window title at the moment of the event (Accessibility). nil when
    /// the permission isn't granted — the box folds it into the session row.
    let windowTitle: String?
    var uploaded: Bool = false

    init(eventType: String, appName: String, bundleId: String?, windowTitle: String? = nil) {
        self.id = nil
        self.timestamp = Date()
        self.eventType = eventType
        self.appName = appName
        self.bundleId = bundleId
        self.windowTitle = windowTitle
        self.uploaded = false
    }

    init(
        timestamp: Date, eventType: String, appName: String, bundleId: String?,
        windowTitle: String? = nil
    ) {
        self.id = nil
        self.timestamp = timestamp
        self.eventType = eventType
        self.appName = appName
        self.bundleId = bundleId
        self.windowTitle = windowTitle
        self.uploaded = false
    }
    
    /// Focus AND presence transitions travel as one ordered stream.
    ///
    /// They have to: sessionizing correctly means interleaving "you switched to
    /// Cursor" with "you walked away" in true time order, and two separate arrays
    /// would have to be merged back together on arrival anyway. One stream, sorted
    /// by timestamp, is the thing the box actually needs.
    enum EventType {
        static let focus = "focus_gained"
        static let unfocus = "focus_lost"
        static let launch = "launch"
        static let quit = "quit"

        /// The focused app is STILL focused. Without this, any unclean shutdown —
        /// a crash, a power loss, a binary swap — leaves a session open with no
        /// end, and the box can only clamp it back to its own start: duration
        /// zero, dropped. Exactly the bug this whole rewrite exists to kill, with
        /// a different cause. A heartbeat bounds that loss to one interval.
        static let heartbeat = "heartbeat"

        // Presence. `idle` is the absence of input; `watching` is the absence of
        // input while the focused app is holding the display awake — a video, a
        // call. The distinction is the difference between "you watched a lecture"
        // and "you left the room."
        static let idleStart = "idle_start"
        static let idleEnd = "idle_end"
        static let watchStart = "watch_start"
        static let watchEnd = "watch_end"
        static let lock = "lock"
        static let unlock = "unlock"
        static let sleep = "sleep"
        static let wake = "wake"
    }
    
    var toDictionary: [String: Any] {
        var dict: [String: Any] = [
            "timestamp": ISO8601DateFormatter().string(from: timestamp),
            "event_type": eventType,
            "app_name": appName
        ]
        if let bundleId = bundleId {
            dict["bundle_id"] = bundleId
        }
        // `mac_ingest` reads this and folds it into data_activity_app_usage.
        if let windowTitle = windowTitle {
            dict["window_title"] = windowTitle
        }
        return dict
    }
}