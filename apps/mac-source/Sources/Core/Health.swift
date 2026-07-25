import ApplicationServices
import Foundation

/// The daemon's own view of what it is currently allowed to read, persisted so
/// that other processes can report the DAEMON's truth instead of accidentally
/// reporting their own.
///
/// This type exists because of a specific, costly bug. macOS TCC grants are per
/// *process*, not per binary path: `virtues-collector status` run from a
/// terminal that happens to hold Full Disk Access performs a successful probe
/// and prints "Full Disk Access: ✓", while the launchd daemon — a different TCC
/// principal running the same executable — is being denied. The one diagnostic
/// a person reaches for reported the exact opposite of reality, and an iMessage
/// outage went unnoticed for four days behind it.
///
/// So: only the daemon writes this file, and everyone else reads it rather than
/// probing. A reader that finds no file, or a stale one, must say so — silently
/// falling back to its own probe is the bug this replaces.
struct CollectorHealth: Codable {
    /// Can the daemon read `~/Library/Messages/chat.db`? Gates iMessages *and*
    /// Safari history, which live behind the same grant.
    var fullDiskAccess: Bool
    /// Can the daemon observe window/focus state?
    var accessibility: Bool
    /// When the daemon last evaluated the above.
    var updatedAt: Date

    /// Older than this and we no longer trust the record — the daemon evaluates
    /// permissions every 5 minutes, so three missed rounds means it is wedged,
    /// not running, or running a build that never wrote one.
    static let staleAfter: TimeInterval = 15 * 60

    var isStale: Bool {
        Date().timeIntervalSince(updatedAt) > Self.staleAfter
    }

    static var fileURL: URL {
        Config.configDir.appendingPathComponent("health.json")
    }

    /// Capability names the box knows, for anything currently denied. Empty
    /// when everything the daemon needs is granted.
    var deniedCapabilities: [String] {
        var denied: [String] = []
        if !fullDiskAccess { denied.append("full_disk_access") }
        if !accessibility { denied.append("accessibility") }
        return denied
    }

    // ── daemon side ──────────────────────────────────────────────────────────

    /// Evaluate the current process's permissions. Only meaningful when called
    /// **from the daemon**; the CLI must not use this to describe the daemon.
    static func probeCurrentProcess() -> CollectorHealth {
        CollectorHealth(
            fullDiskAccess: MessageMonitor.canReadMessagesDB(),
            accessibility: AXIsProcessTrusted(),
            updatedAt: Date()
        )
    }

    /// Probe and persist. Called by the daemon on startup and on every
    /// permission re-check, so the record tracks reality within ~5 minutes —
    /// including the good transition, when a user grants access and we want the
    /// UI to stop complaining without waiting for a restart.
    @discardableResult
    static func recordFromDaemon() -> CollectorHealth {
        let health = probeCurrentProcess()
        health.save()
        return health
    }

    private func save() {
        do {
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(self)
            try FileManager.default.createDirectory(
                at: Config.configDir, withIntermediateDirectories: true)
            // Atomic: a reader must never see a half-written record.
            try data.write(to: Self.fileURL, options: .atomic)
        } catch {
            // Health reporting must never take the collector down — losing the
            // record degrades diagnostics, which is strictly better than losing
            // collection.
            print("⚠️ could not write health record: \(error.localizedDescription)")
        }
    }

    // ── reader side ──────────────────────────────────────────────────────────

    /// The daemon's last self-report, or `nil` if it has never written one.
    static func load() -> CollectorHealth? {
        guard let data = try? Data(contentsOf: fileURL) else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try? decoder.decode(CollectorHealth.self, from: data)
    }
}
