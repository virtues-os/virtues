import ArgumentParser
import Foundation

struct StopCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "stop",
        abstract: "Stop the background daemon (keeps it installed)"
    )

    func run() throws {
        print("Stopping daemon...")

        // Check both old and new plist paths
        let newPlistPath = "~/Library/LaunchAgents/com.virtues.collector.plist".expandingTildeInPath
        let oldPlistPath = "~/Library/LaunchAgents/com.virtues.mac.plist".expandingTildeInPath

        var plistPath: String?
        if FileManager.default.fileExists(atPath: newPlistPath) {
            plistPath = newPlistPath
        } else if FileManager.default.fileExists(atPath: oldPlistPath) {
            plistPath = oldPlistPath
        }

        guard let path = plistPath else {
            print("LaunchAgent not installed")
            print("Run 'virtues-collector install' to install")
            return
        }

        // Unload the agent
        let userId = getCurrentUserId()
        let (result, exitCode) = safeExec("/bin/launchctl", ["bootout", "gui/\(userId)", path])

        if exitCode != 0 && !result.contains("Could not find") {
            print("\u{26A0}  Warning: \(result.trimmingCharacters(in: .whitespacesAndNewlines))")
        } else {
            print("\u{2713} Daemon stopped")
        }

        // Verify it's stopped
        Thread.sleep(forTimeInterval: 0.5)
        let isStillRunning = isLaunchAgentRunning(label: "com.virtues")
        if !isStillRunning {
            print("\u{2713} Service is no longer running")
            print("")
            print("To restart: virtues-collector install")
            print("To uninstall: virtues-collector uninstall")
        } else {
            print("\u{26A0}  Service may still be running")
        }
    }
}
