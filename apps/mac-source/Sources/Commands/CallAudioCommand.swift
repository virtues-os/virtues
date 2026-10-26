import ArgumentParser
import Foundation

/// Runs only the call audio monitor, in the foreground, for testing call
/// detection and recording without a second copy of the whole collector
/// uploading alongside the installed one.
struct CallAudioCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "call-audio",
        abstract: "Run only call recording in the foreground (for testing)"
    )

    @Option(help: "Where to write recordings (default ~/.virtues/call-audio)")
    var output: String?

    @Option(help: "Stop after this many seconds")
    var seconds: Double?

    func run() throws {
        setvbuf(stdout, nil, _IOLBF, 0)
        guard #available(macOS 14.4, *) else {
            print("Call recording needs macOS 14.4 or later.")
            throw ExitCode.failure
        }
        print("System Audio Recording: \(describe(CallAudioPermission.systemAudio))")
        print("Microphone:             \(describe(CallAudioPermission.microphone))")
        CallAudioPermission.requestIfNeeded()

        let directory = output.map { URL(fileURLWithPath: $0.expandingTildeInPath) } ?? CallAudioMonitor.directory
        let monitor = CallAudioMonitor(directory: directory)
        monitor.start()
        if let seconds {
            RunLoop.main.run(until: Date().addingTimeInterval(seconds))
            monitor.stop()
        } else {
            RunLoop.main.run()
        }
    }

    private func describe(_ granted: Bool?) -> String {
        switch granted {
        case true?: return "granted"
        case false?: return "DENIED"
        case nil: return "not asked yet"
        }
    }
}
