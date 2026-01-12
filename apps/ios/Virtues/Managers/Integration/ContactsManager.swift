//
//  ContactsManager.swift
//  Virtues
//
//  Manages Contacts authorization and sync
//

import Foundation
import Contacts
import Combine

class ContactsManager: ObservableObject, HealthCheckable {
    static let shared = ContactsManager()

    private let contactStore = CNContactStore()

    // MARK: - HealthCheckable

    var healthCheckName: String { "ContactsManager" }

    func performHealthCheck() -> HealthStatus {
        // Check if syncing is disabled by user
        guard isEnabled else {
            return .disabled
        }

        // Check if we have authorization
        guard isAuthorized else {
            return .unhealthy(reason: "Contacts access not authorized")
        }

        // Check if sync is stale (>48 hours without sync when enabled)
        if let lastSync = lastSyncDate {
            let hoursSinceLastSync = Date().timeIntervalSince(lastSync) / 3600
            if hoursSinceLastSync > 48 {
                // Trigger a sync and report unhealthy
                Task {
                    _ = await performSync()
                }
                return .unhealthy(reason: "Sync overdue (\(Int(hoursSinceLastSync))h since last sync)")
            }
        } else if isEnabled {
            // Enabled but never synced - trigger initial sync
            Task {
                _ = await performSync()
            }
            return .unhealthy(reason: "Initial sync not completed")
        }

        return .healthy
    }

    @Published var isAuthorized = false
    @Published var isEnabled = false  // User toggle for contact syncing
    @Published var lastSyncDate: Date?
    @Published var contactCount: Int = 0
    @Published var isSyncing = false

    private let isEnabledKey = "com.virtues.contacts.isEnabled"

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let lastSyncKey = "com.virtues.contacts.lastSync"
    private let hasRequestedAuthKey = "com.virtues.contacts.hasRequestedAuth"

    private var hasRequestedAuthorization: Bool {
        get { UserDefaults.standard.bool(forKey: hasRequestedAuthKey) }
        set { UserDefaults.standard.set(newValue, forKey: hasRequestedAuthKey) }
    }

    /// Notification observer for contact store changes
    private var contactStoreObserver: NSObjectProtocol?

    // Keys to fetch from contacts
    private let keysToFetch: [CNKeyDescriptor] = [
        CNContactIdentifierKey,
        CNContactGivenNameKey,
        CNContactFamilyNameKey,
        CNContactOrganizationNameKey,
        CNContactPhoneNumbersKey,
        CNContactEmailAddressesKey,
        CNContactBirthdayKey,
        CNContactDatesKey
    ] as [CNKeyDescriptor]

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        loadLastSyncDate()
        loadEnabledState()
        checkAuthorizationStatus()

        // Register for centralized health monitoring
        HealthCheckCoordinator.shared.register(self)

        // Observe contact store changes for real-time incremental sync
        setupContactStoreObserver()
    }

    deinit {
        if let observer = contactStoreObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    // MARK: - Contact Store Observer

    private func setupContactStoreObserver() {
        contactStoreObserver = NotificationCenter.default.addObserver(
            forName: .CNContactStoreDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            guard let self = self, self.isEnabled, self.isAuthorized else { return }
            print("📇 Contact store changed - performing full sync")
            Task {
                _ = await self.performSync()
            }
        }
    }

    // MARK: - Enable/Disable

    func startSyncing() {
        guard isAuthorized else { return }
        DispatchQueue.main.async {
            self.isEnabled = true
        }
        UserDefaults.standard.set(true, forKey: isEnabledKey)
        print("📇 Contacts syncing enabled")
    }

    func stopSyncing() {
        DispatchQueue.main.async {
            self.isEnabled = false
        }
        UserDefaults.standard.set(false, forKey: isEnabledKey)
        print("📇 Contacts syncing disabled")
    }

    private func loadEnabledState() {
        isEnabled = UserDefaults.standard.bool(forKey: isEnabledKey)
    }

    /// Legacy singleton initializer - uses default dependencies
    private convenience init() {
        self.init(
            configProvider: DeviceManager.shared,
            storageProvider: SQLiteManager.shared,
            dataUploader: BatchUploadCoordinator.shared
        )
    }

    // MARK: - Authorization

    func requestAuthorization() async -> Bool {
        do {
            let granted = try await contactStore.requestAccess(for: .contacts)

            hasRequestedAuthorization = true

            await MainActor.run {
                self.isAuthorized = granted
            }

            if granted {
                print("📇 Contacts access granted")
                // Enable syncing and perform initial sync
                startSyncing()
                _ = await performSync()
            } else {
                print("📇 Contacts access denied")
            }

            return granted
        } catch {
            print("📇 Contacts authorization error: \(error)")
            return false
        }
    }

    func checkAuthorizationStatus() {
        let status = CNContactStore.authorizationStatus(for: .contacts)

        switch status {
        case .authorized:
            isAuthorized = true
            print("📇 Contacts: Already authorized")
        case .denied, .restricted:
            isAuthorized = false
            print("📇 Contacts: Access denied/restricted")
        case .notDetermined:
            isAuthorized = false
            print("📇 Contacts: Not yet requested")
        case .limited:
            isAuthorized = true  // Limited access still allows reading
            print("📇 Contacts: Limited access")
        @unknown default:
            isAuthorized = false
        }
    }

    var hasPermission: Bool {
        let status = CNContactStore.authorizationStatus(for: .contacts)
        return status == .authorized || status == .limited
    }

    // MARK: - Sync

    func performSync() async -> Bool {
        guard isAuthorized else {
            print("📇 Cannot sync - not authorized")
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

        print("📇 Starting contacts sync...")

        do {
            let contacts = try await fetchAllContacts()

            await MainActor.run {
                self.contactCount = contacts.count
            }

            print("📇 Fetched \(contacts.count) contacts")

            // Convert to our format
            let records = contacts.map { contact -> ContactRecord in
                ContactRecord(
                    identifier: contact.identifier,
                    givenName: contact.givenName,
                    familyName: contact.familyName,
                    organizationName: contact.organizationName.isEmpty ? nil : contact.organizationName,
                    phones: contact.phoneNumbers.map { phone in
                        ContactPhone(
                            label: CNLabeledValue<CNPhoneNumber>.localizedString(forLabel: phone.label ?? ""),
                            number: phone.value.stringValue
                        )
                    },
                    emails: contact.emailAddresses.map { email in
                        ContactEmail(
                            label: CNLabeledValue<NSString>.localizedString(forLabel: email.label ?? ""),
                            address: email.value as String
                        )
                    },
                    birthday: contact.birthday?.date
                )
            }

            // Save to queue
            let success = await saveContactsToQueue(records)

            if success {
                let now = Date()
                UserDefaults.standard.set(now.timeIntervalSince1970, forKey: lastSyncKey)
                await MainActor.run {
                    self.lastSyncDate = now
                }

                print("✅ Contacts sync completed successfully")
            }

            return success

        } catch {
            print("❌ Contacts sync failed: \(error)")
            return false
        }
    }

    /// Check if sync is needed (>24 hours since last sync)
    func syncIfNeeded() async {
        guard isAuthorized else { return }

        let dayInSeconds: TimeInterval = 24 * 60 * 60

        if let lastSync = lastSyncDate {
            let timeSinceLastSync = Date().timeIntervalSince(lastSync)
            if timeSinceLastSync > dayInSeconds {
                print("📇 Contacts sync needed (last sync: \(Int(timeSinceLastSync / 3600)) hours ago)")
                _ = await performSync()
            } else {
                print("📇 Contacts sync not needed (last sync: \(Int(timeSinceLastSync / 3600)) hours ago)")
            }
        } else {
            print("📇 No previous sync - performing initial sync")
            _ = await performSync()
        }
    }

    // MARK: - Data Fetching

    private func fetchAllContacts() async throws -> [CNContact] {
        return try await withCheckedThrowingContinuation { continuation in
            var contacts: [CNContact] = []

            let request = CNContactFetchRequest(keysToFetch: keysToFetch)
            request.sortOrder = .givenName

            do {
                try contactStore.enumerateContacts(with: request) { contact, _ in
                    contacts.append(contact)
                }
                continuation.resume(returning: contacts)
            } catch {
                continuation.resume(throwing: error)
            }
        }
    }

    // MARK: - Data Persistence

    private func saveContactsToQueue(_ contacts: [ContactRecord]) async -> Bool {
        guard !contacts.isEmpty else {
            print("📇 No contacts to save")
            return true
        }

        let deviceId = configProvider.deviceId

        let streamData = ContactsStreamData(
            deviceId: deviceId,
            syncTimestamp: Date(),
            contacts: contacts
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601

        guard let data = try? encoder.encode(streamData) else {
            print("❌ Failed to encode contacts")
            return false
        }

        let success = storageProvider.enqueue(streamName: "ios_contacts", data: data)

        if success {
            print("✅ Saved \(contacts.count) contacts to SQLite queue (\(data.count) bytes)")
            dataUploader.updateUploadStats()
        } else {
            print("❌ Failed to save contacts to SQLite")
        }

        return success
    }

    // MARK: - Helpers

    private func loadLastSyncDate() {
        if let timestamp = UserDefaults.standard.object(forKey: lastSyncKey) as? TimeInterval {
            lastSyncDate = Date(timeIntervalSince1970: timestamp)
        }
    }
}

// MARK: - Data Models

struct ContactRecord: Codable {
    let identifier: String
    let givenName: String
    let familyName: String
    let organizationName: String?
    let phones: [ContactPhone]
    let emails: [ContactEmail]
    let birthday: Date?
}

struct ContactPhone: Codable {
    let label: String?
    let number: String
}

struct ContactEmail: Codable {
    let label: String?
    let address: String
}

struct ContactsStreamData: Codable {
    let deviceId: String
    let syncTimestamp: Date
    let contacts: [ContactRecord]
}
