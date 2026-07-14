import AppKit
import Foundation
import IOKit.pwr_mgt

/// Records whether the human is actually here.
///
/// Without this, absence is invisible: the collector only emits events when the
/// focused app CHANGES, so walking away with Cursor focused is indistinguishable
/// from forty minutes of concentration. The only absence signal we had was
/// `loginwindow` arriving as if it were an app you used — which is how the box
/// came to believe the user's most-used application was the lock screen, for 211
/// of 429 recorded hours.
///
/// Five states, and the interesting one is `watching`:
///
///     input recently                          → active
///     no input, focused app holds the display → watching   (a video, a call)
///     no input, nothing holding the display   → idle
///     screen locked / screensaver / switched  → locked
///     machine suspended (lid closed)          → suspended
///
/// `watching` counts as usage; `idle` does not. A naive HID-idle check would call
/// a 40-minute lecture "away" and delete it.
///
/// SUSPENDED, not "asleep". A Mac can observe its own lid closing; it cannot
/// observe you sleeping. Human sleep already has a table — data_health_sleep, fed
/// by a watch that can actually measure it — and letting this collector emit
/// "asleep" would turn closing the lid at lunch into a nap in your life story.
/// Everything here is what the MACHINE saw; whether you were present, working, or
/// asleep is an inference for the narrative layer to make by fusing devices, with
/// the evidence still there to overrule it.
final class PresenceMonitor {
    private let queue: Queue
    private var timer: DispatchSourceTimer?

    /// How long without input before we call it idle.
    ///
    /// 10 minutes, not 5. HID-idle assumes attention produces keystrokes, and
    /// reading a long document or thinking about a bug produces none. Under-
    /// counting idle is a far cheaper error than deleting real attention — and the
    /// raw events are archived in the lake, so if this number is wrong we can
    /// re-derive the sessions instead of re-collecting a month of someone's life.
    private let idleThreshold: TimeInterval = 600

    /// Poll cadence. Accuracy does NOT depend on this: idle onset is back-dated
    /// from the system's own "seconds since last input", so a slow poll costs us
    /// latency, never precision.
    private let pollInterval: TimeInterval = 30

    private var isIdle = false
    private var isWatching = false
    private var isLocked = false

    init(queue: Queue) {
        self.queue = queue
    }

    func start() {
        print("Starting presence monitor...")

        let dnc = DistributedNotificationCenter.default()
        dnc.addObserver(
            self, selector: #selector(screenLocked), name: .init("com.apple.screenIsLocked"),
            object: nil)
        dnc.addObserver(
            self, selector: #selector(screenUnlocked), name: .init("com.apple.screenIsUnlocked"),
            object: nil)
        // A screensaver is a lock for our purposes: you are not at the machine.
        dnc.addObserver(
            self, selector: #selector(screenLocked),
            name: .init("com.apple.screensaver.didstart"), object: nil)
        dnc.addObserver(
            self, selector: #selector(screenUnlocked),
            name: .init("com.apple.screensaver.didstop"), object: nil)

        let wnc = NSWorkspace.shared.notificationCenter
        wnc.addObserver(
            self, selector: #selector(willSleep), name: NSWorkspace.willSleepNotification,
            object: nil)
        wnc.addObserver(
            self, selector: #selector(didWake), name: NSWorkspace.didWakeNotification, object: nil)
        // Fast user switching: someone else is at the machine, so you are not.
        wnc.addObserver(
            self, selector: #selector(screenLocked),
            name: NSWorkspace.sessionDidResignActiveNotification, object: nil)
        wnc.addObserver(
            self, selector: #selector(screenUnlocked),
            name: NSWorkspace.sessionDidBecomeActiveNotification, object: nil)

        let t = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
        t.schedule(deadline: .now() + pollInterval, repeating: pollInterval)
        t.setEventHandler { [weak self] in self?.poll() }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
        DistributedNotificationCenter.default().removeObserver(self)
        NSWorkspace.shared.notificationCenter.removeObserver(self)
    }

    // MARK: - Notifications

    @objc private func screenLocked() {
        guard !isLocked else { return }
        isLocked = true
        // Locking supersedes idle/watching — you're simply gone.
        clearIdleAndWatching(at: Date())
        emit(Event.EventType.lock, at: Date())
    }

    @objc private func screenUnlocked() {
        guard isLocked else { return }
        isLocked = false
        emit(Event.EventType.unlock, at: Date())
    }

    /// Delivered BEFORE the machine suspends, and the window is short and
    /// synchronous — enqueue and do nothing else.
    @objc private func willSleep() {
        clearIdleAndWatching(at: Date())
        emit(Event.EventType.suspend, at: Date())
    }

    @objc private func didWake() {
        emit(Event.EventType.resume, at: Date())
    }

    // MARK: - Idle / watching

    private func poll() {
        if isPaused() || isLocked { return }

        let now = Date()
        let idleSeconds = Self.secondsSinceLastInput()

        // BACK-DATE. The system tells us how long input has been absent, so idle
        // began at (now - idleSeconds) — the moment input actually stopped, not the
        // moment this poll happened to notice. Without this, every idle transition
        // silently credits up to one poll interval of absence as work, forever.
        let idleSince = now.addingTimeInterval(-idleSeconds)
        let idleNow = idleSeconds >= idleThreshold

        if idleNow, !isIdle, !isWatching {
            // Is the focused app holding the display awake? Then this isn't absence,
            // it's a video or a call.
            if focusedAppIsHoldingDisplayAwake() {
                isWatching = true
                emit(Event.EventType.watchStart, at: idleSince)
            } else {
                isIdle = true
                emit(Event.EventType.idleStart, at: idleSince)
            }
            return
        }

        // A watcher that stopped holding the display (video ended) but still isn't
        // touching anything has become genuinely idle.
        if isWatching, idleNow, !focusedAppIsHoldingDisplayAwake() {
            isWatching = false
            emit(Event.EventType.watchEnd, at: now)
            isIdle = true
            emit(Event.EventType.idleStart, at: now)
            return
        }

        // Input again — whichever state we were in has ended.
        if !idleNow {
            clearIdleAndWatching(at: now)
        }
    }

    private func clearIdleAndWatching(at date: Date) {
        if isIdle {
            isIdle = false
            emit(Event.EventType.idleEnd, at: date)
        }
        if isWatching {
            isWatching = false
            emit(Event.EventType.watchEnd, at: date)
        }
    }

    /// Seconds since the last human input of any kind.
    private static func secondsSinceLastInput() -> TimeInterval {
        // kCGAnyInputEventType — any HID event, not just one kind.
        let anyInput = CGEventType(rawValue: ~0)!
        return CGEventSource.secondsSinceLastEventType(.hidSystemState, eventType: anyInput)
    }

    /// Is the FOCUSED app preventing display sleep?
    ///
    /// Scoping to the focused app's own pid is what keeps `watching` honest. Plenty
    /// of things hold a display-sleep assertion — a long build, screen sharing,
    /// `caffeinate`, a background download — and crediting any of them would mean
    /// walking away from a compiling Cursor counted as forty minutes of attention.
    /// Only the app you are actually looking at can earn `watching`.
    private func focusedAppIsHoldingDisplayAwake() -> Bool {
        guard let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier else {
            return false
        }

        var assertions: Unmanaged<CFDictionary>?
        guard IOPMCopyAssertionsByProcess(&assertions) == kIOReturnSuccess,
            let byProcess = assertions?.takeRetainedValue() as? [NSNumber: [[String: Any]]]
        else {
            return false
        }

        guard let mine = byProcess[NSNumber(value: pid)] else { return false }
        return mine.contains { assertion in
            let type = assertion[kIOPMAssertionTypeKey as String] as? String
            return type == kIOPMAssertionTypePreventUserIdleDisplaySleep as String
                || type == kIOPMAssertionTypeNoDisplaySleep as String
        }
    }

    // MARK: - Plumbing

    private func isPaused() -> Bool {
        FileManager.default.fileExists(
            atPath: Config.configDir.appendingPathComponent("paused").path)
    }

    /// Presence events carry the app that was focused when they happened — the box
    /// needs to know whose session to close.
    private func emit(_ type: String, at date: Date) {
        if isPaused() { return }

        let app = NSWorkspace.shared.frontmostApplication
        let event = Event(
            timestamp: date,
            eventType: type,
            appName: app?.localizedName ?? "system",
            bundleId: app?.bundleIdentifier)

        queue.addEvent(event) { result in
            if case .failure(let error) = result {
                print("⚠️ presence: failed to record \(type): \(error)")
            } else {
                print("· presence: \(type) @ \(ISO8601DateFormatter().string(from: date))")
            }
        }
    }
}
