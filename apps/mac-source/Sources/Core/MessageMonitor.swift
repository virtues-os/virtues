import Foundation
import SQLite3

class MessageMonitor {
    private let queue: Queue
    private var lastSyncDate: Date?
    private let dbPath = NSString(string: "~/Library/Messages/chat.db").expandingTildeInPath
    private var timer: DispatchSourceTimer?
    private let syncInterval: TimeInterval = 300 // 5 minutes
    
    // Configuration
    //
    // Backfill ALL of it. chat.db holds years — often the single richest record a
    // person has of their own relationships — and we were taking the last week, which
    // meant the most valuable stream in the system had an 8-day memory. There is no
    // reason for the floor: the messages are already on this disk, we are not
    // re-fetching them from anyone, and the box dedups on GUID so a re-read is free.
    //
    // It grinds rather than gulps: `batchSize` per 5-minute tick, cursor advancing, so
    // a decade of history lands over a day or so of background sync without ever
    // blocking a live message. Bumped to 1000 so the catch-up isn't glacial.
    private let initialSyncDays = 365 * 20
    private let batchSize = 1000
    
    // Full Disk Access detection
    private var hasFullDiskAccess = false
    private var lastPermissionCheck = Date.distantPast
    private let permissionCheckInterval: TimeInterval = 300 // Check every 5 minutes
    private var permissionCheckAttempts = 0
    
    init(queue: Queue) {
        self.queue = queue
        loadLastSyncDate()
        // Check initial permission state, and publish it: this process is the
        // daemon, so its probe is the only one that describes the daemon. See
        // CollectorHealth.
        hasFullDiskAccess = canAccessMessagesDB()
        CollectorHealth.recordFromDaemon()
    }
    
    func start() {
        print("Starting message monitor...")
        print("  Database path: \(dbPath)")
        print("  Has Full Disk Access: \(hasFullDiskAccess)")
        if let lastSync = lastSyncDate {
            print("  Last sync date: \(ISO8601DateFormatter().string(from: lastSync))")
        } else {
            print("  Last sync date: nil (will perform initial \(initialSyncDays)-day sync)")
        }

        // Perform initial sync asynchronously to avoid blocking caller
        DispatchQueue.global(qos: .background).async { [weak self] in
            self?.syncMessages()
        }

        // Set up periodic sync using DispatchSourceTimer (more reliable than Timer for background execution)
        let syncTimer = DispatchSource.makeTimerSource(queue: .global(qos: .background))
        syncTimer.schedule(deadline: .now() + syncInterval, repeating: syncInterval)
        syncTimer.setEventHandler { [weak self] in
            self?.syncMessages()
        }
        syncTimer.resume()
        self.timer = syncTimer

        print("Message monitor started (syncing every \(Int(syncInterval)) seconds)")
    }
    
    func stop() {
        timer?.cancel()
        timer = nil
        saveLastSyncDate()
        print("Message monitor stopped")
    }
    
    private func syncMessages() {
        // Republish permissions FIRST, on every tick, before any early return.
        //
        // This used to live inside the `!hasFullDiskAccess` branch below, which
        // made the record fresh only while something was broken: the moment a
        // user granted the permission, the branch became unreachable and the
        // record froze at its last value forever. A permission REVOKED after
        // startup was then never noticed — the box kept being told "granted"
        // indefinitely — and `isStale` flipped true 15 minutes into normal,
        // healthy operation, which is what taught readers to ignore it.
        //
        // Before the pause check too: pausing collection is a statement about
        // data, not about permissions, and a paused Mac still owes the box an
        // honest answer about what it can read.
        CollectorHealth.recordFromDaemon()

        // Check pause state - skip syncing when paused
        let pausePath = Config.configDir.appendingPathComponent("paused").path
        if FileManager.default.fileExists(atPath: pausePath) {
            return
        }

        // Check for Full Disk Access if we don't have it yet
        if !hasFullDiskAccess {
            let now = Date()
            if now.timeIntervalSince(lastPermissionCheck) >= permissionCheckInterval {
                lastPermissionCheck = now
                permissionCheckAttempts += 1

                if canAccessMessagesDB() {
                    print("✅ Full Disk Access detected! Starting iMessage sync...")
                    hasFullDiskAccess = true
                    // Reset attempts counter
                    permissionCheckAttempts = 0
                    // Fall through to perform sync
                } else {
                    if permissionCheckAttempts == 1 {
                        print("⚠️ Cannot read Messages database - Full Disk Access required")
                        print("   To enable: System Settings → Privacy & Security → Full Disk Access → turn on virtues-collector (or click + and add ~/.virtues/bin/virtues-collector)")
                        print("   Virtues will automatically detect when permission is granted (checking every 5 minutes)")
                    } else if permissionCheckAttempts % 12 == 0 { // Log every hour
                        print("⏳ Still waiting for Full Disk Access (checked \(permissionCheckAttempts) times)")
                    }
                    return
                }
            } else {
                // Not time to check yet, skip this sync cycle
                return
            }
        }
        
        print("Syncing messages...")
        
        guard FileManager.default.fileExists(atPath: dbPath) else {
            print("⚠️ Messages database not found at: \(dbPath)")
            return
        }
        
        var db: OpaquePointer?
        defer {
            if db != nil {
                sqlite3_close(db)
            }
        }
        
        // Open database in read-only mode
        if sqlite3_open_v2(dbPath, &db, SQLITE_OPEN_READONLY, nil) != SQLITE_OK {
            let error = String(cString: sqlite3_errmsg(db))
            // If we lose access, reset the flag
            if error.contains("authorization denied") || error.contains("Operation not permitted") {
                hasFullDiskAccess = false
                print("⚠️ Lost Full Disk Access - will check again in 5 minutes")
            } else {
                print("⚠️ Unable to open messages database: \(error)")
            }
            return
        }
        
        print("✓ Opened Messages database successfully")
        
        // Determine sync window
        let syncFromDate: Date
        if let lastSync = lastSyncDate {
            // Incremental sync: from last sync date
            syncFromDate = lastSync
            print("Incremental sync from: \(ISO8601DateFormatter().string(from: syncFromDate))")
        } else {
            // Initial sync: last N days
            syncFromDate = Calendar.current.date(byAdding: .day, value: -initialSyncDays, to: Date()) ?? Date()
            print("Initial sync from: \(ISO8601DateFormatter().string(from: syncFromDate))")
        }
        
        // Convert to Core Data timestamp
        let coreDataTimestamp = dateToCoreDateTimestamp(syncFromDate)
        
        // Safety check: ensure timestamp is reasonable (not in far future)
        // Messages timestamps should be between 2001 and current time
        let maxTimestamp = dateToCoreDateTimestamp(Date()) + (365 * 24 * 60 * 60 * 1_000_000_000) // 1 year in future max
        guard coreDataTimestamp < maxTimestamp else {
            print("⚠️ Invalid sync timestamp detected. Resetting to 7 days ago.")
            lastSyncDate = nil
            syncMessages() // Retry with fresh sync
            return
        }
        
        // Query for messages
        let query = """
            SELECT
                m.guid as message_id,
                c.guid as chat_id,
                -- `m.handle_id` is a ROWID into `handle`, not an address. Selecting it
                -- raw shipped every message with from_identifier="173" instead of a
                -- phone number or email, so resolve it here.
                h.id as handle,
                m.text,
                m.attributedBody,
                m.service,
                m.is_from_me,
                m.date,
                m.date_read,
                m.date_delivered,
                m.is_read,
                m.is_delivered,
                m.is_sent,
                m.cache_has_attachments,
                c.display_name as group_title,
                m.associated_message_guid,
                m.associated_message_type,
                m.expressive_send_style_id,
                (SELECT COUNT(*) FROM message_attachment_join WHERE message_id = m.ROWID) as attachment_count,
                -- Needed to join attachments on afterwards. Local to chat.db; never uploaded.
                m.ROWID as row_id
            FROM message m
            LEFT JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
            LEFT JOIN chat c ON cmj.chat_id = c.ROWID
            LEFT JOIN handle h ON m.handle_id = h.ROWID
            WHERE m.date > ?
            ORDER BY m.date ASC
            LIMIT ?
        """
        
        var statement: OpaquePointer?
        defer {
            if statement != nil {
                sqlite3_finalize(statement)
            }
        }
        
        if sqlite3_prepare_v2(db, query, -1, &statement, nil) != SQLITE_OK {
            print("Failed to prepare query: \(String(cString: sqlite3_errmsg(db)))")
            return
        }
        
        // Bind parameters
        sqlite3_bind_int64(statement, 1, Int64(coreDataTimestamp))
        sqlite3_bind_int(statement, 2, Int32(batchSize))
        
        var messages: [Message] = []
        var rowIds: [Int64] = []   // parallel to `messages`; chat.db-local, never uploaded
        var latestMessageDate: Date?

        // Execute query and collect results
        var rowCount = 0
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let stmt = statement else { continue }

            rowCount += 1
            if rowCount % 100 == 0 {
                print("Processing message \(rowCount)...")
            }

            let message = parseMessageRow(statement: stmt)
            messages.append(message)
            rowIds.append(sqlite3_column_int64(stmt, 19))

            // Track the latest message date for next sync (only if valid)
            let calendar = Calendar.current
            let year = calendar.component(.year, from: message.date)
            if year >= 2000 && year <= 2100 {
                if latestMessageDate == nil || message.date > latestMessageDate! {
                    latestMessageDate = message.date
                }
            } else {
                print("⚠️ Skipping message with invalid date: \(message.date) (year: \(year))")
            }
        }
        
        if messages.isEmpty {
            print("No new messages to sync")
        } else {
            print("Found \(messages.count) messages to sync")

            // Enrich with attachment metadata. A message whose only content is a photo
            // arrives as a single U+FFFC — an invisible box — so without this, 7% of the
            // thread is literally blank.
            let attachmentsByRow = fetchAttachments(db: db, rowIds: rowIds)
            if !attachmentsByRow.isEmpty {
                for i in messages.indices {
                    messages[i].attachmentInfo = attachmentsByRow[rowIds[i]]
                }
                let enriched = messages.filter { $0.attachmentInfo != nil }.count
                print("  Attachments: \(attachmentsByRow.count) found, \(enriched) attached to messages")
            }

            // The watermark is a DURABILITY claim, so it may only advance past a
            // message once that message is actually in the local queue.
            //
            // `addMessage` is `queue.async`: calling it in a loop and saving the
            // date immediately persisted "handled" before the INSERT had even
            // been dispatched, let alone stepped. A prepare/step failure — disk
            // full during a long backfill, a corrupt db — then silently ate up
            // to a full batch per tick, permanently, because chat.db is only
            // ever re-read forward from this date. `addBrowserVisits` already
            // has the right shape (synchronous, throwing, commit-then-advance);
            // this waits for the same guarantee.
            //
            // Messages whose date is outside the plausible window are dropped
            // here rather than passed down: `addMessage` rejects them too, and
            // counting that rejection as a storage failure would wedge the
            // watermark forever on one malformed row.
            let calendar = Calendar.current
            let storable = messages.filter { m in
                let year = calendar.component(.year, from: m.date)
                return year >= 2000 && year <= 2100
            }

            let group = DispatchGroup()
            let lock = NSLock()
            var allStored = true
            for message in storable {
                group.enter()
                queue.addMessage(message) { result in
                    if case .failure = result {
                        lock.lock(); allStored = false; lock.unlock()
                    }
                    group.leave()
                }
            }
            group.wait()

            guard allStored else {
                // Hold the watermark: these messages are still in chat.db, so
                // the next tick re-reads the same window and tries again. The
                // box dedups on GUID, so a retry costs nothing.
                print("⚠️ Some messages failed to queue — holding sync date so they are re-read")
                return
            }

            // Update last sync date
            if let latestDate = latestMessageDate {
                lastSyncDate = latestDate
                saveLastSyncDate()
            }
        }
    }
    
    /// Attachment metadata for a batch of messages, keyed by message ROWID.
    ///
    /// # Why this is a separate query and not a join
    ///
    /// The `attachment` table's columns have drifted across macOS releases, and one
    /// unknown column name fails `sqlite3_prepare_v2` for the ENTIRE statement. Folded
    /// into the message SELECT, a column that doesn't exist on someone's older Mac
    /// would stop *message sync itself* — taking down the most valuable stream we have
    /// in order to add a nicety. Enrichment must never be able to kill the thing it
    /// enriches, so this fails on its own: log, return empty, messages still sync.
    ///
    /// # Metadata only — no bytes
    ///
    /// We take the filename, type, size, and the on-disk path. The path is the
    /// load-bearing field: chat.db keeps the file forever under
    /// `~/Library/Messages/Attachments/`, so recording where it lives makes a future
    /// image backfill a *backfill*, rather than archaeology against a thread we can no
    /// longer interpret.
    private func fetchAttachments(db: OpaquePointer?, rowIds: [Int64]) -> [Int64: [[String: Any]]] {
        guard let db, !rowIds.isEmpty else { return [:] }

        var out: [Int64: [[String: Any]]] = [:]

        // Chunked: SQLITE_MAX_VARIABLE_NUMBER is 999 on older SQLite, and `batchSize` is
        // 1000 — one over the line, which would have failed on exactly the older Macs
        // this fallback exists for.
        for chunk in stride(from: 0, to: rowIds.count, by: 500).map({
            Array(rowIds[$0..<min($0 + 500, rowIds.count)])
        }) {
            let placeholders = chunk.map { _ in "?" }.joined(separator: ",")
            // NOTE the naming, which is genuinely backwards in chat.db:
            //   attachment.filename      → the full PATH on disk
            //   attachment.transfer_name → the display filename ("IMG_4821.HEIC")
            let sql = """
                SELECT maj.message_id, a.guid, a.mime_type, a.transfer_name,
                       a.total_bytes, a.uti, a.is_sticker, a.filename
                FROM message_attachment_join maj
                JOIN attachment a ON a.ROWID = maj.attachment_id
                WHERE maj.message_id IN (\(placeholders))
                ORDER BY maj.ROWID
            """

            var stmt: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
                print("⚠️ Attachment metadata unavailable (messages still syncing): \(String(cString: sqlite3_errmsg(db)))")
                sqlite3_finalize(stmt)
                return [:]
            }
            defer { sqlite3_finalize(stmt) }

            for (i, rowId) in chunk.enumerated() {
                sqlite3_bind_int64(stmt, Int32(i + 1), rowId)
            }

            while sqlite3_step(stmt) == SQLITE_ROW {
                guard let s = stmt else { continue }
                let messageRowId = sqlite3_column_int64(s, 0)

                var info: [String: Any] = [:]
                func text(_ col: Int32) -> String? {
                    sqlite3_column_type(s, col) != SQLITE_NULL
                        ? String(cString: sqlite3_column_text(s, col))
                        : nil
                }
                if let v = text(1) { info["guid"] = v }
                if let v = text(2) { info["mime_type"] = v }
                if let v = text(3) { info["filename"] = v }
                if sqlite3_column_type(s, 4) != SQLITE_NULL {
                    info["size_bytes"] = sqlite3_column_int64(s, 4)
                }
                if let v = text(5) { info["uti"] = v }
                info["is_sticker"] = sqlite3_column_int(s, 6) != 0
                if let v = text(7) { info["path"] = v }   // the pointer for a v2 backfill

                out[messageRowId, default: []].append(info)
            }
        }

        return out
    }

    private func parseMessageRow(statement: OpaquePointer) -> Message {
        // Extract all fields from the query result
        let messageId = sqlite3_column_type(statement, 0) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 0))
            : ""
        let chatId = sqlite3_column_type(statement, 1) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 1))
            : ""

        let handleId: String? = sqlite3_column_type(statement, 2) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 2))
            : nil

        var text: String? = sqlite3_column_type(statement, 3) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 3))
            : nil

        // Extract attributedBody blob for sent messages (column 4)
        if text == nil, sqlite3_column_type(statement, 4) == SQLITE_BLOB {
            if let blobPointer = sqlite3_column_blob(statement, 4) {
                let blobSize = sqlite3_column_bytes(statement, 4)
                let data = Data(bytes: blobPointer, count: Int(blobSize))
                text = extractTextFromAttributedBody(data)
            }
        }

        let service = sqlite3_column_type(statement, 5) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 5))
            : "iMessage"

        let isFromMe = sqlite3_column_int(statement, 6) != 0

        // Convert Core Data timestamps to Date
        let dateTimestamp = Double(sqlite3_column_int64(statement, 7))
        let date = Message.dateFromCoreDataTimestamp(dateTimestamp)

        let dateRead: Date? = sqlite3_column_type(statement, 8) != SQLITE_NULL
            ? Message.dateFromCoreDataTimestamp(Double(sqlite3_column_int64(statement, 8)))
            : nil

        let dateDelivered: Date? = sqlite3_column_type(statement, 9) != SQLITE_NULL
            ? Message.dateFromCoreDataTimestamp(Double(sqlite3_column_int64(statement, 9)))
            : nil

        let isRead = sqlite3_column_int(statement, 10) != 0
        let isDelivered = sqlite3_column_int(statement, 11) != 0
        let isSent = sqlite3_column_int(statement, 12) != 0
        let cacheHasAttachments = sqlite3_column_int(statement, 13) != 0

        let groupTitle: String? = sqlite3_column_type(statement, 14) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 14))
            : nil

        let associatedMessageGuid: String? = sqlite3_column_type(statement, 15) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 15))
            : nil

        let associatedMessageType: Int? = sqlite3_column_type(statement, 16) != SQLITE_NULL
            ? Int(sqlite3_column_int(statement, 16))
            : nil

        let expressiveSendStyleId: String? = sqlite3_column_type(statement, 17) != SQLITE_NULL
            ? String(cString: sqlite3_column_text(statement, 17))
            : nil

        let attachmentCount: Int? = sqlite3_column_type(statement, 18) != SQLITE_NULL
            ? Int(sqlite3_column_int(statement, 18))
            : nil
        
        return Message(
            messageId: messageId,
            chatId: chatId,
            handleId: handleId,
            text: text,
            service: service,
            isFromMe: isFromMe,
            date: date,
            dateRead: dateRead,
            dateDelivered: dateDelivered,
            isRead: isRead,
            isDelivered: isDelivered,
            isSent: isSent,
            cacheHasAttachments: cacheHasAttachments,
            attachmentCount: attachmentCount,
            attachmentInfo: nil, // TODO: Query attachments separately if needed
            groupTitle: groupTitle,
            associatedMessageGuid: associatedMessageGuid,
            associatedMessageType: associatedMessageType,
            expressiveSendStyleId: expressiveSendStyleId
        )
    }

    /// Extract plain text from the `attributedBody` blob in chat.db.
    ///
    /// Modern Messages leaves `message.text` NULL and puts the body here, so this is
    /// the ONLY path for the large majority of messages — when it fails they arrive
    /// blank (it was failing for ~94% of them).
    ///
    /// The blob is NOT an NSKeyedArchiver plist: it's a legacy **typedstream**
    /// (`NSArchiver`, header "streamtyped"), which NSKeyedUnarchiver cannot decode at
    /// all — so both the secure and the "legacy" NSKeyedUnarchiver paths simply threw.
    /// NSUnarchiver, which could read it, is unavailable in Swift. So we parse out the
    /// string ourselves: find the NSString marker, then read the length-prefixed UTF-8
    /// payload that follows.
    private func extractTextFromAttributedBody(_ data: Data) -> String? {
        guard !data.isEmpty else { return nil }

        // Keep the keyed path first: harmless, and covers any blob that *is* keyed.
        if let s = try? NSKeyedUnarchiver.unarchivedObject(
            ofClass: NSAttributedString.self, from: data), !s.string.isEmpty {
            return s.string
        }

        return Self.parseTypedStreamString(data)
    }

    /// Pull the message body out of a typedstream archive.
    ///
    /// Layout after the class name: `NSString` ... `+` (0x2B) then a length, then the
    /// UTF-8 bytes. Lengths < 0x80 are a single byte; 0x81/0x82 introduce a 2- or
    /// 3-byte little-endian length (that's how anything longer than 127 chars encodes).
    static func parseTypedStreamString(_ data: Data) -> String? {
        guard let marker = data.range(of: Data("NSString".utf8)) else { return nil }
        let bytes = [UInt8](data)

        // The body is introduced by '+' shortly after the class name.
        let searchFrom = data.distance(from: data.startIndex, to: marker.upperBound)
        guard let plus = bytes[searchFrom...].firstIndex(of: 0x2B) else { return nil }
        var i = plus + 1
        guard i < bytes.count else { return nil }

        var length = Int(bytes[i])
        i += 1
        if length == 0x81 {
            guard i + 1 < bytes.count else { return nil }
            length = Int(bytes[i]) | (Int(bytes[i + 1]) << 8)
            i += 2
        } else if length == 0x82 {
            guard i + 2 < bytes.count else { return nil }
            length = Int(bytes[i]) | (Int(bytes[i + 1]) << 8) | (Int(bytes[i + 2]) << 16)
            i += 3
        }

        guard length > 0, i + length <= bytes.count else { return nil }
        let text = String(decoding: bytes[i..<(i + length)], as: UTF8.self)
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func dateToCoreDateTimestamp(_ date: Date) -> Double {
        // Convert Date to Core Data timestamp (nanoseconds since 2001-01-01)
        let coreDataEpochOffset: TimeInterval = 978307200
        let secondsSince2001 = date.timeIntervalSince1970 - coreDataEpochOffset
        // Convert to nanoseconds for Messages.app database
        return secondsSince2001 * 1_000_000_000
    }
    
    private func loadLastSyncDate() {
        // Load from UserDefaults or local storage
        if let storedDate = UserDefaults.standard.object(forKey: "virtues.messages.lastSyncDate") as? Date {
            // Validate the date is reasonable (between 2000 and 2100)
            let calendar = Calendar.current
            let year = calendar.component(.year, from: storedDate)
            
            if year >= 2000 && year <= 2100 {
                lastSyncDate = storedDate
                print("Loaded last sync date: \(ISO8601DateFormatter().string(from: storedDate))")
            } else {
                print("⚠️ Discarding invalid stored sync date (year \(year)). Will perform initial sync.")
                // Clear the corrupted value
                UserDefaults.standard.removeObject(forKey: "virtues.messages.lastSyncDate")
                lastSyncDate = nil
            }
        }
    }
    
    private func saveLastSyncDate() {
        if let date = lastSyncDate {
            // Validate before saving
            let calendar = Calendar.current
            let year = calendar.component(.year, from: date)
            
            if year >= 2000 && year <= 2100 {
                UserDefaults.standard.set(date, forKey: "virtues.messages.lastSyncDate")
                print("Saved last sync date: \(ISO8601DateFormatter().string(from: date))")
            } else {
                print("⚠️ Refusing to save invalid sync date (year \(year))")
            }
        }
    }
    
    private func canAccessMessagesDB() -> Bool {
        // Check file exists first
        guard FileManager.default.fileExists(atPath: dbPath) else {
            print("⚠️ Messages database file does not exist at: \(dbPath)")
            return false
        }

        // Try multiple times with small delays (WAL mode can cause transient locks)
        for attempt in 1...3 {
            if MessageMonitor.canReadMessagesDB() {
                return true
            }
            // If not last attempt, wait briefly and retry
            if attempt < 3 {
                Thread.sleep(forTimeInterval: 0.1)  // 100ms delay
            }
        }

        print("⚠️ Failed to open Messages database after 3 attempts")
        return false
    }

    /// One real read-open of the Messages DB. The single source of truth for
    /// "do we have Full Disk Access?" — shared by the monitor and by
    /// `status`/daemon startup.
    ///
    /// Why an `sqlite3_open_v2` and not `FileManager.isReadableFile`: a stat
    /// does NOT trip macOS TCC, so it neither reports FDA accurately nor gets
    /// this binary listed under System Settings → Full Disk Access. A real
    /// open() attempt does both — a *denied* open is exactly how macOS enrolls
    /// `virtues-collector` in the FDA list, giving the user a row to toggle on.
    static func canReadMessagesDB() -> Bool {
        let path = NSString(string: "~/Library/Messages/chat.db").expandingTildeInPath
        guard FileManager.default.fileExists(atPath: path) else { return false }
        var db: OpaquePointer?
        defer {
            if db != nil { sqlite3_close(db) }
        }
        return sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK
    }
}