//
//  FinanceKitManager.swift
//  Virtues
//
//  Manages FinanceKit authorization and data collection for Apple Card, Apple Cash, and Savings.
//

import Foundation
import FinanceKit
import Combine

class FinanceKitManager: ObservableObject {
    static let shared = FinanceKitManager()
    
    private let financeStore = FinanceStore.shared
    
    @Published var isAuthorized = false
    @Published var isMonitoring = false
    @Published var lastSyncDate: Date?
    @Published var isSyncing = false
    
    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader
    
    private let lastSyncKey = "com.virtues.financekit.lastSync"
    private let hasRequestedAuthKey = "com.virtues.financekit.hasRequestedAuth"
    private var financeTimer: ReliableTimer?
    
    private var hasRequestedAuthorization: Bool {
        get { UserDefaults.standard.bool(forKey: hasRequestedAuthKey) }
        set { UserDefaults.standard.set(newValue, forKey: hasRequestedAuthKey) }
    }

    /// Public accessor for UI to check if authorization has been requested
    var hasRequestedFinanceKitAuthorization: Bool {
        hasRequestedAuthorization
    }
    
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
        guard isAuthorized else {
            print("❌ FinanceKit not authorized, cannot start monitoring")
            return
        }
        
        stopMonitoring()
        
        // Start the 5-minute timer (aligned with sync interval)
        financeTimer = ReliableTimer.builder()
            .interval(300.0)  // 5 minutes
            .qos(.userInitiated)
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
        print("💳 Started FinanceKit monitoring")
    }
    
    func stopMonitoring() {
        financeTimer?.cancel()
        financeTimer = nil
        isMonitoring = false
    }
    
    // MARK: - Authorization
    
    func requestAuthorization() async -> Bool {
        do {
            let status = try await financeStore.requestAuthorization()
            hasRequestedAuthorization = true
            
            let authorized = (status == .authorized)
            await MainActor.run {
                self.isAuthorized = authorized
            }
            return authorized
        } catch {
            print("❌ FinanceKit authorization failed: \(error)")
            return false
        }
    }
    
    func checkAuthorizationStatus() {
        Task {
            do {
                let status = try await financeStore.authorizationStatus()
                let authorized = (status == .authorized)
                await MainActor.run {
                    self.isAuthorized = authorized
                }
            } catch {
                print("❌ Failed to check FinanceKit authorization status: \(error)")
            }
        }
    }
    
    // MARK: - Initial Sync
    
    func performInitialSync(progressHandler: @escaping (Double) -> Void) async -> Bool {
        guard isAuthorized else {
            print("❌ FinanceKit not authorized for initial sync")
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
        
        // Initial sync: go back 10 years in yearly chunks
        let yearsToSync = 10
        let now = Date()
        var allSuccess = true
        
        print("🏁 Starting FinanceKit initial sync for \(yearsToSync) years")
        
        // First, fetch and sync accounts
        let accounts = try? await fetchAccounts()
        if let accounts = accounts, !accounts.isEmpty {
            print("💳 Found \(accounts.count) FinanceKit accounts. Saving...")
            _ = await saveFinanceDataToQueue(accounts: accounts, transactions: [])
        }
        
        for yearOffset in 0..<yearsToSync {
            let chunkEndDate = Calendar.current.date(byAdding: .year, value: -yearOffset, to: now)!
            let chunkStartDate = Calendar.current.date(byAdding: .year, value: -1, to: chunkEndDate)!
            
            print("📅 Syncing FinanceKit chunk: \(chunkStartDate) to \(chunkEndDate)")
            
            do {
                let transactions = try await fetchTransactions(from: chunkStartDate, to: chunkEndDate)
                if !transactions.isEmpty {
                    print("💳 Collected \(transactions.count) transactions for chunk. Saving...")
                    let success = await saveFinanceDataToQueue(accounts: [], transactions: transactions)
                    if !success {
                        allSuccess = false
                    }
                }
            } catch {
                print("❌ Failed to fetch FinanceKit transactions for chunk: \(error)")
                allSuccess = false
            }
            
            // Update progress
            let progress = Double(yearOffset + 1) / Double(yearsToSync)
            await MainActor.run {
                progressHandler(progress)
            }
        }
        
        if allSuccess {
            saveLastSyncDate(now)
        }
        
        return allSuccess
    }
    
    // MARK: - Data Collection
    
    private func collectNewData() async {
        guard isAuthorized else { return }
        
        let now = Date()
        let startDate = lastSyncDate ?? Calendar.current.date(byAdding: .day, value: -30, to: now)!
        
        print("💳 Fetching new FinanceKit data since \(startDate)")
        
        do {
            let accounts = try await fetchAccounts()
            let transactions = try await fetchTransactions(from: startDate, to: now)
            
            if !accounts.isEmpty || !transactions.isEmpty {
                print("💳 Found \(accounts.count) accounts and \(transactions.count) new transactions")
                let success = await saveFinanceDataToQueue(accounts: accounts, transactions: transactions)
                if success {
                    saveLastSyncDate(now)
                }
            } else {
                print("💳 No new FinanceKit data found")
            }
        } catch {
            print("❌ Failed to fetch FinanceKit data: \(error)")
        }
    }
    
    private func fetchAccounts() async throws -> [FinanceKitAccount] {
        let query = AccountQuery()
        let accounts = try await financeStore.accounts(query: query)
        return accounts.map { FinanceKitAccount(from: $0) }
    }
    
    private func fetchTransactions(from startDate: Date, to endDate: Date) async throws -> [FinanceKitTransaction] {
        let predicate = #Predicate<Transaction> { transaction in
            transaction.transactionDate >= startDate && transaction.transactionDate <= endDate
        }
        let query = TransactionQuery(sortDescriptors: [], predicate: predicate)

        let transactions = try await financeStore.transactions(query: query)
        return transactions.map { FinanceKitTransaction(from: $0) }
    }
    
    private func saveFinanceDataToQueue(accounts: [FinanceKitAccount], transactions: [FinanceKitTransaction]) async -> Bool {
        let deviceId = configProvider.deviceId
        let streamData = FinanceKitStreamData(deviceId: deviceId, accounts: accounts, transactions: transactions)
        
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        
        do {
            let data = try encoder.encode(streamData)
            let success = storageProvider.enqueue(streamName: "ios_finance", data: data)
            if success {
                dataUploader.updateUploadStats()
                return true
            }
        } catch {
            print("❌ Failed to encode FinanceKit data: \(error)")
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
struct FinanceKitStreamData: Codable {
    let source: String
    let stream: String
    let deviceId: String
    let records: [FinanceKitRecord]
    let timestamp: String
    let checkpoint: String?

    private enum CodingKeys: String, CodingKey {
        case source, stream
        case deviceId = "device_id"
        case records, timestamp, checkpoint
    }

    init(deviceId: String, accounts: [FinanceKitAccount], transactions: [FinanceKitTransaction], checkpoint: String? = nil) {
        self.source = "ios"
        self.stream = "financekit"
        self.deviceId = deviceId
        self.records = [FinanceKitRecord(accounts: accounts, transactions: transactions)]
        self.timestamp = ISO8601DateFormatter().string(from: Date())
        self.checkpoint = checkpoint
    }
}

/// A single record within the FinanceKit ingest payload
struct FinanceKitRecord: Codable {
    let accounts: [FinanceKitAccount]
    let transactions: [FinanceKitTransaction]
}

struct FinanceKitAccount: Codable {
    let id: String
    let name: String
    let institutionName: String
    let accountDescription: String?

    init(from account: Account) {
        self.id = account.id.uuidString
        self.name = account.displayName
        self.institutionName = account.institutionName
        self.accountDescription = account.accountDescription
    }
}

struct FinanceKitTransaction: Codable {
    let id: String
    let accountId: String
    let amount: Double
    let currencyCode: String
    let date: Date
    let postedDate: Date?
    let merchantName: String?
    let creditDebitIndicator: String
    let transactionDescription: String?

    init(from transaction: Transaction) {
        self.id = transaction.id.uuidString
        self.accountId = transaction.accountID.uuidString
        self.amount = Double(truncating: transaction.transactionAmount.amount as NSNumber)
        self.currencyCode = transaction.transactionAmount.currencyCode
        self.date = transaction.transactionDate
        self.postedDate = transaction.postedDate
        self.merchantName = transaction.merchantName
        self.creditDebitIndicator = Self.mapCreditDebitIndicator(transaction.creditDebitIndicator)
        self.transactionDescription = transaction.transactionDescription
    }

    private static func mapCreditDebitIndicator(_ indicator: CreditDebitIndicator) -> String {
        switch indicator {
        case .credit: return "credit"
        case .debit: return "debit"
        @unknown default: return "unknown"
        }
    }
}

// MARK: - HealthCheckable

extension FinanceKitManager: HealthCheckable {
    var healthCheckName: String { "FinanceKitManager" }
    
    func performHealthCheck() -> HealthStatus {
        guard isAuthorized else { return .disabled }
        if isMonitoring && financeTimer == nil {
            startMonitoring()
            return .unhealthy(reason: "Timer stopped unexpectedly, restarting")
        }
        return .healthy
    }
}
