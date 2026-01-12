//
//  SQLiteManager.swift
//  Virtues
//
//  Manages SQLite database for upload queue with retry logic
//

import Foundation
import SQLite3

/// Errors that can occur during enqueue validation
enum EnqueueError: Error, LocalizedError {
    case emptyPayload
    case invalidStreamName(String)
    case payloadTooLarge(size: Int, maxSize: Int)
    case queueFull(currentSize: Int64, maxSize: Int64)
    case storageFailed(reason: String)
    case emergencyStorageFull(available: Int64)

    var errorDescription: String? {
        switch self {
        case .emptyPayload:
            return "Cannot enqueue empty data"
        case .invalidStreamName(let name):
            return "Invalid stream name: \(name)"
        case .payloadTooLarge(let size, let maxSize):
            return "Payload too large: \(size) bytes (max: \(maxSize) bytes)"
        case .queueFull(let currentSize, let maxSize):
            return "Queue full: \(currentSize) bytes (max: \(maxSize) bytes)"
        case .storageFailed(let reason):
            return "Storage failed: \(reason)"
        case .emergencyStorageFull(let available):
            return "Device storage critically low: \(available / 1_000_000) MB available"
        }
    }
}

class SQLiteManager {
    static let shared = SQLiteManager()

    // MARK: - Configuration

    /// Maximum size of a single payload (10MB)
    private let maxPayloadSize = 10_000_000

    /// Maximum total queue size before backpressure kicks in (500MB)
    private let maxQueueSizeBytes: Int64 = 500_000_000

    /// Threshold for warning-level storage (50MB)
    private let storageWarningThreshold: Int64 = 50_000_000

    /// Threshold for critical storage - stop collecting (10MB)
    private let storageCriticalThreshold: Int64 = 10_000_000

    private var db: OpaquePointer?
    private let dbPath: String
    private let queue = DispatchQueue(label: "com.virtues.sqlite")
    
    private init() {
        // Get documents directory
        let documentsPath = NSSearchPathForDirectoriesInDomains(.documentDirectory, .userDomainMask, true).first!
        dbPath = "\(documentsPath)/virtues_upload_queue.db"

        // Initialize database on the serial queue
        queue.sync {
            openDatabase()
            createTables()
            resetStuckRecords()
        }
    }
    
    deinit {
        closeDatabase()
    }
    
    // MARK: - Database Setup
    
    private func openDatabase() {
        if sqlite3_open(dbPath, &db) != SQLITE_OK {
            print("Unable to open database at \(dbPath)")
        }
    }
    
    private func closeDatabase() {
        if db != nil {
            sqlite3_close(db)
            db = nil
        }
    }
    
    private func createTables() {
        let createTableString = UploadEvent.createTableSQL

        if sqlite3_exec(db, createTableString, nil, nil, nil) != SQLITE_OK {
            print("Error creating upload_queue table")
        }
    }

    /// Reset any records stuck in 'uploading' status from previous crashed syncs (at init)
    private func resetStuckRecords() {
        let resetSQL = """
            UPDATE upload_queue
            SET status = 'pending'
            WHERE status = 'uploading'
        """

        var statement: OpaquePointer?

        if sqlite3_prepare_v2(db, resetSQL, -1, &statement, nil) == SQLITE_OK {
            if sqlite3_step(statement) == SQLITE_DONE {
                let changedRows = sqlite3_changes(db)
                if changedRows > 0 {
                    print("🔄 Reset \(changedRows) stuck record(s) from 'uploading' to 'pending' status")
                }
            } else {
                let errorMsg = String(cString: sqlite3_errmsg(db))
                print("❌ Error resetting stuck records: \(errorMsg)")
            }
        }

        sqlite3_finalize(statement)
    }

    /// Reset records that have been stuck in 'uploading' for more than 10 minutes
    /// Call this at the start of each sync cycle to handle interrupted syncs
    func resetStaleUploads() -> Int {
        return queue.sync {
            var resetCount = 0

            let resetSQL = """
                UPDATE upload_queue
                SET status = 'pending',
                    upload_attempts = upload_attempts + 1
                WHERE status = 'uploading'
                AND last_attempt_date < ?
            """

            var statement: OpaquePointer?
            let tenMinutesAgo = Date().addingTimeInterval(-10 * 60).timeIntervalSince1970

            if sqlite3_prepare_v2(db, resetSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, tenMinutesAgo)

                if sqlite3_step(statement) == SQLITE_DONE {
                    resetCount = Int(sqlite3_changes(db))
                    #if DEBUG
                    if resetCount > 0 {
                        print("🔄 Reset \(resetCount) stale upload(s) that were stuck for >10 minutes")
                    }
                    #endif
                }
            }

            sqlite3_finalize(statement)
            return resetCount
        }
    }

    // MARK: - Queue Operations

    /// Enqueue data with validation. Returns Bool for backward compatibility.
    func enqueue(streamName: String, data: Data) -> Bool {
        switch enqueueValidated(streamName: streamName, data: data) {
        case .success:
            return true
        case .failure(let error):
            print("❌ Enqueue failed: \(error.localizedDescription)")
            return false
        }
    }

    /// Enqueue data with full validation and detailed error reporting
    func enqueueValidated(streamName: String, data: Data) -> Result<Int64, EnqueueError> {
        // Validate stream name (must be a known StreamType)
        guard StreamType(rawValue: streamName) != nil else {
            return .failure(.invalidStreamName(streamName))
        }

        // Validate non-empty payload
        guard !data.isEmpty else {
            return .failure(.emptyPayload)
        }

        // Validate payload size
        guard data.count <= maxPayloadSize else {
            return .failure(.payloadTooLarge(size: data.count, maxSize: maxPayloadSize))
        }

        // Proactive storage check - check BEFORE write fails
        if let freeSpace = getAvailableStorage(), freeSpace < storageWarningThreshold {
            #if DEBUG
            print("⚠️ Low storage warning: \(freeSpace / 1_000_000) MB available - triggering aggressive cleanup")
            #endif

            // Trigger aggressive cleanup - delete ALL completed events
            _ = cleanupCompletedEventsAggressive()

            // Re-check after cleanup
            if let newFreeSpace = getAvailableStorage(), newFreeSpace < storageCriticalThreshold {
                #if DEBUG
                print("❌ Critical storage: \(newFreeSpace / 1_000_000) MB available - rejecting enqueue")
                #endif
                return .failure(.emergencyStorageFull(available: newFreeSpace))
            }
        }

        return queue.sync {
            // Check backpressure - is queue too full?
            let stats = getQueueStatsInternal()
            if stats.totalSize > maxQueueSizeBytes {
                // Trigger aggressive cleanup
                _ = cleanupOldEventsInternal()

                // Re-check after cleanup
                let newStats = getQueueStatsInternal()
                if newStats.totalSize > maxQueueSizeBytes {
                    return .failure(.queueFull(currentSize: newStats.totalSize, maxSize: maxQueueSizeBytes))
                }
            }

            // Perform the actual insert
            let insertSQL = """
                INSERT INTO upload_queue (stream_name, data_blob, created_at, upload_attempts, status)
                VALUES (?, ?, ?, 0, 'pending')
            """

            var statement: OpaquePointer?

            guard sqlite3_prepare_v2(db, insertSQL, -1, &statement, nil) == SQLITE_OK else {
                let errorMsg = String(cString: sqlite3_errmsg(db))
                return .failure(.storageFailed(reason: errorMsg))
            }

            defer { sqlite3_finalize(statement) }

            // Use NSString for proper memory handling with SQLite
            sqlite3_bind_text(statement, 1, (streamName as NSString).utf8String, -1, nil)
            sqlite3_bind_blob(statement, 2, (data as NSData).bytes, Int32(data.count), nil)
            sqlite3_bind_double(statement, 3, Date().timeIntervalSince1970)

            guard sqlite3_step(statement) == SQLITE_DONE else {
                let errorMsg = String(cString: sqlite3_errmsg(db))
                return .failure(.storageFailed(reason: errorMsg))
            }

            let rowId = sqlite3_last_insert_rowid(db)
            print("✅ Enqueued event for stream: \(streamName) (ID: \(rowId), Size: \(data.count) bytes)")
            return .success(rowId)
        }
    }

    /// Internal version of getQueueStats for use within queue.sync blocks
    private func getQueueStatsInternal() -> (pending: Int, failed: Int, total: Int, totalSize: Int64) {
        var pending = 0
        var failed = 0
        var total = 0
        var totalSize: Int64 = 0

        let statsSQL = """
            SELECT status, COUNT(*) as count, SUM(LENGTH(data_blob)) as size
            FROM upload_queue
            WHERE status IN ('pending', 'failed', 'uploading')
            GROUP BY status
        """

        var statement: OpaquePointer?

        if sqlite3_prepare_v2(db, statsSQL, -1, &statement, nil) == SQLITE_OK {
            while sqlite3_step(statement) == SQLITE_ROW {
                let status = String(cString: sqlite3_column_text(statement, 0))
                let count = Int(sqlite3_column_int(statement, 1))
                let size = sqlite3_column_int64(statement, 2)

                switch status {
                case "pending", "uploading":
                    pending += count
                case "failed":
                    failed += count
                default:
                    break
                }

                total += count
                totalSize += size
            }
        }

        sqlite3_finalize(statement)
        return (pending, failed, total, totalSize)
    }

    /// Internal version of cleanupOldEvents for use within queue.sync blocks
    private func cleanupOldEventsInternal() -> Int {
        var deletedCount = 0

        // Delete completed events older than 3 days
        let deleteSQL = """
            DELETE FROM upload_queue
            WHERE status = 'completed'
            AND created_at < ?
        """

        var statement: OpaquePointer?
        let threeDaysAgo = Date().addingTimeInterval(-3 * 24 * 60 * 60).timeIntervalSince1970

        if sqlite3_prepare_v2(db, deleteSQL, -1, &statement, nil) == SQLITE_OK {
            sqlite3_bind_double(statement, 1, threeDaysAgo)

            if sqlite3_step(statement) == SQLITE_DONE {
                deletedCount = Int(sqlite3_changes(db))
            }
        }

        sqlite3_finalize(statement)

        // Also delete failed events with max retries older than 3 days
        let deleteFailedSQL = """
            DELETE FROM upload_queue
            WHERE status = 'failed'
            AND upload_attempts >= 5
            AND created_at < ?
        """

        if sqlite3_prepare_v2(db, deleteFailedSQL, -1, &statement, nil) == SQLITE_OK {
            sqlite3_bind_double(statement, 1, threeDaysAgo)

            if sqlite3_step(statement) == SQLITE_DONE {
                deletedCount += Int(sqlite3_changes(db))
            }
        }

        sqlite3_finalize(statement)
        return deletedCount
    }
    
    func dequeueNext(limit: Int = 10) -> [UploadEvent] {
        // CRITICAL: Perform SELECT and UPDATE atomically in a single sync block
        // to prevent race conditions where multiple threads could dequeue the same events
        return queue.sync {
            var events: [UploadEvent] = []
            var eventIds: [Int64] = []

            let selectSQL = """
                SELECT id, stream_name, data_blob, created_at, upload_attempts, last_attempt_date, status
                FROM upload_queue
                WHERE status IN ('pending', 'failed')
                AND (upload_attempts < 5 OR upload_attempts IS NULL)
                ORDER BY created_at ASC
                LIMIT ?
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_int(statement, 1, Int32(limit))

                while sqlite3_step(statement) == SQLITE_ROW {
                    let id = sqlite3_column_int64(statement, 0)
                    let streamName: String
                    if let streamNameCStr = sqlite3_column_text(statement, 1) {
                        streamName = String(cString: streamNameCStr)
                    } else {
                        streamName = ""
                    }

                    let blobPointer = sqlite3_column_blob(statement, 2)
                    let blobSize = sqlite3_column_bytes(statement, 2)
                    let dataBlob = blobPointer.map { Data(bytes: $0, count: Int(blobSize)) } ?? Data()

                    let createdAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 3))
                    let uploadAttempts = Int(sqlite3_column_int(statement, 4))

                    let lastAttemptInterval = sqlite3_column_double(statement, 5)
                    let lastAttemptDate = lastAttemptInterval > 0 ? Date(timeIntervalSince1970: lastAttemptInterval) : nil

                    let statusString: String
                    if let statusCStr = sqlite3_column_text(statement, 6) {
                        statusString = String(cString: statusCStr)
                    } else {
                        statusString = "pending"
                    }
                    let status = UploadEvent.UploadStatus(rawValue: statusString) ?? .pending

                    let event = UploadEvent(
                        id: id,
                        streamName: streamName,
                        dataBlob: dataBlob,
                        createdAt: createdAt,
                        uploadAttempts: uploadAttempts,
                        lastAttemptDate: lastAttemptDate,
                        status: status
                    )

                    // Only include if retry delay has passed and stream name is valid
                    if (event.shouldRetry || event.status == .pending) && !streamName.isEmpty {
                        events.append(event)
                        eventIds.append(id)
                        print("📦 Dequeued event id=\(id) stream=\(streamName) status=\(statusString)")
                    } else if streamName.isEmpty {
                        print("⚠️ Skipping event id=\(id) with empty stream name")
                    }
                }
            }

            sqlite3_finalize(statement)

            // Immediately mark selected events as 'uploading' within the same atomic block
            for id in eventIds {
                let updateSQL = """
                    UPDATE upload_queue
                    SET status = 'uploading'
                    WHERE id = ?
                """

                var updateStatement: OpaquePointer?

                if sqlite3_prepare_v2(db, updateSQL, -1, &updateStatement, nil) == SQLITE_OK {
                    sqlite3_bind_int64(updateStatement, 1, id)
                    if sqlite3_step(updateStatement) != SQLITE_DONE {
                        print("⚠️ Failed to mark event \(id) as uploading")
                    }
                }

                sqlite3_finalize(updateStatement)
            }

            return events
        }
    }
    
    func markAsComplete(id: Int64) {
        queue.sync {
            let updateSQL = """
                UPDATE upload_queue
                SET status = 'completed'
                WHERE id = ?
            """
            
            var statement: OpaquePointer?
            
            if sqlite3_prepare_v2(db, updateSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_int64(statement, 1, id)
                sqlite3_step(statement)
            }
            
            sqlite3_finalize(statement)
        }
    }
    
    func incrementRetry(id: Int64) {
        queue.sync {
            let updateSQL = """
                UPDATE upload_queue
                SET status = 'failed',
                    upload_attempts = upload_attempts + 1,
                    last_attempt_date = ?
                WHERE id = ?
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, updateSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, Date().timeIntervalSince1970)
                sqlite3_bind_int64(statement, 2, id)
                sqlite3_step(statement)
            }

            sqlite3_finalize(statement)
        }
    }

    /// Mark an event as permanently failed - sets max retries so it won't be picked up again
    func markAsFailed(id: Int64) {
        queue.sync {
            let updateSQL = """
                UPDATE upload_queue
                SET status = 'failed',
                    upload_attempts = 5,
                    last_attempt_date = ?
                WHERE id = ?
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, updateSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, Date().timeIntervalSince1970)
                sqlite3_bind_int64(statement, 2, id)
                sqlite3_step(statement)
            }

            sqlite3_finalize(statement)
        }
    }
    
    // MARK: - Cleanup Operations
    
    func cleanupBadEvents() -> Int {
        queue.sync {
            var deletedCount = 0
            
            print("🧹 Running cleanup for bad events...")
            
            // Delete events with empty stream names
            let deleteEmptySQL = """
                DELETE FROM upload_queue
                WHERE LENGTH(TRIM(stream_name)) = 0 OR stream_name IS NULL
            """
            
            var statement: OpaquePointer?
            
            if sqlite3_prepare_v2(db, deleteEmptySQL, -1, &statement, nil) == SQLITE_OK {
                if sqlite3_step(statement) == SQLITE_DONE {
                    deletedCount = Int(sqlite3_changes(db))
                    if deletedCount > 0 {
                        print("🗑️ Cleaned up \(deletedCount) events with empty stream names")
                    } else {
                        print("✅ No bad events to clean up")
                    }
                } else {
                    print("❌ Failed to execute cleanup: \(String(cString: sqlite3_errmsg(db)))")
                }
            } else {
                print("❌ Failed to prepare cleanup statement: \(String(cString: sqlite3_errmsg(db)))")
            }
            
            sqlite3_finalize(statement)
            
            return deletedCount
        }
    }
    
    func cleanupOldEvents() -> Int {
        queue.sync {
            var deletedCount = 0
            
            // Delete completed events older than 3 days
            let deleteSQL = """
                DELETE FROM upload_queue
                WHERE status = 'completed'
                AND created_at < ?
            """
            
            var statement: OpaquePointer?
            let threeDaysAgo = Date().addingTimeInterval(-3 * 24 * 60 * 60).timeIntervalSince1970
            
            if sqlite3_prepare_v2(db, deleteSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, threeDaysAgo)
                
                if sqlite3_step(statement) == SQLITE_DONE {
                    deletedCount = Int(sqlite3_changes(db))
                }
            }
            
            sqlite3_finalize(statement)
            
            // Also delete failed events with max retries older than 3 days
            let deleteFailedSQL = """
                DELETE FROM upload_queue
                WHERE status = 'failed'
                AND upload_attempts >= 5
                AND created_at < ?
            """
            
            if sqlite3_prepare_v2(db, deleteFailedSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, threeDaysAgo)
                
                if sqlite3_step(statement) == SQLITE_DONE {
                    deletedCount += Int(sqlite3_changes(db))
                }
            }
            
            sqlite3_finalize(statement)
            
            return deletedCount
        }
    }
    
    // MARK: - Queue Statistics
    
    func getQueueStats() -> (pending: Int, failed: Int, total: Int, totalSize: Int64) {
        queue.sync {
            var pending = 0
            var failed = 0
            var total = 0
            var totalSize: Int64 = 0
            
            let statsSQL = """
                SELECT status, COUNT(*) as count, SUM(LENGTH(data_blob)) as size
                FROM upload_queue
                WHERE status IN ('pending', 'failed', 'uploading')
                GROUP BY status
            """
            
            var statement: OpaquePointer?
            
            if sqlite3_prepare_v2(db, statsSQL, -1, &statement, nil) == SQLITE_OK {
                while sqlite3_step(statement) == SQLITE_ROW {
                    let status = String(cString: sqlite3_column_text(statement, 0))
                    let count = Int(sqlite3_column_int(statement, 1))
                    let size = sqlite3_column_int64(statement, 2)
                    
                    switch status {
                    case "pending", "uploading":
                        pending += count
                    case "failed":
                        failed += count
                    default:
                        break
                    }
                    
                    total += count
                    totalSize += size
                }
            }
            
            sqlite3_finalize(statement)
            
            return (pending, failed, total, totalSize)
        }
    }
    
    func getStreamCounts() -> (healthkit: Int, location: Int, audio: Int) {
        queue.sync {
            var healthkit = 0
            var location = 0
            var audio = 0

            let streamSQL = """
                SELECT stream_name, COUNT(*) as count
                FROM upload_queue
                WHERE status IN ('pending', 'uploading')
                GROUP BY stream_name
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, streamSQL, -1, &statement, nil) == SQLITE_OK {
                while sqlite3_step(statement) == SQLITE_ROW {
                    let streamName = String(cString: sqlite3_column_text(statement, 0))
                    let count = Int(sqlite3_column_int(statement, 1))

                    switch streamName {
                    case "ios_healthkit":
                        healthkit = count
                    case "ios_location":
                        location = count
                    case "ios_mic":
                        audio = count
                    default:
                        break
                    }
                }
            }

            sqlite3_finalize(statement)

            return (healthkit, location, audio)
        }
    }

    /// Fetches recent events for the activity log (most recent first)
    func getRecentEvents(limit: Int = 50) -> [UploadEvent] {
        queue.sync {
            var events: [UploadEvent] = []

            let selectSQL = """
                SELECT id, stream_name, data_blob, created_at, upload_attempts, last_attempt_date, status
                FROM upload_queue
                ORDER BY created_at DESC
                LIMIT ?
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_int(statement, 1, Int32(limit))

                while sqlite3_step(statement) == SQLITE_ROW {
                    let id = sqlite3_column_int64(statement, 0)

                    let streamName: String
                    if let streamNameCStr = sqlite3_column_text(statement, 1) {
                        streamName = String(cString: streamNameCStr)
                    } else {
                        streamName = ""
                    }

                    let blobPointer = sqlite3_column_blob(statement, 2)
                    let blobSize = sqlite3_column_bytes(statement, 2)
                    let dataBlob = blobPointer.map { Data(bytes: $0, count: Int(blobSize)) } ?? Data()

                    let createdAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 3))
                    let uploadAttempts = Int(sqlite3_column_int(statement, 4))

                    let lastAttemptInterval = sqlite3_column_double(statement, 5)
                    let lastAttemptDate = lastAttemptInterval > 0 ? Date(timeIntervalSince1970: lastAttemptInterval) : nil

                    let statusString: String
                    if let statusCStr = sqlite3_column_text(statement, 6) {
                        statusString = String(cString: statusCStr)
                    } else {
                        statusString = "pending"
                    }
                    let status = UploadEvent.UploadStatus(rawValue: statusString) ?? .pending

                    let event = UploadEvent(
                        id: id,
                        streamName: streamName,
                        dataBlob: dataBlob,
                        createdAt: createdAt,
                        uploadAttempts: uploadAttempts,
                        lastAttemptDate: lastAttemptDate,
                        status: status
                    )

                    events.append(event)
                }
            }

            sqlite3_finalize(statement)
            return events
        }
    }
    
    // MARK: - Debug Methods
    
    func debugPrintAllEvents() {
        queue.sync {
            print("🔍 DEBUG: All events in upload_queue:")
            
            let selectSQL = "SELECT id, stream_name, status, upload_attempts, LENGTH(data_blob) as size FROM upload_queue ORDER BY id"
            var statement: OpaquePointer?
            
            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                var count = 0
                while sqlite3_step(statement) == SQLITE_ROW {
                    count += 1
                    let id = sqlite3_column_int64(statement, 0)
                    
                    let streamName: String
                    if let streamNameCStr = sqlite3_column_text(statement, 1) {
                        streamName = String(cString: streamNameCStr)
                    } else {
                        streamName = "(NULL)"
                    }
                    
                    let status: String
                    if let statusCStr = sqlite3_column_text(statement, 2) {
                        status = String(cString: statusCStr)
                    } else {
                        status = "(NULL)"
                    }
                    
                    let attempts = Int(sqlite3_column_int(statement, 3))
                    let size = Int(sqlite3_column_int(statement, 4))
                    
                    print("  ID: \(id), Stream: '\(streamName)', Status: \(status), Attempts: \(attempts), Size: \(size) bytes")
                }
                print("  Total events: \(count)")
            }
            
            sqlite3_finalize(statement)
        }
    }
    
    // MARK: - Storage Management

    func getTotalDatabaseSize() -> Int64 {
        var size: Int64 = 0

        if let attributes = try? FileManager.default.attributesOfItem(atPath: dbPath) {
            size = attributes[.size] as? Int64 ?? 0
        }

        return size
    }

    /// Get available device storage in bytes
    func getAvailableStorage() -> Int64? {
        let fileURL = URL(fileURLWithPath: dbPath).deletingLastPathComponent()
        do {
            let values = try fileURL.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
            return values.volumeAvailableCapacityForImportantUsage
        } catch {
            #if DEBUG
            print("⚠️ Failed to get available storage: \(error)")
            #endif
            return nil
        }
    }

    /// Check if storage is critically low
    func isStorageCritical() -> Bool {
        guard let available = getAvailableStorage() else { return false }
        return available < storageCriticalThreshold
    }

    /// Perform aggressive cleanup - delete ALL completed events
    func cleanupCompletedEventsAggressive() -> Int {
        return queue.sync {
            var deletedCount = 0

            let deleteSQL = "DELETE FROM upload_queue WHERE status = 'completed'"

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, deleteSQL, -1, &statement, nil) == SQLITE_OK {
                if sqlite3_step(statement) == SQLITE_DONE {
                    deletedCount = Int(sqlite3_changes(db))
                    #if DEBUG
                    if deletedCount > 0 {
                        print("🗑️ Aggressive cleanup: deleted \(deletedCount) completed events")
                    }
                    #endif
                }
            }

            sqlite3_finalize(statement)
            return deletedCount
        }
    }

    // MARK: - Speech Timeline

    /// Fetches today's audio chunks with their timestamps for timeline display
    func getTodaysSpeechBlocks() -> [SpeechBlock] {
        return queue.sync {
            var blocks: [SpeechBlock] = []

            let calendar = Calendar.current
            let startOfToday = calendar.startOfDay(for: Date())
            let startTimestamp = startOfToday.timeIntervalSince1970

            let selectSQL = """
                SELECT data_blob FROM upload_queue
                WHERE stream_name = 'ios_mic' AND created_at >= ?
                ORDER BY created_at ASC
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, startTimestamp)

                let dateFormatter = ISO8601DateFormatter()

                while sqlite3_step(statement) == SQLITE_ROW {
                    guard let blobPointer = sqlite3_column_blob(statement, 0) else { continue }
                    let blobSize = sqlite3_column_bytes(statement, 0)
                    let dataBlob = Data(bytes: blobPointer, count: Int(blobSize))

                    // Decode AudioStreamData to extract chunk timestamps
                    if let streamData = try? JSONDecoder().decode(AudioStreamData.self, from: dataBlob) {
                        for chunk in streamData.records {
                            if let startDate = dateFormatter.date(from: chunk.timestampStart),
                               let endDate = dateFormatter.date(from: chunk.timestampEnd) {
                                blocks.append(SpeechBlock(
                                    id: chunk.id,
                                    startTime: startDate,
                                    endTime: endDate,
                                    duration: chunk.duration
                                ))
                            }
                        }
                    }
                }
            }

            sqlite3_finalize(statement)
            return blocks
        }
    }

    // MARK: - Battery Timeline

    /// Fetches today's battery metrics for chart display
    func getTodaysBatteryHistory() -> [BatteryDataPoint] {
        return queue.sync {
            var dataPoints: [BatteryDataPoint] = []

            let calendar = Calendar.current
            let startOfToday = calendar.startOfDay(for: Date())
            let startTimestamp = startOfToday.timeIntervalSince1970

            let selectSQL = """
                SELECT data_blob FROM upload_queue
                WHERE stream_name = 'ios_battery' AND created_at >= ?
                ORDER BY created_at ASC
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, startTimestamp)

                let decoder = JSONDecoder()
                decoder.dateDecodingStrategy = .iso8601

                while sqlite3_step(statement) == SQLITE_ROW {
                    guard let blobPointer = sqlite3_column_blob(statement, 0) else { continue }
                    let blobSize = sqlite3_column_bytes(statement, 0)
                    let dataBlob = Data(bytes: blobPointer, count: Int(blobSize))

                    // Decode BatteryStreamData
                    if let streamData = try? decoder.decode(BatteryStreamData.self, from: dataBlob) {
                        for metric in streamData.metrics {
                            dataPoints.append(BatteryDataPoint(
                                date: metric.timestamp,
                                level: metric.level,
                                isCharging: metric.state == "charging" || metric.state == "full"
                            ))
                        }
                    }
                }
            }

            sqlite3_finalize(statement)
            return dataPoints
        }
    }

    // MARK: - Location Timeline

    /// Fetches today's location coordinates for map display
    func getTodaysLocationTrack() -> [LocationDataPoint] {
        return queue.sync {
            var dataPoints: [LocationDataPoint] = []

            let calendar = Calendar.current
            let startOfToday = calendar.startOfDay(for: Date())
            let startTimestamp = startOfToday.timeIntervalSince1970

            let selectSQL = """
                SELECT data_blob FROM upload_queue
                WHERE stream_name = 'ios_location' AND created_at >= ?
                ORDER BY created_at ASC
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, startTimestamp)

                let dateFormatter = ISO8601DateFormatter()

                while sqlite3_step(statement) == SQLITE_ROW {
                    guard let blobPointer = sqlite3_column_blob(statement, 0) else { continue }
                    let blobSize = sqlite3_column_bytes(statement, 0)
                    let dataBlob = Data(bytes: blobPointer, count: Int(blobSize))

                    // Decode CoreLocationStreamData
                    if let streamData = try? JSONDecoder().decode(CoreLocationStreamData.self, from: dataBlob) {
                        for record in streamData.records {
                            if let date = dateFormatter.date(from: record.timestamp) {
                                dataPoints.append(LocationDataPoint(
                                    date: date,
                                    latitude: record.latitude,
                                    longitude: record.longitude
                                ))
                            }
                        }
                    }
                }
            }

            sqlite3_finalize(statement)
            return dataPoints
        }
    }

    // MARK: - Barometer Timeline

    /// Fetches today's barometer metrics for chart display
    func getTodaysBarometerHistory() -> [BarometerDataPoint] {
        return queue.sync {
            var dataPoints: [BarometerDataPoint] = []

            let calendar = Calendar.current
            let startOfToday = calendar.startOfDay(for: Date())
            let startTimestamp = startOfToday.timeIntervalSince1970

            let selectSQL = """
                SELECT data_blob FROM upload_queue
                WHERE stream_name = 'ios_barometer' AND created_at >= ?
                ORDER BY created_at ASC
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, startTimestamp)

                let decoder = JSONDecoder()
                decoder.dateDecodingStrategy = .iso8601

                while sqlite3_step(statement) == SQLITE_ROW {
                    guard let blobPointer = sqlite3_column_blob(statement, 0) else { continue }
                    let blobSize = sqlite3_column_bytes(statement, 0)
                    let dataBlob = Data(bytes: blobPointer, count: Int(blobSize))

                    // Decode BarometerStreamData
                    if let streamData = try? decoder.decode(BarometerStreamData.self, from: dataBlob) {
                        for metric in streamData.metrics {
                            dataPoints.append(BarometerDataPoint(
                                date: metric.timestamp,
                                pressureKPa: metric.pressureKPa,
                                altitudeMeters: metric.relativeAltitudeMeters
                            ))
                        }
                    }
                }
            }

            sqlite3_finalize(statement)
            return dataPoints
        }
    }

    // MARK: - Contacts Timeline

    /// Fetches today's synced contacts for display
    func getTodaysNewContacts() -> [NewContact] {
        return queue.sync {
            var contacts: [NewContact] = []

            let calendar = Calendar.current
            let startOfToday = calendar.startOfDay(for: Date())
            let startTimestamp = startOfToday.timeIntervalSince1970

            let selectSQL = """
                SELECT data_blob, created_at FROM upload_queue
                WHERE stream_name = 'ios_contacts' AND created_at >= ?
                ORDER BY created_at DESC
                LIMIT 1
            """

            var statement: OpaquePointer?

            if sqlite3_prepare_v2(db, selectSQL, -1, &statement, nil) == SQLITE_OK {
                sqlite3_bind_double(statement, 1, startTimestamp)

                let decoder = JSONDecoder()
                decoder.dateDecodingStrategy = .iso8601

                if sqlite3_step(statement) == SQLITE_ROW {
                    guard let blobPointer = sqlite3_column_blob(statement, 0) else {
                        sqlite3_finalize(statement)
                        return contacts
                    }
                    let blobSize = sqlite3_column_bytes(statement, 0)
                    let dataBlob = Data(bytes: blobPointer, count: Int(blobSize))
                    let syncedAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 1))

                    // Decode ContactsStreamData
                    if let streamData = try? decoder.decode(ContactsStreamData.self, from: dataBlob) {
                        for contact in streamData.contacts {
                            let name = [contact.givenName, contact.familyName]
                                .filter { !$0.isEmpty }
                                .joined(separator: " ")

                            // Skip contacts with no name
                            guard !name.isEmpty else { continue }

                            contacts.append(NewContact(
                                id: contact.identifier,
                                name: name,
                                syncedAt: syncedAt
                            ))
                        }
                    }
                }
            }

            sqlite3_finalize(statement)
            return contacts
        }
    }
}

// MARK: - Speech Block Model

/// Represents a speech recording time block for timeline visualization
struct SpeechBlock: Identifiable {
    let id: String
    let startTime: Date
    let endTime: Date
    let duration: TimeInterval
}

// MARK: - Battery Data Point Model

/// Represents a battery level reading for chart display
struct BatteryDataPoint: Identifiable {
    let id = UUID()
    let date: Date
    let level: Float
    let isCharging: Bool
}

// MARK: - Location Data Point Model

/// Represents a location coordinate for map display
struct LocationDataPoint: Identifiable {
    let id = UUID()
    let date: Date
    let latitude: Double
    let longitude: Double
}

// MARK: - Barometer Data Point Model

/// Represents a barometer reading for chart display
struct BarometerDataPoint: Identifiable {
    let id = UUID()
    let date: Date
    let pressureKPa: Double
    let altitudeMeters: Double
}

// MARK: - New Contact Model

/// Represents a contact synced today for display
struct NewContact: Identifiable {
    let id: String  // contact identifier
    let name: String
    let syncedAt: Date
}