import Foundation
import AppKit

class Monitor {
    private let queue: Queue
    private var frontmostApp: NSRunningApplication?
    private var timer: DispatchSourceTimer?
    private var heartbeat: DispatchSourceTimer?

    /// How stale an interrupted session can be. One minute of slop is nothing
    /// against a session that would otherwise be lost entirely.
    private let heartbeatInterval: TimeInterval = 60

    init(queue: Queue) {
        self.queue = queue
    }
    
    func start() {
        print("Starting activity monitor...")
        
        // Track current frontmost app
        frontmostApp = NSWorkspace.shared.frontmostApplication
        if let app = frontmostApp {
            recordEvent(app: app, eventType: Event.EventType.focus)
        }
        
        // Set up workspace notifications
        let workspace = NSWorkspace.shared
        let center = workspace.notificationCenter
        
        center.addObserver(
            self,
            selector: #selector(appLaunched(_:)),
            name: NSWorkspace.didLaunchApplicationNotification,
            object: nil
        )
        
        center.addObserver(
            self,
            selector: #selector(appTerminated(_:)),
            name: NSWorkspace.didTerminateApplicationNotification,
            object: nil
        )
        
        center.addObserver(
            self,
            selector: #selector(appActivated(_:)),
            name: NSWorkspace.didActivateApplicationNotification,
            object: nil
        )
        
        // Poll for frontmost app changes (backup for notifications)
        // Use DispatchSourceTimer for reliability in menu bar apps
        let pollTimer = DispatchSource.makeTimerSource(queue: .main)
        pollTimer.schedule(deadline: .now() + 1.0, repeating: 1.0)
        pollTimer.setEventHandler { [weak self] in
            self?.checkFrontmostApp()
        }
        pollTimer.resume()
        self.timer = pollTimer

        // Heartbeat: "the focused app is STILL focused".
        //
        // Sessions are opened by a focus event and closed by the matching unfocus.
        // If this process dies in between — a crash, a power cut, or simply an
        // update swapping the binary — that unfocus never arrives, and the box can
        // only clamp the orphaned session back to the last thing it heard about:
        // its own start. Duration zero. Dropped. That is *precisely* the bug this
        // rewrite exists to fix (a real 40-minute session recording nothing),
        // reintroduced by a different route.
        //
        // A heartbeat is the last thing the box heard, so an interrupted session is
        // clamped to within one interval of the truth instead of to nothing.
        let beat = DispatchSource.makeTimerSource(queue: .main)
        beat.schedule(deadline: .now() + heartbeatInterval, repeating: heartbeatInterval)
        beat.setEventHandler { [weak self] in
            guard let self, let app = NSWorkspace.shared.frontmostApplication else { return }
            self.recordEvent(app: app, eventType: Event.EventType.heartbeat)
        }
        beat.resume()
        self.heartbeat = beat

        print("Activity monitor started")
    }

    func stop() {
        timer?.cancel()
        timer = nil
        heartbeat?.cancel()
        heartbeat = nil
        NSWorkspace.shared.notificationCenter.removeObserver(self)
        print("Activity monitor stopped")
    }
    
    @objc private func appLaunched(_ notification: Notification) {
        guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { 
            return 
        }
        recordEvent(app: app, eventType: Event.EventType.launch)
    }
    
    @objc private func appTerminated(_ notification: Notification) {
        guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { 
            return 
        }
        recordEvent(app: app, eventType: Event.EventType.quit)
    }
    
    @objc private func appActivated(_ notification: Notification) {
        guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { 
            return 
        }
        
        // Record unfocus for previous app
        if let previousApp = frontmostApp, previousApp != app {
            recordEvent(app: previousApp, eventType: Event.EventType.unfocus)
        }
        
        // Record focus for new app
        recordEvent(app: app, eventType: Event.EventType.focus)
        frontmostApp = app
    }
    
    private func checkFrontmostApp() {
        let currentFrontmost = NSWorkspace.shared.frontmostApplication
        
        if currentFrontmost != frontmostApp {
            // App focus changed
            if let previous = frontmostApp {
                recordEvent(app: previous, eventType: Event.EventType.unfocus)
            }
            
            if let current = currentFrontmost {
                recordEvent(app: current, eventType: Event.EventType.focus)
            }
            
            frontmostApp = currentFrontmost
        }
    }
    
    private func recordEvent(app: NSRunningApplication, eventType: String) {
        // Check pause state - skip recording when paused
        let pausePath = Config.configDir.appendingPathComponent("paused").path
        if FileManager.default.fileExists(atPath: pausePath) {
            return
        }

        let appName = app.localizedName ?? "Unknown"
        let bundleId = app.bundleIdentifier

        // Skip our own app
        if bundleId == "com.virtues.collector" || appName == "virtues-collector" {
            return
        }

        // Capture the focused window title — the difference between "used Chrome
        // for 40 min" and "read <page>". On focus/launch, and on each heartbeat:
        // within one 40-minute Cursor session you touch six files, and the
        // heartbeats are what let the box keep that as a title timeline on ONE
        // session instead of fragmenting it into six.
        //
        // NOT on unfocus/quit: the window is already gone or the app is tearing
        // down, so the read is stale or blocks. nil when Accessibility isn't
        // granted (degrades to app-name-only).
        let windowTitle: String? =
            (eventType == Event.EventType.focus || eventType == Event.EventType.launch
                || eventType == Event.EventType.heartbeat)
            ? WindowTitle.focused(pid: app.processIdentifier)
            : nil

        let event = Event(
            eventType: eventType, appName: appName, bundleId: bundleId, windowTitle: windowTitle)

        // Heartbeats fire every 60s and would drown the log; they're only
        // interesting when they fail.
        let quiet = eventType == Event.EventType.heartbeat

        // Add event asynchronously (non-blocking)
        queue.addEvent(event) { result in
            if quiet {
                if case .failure(let error) = result {
                    print("⚠️ Error recording heartbeat: \(error)")
                }
                return
            }
            switch result {
            case .success:
                print("✓ [\(Date())] \(eventType): \(appName)")
            case .failure(let error):
                print("⚠️ Error recording event: \(error)")
            }
        }
    }
}