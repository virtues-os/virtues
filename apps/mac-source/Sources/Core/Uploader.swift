import Foundation

class Uploader {
    private let queue: Queue
    private let config: Config
    private var timer: DispatchSourceTimer?

    /// Resolved `mac_ingest` webhook action id. Normally comes from
    /// `config.actionIds`; refetched into here when that one is dead.
    ///
    /// Takes precedence over `config.actionIds` once set. It has to: the
    /// config is a `let` read from disk at pair time, so if the id there ever
    /// stops existing box-side there is no other way to move off it.
    private var cachedActionId: String?

    /// Set when the box answers 404 for the id we used, which means that id is
    /// gone — the applet was deleted, re-created, or renamed. Distinct from the
    /// transient failures the backoff is for: retrying a 404 with the same id
    /// never succeeds, however long you wait. Cleared once a refetch produces
    /// a new one.
    private var actionIdIsStale = false

    // Thread-safe state management
    private let stateQueue = DispatchQueue(label: "com.virtues.uploader.state")
    private var _retryDelay: TimeInterval = 60 // Start with 1 minute
    private let maxRetryDelay: TimeInterval = 960 // Max 16 minutes
    private var _consecutive401Errors = 0
    private let max401Errors = 3 // Pause after 3 consecutive 401s (~15 minutes)
    private var _isAuthPaused = false
    private var _authPauseUntil: Date?
    private var _authPauseDuration: TimeInterval = 3600 // Start with 1 hour

    private var retryDelay: TimeInterval {
        get { stateQueue.sync { _retryDelay } }
        set { stateQueue.sync(flags: .barrier) { _retryDelay = newValue } }
    }

    private var consecutive401Errors: Int {
        get { stateQueue.sync { _consecutive401Errors } }
        set { stateQueue.sync(flags: .barrier) { _consecutive401Errors = newValue } }
    }

    private var isAuthPaused: Bool {
        get { stateQueue.sync { _isAuthPaused } }
        set { stateQueue.sync(flags: .barrier) { _isAuthPaused = newValue } }
    }

    private var authPauseUntil: Date? {
        get { stateQueue.sync { _authPauseUntil } }
        set { stateQueue.sync(flags: .barrier) { _authPauseUntil = newValue } }
    }

    private var authPauseDuration: TimeInterval {
        get { stateQueue.sync { _authPauseDuration } }
        set { stateQueue.sync(flags: .barrier) { _authPauseDuration = newValue } }
    }

    /// Callback invoked after successful upload
    var onUploadComplete: (() -> Void)?

    /// Callback invoked when auth fails repeatedly
    var onAuthFailure: (() -> Void)?

    init(queue: Queue, config: Config) {
        self.queue = queue
        self.config = config
    }

    func start() {
        print("Starting uploader (5-minute intervals)...")

        // Upload immediately on start
        Task {
            await upload()
        }

        // Create dispatch timer for reliable firing in menu bar app
        // This works even when menus are open, unlike NSTimer/RunLoop
        let newTimer = DispatchSource.makeTimerSource(queue: .main)
        newTimer.schedule(deadline: .now() + 300, repeating: 300) // 5 minutes
        newTimer.setEventHandler { [weak self] in
            guard let self = self else { return }
            Task {
                await self.upload()
            }
        }
        newTimer.resume()
        self.timer = newTimer
    }

    func stop() {
        timer?.cancel()
        timer = nil
        print("Uploader stopped")
    }

    func uploadNow() async -> (uploaded: Int, failed: Int) {
        return await upload()
    }

    @discardableResult
    private func upload() async -> (uploaded: Int, failed: Int) {
        // Check if uploads are paused due to auth failures
        if isAuthPaused {
            if let pauseUntil = authPauseUntil, Date() < pauseUntil {
                let timeRemaining = Int(pauseUntil.timeIntervalSinceNow / 60)
                print("⏸️ Uploads paused due to auth failure (resuming in \(timeRemaining) minutes)")
                return (0, 0)
            } else {
                // Pause expired, try again
                print("🔄 Auth pause expired, resuming uploads...")
                isAuthPaused = false
                authPauseUntil = nil
                consecutive401Errors = 0
            }
        }

        var totalUploaded = 0
        var totalFailed = 0

        // One combined batch → the single `mac_ingest` webhook action.
        let result = await uploadBatch()
        totalUploaded += result.uploaded
        totalFailed += result.failed

        // Log summary
        if totalUploaded > 0 || totalFailed > 0 {
            print("📤 Upload summary: \(totalUploaded) successful, \(totalFailed) failed")
        }

        // Call completion callback if any records were uploaded
        if totalUploaded > 0 {
            onUploadComplete?()
        }

        return (totalUploaded, totalFailed)
    }

    /// Upload all pending app events + iMessages as ONE batch to the
    /// `mac_ingest` webhook action, authenticated with the device bearer.
    /// The action expects a flat payload with top-level `app_events` /
    /// `browser_history` / `imessages` arrays (see `actions/mac_ingest`).
    private func uploadBatch() async -> (uploaded: Int, failed: Int) {
        do {
            let eventsWithIds = try queue.getPendingEvents()
            let messagesWithIds = try queue.getPendingMessages()
            let visitsWithIds = try queue.getPendingBrowserVisits()
            let snapshotsWithIds = try queue.getPendingBookmarkSnapshots()
            let pending =
                eventsWithIds.count + messagesWithIds.count + visitsWithIds.count
                + snapshotsWithIds.count
            if pending == 0 {
                return (0, 0)
            }

            // Resolve the webhook target (from pair, else a one-shot refetch).
            guard let actionId = await macActivityActionId() else {
                print("⚠️ No 'mac_ingest' action id — re-pair this collector " +
                      "(`virtues-collector init <token>`). Skipping upload.")
                return (0, pending)
            }

            print(
                "Uploading \(eventsWithIds.count) app events + \(visitsWithIds.count) visits "
                    + "+ \(messagesWithIds.count) messages…")

            // Map records to the action's field contract. app_events already
            // match (`timestamp`, `bundle_id`, `app_name`); iMessages need
            // `message_id`→`guid` and `handle_id`→`from_handle`. Browser visits
            // already match (`url`, `title`, `timestamp`, `browser`).
            let appEvents = eventsWithIds.map { $0.event.toDictionary }
            let imessages = messagesWithIds.map { mapMessageForWebhook($0.message.toDictionary) }
            let browserHistory = visitsWithIds.map { $0.visit.toDictionary() }
            // Per-browser FULL snapshots (absence is the delete signal on the
            // box, so a snapshot must never be sent partially). Rarely present —
            // the monitor only queues one when a bookmark file's hash moved.
            // Track WHICH ids made it into the payload: a row that fails to
            // re-parse is dropped here, and marking it uploaded anyway would
            // orphan it as sent-but-never-sent.
            var sentSnapshotIds: [Int64] = []
            var bookmarkSnapshots: [[String: Any]] = []
            for snap in snapshotsWithIds {
                guard let data = snap.recordsJSON.data(using: .utf8),
                    let records = try? JSONSerialization.jsonObject(with: data)
                else {
                    print("⚠️ bookmark snapshot[\(snap.browser)] failed to re-parse — skipping")
                    continue
                }
                bookmarkSnapshots.append(["browser": snap.browser, "records": records])
                sentSnapshotIds.append(snap.id)
            }
            var payload: [String: Any] = [
                // Sessions are stateful on the box: it holds an app's session open
                // across batches and closes it when the matching unfocus arrives. So
                // it has to know WHOSE session — two Macs (a laptop and a desktop)
                // would otherwise close each other's and produce nonsense. The action
                // receives no device identity of its own, so we send it.
                "device_id": config.deviceId,
                "app_events": appEvents,
                "browser_history": browserHistory,
                "imessages": imessages,
                // Why the box needs this: an upload carrying zero messages
                // because the user has no new messages and one carrying zero
                // because macOS is denying us `chat.db` are otherwise
                // byte-identical. Without this field the box can only notice
                // the silence, days later, and never learn the reason — which
                // is exactly how a four-day iMessage outage looked "healthy".
                "collector_health": collectorHealthPayload(),
            ]
            if !bookmarkSnapshots.isEmpty {
                payload["bookmarks"] = bookmarkSnapshots
            }

            // Host is ignored over iroh (the box is dialed by EndpointId); only
            // the path matters. Auth is this device's allowlisted key — no bearer.
            guard let url = URL(string: "\(config.apiEndpoint)/webhook/\(actionId)") else {
                print("Invalid API endpoint")
                return (0, pending)
            }

            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: payload)

            let (data, httpResponse) = try await BoxTransport.shared.send(request)

            if httpResponse.statusCode == 200 {
                try queue.markEventsAsUploaded(eventsWithIds.map { $0.id })
                try queue.markMessagesAsUploaded(messagesWithIds.map { $0.id })
                try queue.markBrowserVisitsUploaded(ids: visitsWithIds.map { $0.id })
                try queue.markBookmarkSnapshotsUploaded(ids: sentSnapshotIds)
                try queue.cleanupOldEvents()
                try queue.cleanupOldMessages()
                try queue.cleanupOldBrowserVisits()
                try queue.cleanupOldBookmarkSnapshots()

                print(
                    "✓ Uploaded \(eventsWithIds.count) events + \(visitsWithIds.count) visits "
                        + "+ \(messagesWithIds.count) messages")
                retryDelay = 60
                consecutive401Errors = 0
                if isAuthPaused {
                    print("🔄 Auth successful - resuming normal uploads")
                    isAuthPaused = false
                    authPauseUntil = nil
                    authPauseDuration = 3600
                }
                return (pending, 0)
            } else if httpResponse.statusCode == 401 {
                handle401(data)
                return (0, pending)
            } else if httpResponse.statusCode == 404 {
                // The box does not know this applet id. That is terminal for
                // the id, not for the upload: nothing is dropped, the records
                // stay pending, and the next cycle resolves a fresh id rather
                // than posting to the same dead one. Backoff is deliberately
                // NOT escalated — the retry will differ, so waiting longer
                // buys nothing.
                print(
                    "⚠️ Webhook 404 for action id \(actionId) — it no longer exists on the box. "
                        + "Refetching on the next cycle; \(pending) records held.")
                actionIdIsStale = true
                cachedActionId = nil
                consecutive401Errors = 0
                return (0, pending)
            } else {
                print("Upload failed with status: \(httpResponse.statusCode)")
                if let body = String(data: data, encoding: .utf8) {
                    print("Response: \(body)")
                }
                consecutive401Errors = 0
                retryDelay = min(retryDelay * 2, maxRetryDelay)
                return (0, pending)
            }
        } catch {
            print("Upload error: \(error)")
            retryDelay = min(retryDelay * 2, maxRetryDelay)
            return (0, 0)
        }
    }

    /// Shared 401 handling — count, and pause uploads with exponential backoff
    /// after `max401Errors` consecutive failures (device likely unpaired).
    private func handle401(_ data: Data) {
        print("❌ Upload failed: Authentication error (401)")
        if let body = String(data: data, encoding: .utf8) {
            print("   Response: \(body)")
        }
        consecutive401Errors += 1
        print("   Consecutive 401 errors: \(consecutive401Errors)/\(max401Errors)")

        if consecutive401Errors >= max401Errors {
            let pauseMinutes = Int(authPauseDuration / 60)
            print("❌ CRITICAL: Auth failed \(consecutive401Errors) times - pausing uploads for \(pauseMinutes) minutes")
            print("   Your device may have been unpaired or the source deleted")
            print("   Re-pair this device to resume immediately")

            authPauseUntil = Date().addingTimeInterval(authPauseDuration)
            isAuthPaused = true
            authPauseDuration = min(authPauseDuration * 2, 24 * 3600) // Max 24 hours
            onAuthFailure?()
        }
    }

    /// The `mac_ingest` action id: a refetched one if we have it, else the
    /// paired config, else a one-shot refetch.
    ///
    /// The refetch used to be unreachable whenever the config held any value —
    /// and the config is a `let` loaded at pair time, so a collector whose id
    /// had gone away box-side would post to it forever, take a 404, back off,
    /// and retry the same dead id until someone re-paired. Nothing surfaced but
    /// a `print`. So the order is deliberate: a value we fetched from the box
    /// beats a value we read off disk, and `actionIdIsStale` forces a refetch
    /// even when the config still has something to offer.
    private func macActivityActionId() async -> String? {
        if !actionIdIsStale, let id = cachedActionId {
            return id
        }
        if !actionIdIsStale, let id = config.actionIds["mac_ingest"] {
            return id
        }
        if let id = await refetchMacActivityActionId() {
            cachedActionId = id
            actionIdIsStale = false
            return id
        }
        // The refetch failed (box unreachable, or it genuinely has no
        // mac_ingest applet). Keep the stale flag set so the next cycle tries
        // again rather than falling back to the id we already know is dead.
        return nil
    }

    private func refetchMacActivityActionId() async -> String? {
        guard let url = URL(string: "\(config.apiEndpoint)/api/devices/action-ids") else {
            return nil
        }
        // Over iroh (BoxTransport) — authenticated by this device's proven key.
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        do {
            let (data, http) = try await BoxTransport.shared.send(request)
            guard http.statusCode == 200,
                  let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let ids = json["action_ids"] as? [String: String] else {
                return nil
            }
            return ids["mac_ingest"]
        } catch {
            return nil
        }
    }

    /// Rename the iMessage record keys to the `mac_ingest` action's contract
    /// (`message_id`→`guid`, `handle_id`→`from_handle`). The action drops any
    /// message with an empty `guid`, so the rename is required.
    private func mapMessageForWebhook(_ dict: [String: Any]) -> [String: Any] {
        var out = dict
        if let mid = out["message_id"] {
            out["guid"] = mid
        }
        if let handle = out["handle_id"] {
            out["from_handle"] = handle
        }
        return out
    }

    /// The daemon's last self-reported capabilities, for the box.
    ///
    /// Read from the record rather than probed here so that what the box is
    /// told is the same thing `virtues-collector status` shows — one source of
    /// truth, no chance of the two disagreeing. Omitted entirely when no record
    /// exists, so the box can tell "denied" apart from "this collector is too
    /// old to say".
    private func collectorHealthPayload() -> [String: Any] {
        guard let health = CollectorHealth.load() else { return [:] }
        return [
            "full_disk_access": health.fullDiskAccess,
            "accessibility": health.accessibility,
            "denied": health.deniedCapabilities,
            "checked_at": ISO8601DateFormatter().string(from: health.updatedAt),
            "stale": health.isStale,
        ]
    }
}
