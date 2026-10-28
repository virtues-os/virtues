import Foundation

/// The pure rules for recording calls: which processes count as a call app,
/// which processes to tap for that app's far side, and when a call has started
/// and ended. No Core Audio here, so every rule is testable (`swift test`);
/// `CallAudioMonitor` does the listening and recording.
public enum CallPolicy {
    /// One app whose calls are recorded.
    public struct App: Equatable, Hashable {
        /// Shown in logs and stored with the recording.
        public let name: String
        /// A process belongs to this app when its bundle id starts with one of
        /// these. Browsers and Electron apps play call audio from helper
        /// processes, so a prefix match is what reaches the audio.
        public let bundlePrefixes: [String]
    }

    /// The fixed list of call apps (decided 2026-10-26). Any app that opens
    /// the mic would also catch dictation tools, Siri, Voice Memos and screen
    /// recorders, which hold the input with no call. A new call app is added
    /// here in a collector update.
    public static let apps: [App] = [
        App(name: "Zoom", bundlePrefixes: ["us.zoom."]),
        App(name: "Microsoft Teams", bundlePrefixes: ["com.microsoft.teams"]),
        App(name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"]),
        App(name: "Discord", bundlePrefixes: ["com.hnc.Discord"]),
        App(name: "WhatsApp", bundlePrefixes: ["net.whatsapp.WhatsApp"]),
        App(name: "Signal", bundlePrefixes: ["org.whispersystems.signal-desktop"]),
        // FaceTime and iPhone calls taken on the Mac: the audio runs in these
        // system daemons, not in the FaceTime app.
        App(name: "FaceTime", bundlePrefixes: ["com.apple.avconferenced", "com.apple.TelephonyUtilities"]),
        // Browsers, for Meet and every other web call. A browser tap also
        // hears the browser's other tabs.
        App(name: "Chrome", bundlePrefixes: ["com.google.Chrome"]),
        App(name: "Arc or Dia", bundlePrefixes: ["company.thebrowser."]),
        App(name: "Edge", bundlePrefixes: ["com.microsoft.edgemac"]),
        App(name: "Brave", bundlePrefixes: ["com.brave.Browser"]),
        App(name: "Firefox", bundlePrefixes: ["org.mozilla.firefox"]),
        // Safari and every WebKit view play audio from the shared WebKit GPU
        // process, so a Safari call taps all WebKit audio.
        App(name: "Safari", bundlePrefixes: ["com.apple.WebKit.GPU", "com.apple.Safari"]),
    ]

    /// The call app a process belongs to, or nil when it is not one.
    public static func app(forBundleID bundleID: String) -> App? {
        guard !bundleID.isEmpty else { return nil }
        return apps.first { app in app.bundlePrefixes.contains { bundleID.hasPrefix($0) } }
    }

    /// How long no call app may hold the mic before the call counts as over.
    /// Long enough that muting and unmuting, or a call app reopening its input
    /// when the route changes, does not split one call into two.
    public static let endGrace: TimeInterval = 10

    /// Turns once-a-second observations of which call apps hold the mic into
    /// call start and end events. A struct with no clock of its own: the
    /// caller passes the time, so tests drive it directly.
    public struct Tracker {
        public enum Event: Equatable {
            case started(App)
            case ended(App)
        }

        /// The app being recorded, if a call is in progress.
        public private(set) var current: App?
        private var lastSeen: Date?

        public init() {}

        /// Feed one observation: the call apps holding the mic at `now`.
        ///
        /// One call is recorded at a time. When two call apps hold the mic,
        /// the one already being recorded keeps it; otherwise the first in
        /// `CallPolicy.apps` order wins, so the choice does not flip between
        /// polls.
        public mutating func observe(_ holding: Set<App>, at now: Date) -> [Event] {
            if let app = current {
                if holding.contains(app) {
                    lastSeen = now
                    return []
                }
                guard let seen = lastSeen, now.timeIntervalSince(seen) >= CallPolicy.endGrace else {
                    return []
                }
                current = nil
                lastSeen = nil
                return [.ended(app)] + observe(holding, at: now)
            }
            guard let next = CallPolicy.apps.first(where: holding.contains) else { return [] }
            current = next
            lastSeen = now
            return [.started(next)]
        }
    }
}
