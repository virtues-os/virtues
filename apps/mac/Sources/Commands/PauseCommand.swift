import ArgumentParser
import Foundation

struct PauseCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "pause",
        abstract: "Pause data collection (daemon keeps running)"
    )

    func run() throws {
        let pauseFile = "~/.virtues/paused".expandingTildeInPath
        let pauseDir = "~/.virtues".expandingTildeInPath

        // Ensure directory exists
        try FileManager.default.createDirectory(
            atPath: pauseDir,
            withIntermediateDirectories: true
        )

        // Create pause flag file
        FileManager.default.createFile(atPath: pauseFile, contents: nil)

        print("\u{23F8}  Collection paused")
        print("")
        print("The collector daemon is still running but not collecting data.")
        print("Resume with: virtues-collector resume")
    }
}
