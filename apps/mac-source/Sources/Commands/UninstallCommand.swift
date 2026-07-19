import ArgumentParser
import Foundation

struct UninstallCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "uninstall",
        abstract: "Remove collector background service"
    )

    @Flag(name: .long, help: "Also delete collected data and configuration")
    var deleteData = false

    func run() throws {
        let plistPath = "~/Library/LaunchAgents/com.virtues.collector.plist".expandingTildeInPath
        let installPath = "~/.virtues/bin/virtues-collector".expandingTildeInPath

        // 1. Stop and unload LaunchAgent
        print("Stopping service...")
        let userId = getCurrentUserId()
        let _ = safeExec("/bin/launchctl", ["bootout", "gui/\(userId)", plistPath])
        Thread.sleep(forTimeInterval: 0.5)

        // 2. Remove plist
        if FileManager.default.fileExists(atPath: plistPath) {
            try FileManager.default.removeItem(atPath: plistPath)
            print("\u{2713} Removed LaunchAgent plist")
        }

        // 3. Remove installed binary
        if FileManager.default.fileExists(atPath: installPath) {
            try FileManager.default.removeItem(atPath: installPath)
            print("\u{2713} Removed installed binary")
        }

        // 4. Optionally delete data
        if deleteData {
            print("Deleting data...")

            // Delete activity database
            let dbPath = "~/.virtues/activity.db".expandingTildeInPath
            if FileManager.default.fileExists(atPath: dbPath) {
                try FileManager.default.removeItem(atPath: dbPath)
                print("\u{2713} Deleted activity database")
            }

            // Delete config file
            let configPath = "~/.virtues/config.json".expandingTildeInPath
            if FileManager.default.fileExists(atPath: configPath) {
                try FileManager.default.removeItem(atPath: configPath)
                print("\u{2713} Deleted config file")
            }

            // Delete token from Keychain
            try? Config.delete()
            print("\u{2713} Deleted credentials from Keychain")

            // Delete logs
            let logsPath = "~/.virtues/logs".expandingTildeInPath
            if FileManager.default.fileExists(atPath: logsPath) {
                try FileManager.default.removeItem(atPath: logsPath)
                print("\u{2713} Deleted logs")
            }

            // Delete pause state
            let pausePath = "~/.virtues/paused".expandingTildeInPath
            try? FileManager.default.removeItem(atPath: pausePath)
        }

        print("")
        print("\u{2705} Collector uninstalled")

        if !deleteData {
            print("")
            print("Data preserved at ~/.virtues/")
            print("To also delete data: virtues-collector uninstall --delete-data")
        }
    }
}
