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
    
    enum EventType {
        static let focus = "focus_gained"
        static let unfocus = "focus_lost"
        static let launch = "launch"
        static let quit = "quit"
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