import ArgumentParser
import ApplicationServices
import Foundation

/// Status output structure for JSON serialization
struct CollectorStatus: Codable {
    let running: Bool
    let paused: Bool
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

        // Check permissions
        let messagesDbPath = "~/Library/Messages/chat.db".expandingTildeInPath
        let hasFullDiskAccess = FileManager.default.isReadableFile(atPath: messagesDbPath)
        let hasAccessibility = checkAccessibility()

        // Get last sync time (from log or config)
        let lastSync = getLastSyncTime()

        return CollectorStatus(
            running: isRunning,
            paused: isPaused,
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

        // Permissions
        print("Permissions:")
        print("  Accessibility: \(status.hasAccessibility ? "\u{2713}" : "\u{2717}")")
        print("  Full Disk Access: \(status.hasFullDiskAccess ? "\u{2713}" : "\u{2717}")")
        if !status.hasFullDiskAccess {
            print("    \u{2192} System Settings \u{2192} Privacy & Security \u{2192} Full Disk Access")
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
