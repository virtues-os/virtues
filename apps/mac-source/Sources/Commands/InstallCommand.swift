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

    @Flag(
        name: .long,
        help: "Install even if this binary is ad-hoc signed (its macOS permissions will not survive a rebuild)"
    )
    var force = false

    func run() throws {
        // 0. Refuse an ad-hoc signed binary. See CodeIdentity for why: TCC pins
        // an ad-hoc grant to the cdhash, so installing this build silently
        // voids Full Disk Access and Accessibility the next time anyone
        // rebuilds — and the settings UI goes on claiming both are granted.
        // Blocking here is the only point where a person is present to read it.
        let runningPath = URL(fileURLWithPath: ProcessInfo.processInfo.arguments[0])
            .standardized.path
        if CodeIdentity.isAdHocSigned(path: runningPath) == true {
            guard force else { throw InstallError.adHocSigned(path: runningPath) }
            print("\u{26A0}  Ad-hoc signed build installed with --force.")
            print(
                "   macOS permissions will be voided by the next rebuild. When that "
                    + "happens, REMOVE and re-add virtues-collector in System Settings →")
            print(
                "   Privacy & Security → Full Disk Access and Accessibility. Toggling "
                    + "them off and on does not repair the grant.")
            print("")
        }

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
            print("Pairing...")
            let pair = try runAsyncAndWait {
                try await Config.pairConsume(token: token)
            }

            let config = Config(
                deviceId: pair.deviceId,
                apiEndpoint: pair.endpoint,
                appletIds: pair.appletIds,
                boxNodeId: pair.boxNodeId,
                relayUrl: pair.relayUrl,
                createdAt: Date(),
                deviceSeed: pair.seed
            )
            try config.save()
            print("\u{2713} Paired (auth: iroh key — reach \(pair.boxNodeId))")
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

        // Remove existing if present
        if FileManager.default.fileExists(atPath: installPath) {
            try FileManager.default.removeItem(atPath: installPath)
        }

        // Copy binary
        // The signature is embedded in the Mach-O, so a plain copy preserves
        // it — a Developer ID grant follows the binary here and keeps matching.
        try FileManager.default.copyItem(atPath: runningPath, toPath: installPath)

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

enum InstallError: LocalizedError {
    /// Refusing to install an ad-hoc signed collector. Not a style objection:
    /// macOS ties an ad-hoc TCC grant to the exact cdhash, so this binary's
    /// permissions die at the next rebuild while System Settings keeps drawing
    /// them as granted.
    case adHocSigned(path: String)

    var errorDescription: String? {
        switch self {
        case .adHocSigned(let path):
            return """
                Refusing to install an ad-hoc signed collector.

                  \(path)

                This binary has no signing identity, so macOS pins its Full Disk
                Access and Accessibility grants to this exact build. The next
                rebuild invalidates them — and it does so silently: the switches
                in System Settings stay on while every read fails, so iMessages,
                Safari history and window titles stop arriving with no error
                anywhere except the collector log.

                Build it signed instead:

                  APPLE_SIGNING_IDENTITY="Developer ID Application: … (TEAMID)" \\
                    tools/build-mac-app.sh

                To install anyway — accepting that permissions will need to be
                removed and re-added in System Settings after every rebuild,
                since toggling them off and on does not repair the grant:

                  virtues-collector install --force
                """
        }
    }
}
