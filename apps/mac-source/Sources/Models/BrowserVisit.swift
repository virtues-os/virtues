import Foundation

/// One page visit, in the shape the box's `mac_ingest` action already accepts
/// (`browser_history: [{url, title, timestamp, browser}]` → `data_activity_web_browsing`).
/// `timestamp` is ISO-8601 UTC; the box parses it and derives the domain itself.
struct BrowserVisit {
    let url: String
    let title: String?
    /// ISO-8601 UTC. Each browser stores time in its own epoch — see BrowserMonitor.
    let timestamp: String
    /// "dia" | "safari" | "chrome" | … — lands in the row's metadata.
    let browser: String

    func toDictionary() -> [String: Any] {
        var dict: [String: Any] = [
            "url": url,
            "timestamp": timestamp,
            "browser": browser,
        ]
        if let title { dict["title"] = title }
        return dict
    }
}
