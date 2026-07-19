import ArgumentParser
import Foundation

@main
struct VirtuesCollector: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "virtues-collector",
        abstract: "Virtues data collector daemon",
        version: Version.current,
        subcommands: [
            InitCommand.self,
            StartCommand.self,
            InstallCommand.self,
            UninstallCommand.self,
            StatusCommand.self,
            PauseCommand.self,
            ResumeCommand.self,
            StopCommand.self,
            ResetCommand.self
        ],
        defaultSubcommand: StatusCommand.self
    )
}

// Safe process execution helper - uses direct arguments, no shell interpolation
func safeExec(_ executable: String, _ arguments: [String], captureOutput: Bool = true) -> (output: String, exitCode: Int32) {
    let task = Process()
    let pipe = Pipe()

    task.executableURL = URL(fileURLWithPath: executable)
    task.arguments = arguments

    if captureOutput {
        task.standardOutput = pipe
        task.standardError = pipe
    }

    do {
        try task.run()
        task.waitUntilExit()

        if captureOutput {
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return (String(data: data, encoding: .utf8) ?? "", task.terminationStatus)
        }
        return ("", task.terminationStatus)
    } catch {
        return ("Error: \(error)", -1)
    }
}

// Get current user ID safely
func getCurrentUserId() -> uid_t {
    return getuid()
}

// Check if a LaunchAgent is running
func isLaunchAgentRunning(label: String) -> Bool {
    let (output, _) = safeExec("/bin/launchctl", ["list"])
    return output.contains(label)
}

// Legacy shell helper - ONLY use for commands that genuinely need shell features
// and where paths are validated/controlled
@available(*, deprecated, message: "Use safeExec instead when possible")
func shell(_ command: String) -> String {
    let task = Process()
    let pipe = Pipe()

    task.standardOutput = pipe
    task.standardError = pipe
    task.arguments = ["-c", command]
    task.launchPath = "/bin/bash"

    do {
        try task.run()
        task.waitUntilExit()

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        return String(data: data, encoding: .utf8) ?? ""
    } catch {
        return "Error: \(error)"
    }
}

// String path expansion helper
extension String {
    var expandingTildeInPath: String {
        return NSString(string: self).expandingTildeInPath
    }
}
