import ArgumentParser
import Foundation

struct InitCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "init",
        abstract: "Initialize with a device token from the web UI"
    )
    
    @Argument(help: "Device token from the Virtues web UI")
    var token: String
    
    func run() throws {
        print("Validating device token...")
        
        // Create a class to hold mutable state safely
        class ResultHolder {
            var config: Config?
            var error: Error?
        }
        
        let holder = ResultHolder()
        let group = DispatchGroup()
        
        group.enter()
        Task {
            do {
                // Consume the one-time pair token via the unified pair flow.
                // The token is case-sensitive — do NOT uppercase it.
                let pair = try await Config.pairConsume(token: token)

                print("✓ Paired (auth: iroh key — no bearer)")
                print("✓ Box reach: \(pair.boxNodeId) via \(pair.relayUrl)")

                // Create and save config — the iroh seed is the credential; the
                // reach ticket (boxNodeId/relayUrl) dials the box for uploads.
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

                if config.appletIds["mac_ingest"] == nil {
                    print("⚠️ Warning: the box returned no 'mac_ingest' action — uploads")
                    print("   won't work until the box is updated. Re-run `init` after upgrading.")
                }

                print("✓ Configuration saved to ~/.virtues/config.json")
                print("\nReady to start monitoring!")
                print("Run 'virtues-collector start' to begin monitoring")
                print("Run 'virtues-collector install' to install as background service")

                holder.config = config
            } catch {
                holder.error = error
            }
            group.leave()
        }
        
        group.wait()
        
        if let error = holder.error {
            throw error
        } else if holder.config != nil {
            return
        } else {
            throw ConfigError.networkError("Unknown error")
        }
    }
}