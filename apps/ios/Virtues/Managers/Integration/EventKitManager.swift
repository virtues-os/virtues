//
//  EventKitManager.swift
//  Virtues
//
//  Manages EventKit authorization and data collection for Calendar events and Reminders.
//

import Foundation
import EventKit
import Combine

class EventKitManager: ObservableObject {
    static let shared = EventKitManager()

    private let eventStore = EKEventStore()

    @Published var isCalendarAuthorized = false
    @Published var isRemindersAuthorized = false
    @Published var isMonitoring = false
    @Published var lastSyncDate: Date?
    @Published var isSyncing = false

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let lastSyncKey = "com.virtues.eventkit.lastSync"
    private var eventKitTimer: ReliableTimer?

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        loadLastSyncDate()
        checkAuthorizationStatus()

        // Register with centralized health check coordinator
        HealthCheckCoordinator.shared.register(self)
    }

    /// Legacy singleton initializer - uses default dependencies
    private convenience init() {
        self.init(
            configProvider: DeviceManager.shared,
            storageProvider: SQLiteManager.shared,
            dataUploader: BatchUploadCoordinator.shared
        )
    }

    // MARK: - Monitoring Control

    func startMonitoring() {
        guard isCalendarAuthorized || isRemindersAuthorized else {
            print("❌ EventKit not authorized, cannot start monitoring")
            return
        }

        stopMonitoring()

        // Start the 5-minute timer (aligned with sync interval)
        eventKitTimer = ReliableTimer.builder()
            .interval(300.0)  // 5 minutes
            .qos(.utility)    // Lower priority than audio/location
            .handler { [weak self] in
                Task {
                    await self?.collectNewData()
                }
            }
            .build()

        // Fire immediately
        Task {
            await collectNewData()
        }

        isMonitoring = true
        print("📅 Started EventKit monitoring")
    }

    func stopMonitoring() {
        eventKitTimer?.cancel()
        eventKitTimer = nil
        isMonitoring = false
    }

    // MARK: - Authorization

    func requestCalendarAuthorization() async -> Bool {
        do {
            let granted = try await eventStore.requestFullAccessToEvents()
            await MainActor.run {
                self.isCalendarAuthorized = granted
            }
            return granted
        } catch {
            print("❌ Calendar authorization failed: \(error)")
            return false
        }
    }

    func requestRemindersAuthorization() async -> Bool {
        do {
            let granted = try await eventStore.requestFullAccessToReminders()
            await MainActor.run {
                self.isRemindersAuthorized = granted
            }
            return granted
        } catch {
            print("❌ Reminders authorization failed: \(error)")
            return false
        }
    }

    func checkAuthorizationStatus() {
        let calendarStatus = EKEventStore.authorizationStatus(for: .event)
        let remindersStatus = EKEventStore.authorizationStatus(for: .reminder)

        Task { @MainActor in
            self.isCalendarAuthorized = (calendarStatus == .fullAccess)
            self.isRemindersAuthorized = (remindersStatus == .fullAccess)
        }
    }

    var hasAnyPermission: Bool {
        return isCalendarAuthorized || isRemindersAuthorized
    }

    // MARK: - Initial Sync

    func performInitialSync(progressHandler: @escaping (Double) -> Void) async -> Bool {
        guard hasAnyPermission else {
            print("❌ EventKit not authorized for initial sync")
            return false
        }

        await MainActor.run {
            self.isSyncing = true
        }

        defer {
            Task { @MainActor in
                self.isSyncing = false
            }
        }

        let now = Date()
        var allSuccess = true

        print("🏁 Starting EventKit initial sync")

        // Sync calendar events (-30 days to +90 days)
        if isCalendarAuthorized {
            let startDate = Calendar.current.date(byAdding: .day, value: -30, to: now)!
            let endDate = Calendar.current.date(byAdding: .day, value: 90, to: now)!

            do {
                let events = try fetchCalendarEvents(from: startDate, to: endDate)
                if !events.isEmpty {
                    print("📅 Found \(events.count) calendar events. Saving...")
                    let success = await saveEventKitDataToQueue(events: events, reminders: [])
                    if !success { allSuccess = false }
                }
            } catch {
                print("❌ Failed to fetch calendar events: \(error)")
                allSuccess = false
            }

            progressHandler(0.5)
        }

        // Sync reminders (incomplete + completed in last 30 days)
        if isRemindersAuthorized {
            do {
                let reminders = try await fetchReminders()
                if !reminders.isEmpty {
                    print("✅ Found \(reminders.count) reminders. Saving...")
                    let success = await saveEventKitDataToQueue(events: [], reminders: reminders)
                    if !success { allSuccess = false }
                }
            } catch {
                print("❌ Failed to fetch reminders: \(error)")
                allSuccess = false
            }

            progressHandler(1.0)
        }

        if allSuccess {
            saveLastSyncDate(now)
        }

        return allSuccess
    }

    // MARK: - Data Collection

    private func collectNewData() async {
        guard hasAnyPermission else { return }

        let now = Date()

        print("📅 Fetching new EventKit data")

        var events: [EventKitEvent] = []
        var reminders: [EventKitReminder] = []

        // Fetch calendar events (-30 days to +90 days)
        if isCalendarAuthorized {
            let startDate = Calendar.current.date(byAdding: .day, value: -30, to: now)!
            let endDate = Calendar.current.date(byAdding: .day, value: 90, to: now)!

            do {
                events = try fetchCalendarEvents(from: startDate, to: endDate)
            } catch {
                print("❌ Failed to fetch calendar events: \(error)")
            }
        }

        // Fetch reminders
        if isRemindersAuthorized {
            do {
                reminders = try await fetchReminders()
            } catch {
                print("❌ Failed to fetch reminders: \(error)")
            }
        }

        if !events.isEmpty || !reminders.isEmpty {
            print("📅 Found \(events.count) events and \(reminders.count) reminders")
            let success = await saveEventKitDataToQueue(events: events, reminders: reminders)
            if success {
                saveLastSyncDate(now)
            }
        } else {
            print("📅 No EventKit data found")
        }
    }

    private func fetchCalendarEvents(from startDate: Date, to endDate: Date) throws -> [EventKitEvent] {
        let calendars = eventStore.calendars(for: .event)
        let predicate = eventStore.predicateForEvents(withStart: startDate, end: endDate, calendars: calendars)
        let ekEvents = eventStore.events(matching: predicate)

        return ekEvents.map { EventKitEvent(from: $0) }
    }

    private func fetchReminders() async throws -> [EventKitReminder] {
        return try await withCheckedThrowingContinuation { continuation in
            // Fetch all reminders (incomplete + completed)
            let predicate = eventStore.predicateForReminders(in: nil)

            eventStore.fetchReminders(matching: predicate) { reminders in
                guard let reminders = reminders else {
                    continuation.resume(returning: [])
                    return
                }

                // Filter to incomplete + completed in last 30 days
                let thirtyDaysAgo = Calendar.current.date(byAdding: .day, value: -30, to: Date())!
                let filtered = reminders.filter { reminder in
                    if !reminder.isCompleted {
                        return true  // Include all incomplete
                    }
                    // Include completed if within last 30 days
                    if let completionDate = reminder.completionDate {
                        return completionDate >= thirtyDaysAgo
                    }
                    return false
                }

                let mapped = filtered.map { EventKitReminder(from: $0) }
                continuation.resume(returning: mapped)
            }
        }
    }

    private func saveEventKitDataToQueue(events: [EventKitEvent], reminders: [EventKitReminder]) async -> Bool {
        let deviceId = configProvider.deviceId
        let streamData = EventKitStreamData(deviceId: deviceId, events: events, reminders: reminders)

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601

        do {
            let data = try encoder.encode(streamData)
            let success = storageProvider.enqueue(streamName: "ios_eventkit", data: data)
            if success {
                dataUploader.updateUploadStats()
                return true
            }
        } catch {
            print("❌ Failed to encode EventKit data: \(error)")
        }
        return false
    }

    private func loadLastSyncDate() {
        if let timestamp = UserDefaults.standard.object(forKey: lastSyncKey) as? TimeInterval {
            lastSyncDate = Date(timeIntervalSince1970: timestamp)
        }
    }

    private func saveLastSyncDate(_ date: Date) {
        Task { @MainActor in
            lastSyncDate = date
        }
        UserDefaults.standard.set(date.timeIntervalSince1970, forKey: lastSyncKey)
    }
}

// MARK: - Models

/// Wrapper matching the server's IngestRequest schema
struct EventKitStreamData: Codable {
    let source: String
    let stream: String
    let deviceId: String
    let records: [EventKitRecord]
    let timestamp: String
    let checkpoint: String?

    private enum CodingKeys: String, CodingKey {
        case source, stream
        case deviceId = "device_id"
        case records, timestamp, checkpoint
    }

    init(deviceId: String, events: [EventKitEvent], reminders: [EventKitReminder], checkpoint: String? = nil) {
        self.source = "ios"
        self.stream = "eventkit"
        self.deviceId = deviceId
        self.records = [EventKitRecord(events: events, reminders: reminders)]
        self.timestamp = ISO8601DateFormatter().string(from: Date())
        self.checkpoint = checkpoint
    }
}

/// A single record within the EventKit ingest payload
struct EventKitRecord: Codable {
    let events: [EventKitEvent]
    let reminders: [EventKitReminder]
}

struct EventKitEvent: Codable {
    let id: String
    let calendarId: String
    let calendarTitle: String
    let title: String
    let startDate: Date
    let endDate: Date
    let isAllDay: Bool
    let location: String?
    let notes: String?
    let url: String?
    let lastModified: Date?

    init(from event: EKEvent) {
        self.id = event.eventIdentifier ?? UUID().uuidString
        self.calendarId = event.calendar?.calendarIdentifier ?? ""
        self.calendarTitle = event.calendar?.title ?? ""
        self.title = event.title ?? ""
        self.startDate = event.startDate
        self.endDate = event.endDate
        self.isAllDay = event.isAllDay
        self.location = event.location
        self.notes = event.notes
        self.url = event.url?.absoluteString
        self.lastModified = event.lastModifiedDate
    }
}

struct EventKitReminder: Codable {
    let id: String
    let listId: String
    let listTitle: String
    let title: String
    let dueDate: Date?
    let isCompleted: Bool
    let completionDate: Date?
    let priority: Int
    let notes: String?
    let lastModified: Date?

    init(from reminder: EKReminder) {
        self.id = reminder.calendarItemIdentifier
        self.listId = reminder.calendar?.calendarIdentifier ?? ""
        self.listTitle = reminder.calendar?.title ?? ""
        self.title = reminder.title ?? ""
        self.dueDate = reminder.dueDateComponents?.date
        self.isCompleted = reminder.isCompleted
        self.completionDate = reminder.completionDate
        self.priority = reminder.priority  // 0=none, 1-4=high, 5=medium, 6-9=low
        self.notes = reminder.notes
        self.lastModified = reminder.lastModifiedDate
    }
}

// MARK: - HealthCheckable

extension EventKitManager: HealthCheckable {
    var healthCheckName: String { "EventKitManager" }

    func performHealthCheck() -> HealthStatus {
        guard hasAnyPermission else { return .disabled }
        if isMonitoring && eventKitTimer == nil {
            startMonitoring()
            return .unhealthy(reason: "Timer stopped unexpectedly, restarting")
        }
        return .healthy
    }
}
