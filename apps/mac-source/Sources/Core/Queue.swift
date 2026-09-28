import CollectorPolicy
import Foundation
import SQLite3

// SQLite constants for text binding
private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

class Queue {
    private var db: OpaquePointer?
    private let dbPath: String
    // Serial queue ensures thread-safe SQLite access with single connection
    private let queue = DispatchQueue(label: "com.virtues.collector.queue")
    
    init() throws {
        let dbDir = Config.configDir
        try FileManager.default.createDirectory(at: dbDir, withIntermediateDirectories: true)
        self.dbPath = dbDir.appendingPathComponent("activity.db").path
        
        try openDatabase()
        try createTable()
    }
    
    deinit {
        sqlite3_close(db)
    }

    /// Run `body` on the queue's serial SQLite connection. For the extension
    /// methods, which cannot see the private `db`/`queue`.
    fileprivate func withDB<T>(_ body: (OpaquePointer?) throws -> T) throws -> T {
        try queue.sync { try body(db) }
    }
    
    private func openDatabase() throws {
        if sqlite3_open(dbPath, &db) != SQLITE_OK {
            throw QueueError.cannotOpenDatabase
        }
    }
    
    private func createTable() throws {
        // Create events table
        let createEventsTableSQL = """
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                app_name TEXT NOT NULL,
                bundle_id TEXT,
                window_title TEXT,
                uploaded INTEGER DEFAULT 0,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_events_uploaded ON events(uploaded);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        """

        if sqlite3_exec(db, createEventsTableSQL, nil, nil, nil) != SQLITE_OK {
            throw QueueError.cannotCreateTable
        }

        // Migration: `window_title` was added after the first releases, and
        // CREATE TABLE IF NOT EXISTS won't touch an existing activity.db. ALTER
        // fails harmlessly with "duplicate column" once it's already there, so
        // ignore the result rather than gate on a schema probe.
        sqlite3_exec(db, "ALTER TABLE events ADD COLUMN window_title TEXT", nil, nil, nil)
        
        // Browser visits. UNIQUE(url, visit_time) is the dedup key and it matters:
        // each sync re-reads an overlapping window out of the browser's own history
        // DB (cursors are per-browser and we re-scan from the last visit we saw), so
        // without it every overlap would re-queue the same visits. It also matches
        // how the box dedups (`source_stream_id = url:timestamp`), so the two layers
        // agree on what "the same visit" means.
        let createBrowserTableSQL = """
            CREATE TABLE IF NOT EXISTS browser_visits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT,
                visit_time TEXT NOT NULL,
                browser TEXT NOT NULL,
                uploaded INTEGER DEFAULT 0,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(url, visit_time)
            );
            CREATE INDEX IF NOT EXISTS idx_browser_uploaded ON browser_visits(uploaded);
            CREATE INDEX IF NOT EXISTS idx_browser_time ON browser_visits(visit_time);
        """

        if sqlite3_exec(db, createBrowserTableSQL, nil, nil, nil) != SQLITE_OK {
            throw QueueError.cannotCreateTable
        }

        // Create messages table
        let createMessagesTableSQL = """
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT NOT NULL UNIQUE,
                chat_id TEXT NOT NULL,
                handle_id TEXT,
                text TEXT,
                service TEXT,
                is_from_me INTEGER,
                date TEXT NOT NULL,
                date_read TEXT,
                date_delivered TEXT,
                is_read INTEGER,
                is_delivered INTEGER,
                is_sent INTEGER,
                cache_has_attachments INTEGER,
                attachment_count INTEGER,
                attachment_info TEXT,
                group_title TEXT,
                associated_message_guid TEXT,
                associated_message_type INTEGER,
                expressive_send_style_id TEXT,
                uploaded INTEGER DEFAULT 0,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_messages_uploaded ON messages(uploaded);
            CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date);
            CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id);
        """
        
        if sqlite3_exec(db, createMessagesTableSQL, nil, nil, nil) != SQLITE_OK {
            throw QueueError.cannotCreateTable
        }

        // Bookmark snapshots. Unlike the row-per-event tables above, a snapshot
        // is one browser's ENTIRE bookmark state as a JSON records array — the
        // box reconciles (upsert + tombstone-by-absence), so a newer snapshot
        // for a browser supersedes an older pending one rather than queueing
        // behind it (see replacePendingBookmarkSnapshot).
        let createBookmarkTableSQL = """
            CREATE TABLE IF NOT EXISTS bookmark_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                browser TEXT NOT NULL,
                records_json TEXT NOT NULL,
                uploaded INTEGER DEFAULT 0,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_bookmark_snapshots_uploaded
                ON bookmark_snapshots(uploaded);
        """

        if sqlite3_exec(db, createBookmarkTableSQL, nil, nil, nil) != SQLITE_OK {
            throw QueueError.cannotCreateTable
        }

        // iMessage attachments to send, one row per chat.db attachment GUID,
        // kept for good (not cleaned up): the row is how we know what the box
        // holds, which is what the deletion check walks.
        //
        //   pending        eligible, not sent yet
        //   unavailable    file not downloaded yet (iCloud); retry at next_attempt_at
        //   uploaded       the box has it
        //   skipped        never sendable (video/audio, over the cap, unconvertible)
        //   pending_delete message gone from chat.db; the box must be told
        //   deleted        the box was told
        //
        // Not fatal on failure: attachments enrich messages, and must never be
        // able to stop the collector that delivers them.
        let createAttachmentsTableSQL = """
            CREATE TABLE IF NOT EXISTS attachments (
                attachment_guid TEXT PRIMARY KEY,
                message_guid TEXT NOT NULL,
                idx INTEGER NOT NULL,
                path TEXT,
                mime_type TEXT,
                uti TEXT,
                filename TEXT,
                size_bytes INTEGER,
                message_date TEXT NOT NULL,
                state TEXT NOT NULL,
                skip_reason TEXT,
                attempts INTEGER NOT NULL DEFAULT 0,
                next_attempt_at TEXT,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_attachments_state ON attachments(state, message_date);
            CREATE INDEX IF NOT EXISTS idx_attachments_message ON attachments(message_guid);
        """
        if sqlite3_exec(db, createAttachmentsTableSQL, nil, nil, nil) != SQLITE_OK {
            print("⚠️ attachments table unavailable (messages still sync): \(String(cString: sqlite3_errmsg(db)))")
        }
    }
    
    func addEvent(_ event: Event, completion: ((Result<Void, Error>) -> Void)? = nil) {
        queue.async {
            do {
                let insertSQL = """
                    INSERT INTO events (timestamp, event_type, app_name, bundle_id, window_title, uploaded)
                    VALUES (?, ?, ?, ?, ?, 0)
                """

                var statement: OpaquePointer?
                defer { sqlite3_finalize(statement) }

                guard sqlite3_prepare_v2(self.db, insertSQL, -1, &statement, nil) == SQLITE_OK else {
                    throw QueueError.cannotPrepareStatement
                }

                let timestamp = ISO8601DateFormatter().string(from: event.timestamp)

                // Use NSString to ensure proper memory management for SQLite binding
                let timestampNS = timestamp as NSString
                let eventTypeNS = event.eventType as NSString
                let appNameNS = event.appName as NSString

                sqlite3_bind_text(statement, 1, timestampNS.utf8String, -1, SQLITE_TRANSIENT)
                sqlite3_bind_text(statement, 2, eventTypeNS.utf8String, -1, SQLITE_TRANSIENT)
                sqlite3_bind_text(statement, 3, appNameNS.utf8String, -1, SQLITE_TRANSIENT)

                if let bundleId = event.bundleId {
                    let bundleIdNS = bundleId as NSString
                    sqlite3_bind_text(statement, 4, bundleIdNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 4)
                }

                if let windowTitle = event.windowTitle {
                    let windowTitleNS = windowTitle as NSString
                    sqlite3_bind_text(statement, 5, windowTitleNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 5)
                }

                guard sqlite3_step(statement) == SQLITE_DONE else {
                    throw QueueError.cannotInsertEvent
                }

                completion?(.success(()))
            } catch {
                completion?(.failure(error))
            }
        }
    }
    
    func getPendingEvents(limit: Int = 500) throws -> [(id: Int64, event: Event)] {
        try queue.sync {
            let querySQL = """
                SELECT id, timestamp, event_type, app_name, bundle_id, window_title
                FROM events
                WHERE uploaded = 0
                ORDER BY timestamp ASC
                LIMIT ?
            """
            
            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }
            
            guard sqlite3_prepare_v2(db, querySQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            
            sqlite3_bind_int(statement, 1, Int32(limit))
            
            var events: [(id: Int64, event: Event)] = []
            
            while sqlite3_step(statement) == SQLITE_ROW {
                let id = sqlite3_column_int64(statement, 0)

                // Skip if required columns are NULL
                guard sqlite3_column_type(statement, 1) != SQLITE_NULL,
                      sqlite3_column_type(statement, 2) != SQLITE_NULL,
                      sqlite3_column_type(statement, 3) != SQLITE_NULL else {
                    continue
                }

                // Read timestamp from database
                let timestampString = String(cString: sqlite3_column_text(statement, 1))
                let timestamp = ISO8601DateFormatter().date(from: timestampString) ?? Date()

                let eventType = String(cString: sqlite3_column_text(statement, 2))
                let appName = String(cString: sqlite3_column_text(statement, 3))

                let bundleId: String? = if sqlite3_column_type(statement, 4) != SQLITE_NULL {
                    String(cString: sqlite3_column_text(statement, 4))
                } else {
                    nil
                }

                let windowTitle: String? = if sqlite3_column_type(statement, 5) != SQLITE_NULL {
                    String(cString: sqlite3_column_text(statement, 5))
                } else {
                    nil
                }

                // Create event with original timestamp from database
                let event = Event(
                    timestamp: timestamp, eventType: eventType, appName: appName,
                    bundleId: bundleId, windowTitle: windowTitle)
                events.append((id: id, event: event))
            }
            
            return events
        }
    }
    
    func markEventsAsUploaded(_ eventIds: [Int64]) throws {
        try queue.sync {
            // Begin transaction for atomic updates
            guard sqlite3_exec(db, "BEGIN TRANSACTION", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            // Ensure rollback on failure
            var shouldCommit = false
            defer {
                if !shouldCommit {
                    sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
                }
            }

            let updateSQL = "UPDATE events SET uploaded = 1 WHERE id = ?"

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, updateSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            for id in eventIds {
                sqlite3_bind_int64(statement, 1, id)

                // Check return value to catch failures
                guard sqlite3_step(statement) == SQLITE_DONE else {
                    throw QueueError.cannotUpdateEvent
                }

                sqlite3_reset(statement)
            }

            // Commit transaction
            guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            shouldCommit = true
        }
    }
    
    // MARK: - Browser visits

    /// Queue a batch of visits. `INSERT OR IGNORE` leans on UNIQUE(url, visit_time)
    /// so a re-scanned overlap is a no-op rather than a duplicate.
    func addBrowserVisits(_ visits: [BrowserVisit]) throws -> Int {
        try queue.sync {
            guard sqlite3_exec(db, "BEGIN TRANSACTION", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            var committed = false
            defer { if !committed { sqlite3_exec(db, "ROLLBACK", nil, nil, nil) } }

            let sql = """
                INSERT OR IGNORE INTO browser_visits (url, title, visit_time, browser)
                VALUES (?, ?, ?, ?)
            """
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            var inserted = 0
            for visit in visits {
                sqlite3_reset(statement)
                sqlite3_clear_bindings(statement)
                sqlite3_bind_text(statement, 1, (visit.url as NSString).utf8String, -1, SQLITE_TRANSIENT)
                if let title = visit.title {
                    sqlite3_bind_text(statement, 2, (title as NSString).utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 2)
                }
                sqlite3_bind_text(statement, 3, (visit.timestamp as NSString).utf8String, -1, SQLITE_TRANSIENT)
                sqlite3_bind_text(statement, 4, (visit.browser as NSString).utf8String, -1, SQLITE_TRANSIENT)

                guard sqlite3_step(statement) == SQLITE_DONE else {
                    throw QueueError.cannotInsertEvent
                }
                inserted += sqlite3_changes(db) > 0 ? 1 : 0
            }

            guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            committed = true
            return inserted
        }
    }

    func getPendingBrowserVisits(limit: Int = 500) throws -> [(id: Int64, visit: BrowserVisit)] {
        try queue.sync {
            let sql = """
                SELECT id, url, title, visit_time, browser FROM browser_visits
                WHERE uploaded = 0 ORDER BY visit_time ASC LIMIT ?
            """
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            sqlite3_bind_int(statement, 1, Int32(limit))

            var out: [(id: Int64, visit: BrowserVisit)] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                let id = sqlite3_column_int64(statement, 0)
                let url = String(cString: sqlite3_column_text(statement, 1))
                let title: String? = sqlite3_column_type(statement, 2) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 2)) : nil
                let time = String(cString: sqlite3_column_text(statement, 3))
                let browser = String(cString: sqlite3_column_text(statement, 4))
                out.append((id, BrowserVisit(url: url, title: title, timestamp: time, browser: browser)))
            }
            return out
        }
    }

    func markBrowserVisitsUploaded(ids: [Int64]) throws {
        guard !ids.isEmpty else { return }
        try queue.sync {
            let placeholders = ids.map { _ in "?" }.joined(separator: ",")
            let sql = "UPDATE browser_visits SET uploaded = 1 WHERE id IN (\(placeholders))"
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            for (i, id) in ids.enumerated() {
                sqlite3_bind_int64(statement, Int32(i + 1), id)
            }
            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotUpdateEvent
            }
        }
    }

    /// Enqueue a browser's full bookmark snapshot, superseding any pending one
    /// for the same browser. A snapshot is total state, so uploading two of
    /// them in one batch is pure waste and the older one is simply wrong.
    /// Uploaded rows are left alone (cleanup reaps them).
    func replacePendingBookmarkSnapshot(browser: String, recordsJSON: String) throws {
        try queue.sync {
            guard sqlite3_exec(db, "BEGIN TRANSACTION", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            var committed = false
            defer { if !committed { sqlite3_exec(db, "ROLLBACK", nil, nil, nil) } }

            var del: OpaquePointer?
            defer { if del != nil { sqlite3_finalize(del) } }
            guard sqlite3_prepare_v2(
                db, "DELETE FROM bookmark_snapshots WHERE uploaded = 0 AND browser = ?",
                -1, &del, nil) == SQLITE_OK
            else { throw QueueError.cannotPrepareStatement }
            sqlite3_bind_text(del, 1, (browser as NSString).utf8String, -1, SQLITE_TRANSIENT)
            guard sqlite3_step(del) == SQLITE_DONE else { throw QueueError.cannotDeleteEvents }

            var ins: OpaquePointer?
            defer { if ins != nil { sqlite3_finalize(ins) } }
            guard sqlite3_prepare_v2(
                db, "INSERT INTO bookmark_snapshots (browser, records_json) VALUES (?, ?)",
                -1, &ins, nil) == SQLITE_OK
            else { throw QueueError.cannotPrepareStatement }
            sqlite3_bind_text(ins, 1, (browser as NSString).utf8String, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(ins, 2, (recordsJSON as NSString).utf8String, -1, SQLITE_TRANSIENT)
            guard sqlite3_step(ins) == SQLITE_DONE else { throw QueueError.cannotInsertEvent }

            guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            committed = true
        }
    }

    func getPendingBookmarkSnapshots() throws -> [(id: Int64, browser: String, recordsJSON: String)]
    {
        try queue.sync {
            let sql = """
                SELECT id, browser, records_json FROM bookmark_snapshots
                WHERE uploaded = 0 ORDER BY id ASC
            """
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            var out: [(id: Int64, browser: String, recordsJSON: String)] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                out.append((
                    sqlite3_column_int64(statement, 0),
                    String(cString: sqlite3_column_text(statement, 1)),
                    String(cString: sqlite3_column_text(statement, 2))
                ))
            }
            return out
        }
    }

    func markBookmarkSnapshotsUploaded(ids: [Int64]) throws {
        guard !ids.isEmpty else { return }
        try queue.sync {
            let placeholders = ids.map { _ in "?" }.joined(separator: ",")
            let sql = "UPDATE bookmark_snapshots SET uploaded = 1 WHERE id IN (\(placeholders))"
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            for (i, id) in ids.enumerated() {
                sqlite3_bind_int64(statement, Int32(i + 1), id)
            }
            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotUpdateEvent
            }
        }
    }

    func cleanupOldBookmarkSnapshots() throws {
        try queue.sync {
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(
                db, "DELETE FROM bookmark_snapshots WHERE uploaded = 1",
                -1, &statement, nil) == SQLITE_OK
            else { throw QueueError.cannotPrepareStatement }
            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotDeleteEvents
            }
        }
    }

    func cleanupOldBrowserVisits(olderThanHours: Int = 168) throws {
        try queue.sync {
            let cutoff = Date().addingTimeInterval(TimeInterval(-olderThanHours * 3600))
            let cutoffString = ISO8601DateFormatter().string(from: cutoff)
            var statement: OpaquePointer?
            defer { if statement != nil { sqlite3_finalize(statement) } }
            guard sqlite3_prepare_v2(
                db, "DELETE FROM browser_visits WHERE uploaded = 1 AND created_at < ?",
                -1, &statement, nil) == SQLITE_OK
            else { throw QueueError.cannotPrepareStatement }
            sqlite3_bind_text(statement, 1, (cutoffString as NSString).utf8String, -1, SQLITE_TRANSIENT)
            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotDeleteEvents
            }
        }
    }

    func cleanupOldEvents(olderThanHours: Int = 168) throws {
        try queue.sync {
            // Calculate cutoff date in Swift to avoid string interpolation in SQL
            let cutoffDate = Date().addingTimeInterval(TimeInterval(-olderThanHours * 3600))
            let cutoffString = ISO8601DateFormatter().string(from: cutoffDate)

            let deleteSQL = """
                DELETE FROM events
                WHERE uploaded = 1
                AND created_at < ?
            """

            var statement: OpaquePointer?
            defer {
                if statement != nil {
                    sqlite3_finalize(statement)
                }
            }

            guard sqlite3_prepare_v2(db, deleteSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            sqlite3_bind_text(statement, 1, (cutoffString as NSString).utf8String, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotDeleteEvents
            }
        }
    }
    
    func getStats() throws -> (pending: Int, uploaded: Int, total: Int) {
        try queue.sync {
            let statsSQL = """
                SELECT
                    COUNT(CASE WHEN uploaded = 0 THEN 1 END) as pending,
                    COUNT(CASE WHEN uploaded = 1 THEN 1 END) as uploaded,
                    COUNT(*) as total
                FROM events
            """

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, statsSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return (0, 0, 0)
            }

            let pending = Int(sqlite3_column_int(statement, 0))
            let uploaded = Int(sqlite3_column_int(statement, 1))
            let total = Int(sqlite3_column_int(statement, 2))

            return (pending, uploaded, total)
        }
    }

    /// Get total count of pending (not uploaded) events and messages
    func count() throws -> Int {
        try queue.sync {
            let countSQL = """
                SELECT
                    (SELECT COUNT(*) FROM events WHERE uploaded = 0) +
                    (SELECT COUNT(*) FROM messages WHERE uploaded = 0) as total
            """

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, countSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return 0
            }

            return Int(sqlite3_column_int(statement, 0))
        }
    }

    /// Get count of pending events only
    func pendingEventCount() throws -> Int {
        try queue.sync {
            let countSQL = "SELECT COUNT(*) FROM events WHERE uploaded = 0"

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, countSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return 0
            }

            return Int(sqlite3_column_int(statement, 0))
        }
    }

    /// Get count of pending messages only
    func pendingMessageCount() throws -> Int {
        try queue.sync {
            let countSQL = "SELECT COUNT(*) FROM messages WHERE uploaded = 0"

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, countSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return 0
            }

            return Int(sqlite3_column_int(statement, 0))
        }
    }
    
    func reset() throws {
        try queue.sync {
            let deleteSQL = "DELETE FROM events; DELETE FROM messages"
            if sqlite3_exec(db, deleteSQL, nil, nil, nil) != SQLITE_OK {
                throw QueueError.cannotDeleteEvents
            }
        }
    }
    
    // MARK: - Message Methods
    
    func addMessage(_ message: Message, completion: ((Result<Void, Error>) -> Void)? = nil) {
        queue.async {
            // Validate date is reasonable (between 2000 and 2100)
            let calendar = Calendar.current
            let year = calendar.component(.year, from: message.date)
            guard year >= 2000 && year <= 2100 else {
                print("⚠️ Skipping message with invalid date: \(message.date) (year: \(year))")
                completion?(.failure(QueueError.cannotInsertEvent))
                return
            }
            
            // UPSERT, not `INSERT OR IGNORE`.
            //
            // `OR IGNORE` made this queue write-once: 40,572 messages were already staged
            // here, so when the collector learned to read attachment metadata and re-walked
            // chat.db, every one of those rows was silently discarded on arrival. The
            // correction could be *computed* and never *accepted* — the fix reached only
            // messages that happened to be new.
            //
            // A queue that cannot take a correction is a queue that permanently encodes
            // whatever bug shipped first.
            //
            // `uploaded = 0` re-queues the row for upload, but only via the WHERE below —
            // if nothing actually changed we leave it alone, so an ordinary re-walk doesn't
            // re-send tens of thousands of identical messages.
            let insertSQL = """
                INSERT INTO messages (
                        message_id, chat_id, handle_id, text, service, is_from_me,
                        date, date_read, date_delivered, is_read, is_delivered, is_sent,
                        cache_has_attachments, attachment_count, attachment_info,
                        group_title, associated_message_guid, associated_message_type,
                        expressive_send_style_id, uploaded
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
                ON CONFLICT(message_id) DO UPDATE SET
                        text = excluded.text,
                        attachment_info = excluded.attachment_info,
                        attachment_count = excluded.attachment_count,
                        cache_has_attachments = excluded.cache_has_attachments,
                        uploaded = 0
                    WHERE messages.attachment_info IS NOT excluded.attachment_info
                       OR messages.text IS NOT excluded.text
                """
                
                var statement: OpaquePointer?
                defer { sqlite3_finalize(statement) }
                
                guard sqlite3_prepare_v2(self.db, insertSQL, -1, &statement, nil) == SQLITE_OK else {
                    print("Failed to prepare message insert statement")
                    completion?(.failure(QueueError.cannotPrepareStatement))
                    return
                }
                
                // Bind all parameters
                let messageIdNS = message.messageId as NSString
                let chatIdNS = message.chatId as NSString
                let serviceNS = message.service as NSString
                let dateNS = ISO8601DateFormatter().string(from: message.date) as NSString
                
                sqlite3_bind_text(statement, 1, messageIdNS.utf8String, -1, SQLITE_TRANSIENT)
                sqlite3_bind_text(statement, 2, chatIdNS.utf8String, -1, SQLITE_TRANSIENT)
                
                if let handleId = message.handleId {
                    let handleIdNS = handleId as NSString
                    sqlite3_bind_text(statement, 3, handleIdNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 3)
                }
                
                if let text = message.text {
                    let textNS = text as NSString
                    sqlite3_bind_text(statement, 4, textNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 4)
                }
                
                sqlite3_bind_text(statement, 5, serviceNS.utf8String, -1, SQLITE_TRANSIENT)
                sqlite3_bind_int(statement, 6, message.isFromMe ? 1 : 0)
                sqlite3_bind_text(statement, 7, dateNS.utf8String, -1, SQLITE_TRANSIENT)
                
                if let dateRead = message.dateRead {
                    let dateReadNS = ISO8601DateFormatter().string(from: dateRead) as NSString
                    sqlite3_bind_text(statement, 8, dateReadNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 8)
                }
                
                if let dateDelivered = message.dateDelivered {
                    let dateDeliveredNS = ISO8601DateFormatter().string(from: dateDelivered) as NSString
                    sqlite3_bind_text(statement, 9, dateDeliveredNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 9)
                }
                
                sqlite3_bind_int(statement, 10, message.isRead ? 1 : 0)
                sqlite3_bind_int(statement, 11, message.isDelivered ? 1 : 0)
                sqlite3_bind_int(statement, 12, message.isSent ? 1 : 0)
                sqlite3_bind_int(statement, 13, message.cacheHasAttachments ? 1 : 0)
                
                if let attachmentCount = message.attachmentCount {
                    sqlite3_bind_int(statement, 14, Int32(attachmentCount))
                } else {
                    sqlite3_bind_null(statement, 14)
                }
                
                if let attachmentInfo = message.attachmentInfo {
                    // A silent `try?` here would bind NULL on failure and look exactly like
                    // "this message has no attachments" — indistinguishable, and therefore
                    // undebuggable. If we cannot serialize what we collected, say so.
                    do {
                        let jsonData = try JSONSerialization.data(withJSONObject: attachmentInfo)
                        let jsonString = String(data: jsonData, encoding: .utf8)!
                        let jsonNS = jsonString as NSString
                        sqlite3_bind_text(statement, 15, jsonNS.utf8String, -1, SQLITE_TRANSIENT)
                    } catch {
                        print("⚠️ Could not serialize attachment metadata: \(error)")
                        sqlite3_bind_null(statement, 15)
                    }
                } else {
                    sqlite3_bind_null(statement, 15)
                }
                
                if let groupTitle = message.groupTitle {
                    let groupTitleNS = groupTitle as NSString
                    sqlite3_bind_text(statement, 16, groupTitleNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 16)
                }
                
                if let associatedMessageGuid = message.associatedMessageGuid {
                    let guidNS = associatedMessageGuid as NSString
                    sqlite3_bind_text(statement, 17, guidNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 17)
                }
                
                if let associatedMessageType = message.associatedMessageType {
                    sqlite3_bind_int(statement, 18, Int32(associatedMessageType))
                } else {
                    sqlite3_bind_null(statement, 18)
                }
                
                if let expressiveSendStyleId = message.expressiveSendStyleId {
                    let styleIdNS = expressiveSendStyleId as NSString
                    sqlite3_bind_text(statement, 19, styleIdNS.utf8String, -1, SQLITE_TRANSIENT)
                } else {
                    sqlite3_bind_null(statement, 19)
                }
                
            if sqlite3_step(statement) != SQLITE_DONE {
                print("Failed to insert message: \(String(cString: sqlite3_errmsg(self.db)))")
                completion?(.failure(QueueError.cannotInsertEvent))
            } else {
                completion?(.success(()))
            }
        }
    }
    
    func getPendingMessages(limit: Int = 500) throws -> [(id: Int64, message: Message)] {
        try queue.sync {
            let querySQL = """
                SELECT id, message_id, chat_id, handle_id, text, service, is_from_me,
                       date, date_read, date_delivered, is_read, is_delivered, is_sent,
                       cache_has_attachments, attachment_count, attachment_info,
                       group_title, associated_message_guid, associated_message_type,
                       expressive_send_style_id
                FROM messages
                WHERE uploaded = 0
                -- NEWEST FIRST. The backfill now reaches back twenty years, and
                -- oldest-first meant the box spent hours knowing only 2017 while
                -- today's conversation sat in the queue behind two decades of
                -- history — which is exactly backwards for a life-log, where the
                -- recent past is what anything actually asks about.
                --
                -- Freshness first, history filling in behind it. Correctness is
                -- unaffected (the box dedups on GUID), only the order in which the
                -- box becomes useful.
                ORDER BY date DESC
                LIMIT ?
            """
            
            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }
            
            guard sqlite3_prepare_v2(db, querySQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            
            sqlite3_bind_int(statement, 1, Int32(limit))
            
            var messages: [(id: Int64, message: Message)] = []
            
            while sqlite3_step(statement) == SQLITE_ROW {
                let id = sqlite3_column_int64(statement, 0)
                
                let messageId = String(cString: sqlite3_column_text(statement, 1))
                let chatId = String(cString: sqlite3_column_text(statement, 2))
                
                let handleId: String? = sqlite3_column_type(statement, 3) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 3))
                    : nil
                
                let text: String? = sqlite3_column_type(statement, 4) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 4))
                    : nil
                
                let service = String(cString: sqlite3_column_text(statement, 5))
                let isFromMe = sqlite3_column_int(statement, 6) != 0
                
                let dateString = String(cString: sqlite3_column_text(statement, 7))
                let date = ISO8601DateFormatter().date(from: dateString) ?? Date()
                
                let dateRead: Date? = sqlite3_column_type(statement, 8) != SQLITE_NULL
                    ? ISO8601DateFormatter().date(from: String(cString: sqlite3_column_text(statement, 8)))
                    : nil
                
                let dateDelivered: Date? = sqlite3_column_type(statement, 9) != SQLITE_NULL
                    ? ISO8601DateFormatter().date(from: String(cString: sqlite3_column_text(statement, 9)))
                    : nil
                
                let isRead = sqlite3_column_int(statement, 10) != 0
                let isDelivered = sqlite3_column_int(statement, 11) != 0
                let isSent = sqlite3_column_int(statement, 12) != 0
                let cacheHasAttachments = sqlite3_column_int(statement, 13) != 0
                
                let attachmentCount: Int? = sqlite3_column_type(statement, 14) != SQLITE_NULL
                    ? Int(sqlite3_column_int(statement, 14))
                    : nil
                
                let attachmentInfo: [[String: Any]]?
                if sqlite3_column_type(statement, 15) != SQLITE_NULL {
                    let jsonString = String(cString: sqlite3_column_text(statement, 15))
                    if let data = jsonString.data(using: .utf8),
                       let json = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] {
                        attachmentInfo = json
                    } else {
                        attachmentInfo = nil
                    }
                } else {
                    attachmentInfo = nil
                }
                
                let groupTitle: String? = sqlite3_column_type(statement, 16) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 16))
                    : nil
                
                let associatedMessageGuid: String? = sqlite3_column_type(statement, 17) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 17))
                    : nil
                
                let associatedMessageType: Int? = sqlite3_column_type(statement, 18) != SQLITE_NULL
                    ? Int(sqlite3_column_int(statement, 18))
                    : nil
                
                let expressiveSendStyleId: String? = sqlite3_column_type(statement, 19) != SQLITE_NULL
                    ? String(cString: sqlite3_column_text(statement, 19))
                    : nil
                
                let message = Message(
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
                    attachmentInfo: attachmentInfo,
                    groupTitle: groupTitle,
                    associatedMessageGuid: associatedMessageGuid,
                    associatedMessageType: associatedMessageType,
                    expressiveSendStyleId: expressiveSendStyleId
                )
                
                messages.append((id: id, message: message))
            }
            
            return messages
        }
    }
    
    func markMessagesAsUploaded(_ messageIds: [Int64]) throws {
        try queue.sync {
            // Begin transaction for atomic updates
            guard sqlite3_exec(db, "BEGIN TRANSACTION", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            // Ensure rollback on failure
            var shouldCommit = false
            defer {
                if !shouldCommit {
                    sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
                }
            }

            let updateSQL = "UPDATE messages SET uploaded = 1 WHERE id = ?"

            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }

            guard sqlite3_prepare_v2(db, updateSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            for id in messageIds {
                sqlite3_bind_int64(statement, 1, id)

                // Check return value to catch failures
                guard sqlite3_step(statement) == SQLITE_DONE else {
                    throw QueueError.cannotUpdateMessage
                }

                sqlite3_reset(statement)
            }

            // Commit transaction
            guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            shouldCommit = true
        }
    }
    
    func cleanupOldMessages(olderThanHours: Int = 168) throws {
        try queue.sync {
            // Calculate cutoff date in Swift to avoid string interpolation in SQL
            let cutoffDate = Date().addingTimeInterval(TimeInterval(-olderThanHours * 3600))
            let cutoffString = ISO8601DateFormatter().string(from: cutoffDate)

            let deleteSQL = """
                DELETE FROM messages
                WHERE uploaded = 1
                AND created_at < ?
            """

            var statement: OpaquePointer?
            defer {
                if statement != nil {
                    sqlite3_finalize(statement)
                }
            }

            guard sqlite3_prepare_v2(db, deleteSQL, -1, &statement, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }

            sqlite3_bind_text(statement, 1, (cutoffString as NSString).utf8String, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_DONE else {
                throw QueueError.cannotDeleteMessages
            }
        }
    }
}

// MARK: - iMessage attachments

/// One chat.db attachment, as queued for sending.
struct QueuedAttachment {
    let attachmentGuid: String
    let messageGuid: String
    /// Position in the message's attachments (chat.db join order) — the same
    /// order as `metadata.attachments` on the box.
    let index: Int
    let path: String?
    let mimeType: String?
    let uti: String?
    let filename: String?
    let sizeBytes: Int64?
    let messageDate: Date
    var attempts: Int = 0

    /// Build from the per-message metadata `MessageMonitor.fetchAttachments`
    /// produces. `nil` for an entry without a GUID (nothing to key it on).
    init?(info: [String: Any], index: Int, messageGuid: String, messageDate: Date) {
        guard let guid = info["guid"] as? String, !guid.isEmpty else { return nil }
        self.attachmentGuid = guid
        self.messageGuid = messageGuid
        self.index = index
        self.path = info["path"] as? String
        self.mimeType = info["mime_type"] as? String
        self.uti = info["uti"] as? String
        self.filename = info["filename"] as? String
        self.sizeBytes = (info["size_bytes"] as? NSNumber)?.int64Value
        self.messageDate = messageDate
    }

    init(attachmentGuid: String, messageGuid: String, index: Int, path: String?,
         mimeType: String?, uti: String?, filename: String?, sizeBytes: Int64?,
         messageDate: Date, attempts: Int) {
        self.attachmentGuid = attachmentGuid
        self.messageGuid = messageGuid
        self.index = index
        self.path = path
        self.mimeType = mimeType
        self.uti = uti
        self.filename = filename
        self.sizeBytes = sizeBytes
        self.messageDate = messageDate
        self.attempts = attempts
    }
}

extension Queue {
    /// Queue attachments, deciding eligibility once. Already-known GUIDs are
    /// left alone, so re-reading a message never resets what was sent.
    func enqueueAttachments(_ items: [QueuedAttachment]) throws {
        guard !items.isEmpty else { return }
        try withDB { db in
            try Queue.transaction(db) {
                let sql = """
                    INSERT OR IGNORE INTO attachments
                        (attachment_guid, message_guid, idx, path, mime_type, uti, filename,
                         size_bytes, message_date, state, skip_reason)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """
                var stmt: OpaquePointer?
                defer { sqlite3_finalize(stmt) }
                guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
                    throw QueueError.cannotPrepareStatement
                }
                let iso = ISO8601DateFormatter()
                for a in items {
                    let decision = AttachmentPolicy.decide(
                        mime: a.mimeType, uti: a.uti, filename: a.filename, sizeBytes: a.sizeBytes)
                    Queue.bind(stmt, 1, a.attachmentGuid)
                    Queue.bind(stmt, 2, a.messageGuid)
                    sqlite3_bind_int(stmt, 3, Int32(a.index))
                    Queue.bind(stmt, 4, a.path)
                    Queue.bind(stmt, 5, a.mimeType)
                    Queue.bind(stmt, 6, a.uti)
                    Queue.bind(stmt, 7, a.filename)
                    if let size = a.sizeBytes { sqlite3_bind_int64(stmt, 8, size) } else { sqlite3_bind_null(stmt, 8) }
                    Queue.bind(stmt, 9, iso.string(from: a.messageDate))
                    switch decision {
                    case .upload:
                        Queue.bind(stmt, 10, "pending")
                        Queue.bind(stmt, 11, nil)
                    case .skip(let why):
                        Queue.bind(stmt, 10, "skipped")
                        Queue.bind(stmt, 11, why)
                    }
                    guard sqlite3_step(stmt) == SQLITE_DONE else { throw QueueError.cannotInsertEvent }
                    sqlite3_reset(stmt)
                }
            }
        }
    }

    /// Attachments due for a send attempt, newest message first (today's
    /// photos before 2014's).
    ///
    /// Only for messages already delivered: a message still waiting in the
    /// `messages` queue is not on the box yet, and the box keeps attachments
    /// only for messages it has. A message not in the queue at all was
    /// delivered and cleaned up (the queue's watermark only passes a message
    /// once it is queued).
    func getSendableAttachments(limit: Int, now: Date = Date()) throws -> [QueuedAttachment] {
        try withDB { db in
            let sql = """
                SELECT a.attachment_guid, a.message_guid, a.idx, a.path, a.mime_type, a.uti,
                       a.filename, a.size_bytes, a.message_date, a.attempts
                FROM attachments a
                WHERE (a.state = 'pending'
                       OR (a.state = 'unavailable' AND (a.next_attempt_at IS NULL OR a.next_attempt_at <= ?)))
                  AND NOT EXISTS (SELECT 1 FROM messages m
                                   WHERE m.message_id = a.message_guid AND m.uploaded = 0)
                ORDER BY a.message_date DESC
                LIMIT ?
            """
            var stmt: OpaquePointer?
            defer { sqlite3_finalize(stmt) }
            guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            let iso = ISO8601DateFormatter()
            Queue.bind(stmt, 1, iso.string(from: now))
            sqlite3_bind_int(stmt, 2, Int32(limit))
            var out: [QueuedAttachment] = []
            while sqlite3_step(stmt) == SQLITE_ROW {
                out.append(QueuedAttachment(
                    attachmentGuid: Queue.text(stmt, 0) ?? "",
                    messageGuid: Queue.text(stmt, 1) ?? "",
                    index: Int(sqlite3_column_int(stmt, 2)),
                    path: Queue.text(stmt, 3),
                    mimeType: Queue.text(stmt, 4),
                    uti: Queue.text(stmt, 5),
                    filename: Queue.text(stmt, 6),
                    sizeBytes: sqlite3_column_type(stmt, 7) == SQLITE_NULL ? nil : sqlite3_column_int64(stmt, 7),
                    messageDate: Queue.text(stmt, 8).flatMap { iso.date(from: $0) } ?? Date.distantPast,
                    attempts: Int(sqlite3_column_int(stmt, 9))
                ))
            }
            return out
        }
    }

    func markAttachmentsUploaded(_ guids: [String]) throws {
        try setAttachmentState(guids, state: "uploaded")
    }

    func markAttachmentSkipped(_ guid: String, reason: String) throws {
        try withDB { db in
            try Queue.exec(db, "UPDATE attachments SET state = 'skipped', skip_reason = ?, updated_at = CURRENT_TIMESTAMP WHERE attachment_guid = ?", [reason, guid])
        }
    }

    /// Not downloaded yet: count the attempt and schedule the next look.
    func markAttachmentUnavailable(_ guid: String, attempts: Int, now: Date = Date()) throws {
        let next = now.addingTimeInterval(AttachmentPolicy.retryDelay(afterAttempts: attempts))
        try withDB { db in
            try Queue.exec(db, """
                UPDATE attachments SET state = 'unavailable', attempts = ?, next_attempt_at = ?,
                       updated_at = CURRENT_TIMESTAMP
                WHERE attachment_guid = ?
            """, [String(attempts), ISO8601DateFormatter().string(from: next), guid])
        }
    }

    /// Message GUIDs whose attachments the box holds — what the deletion check
    /// must confirm still exist in chat.db.
    func uploadedAttachmentMessageGuids() throws -> [String] {
        try withDB { db in
            var stmt: OpaquePointer?
            defer { sqlite3_finalize(stmt) }
            guard sqlite3_prepare_v2(db, "SELECT DISTINCT message_guid FROM attachments WHERE state = 'uploaded'", -1, &stmt, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            var out: [String] = []
            while sqlite3_step(stmt) == SQLITE_ROW {
                if let g = Queue.text(stmt, 0) { out.append(g) }
            }
            return out
        }
    }

    /// Messages gone from chat.db. Sent attachments become `pending_delete`
    /// (the box must drop its copy); unsent ones stop being sendable.
    func markMessagesDeletedAtSource(_ messageGuids: [String]) throws {
        guard !messageGuids.isEmpty else { return }
        try withDB { db in
            try Queue.transaction(db) {
                for g in messageGuids {
                    try Queue.exec(db, "UPDATE attachments SET state = 'pending_delete', updated_at = CURRENT_TIMESTAMP WHERE message_guid = ? AND state = 'uploaded'", [g])
                    try Queue.exec(db, "UPDATE attachments SET state = 'skipped', skip_reason = 'message deleted', updated_at = CURRENT_TIMESTAMP WHERE message_guid = ? AND state IN ('pending', 'unavailable')", [g])
                }
            }
        }
    }

    func pendingDeletionMessageGuids() throws -> [String] {
        try withDB { db in
            var stmt: OpaquePointer?
            defer { sqlite3_finalize(stmt) }
            guard sqlite3_prepare_v2(db, "SELECT DISTINCT message_guid FROM attachments WHERE state = 'pending_delete' LIMIT 500", -1, &stmt, nil) == SQLITE_OK else {
                throw QueueError.cannotPrepareStatement
            }
            var out: [String] = []
            while sqlite3_step(stmt) == SQLITE_ROW {
                if let g = Queue.text(stmt, 0) { out.append(g) }
            }
            return out
        }
    }

    func markDeletionsSent(_ messageGuids: [String]) throws {
        guard !messageGuids.isEmpty else { return }
        try withDB { db in
            try Queue.transaction(db) {
                for g in messageGuids {
                    try Queue.exec(db, "UPDATE attachments SET state = 'deleted', updated_at = CURRENT_TIMESTAMP WHERE message_guid = ? AND state = 'pending_delete'", [g])
                }
            }
        }
    }

    private func setAttachmentState(_ guids: [String], state: String) throws {
        guard !guids.isEmpty else { return }
        try withDB { db in
            try Queue.transaction(db) {
                for g in guids {
                    try Queue.exec(db, "UPDATE attachments SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE attachment_guid = ?", [state, g])
                }
            }
        }
    }

    // Small SQLite helpers for the attachment queries.

    fileprivate static func bind(_ stmt: OpaquePointer?, _ idx: Int32, _ value: String?) {
        if let value {
            sqlite3_bind_text(stmt, idx, (value as NSString).utf8String, -1, SQLITE_TRANSIENT)
        } else {
            sqlite3_bind_null(stmt, idx)
        }
    }

    fileprivate static func text(_ stmt: OpaquePointer?, _ col: Int32) -> String? {
        guard sqlite3_column_type(stmt, col) != SQLITE_NULL, let c = sqlite3_column_text(stmt, col) else {
            return nil
        }
        return String(cString: c)
    }

    fileprivate static func exec(_ db: OpaquePointer?, _ sql: String, _ args: [String]) throws {
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QueueError.cannotPrepareStatement
        }
        for (i, a) in args.enumerated() { bind(stmt, Int32(i + 1), a) }
        guard sqlite3_step(stmt) == SQLITE_DONE else { throw QueueError.cannotUpdateMessage }
    }

    fileprivate static func transaction(_ db: OpaquePointer?, _ body: () throws -> Void) throws {
        guard sqlite3_exec(db, "BEGIN TRANSACTION", nil, nil, nil) == SQLITE_OK else {
            throw QueueError.cannotPrepareStatement
        }
        do {
            try body()
        } catch {
            sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
            throw error
        }
        guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
            sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
            throw QueueError.cannotPrepareStatement
        }
    }
}

enum QueueError: LocalizedError {
    case cannotOpenDatabase
    case cannotCreateTable
    case cannotPrepareStatement
    case cannotInsertEvent
    case cannotDeleteEvents
    case cannotUpdateEvent
    case cannotUpdateMessage
    case cannotDeleteMessages

    var errorDescription: String? {
        switch self {
        case .cannotOpenDatabase:
            return "Cannot open database"
        case .cannotCreateTable:
            return "Cannot create events table"
        case .cannotPrepareStatement:
            return "Cannot prepare SQL statement"
        case .cannotInsertEvent:
            return "Cannot insert event"
        case .cannotDeleteEvents:
            return "Cannot delete events"
        case .cannotUpdateEvent:
            return "Cannot update event"
        case .cannotUpdateMessage:
            return "Cannot update message"
        case .cannotDeleteMessages:
            return "Cannot delete messages"
        }
    }
}
// MARK: - Sync cursors

/// The incremental-sync watermarks, which live in UserDefaults rather than in
/// `activity.db`.
///
/// They are collected here because that split is a trap. `reset` and
/// `uninstall --delete-data` both delete the queue database and told the user
/// their data was gone — while every cursor survived in UserDefaults. Re-pairing
/// then resumed messages from the old `lastSyncDate`, browser visits from the
/// old `lastVisit`, and bookmarks from an unchanged content hash, so everything
/// that had been pending at reset time was never read again and the collector
/// reported itself perfectly healthy.
///
/// Deleting the queue and keeping the cursors is never correct: the cursor's
/// only meaning is "everything before this is safely in the queue."
enum SyncCursors {
    /// Prefixes rather than a fixed list — browser and bookmark cursors are
    /// per-source (`virtues.browser.<id>.lastVisit`), so enumerating them by
    /// name would silently miss whichever browser is added next.
    private static let prefixes = [
        "virtues.messages.",
        "virtues.browser.",
        "virtues.bookmarks.",
    ]

    /// Forget every incremental-sync position, so the next run re-reads from
    /// each source's natural starting point.
    static func clearAll() {
        let defaults = UserDefaults.standard
        for (key, _) in defaults.dictionaryRepresentation()
        where prefixes.contains(where: key.hasPrefix) {
            defaults.removeObject(forKey: key)
        }
    }
}
