import ArgumentParser
import CoreWLAN
import Foundation

// Global references for signal handlers
private var globalMonitor: Monitor?
private var globalUploader: Uploader?
private var globalMessageMonitor: MessageMonitor?
private var globalBrowserMonitor: BrowserMonitor?
private var globalPresenceMonitor: PresenceMonitor?

struct StartCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "start",
        abstract: "Start monitoring in foreground"
    )
    
    @Flag(name: .shortAndLong, help: "Show debug output")
    var verbose = false
    
    func run() throws {
        // Line-buffer stdout. Under launchd our stdout is a FILE, not a tty, so libc
        // block-buffers it (4 KB) — which means the log lags reality by however long
        // it takes to fill a buffer. Every diagnosis in this daemon starts by reading
        // that log, and a log that reports the past is worse than no log: it produced
        // several confident misdiagnoses (waiting on lines that had already scrolled,
        // "nothing is happening" when plenty was). Cheap to make it honest.
        setvbuf(stdout, nil, _IOLBF, 0)

        // Load config
        guard let config = Config.load() else {
            throw ConfigError.notConfigured
        }
        
        print("Starting Virtues Mac Monitor...")
        print("Device ID: \(config.deviceId)")
        print("API Endpoint: \(config.apiEndpoint)")
        print("Press Ctrl+C to stop\n")

        // Touch the Messages DB with a real read-open right at startup. A denied
        // open is how macOS enrolls this binary in System Settings → Full Disk
        // Access, so the user gets a "virtues-collector" row to toggle on even
        // before anything is granted. (A stat would not trip TCC.)
        _ = MessageMonitor.canReadMessagesDB()

        // Realize CoreWLAN's ObjC classes BEFORE iroh starts. iroh's macOS network
        // monitor looks up `CWWiFiClient` by name and PANICS if the ObjC runtime
        // hasn't realized it ("class CWWiFiClient could not be found") — which kills
        // every upload. Linking the framework (`-framework CoreWLAN`) is necessary
        // but NOT sufficient on its own; a direct Swift reference is what guarantees
        // the class is actually registered. Verified: NSClassFromString("CWWiFiClient")
        // is nil without this and resolves with it.
        _ = CWWiFiClient.shared()

        // Wire the box reach ticket + iroh seed into the transport so uploads
        // dial the box over iroh (authenticated by this device's key).
        let semaphore = DispatchSemaphore(value: 0)
        Task { await config.activateTransport(); semaphore.signal() }
        semaphore.wait()

        // Initialize components
        let queue = try Queue()
        let monitor = Monitor(queue: queue)
        // Messages. This was BUILT but never wired up — StartCommand only called the
        // static `canReadMessagesDB()` (which exists to trip TCC so the binary shows
        // up in System Settings), so the message collector never actually ran and
        // `data_communication_message` stayed empty no matter what the user granted.
        let messageMonitor = MessageMonitor(queue: queue)
        let browserMonitor = BrowserMonitor(queue: queue)
        // Presence: without this, walking away with an app focused is
        // indistinguishable from using it.
        let presenceMonitor = PresenceMonitor(queue: queue)
        let uploader = Uploader(queue: queue, config: config)

        // Store globally for signal handlers
        globalMonitor = monitor
        globalMessageMonitor = messageMonitor
        globalBrowserMonitor = browserMonitor
        globalPresenceMonitor = presenceMonitor
        globalUploader = uploader

        // Start monitoring and uploading
        monitor.start()
        messageMonitor.start()
        browserMonitor.start()
        presenceMonitor.start()
        uploader.start()
        
        // Set up signal handlers for graceful shutdown
        signal(SIGINT) { _ in
            print("\nShutting down...")
            globalMonitor?.stop()
            globalMessageMonitor?.stop()
            globalBrowserMonitor?.stop()
            globalPresenceMonitor?.stop()
            globalUploader?.stop()
            Foundation.exit(0)
        }
        
        signal(SIGTERM) { _ in
            print("\nShutting down...")
            globalMonitor?.stop()
            globalMessageMonitor?.stop()
            globalBrowserMonitor?.stop()
            globalPresenceMonitor?.stop()
            globalUploader?.stop()
            Foundation.exit(0)
        }
        
        // Run forever
        RunLoop.main.run()
    }
}