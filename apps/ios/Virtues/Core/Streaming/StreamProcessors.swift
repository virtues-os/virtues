//
//  StreamProcessors.swift
//  Virtues
//
//  Concrete stream processor implementations for each data type
//

import Foundation

/// Factory for creating stream processors
enum StreamProcessorFactory {
    /// Create a processor for a given stream name
    static func processor(for streamName: String) -> (any StreamDataProcessor)? {
        switch streamName {
        case "ios_healthkit":
            return HealthKitStreamProcessor()

        case "ios_location":
            return LocationStreamProcessor()

        case "ios_mic":
            return AudioStreamProcessor()

        case "ios_battery":
            return BatteryStreamProcessor()

        case "ios_barometer":
            return BarometerStreamProcessor()

        case "ios_contacts":
            return ContactsStreamProcessor()

        case "ios_finance":
            return FinanceKitStreamProcessor()

        case "ios_eventkit":
            return EventKitStreamProcessor()

        default:
            return nil
        }
    }
}

// MARK: - FinanceKit Stream Processor

struct FinanceKitStreamProcessor: StreamDataProcessor {
    typealias DataType = FinanceKitRecord
    typealias StreamDataType = FinanceKitStreamData

    let streamName = "ios_finance"

    func decode(_ data: Data) throws -> [FinanceKitRecord] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(FinanceKitStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [FinanceKitRecord], deviceId: String) -> FinanceKitStreamData {
        var allAccounts: [FinanceKitAccount] = []
        var allTransactions: [FinanceKitTransaction] = []

        for item in items {
            allAccounts.append(contentsOf: item.accounts)
            allTransactions.append(contentsOf: item.transactions)
        }

        // Deduplicate accounts by ID (keep the latest balance)
        let uniqueAccounts = Array(Dictionary(grouping: allAccounts, by: { $0.id })
            .compactMapValues { $0.last }
            .values)

        return FinanceKitStreamData(deviceId: deviceId, accounts: uniqueAccounts, transactions: allTransactions)
    }
}

// MARK: - HealthKit Stream Processor

struct HealthKitStreamProcessor: StreamDataProcessor {
    typealias DataType = HealthKitMetric
    typealias StreamDataType = HealthKitStreamData

    let streamName = "ios_healthkit"

    func decode(_ data: Data) throws -> [HealthKitMetric] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(HealthKitStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [HealthKitMetric], deviceId: String) -> HealthKitStreamData {
        return HealthKitStreamData(deviceId: deviceId, metrics: items)
    }
}

// MARK: - Location Stream Processor

struct LocationStreamProcessor: StreamDataProcessor {
    typealias DataType = LocationData
    typealias StreamDataType = CoreLocationStreamData

    let streamName = "ios_location"

    func decode(_ data: Data) throws -> [LocationData] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(CoreLocationStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [LocationData], deviceId: String) -> CoreLocationStreamData {
        return CoreLocationStreamData(deviceId: deviceId, locations: items)
    }
}

// MARK: - Audio Stream Processor

struct AudioStreamProcessor: StreamDataProcessor {
    typealias DataType = AudioChunk
    typealias StreamDataType = AudioStreamData

    let streamName = "ios_mic"

    func decode(_ data: Data) throws -> [AudioChunk] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(AudioStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [AudioChunk], deviceId: String) -> AudioStreamData {
        return AudioStreamData(deviceId: deviceId, chunks: items)
    }
}

// MARK: - Battery Stream Processor

struct BatteryStreamProcessor: StreamDataProcessor {
    typealias DataType = BatteryMetric
    typealias StreamDataType = BatteryStreamData

    let streamName = "ios_battery"

    func decode(_ data: Data) throws -> [BatteryMetric] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(BatteryStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [BatteryMetric], deviceId: String) -> BatteryStreamData {
        return BatteryStreamData(deviceId: deviceId, metrics: items)
    }
}

// MARK: - Barometer Stream Processor

struct BarometerStreamProcessor: StreamDataProcessor {
    typealias DataType = BarometerMetric
    typealias StreamDataType = BarometerStreamData

    let streamName = "ios_barometer"

    func decode(_ data: Data) throws -> [BarometerMetric] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(BarometerStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [BarometerMetric], deviceId: String) -> BarometerStreamData {
        return BarometerStreamData(deviceId: deviceId, metrics: items)
    }
}

// MARK: - Contacts Stream Processor

struct ContactsStreamProcessor: StreamDataProcessor {
    typealias DataType = ContactRecord
    typealias StreamDataType = ContactsStreamData

    let streamName = "ios_contacts"

    func decode(_ data: Data) throws -> [ContactRecord] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(ContactsStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [ContactRecord], deviceId: String) -> ContactsStreamData {
        return ContactsStreamData(deviceId: deviceId, syncTimestamp: Date(), contacts: items)
    }
}

// MARK: - EventKit Stream Processor

struct EventKitStreamProcessor: StreamDataProcessor {
    typealias DataType = EventKitRecord
    typealias StreamDataType = EventKitStreamData

    let streamName = "ios_eventkit"

    func decode(_ data: Data) throws -> [EventKitRecord] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let streamData = try decoder.decode(EventKitStreamData.self, from: data)
        return streamData.records
    }

    func combine(_ items: [EventKitRecord], deviceId: String) -> EventKitStreamData {
        var allEvents: [EventKitEvent] = []
        var allReminders: [EventKitReminder] = []

        for item in items {
            allEvents.append(contentsOf: item.events)
            allReminders.append(contentsOf: item.reminders)
        }

        // Deduplicate events and reminders by ID (keep the latest)
        let uniqueEvents = Array(Dictionary(grouping: allEvents, by: { $0.id })
            .compactMapValues { $0.last }
            .values)

        let uniqueReminders = Array(Dictionary(grouping: allReminders, by: { $0.id })
            .compactMapValues { $0.last }
            .values)

        return EventKitStreamData(deviceId: deviceId, events: uniqueEvents, reminders: uniqueReminders)
    }
}
