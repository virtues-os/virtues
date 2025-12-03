import ArgumentParser
import Foundation

/// CLI entry point for Virtues Mac
/// Note: No @main here - AppDelegate routes to this when CLI args detected
struct VirtuesMac: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "virtues-mac",
        abstract: "Virtues Mac activity monitor CLI",
        version: Version.full,
        subcommands: [
            InitCommand.self,
            StartCommand.self,
            DaemonCommand.self,
            StatusCommand.self,
            ResetCommand.self,
            StopCommand.self
        ],
        defaultSubcommand: StatusCommand.self
    )
}