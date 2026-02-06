import ArgumentParser
import Foundation

struct ResumeCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "resume",
        abstract: "Resume data collection"
    )

    func run() throws {
        let pauseFile = "~/.virtues/paused".expandingTildeInPath

        // Remove pause flag file
        if FileManager.default.fileExists(atPath: pauseFile) {
            try FileManager.default.removeItem(atPath: pauseFile)
            print("\u{25B6}  Collection resumed")
        } else {
            print("Collection was not paused")
        }
    }
}
