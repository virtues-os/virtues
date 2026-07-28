import ArgumentParser
import ApplicationServices
import Foundation

/// Status output structure for JSON serialization
struct CollectorStatus: Codable {
    let running: Bool
    let paused: Bool
    /// True when the permission flags below came from the daemon's own
    /// self-report and that report is fresh. False means we are falling back to
    /// this process's probe, which describes THIS process — see CollectorHealth.
    let permissionsReportedByDaemon: Bool
    /// When the daemon last evaluated its permissions, if it ever has.
    let permissionsCheckedAt: String?
    let pendingEvents: Int
    let pendingMessages: Int
    let lastSync: String?
    let hasFullDiskAccess: Bool
    let hasAccessibility: Bool
}

struct StatusCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "status",
        abstract: "Show current status and queue statistics"
    )

    @Flag(name: .long, help: "Output as JSON for programmatic use")
    var json = false

    func run() throws {
        let status = collectStatus()

        if json {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .prettyPrinted
            encoder.keyEncodingStrategy = .convertToSnakeCase
            let data = try encoder.encode(status)
            print(String(data: data, encoding: .utf8)!)
        } else {
            printHumanReadable(status)
        }
    }

    private func collectStatus() -> CollectorStatus {
        // Check if running via LaunchAgent
        let launchAgentPath = "~/Library/LaunchAgents/com.virtues.collector.plist".expandingTildeInPath
        let launchAgentExists = FileManager.default.fileExists(atPath: launchAgentPath)

        var isRunning = false
        if launchAgentExists {
            isRunning = isLaunchAgentRunning(label: "com.virtues.collector")
        }

        // Check pause state from config
        let isPaused = checkPauseState()

        // Get queue statistics
        var pendingEvents = 0
        var pendingMessages = 0
        if let queue = try? Queue() {
            pendingEvents = (try? queue.pendingEventCount()) ?? 0
            pendingMessages = (try? queue.pendingMessageCount()) ?? 0
        }

        // Permissions are reported for the DAEMON, not for whoever is running
        // this command. macOS TCC grants are per-process: run from a terminal
        // that holds Full Disk Access, our own probe succeeds and we would
        // cheerfully print "Full Disk Access: ✓" while the launchd daemon is
        // being denied — which is exactly what hid a four-day iMessage outage.
        //
        // We still perform the live probe, because a *denied* real open is how
        // macOS enrols this executable in the Full Disk Access list and gives
        // the user a row to toggle. But its result only describes this process,
        // so it is never what we report as the daemon's state.
        let selfProbe = MessageMonitor.canReadMessagesDB()
        let daemonHealth = CollectorHealth.load()
        let hasFullDiskAccess = daemonHealth?.fullDiskAccess ?? selfProbe
        let hasAccessibility = daemonHealth?.accessibility ?? checkAccessibility()

        // Get last sync time (from log or config)
        let lastSync = getLastSyncTime()

        return CollectorStatus(
            running: isRunning,
            paused: isPaused,
            permissionsReportedByDaemon: daemonHealth != nil && !(daemonHealth?.isStale ?? true),
            permissionsCheckedAt: daemonHealth.map { ISO8601DateFormatter().string(from: $0.updatedAt) },
            pendingEvents: pendingEvents,
            pendingMessages: pendingMessages,
            lastSync: lastSync,
            hasFullDiskAccess: hasFullDiskAccess,
            hasAccessibility: hasAccessibility
        )
    }

    private func printHumanReadable(_ status: CollectorStatus) {
        print("Virtues Collector Status")
        print("=" * 30)

        // Check config
        if let config = Config.load() {
            print("\u{2713} Configured")
            print("  Device ID: \(config.deviceId)")
            print("  API: \(config.apiEndpoint)")
        } else {
            print("\u{2717} Not configured")
            print("  Run 'virtues-collector init <token>' to configure")
            return
        }

        print("")

        // Monitoring status
        print("Service Status:")
        if status.running {
            if status.paused {
                print("  \u{23F8}  Paused (daemon running, collection stopped)")
            } else {
                print("  \u{2713} Running")
            }
        } else {
            print("  \u{2717} Not running")
            print("  Run 'virtues-collector install' to start")
        }

        print("")

        // Permissions — the daemon's, not this process's.
        print("Permissions:")
        if !status.permissionsReportedByDaemon {
            // Say so loudly. A number you cannot source is worse than no number:
            // this is the line that would have prevented a four-day outage.
            print("  \u{26A0} the daemon has not reported recently — showing THIS")
            print("    process's permissions, which may differ from the daemon's.")
            if status.running {
                print("    (running an older collector build? restart it:")
                print("     launchctl kickstart -k gui/$(id -u)/com.virtues.collector)")
            }
        }
        print("  Accessibility: \(status.hasAccessibility ? "\u{2713}" : "\u{2717}")")
        print("  Full Disk Access: \(status.hasFullDiskAccess ? "\u{2713}" : "\u{2717}")")
        if status.permissionsReportedByDaemon, let checkedAt = status.permissionsCheckedAt {
            print("    (as seen by the daemon at \(checkedAt))")
        }
        if !status.hasFullDiskAccess {
            print("    \u{2192} System Settings \u{2192} Privacy & Security \u{2192} Full Disk Access \u{2192} turn on virtues-collector")
            print("      (not listed? click + and add ~/.virtues/bin/virtues-collector)")
        }

        print("")

        // Queue stats
        print("Queue:")
        print("  Pending events: \(status.pendingEvents)")
        print("  Pending messages: \(status.pendingMessages)")

        if let lastSync = status.lastSync {
            print("  Last sync: \(lastSync)")
        }

        if status.pendingEvents + status.pendingMessages > 100 {
            print("\n\u{26A0}  High number of pending items. Check network connection.")
        }
    }

    private func checkPauseState() -> Bool {
        // Check if paused flag exists in a state file
        let pauseFile = "~/.virtues/paused".expandingTildeInPath
        return FileManager.default.fileExists(atPath: pauseFile)
    }

    private func checkAccessibility() -> Bool {
        // Use AXIsProcessTrusted to check if app has Accessibility permissions
        return AXIsProcessTrusted()
    }

    private func getLastSyncTime() -> String? {
        // Check the log for last successful upload
        let logPath = "~/.virtues/logs/collector.log".expandingTildeInPath
        guard FileManager.default.fileExists(atPath: logPath),
              let content = try? String(contentsOfFile: logPath, encoding: .utf8) else {
            return nil
        }

        // Find last "uploaded" line
        let lines = content.components(separatedBy: .newlines).reversed()
        for line in lines {
            if line.contains("uploaded") || line.contains("sync complete") {
                // Extract timestamp from beginning of line
                if let range = line.range(of: "\\[.*?\\]", options: .regularExpression) {
                    return String(line[range]).trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
                }
            }
        }
        return nil
    }
}

// Helper for string repetition
extension String {
    static func *(lhs: String, rhs: Int) -> String {
        return String(repeating: lhs, count: rhs)
    }
}
