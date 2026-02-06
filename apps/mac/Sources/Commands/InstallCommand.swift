import ArgumentParser
import Foundation

struct InstallCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "install",
        abstract: "Install collector as a background service (LaunchAgent)"
    )

    @Option(name: .long, help: "Device authentication token")
    var token: String?

    @Flag(name: .long, help: "Read token from VIRTUES_TOKEN environment variable")
    var tokenFromEnv = false

    func run() throws {
        // 1. Validate token if provided (or check existing config)
        // Support reading from environment variable for security (avoids token in ps output)
        let effectiveToken: String?
        if tokenFromEnv {
            effectiveToken = ProcessInfo.processInfo.environment["VIRTUES_TOKEN"]
            if effectiveToken == nil {
                throw ConfigError.invalidToken
            }
        } else {
            effectiveToken = token
        }

        if let token = effectiveToken {
            print("Validating token...")
            let (endpoint, deviceName) = try runAsyncAndWait {
                try await Config.validateToken(token)
            }

            let config = Config(
                deviceToken: token,
                deviceId: UUID().uuidString,
                apiEndpoint: endpoint,
                createdAt: Date()
            )
            try config.save()
            print("\u{2713} Configured for \(deviceName)")
        } else {
            // Check if already configured
            guard Config.load() != nil else {
                throw ConfigError.notConfigured
            }
        }

        // 2. Copy binary to ~/.virtues/bin/
        let binDir = "~/.virtues/bin".expandingTildeInPath
        let installPath = "\(binDir)/virtues-collector"

        print("Installing binary...")
        try FileManager.default.createDirectory(
            atPath: binDir,
            withIntermediateDirectories: true
        )

        // Get current executable path
        let currentPath = ProcessInfo.processInfo.arguments[0]
        let resolvedCurrentPath = URL(fileURLWithPath: currentPath).standardized.path

        // Remove existing if present
        if FileManager.default.fileExists(atPath: installPath) {
            try FileManager.default.removeItem(atPath: installPath)
        }

        // Copy binary
        try FileManager.default.copyItem(atPath: resolvedCurrentPath, toPath: installPath)

        // Make executable using FileManager
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: installPath)
        print("\u{2713} Binary installed to \(installPath)")

        // 3. Create logs directory
        let logsDir = "~/.virtues/logs".expandingTildeInPath
        try FileManager.default.createDirectory(atPath: logsDir, withIntermediateDirectories: true)

        // 4. Create LaunchAgent plist
        let launchAgentsDir = "~/Library/LaunchAgents".expandingTildeInPath
        let plistPath = "\(launchAgentsDir)/com.virtues.collector.plist"

        try FileManager.default.createDirectory(
            atPath: launchAgentsDir,
            withIntermediateDirectories: true
        )

        let homeDir = FileManager.default.homeDirectoryForCurrentUser.path
        let plistContent = """
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
            <key>Label</key>
            <string>com.virtues.collector</string>

            <key>ProgramArguments</key>
            <array>
                <string>\(installPath)</string>
                <string>start</string>
            </array>

            <key>RunAtLoad</key>
            <true/>

            <key>KeepAlive</key>
            <dict>
                <key>SuccessfulExit</key>
                <false/>
                <key>Crashed</key>
                <true/>
            </dict>

            <key>ProcessType</key>
            <string>Background</string>

            <key>StandardOutPath</key>
            <string>\(homeDir)/.virtues/logs/collector.log</string>

            <key>StandardErrorPath</key>
            <string>\(homeDir)/.virtues/logs/collector.error.log</string>

            <key>WorkingDirectory</key>
            <string>\(homeDir)</string>

            <key>ThrottleInterval</key>
            <integer>10</integer>
        </dict>
        </plist>
        """

        // Unload if already loaded
        let userId = getCurrentUserId()
        let _ = safeExec("/bin/launchctl", ["bootout", "gui/\(userId)", plistPath])
        Thread.sleep(forTimeInterval: 0.5)

        // Write plist
        try plistContent.write(toFile: plistPath, atomically: true, encoding: .utf8)
        print("\u{2713} Created LaunchAgent plist")

        // 5. Load with launchctl
        let (loadResult, loadExitCode) = safeExec("/bin/launchctl", ["bootstrap", "gui/\(userId)", plistPath])
        if loadExitCode != 0 && !loadResult.contains("already exists") {
            print("\u{26A0}  Warning: \(loadResult.trimmingCharacters(in: .whitespacesAndNewlines))")
        } else {
            print("\u{2713} LaunchAgent loaded")
        }

        // Verify it's running
        Thread.sleep(forTimeInterval: 1)
        let isRunning = isLaunchAgentRunning(label: "com.virtues.collector")
        if isRunning {
            print("\u{2713} Service is running")
            print("")
            print("\u{2705} Collector installed successfully!")
            print("")
            print("Logs: ~/.virtues/logs/collector.log")
            print("Stop: virtues-collector uninstall")
            print("Status: virtues-collector status")
        } else {
            print("\u{26A0}  Service may not have started.")
            print("Check logs: ~/.virtues/logs/collector.error.log")
        }
    }

    /// Helper to run async code synchronously
    private func runAsyncAndWait<T>(_ block: @escaping () async throws -> T) throws -> T {
        var result: Result<T, Error>?
        let semaphore = DispatchSemaphore(value: 0)

        Task {
            do {
                result = .success(try await block())
            } catch {
                result = .failure(error)
            }
            semaphore.signal()
        }

        semaphore.wait()

        switch result! {
        case .success(let value):
            return value
        case .failure(let error):
            throw error
        }
    }
}
