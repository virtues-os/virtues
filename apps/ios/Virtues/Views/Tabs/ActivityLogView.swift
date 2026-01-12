//
//  ActivityLogView.swift
//  Virtues
//
//  Activity log showing recent SQLite queue events
//

import SwiftUI
import AVFoundation

/// Timer container for activity log refresh
private class ActivityLogTimerContainer: ObservableObject {
    var timer: ReliableTimer?
    @Published var refreshTrigger: Int = 0

    func start() {
        timer = ReliableTimer.builder()
            .interval(5.0)
            .qos(.utility)
            .handler { [weak self] in
                DispatchQueue.main.async {
                    self?.refreshTrigger += 1
                }
            }
            .build()
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    deinit {
        stop()
    }
}

struct ActivityLogView: View {
    @State private var events: [UploadEvent] = []
    @State private var selectedFilter: StreamFilter = .all
    @State private var selectedEvent: UploadEvent?
    @StateObject private var timerContainer = ActivityLogTimerContainer()

    /// Filter options for stream types
    enum StreamFilter: String, CaseIterable {
        case all = "All"
        case healthKit = "ios_healthkit"
        case location = "ios_location"
        case audio = "ios_mic"
        case battery = "ios_battery"
        case contacts = "ios_contacts"
        case barometer = "ios_barometer"

        var displayName: String {
            switch self {
            case .all: return "All"
            case .healthKit: return "Health"
            case .location: return "Location"
            case .audio: return "Audio"
            case .battery: return "Battery"
            case .contacts: return "Contacts"
            case .barometer: return "Barometer"
            }
        }
    }

    /// Events filtered by the selected stream type
    private var filteredEvents: [UploadEvent] {
        guard selectedFilter != .all else { return events }
        return events.filter { $0.streamName == selectedFilter.rawValue }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Stream filter picker
            filterPicker

            // Event list or empty state
            if filteredEvents.isEmpty {
                emptyState
            } else {
                List(filteredEvents, id: \.id) { event in
                    ActivityEventRow(event: event)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            selectedEvent = event
                        }
                }
                .listStyle(.plain)
                .refreshable { refreshEvents() }
            }
        }
        .onAppear {
            refreshEvents()
            timerContainer.start()
        }
        .onDisappear {
            timerContainer.stop()
        }
        .onChange(of: timerContainer.refreshTrigger) {
            refreshEvents()
        }
        .sheet(item: $selectedEvent) { event in
            EventDetailSheet(event: event)
        }
    }

    private var filterPicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(StreamFilter.allCases, id: \.self) { filter in
                    FilterChip(
                        title: filter.displayName,
                        isSelected: selectedFilter == filter,
                        action: { selectedFilter = filter }
                    )
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
        .scrollBounceBehavior(.basedOnSize)
        .background(Color(.systemBackground))
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "tray")
                .font(.system(size: 48))
                .foregroundColor(.warmForegroundMuted)

            Text(selectedFilter == .all ? "No Activity Yet" : "No \(selectedFilter.displayName) Events")
                .font(.headline)

            Text(selectedFilter == .all
                 ? "Events will appear here as data is collected and uploaded."
                 : "No events for this stream type yet.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func refreshEvents() {
        events = SQLiteManager.shared.getRecentEvents()
    }
}

// MARK: - Filter Chip

struct FilterChip: View {
    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline)
                .fontWeight(isSelected ? .semibold : .regular)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(isSelected ? Color.accentColor : Color(.secondarySystemBackground))
                .foregroundColor(isSelected ? .white : .primary)
                .clipShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Event Detail Sheet

struct EventDetailSheet: View {
    let event: UploadEvent
    @Environment(\.dismiss) private var dismiss
    @State private var audioPlayer: AVAudioPlayer?
    @State private var isPlaying = false

    var body: some View {
        NavigationView {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Header info
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text(streamDisplayName)
                                .font(.title2)
                                .fontWeight(.semibold)
                            Spacer()
                            statusBadge
                        }

                        Text("Created: \(event.createdAt.formatted(.dateTime))")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)

                        Text("Size: \(event.dataSizeString)")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)

                        if event.uploadAttempts > 0 {
                            Text("Upload attempts: \(event.uploadAttempts)")
                                .font(.caption)
                                .foregroundColor(.warmForegroundMuted)
                        }
                    }
                    .padding()
                    .background(Color(.secondarySystemBackground))
                    .cornerRadius(12)

                    // Audio playback for ios_mic events
                    if event.streamName == "ios_mic" {
                        audioPlaybackSection
                    }

                    // JSON content
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Payload")
                            .font(.headline)

                        ScrollView(.horizontal, showsIndicators: true) {
                            Text(prettyJSON)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundColor(.primary)
                                .textSelection(.enabled)
                        }
                        .padding()
                        .background(Color(.tertiarySystemBackground))
                        .cornerRadius(8)
                    }
                }
                .padding()
            }
            .navigationTitle("Event Details")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .onDisappear {
                audioPlayer?.stop()
                audioPlayer = nil
            }
        }
    }

    // MARK: - Audio Playback

    private var audioPlaybackSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Audio")
                .font(.headline)

            Button(action: togglePlayback) {
                HStack {
                    Image(systemName: isPlaying ? "stop.fill" : "play.fill")
                    Text(isPlaying ? "Stop" : "Play Audio")
                }
                .frame(maxWidth: .infinity)
                .padding()
                .background(Color.warmPrimary)
                .foregroundColor(.white)
                .cornerRadius(10)
            }
        }
        .padding()
        .background(Color(.secondarySystemBackground))
        .cornerRadius(12)
    }

    private func togglePlayback() {
        if isPlaying {
            audioPlayer?.stop()
            isPlaying = false
        } else {
            playAudio()
        }
    }

    private func playAudio() {
        // Extract base64 audio from the JSON
        guard let jsonObject = try? JSONSerialization.jsonObject(with: event.dataBlob) as? [String: Any],
              let records = jsonObject["records"] as? [[String: Any]],
              let firstRecord = records.first,
              let base64Audio = firstRecord["audio_data"] as? String,
              let audioData = Data(base64Encoded: base64Audio) else {
            return
        }

        do {
            audioPlayer = try AVAudioPlayer(data: audioData)
            audioPlayer?.play()
            isPlaying = true
        } catch {
            print("Failed to play audio: \(error)")
        }
    }

    private var streamDisplayName: String {
        StreamType(rawValue: event.streamName)?.displayName ?? event.streamName
    }

    private var statusBadge: some View {
        Text(statusLabel)
            .font(.caption)
            .fontWeight(.medium)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(statusColor.opacity(0.2))
            .foregroundColor(statusColor)
            .clipShape(Capsule())
    }

    private var statusLabel: String {
        switch event.status {
        case .pending: return "Pending"
        case .uploading: return "Uploading"
        case .completed: return "Uploaded"
        case .failed: return "Failed"
        }
    }

    private var statusColor: Color {
        switch event.status {
        case .pending: return .warmWarning
        case .uploading: return .warmInfo
        case .completed: return .warmSuccess
        case .failed: return .warmError
        }
    }

    /// Format the data blob as pretty-printed JSON
    private var prettyJSON: String {
        guard !event.dataBlob.isEmpty else {
            return "(empty)"
        }

        do {
            var jsonObject = try JSONSerialization.jsonObject(with: event.dataBlob) as? [String: Any] ?? [:]

            // Summarize audio_data for ios_mic events
            if event.streamName == "ios_mic", let records = jsonObject["records"] as? [[String: Any]] {
                var summarizedRecords: [[String: Any]] = []
                for var record in records {
                    if let audioData = record["audio_data"] as? String {
                        let bytes = Data(base64Encoded: audioData)?.count ?? audioData.count
                        let kb = Double(bytes) / 1024.0
                        record["audio_data"] = String(format: "<%.1f KB AAC audio>", kb)
                    }
                    summarizedRecords.append(record)
                }
                jsonObject["records"] = summarizedRecords
            }

            let prettyData = try JSONSerialization.data(withJSONObject: jsonObject, options: [.prettyPrinted, .sortedKeys])
            return String(data: prettyData, encoding: .utf8) ?? "(unable to decode)"
        } catch {
            // If it's not valid JSON, show raw data as hex or string
            if let str = String(data: event.dataBlob, encoding: .utf8) {
                return str
            }
            return "(binary data: \(event.dataBlob.count) bytes)"
        }
    }
}

// MARK: - Activity Event Row

struct ActivityEventRow: View {
    let event: UploadEvent

    var body: some View {
        HStack(spacing: 12) {
            // Status icon
            statusIcon

            // Stream name and timestamp
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(streamDisplayName)
                        .font(.subheadline)
                        .fontWeight(.medium)

                    if event.status == .failed && event.uploadAttempts > 0 {
                        Text("(\(event.uploadAttempts) attempts)")
                            .font(.caption2)
                            .foregroundColor(.warmError)
                    }
                }

                Text(event.createdAt.formatted(.relative(presentation: .named)))
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()

            // Status label and size
            VStack(alignment: .trailing, spacing: 2) {
                Text(statusLabel)
                    .font(.caption)
                    .fontWeight(.medium)
                    .foregroundColor(statusColor)

                Text(event.dataSizeString)
                    .font(.caption2)
                    .foregroundColor(.warmForegroundMuted)
            }
        }
        .padding(.vertical, 4)
    }

    private var statusIcon: some View {
        Image(systemName: iconName)
            .font(.title2)
            .foregroundColor(statusColor)
            .frame(width: 32)
    }

    private var iconName: String {
        switch event.status {
        case .pending:
            return "clock.fill"
        case .uploading:
            return "arrow.up.circle.fill"
        case .completed:
            return "checkmark.circle.fill"
        case .failed:
            return "exclamationmark.circle.fill"
        }
    }

    private var statusColor: Color {
        switch event.status {
        case .pending:
            return .warmWarning
        case .uploading:
            return .warmInfo
        case .completed:
            return .warmSuccess
        case .failed:
            return .warmError
        }
    }

    private var statusLabel: String {
        switch event.status {
        case .pending:
            return "Pending"
        case .uploading:
            return "Uploading"
        case .completed:
            return "Uploaded"
        case .failed:
            return "Failed"
        }
    }

    private var streamDisplayName: String {
        // Use StreamType enum for type-safe display name lookup
        StreamType(rawValue: event.streamName)?.displayName ?? event.streamName
    }
}

#Preview {
    ActivityLogView()
}
