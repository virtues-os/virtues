//
//  EventKitInfoView.swift
//  Virtues
//
//  Info and configuration page for EventKit data stream
//

import SwiftUI
import EventKit

struct EventKitInfoView: View {
    @ObservedObject private var eventKitManager = EventKitManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "events": [{
        "id": "E1A2...",
        "calendarTitle": "Personal",
        "title": "Team Standup",
        "startDate": "2025-02-05T09:00:00Z",
        "endDate": "2025-02-05T09:30:00Z",
        "isAllDay": false,
        "location": "Zoom"
      }],
      "reminders": [{
        "id": "R3B4...",
        "listTitle": "Tasks",
        "title": "Buy groceries",
        "isCompleted": false,
        "priority": 1
      }]
    }
    """

    var body: some View {
        NavigationView {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Header
                    headerSection
                        .padding(.bottom, 8)

                    // Status
                    statusSection

                    // About
                    aboutSection

                    // Configuration
                    configSection

                    // Data Preview
                    dataPreviewSection

                    // Privacy
                    privacySection

                    // Troubleshooting
                    troubleshootingSection
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 20)
            }
            .background(Color.warmBackground)
            .navigationTitle("EventKit")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    // MARK: - Sections

    private var headerSection: some View {
        HStack(spacing: 16) {
            Image(systemName: "calendar")
                .font(.system(size: 44))
                .foregroundColor(.red)

            VStack(alignment: .leading, spacing: 4) {
                Text("EventKit")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Calendar events & reminders")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()
        }
        .padding(.horizontal, 4)
    }

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Status")
                    .font(.headline)
                Spacer()
                StreamStatusBadge(status: currentStatus)
            }

            Divider()

            if let lastSync = eventKitManager.lastSyncDate {
                InfoRow(label: "Last Sync", value: lastSync.formatted(.relative(presentation: .named)))
            }

            InfoRow(label: "Calendar Access", value: eventKitManager.isCalendarAuthorized ? "Granted" : "Not Granted")
            InfoRow(label: "Reminders Access", value: eventKitManager.isRemindersAuthorized ? "Granted" : "Not Granted")
            InfoRow(label: "Pending Records", value: "\(uploadCoordinator.streamCounts.eventkit)")
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if eventKitManager.isSyncing {
            return .syncing
        } else if !eventKitManager.hasAnyPermission {
            return .error("Not authorized")
        } else if eventKitManager.isMonitoring {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream syncs your calendar and reminders, including:")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Calendar event titles and times")
                bulletPoint("Event locations and descriptions")
                bulletPoint("All-day event flags")
                bulletPoint("Reminder titles and due dates")
                bulletPoint("Reminder completion status")
            }
            .padding(.vertical, 4)

            Text("Data is synced every 5 minutes. Calendar events cover 30 days past to 90 days future.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Sync Interval", value: "5 minutes")
                InfoRow(label: "Calendar Range", value: "-30 to +90 days")
                InfoRow(label: "Reminders", value: "Incomplete + last 30 days")
            }

            Text("Configuration is managed by the server.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample EventKit Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Data NOT stored:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Event attendee email addresses")
                bulletPoint("Calendar account credentials")
                bulletPoint("Shared calendar access tokens")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Minimal", valueColor: .warmSuccess)

            Text("EventKit data is read from the local calendar database. No additional network calls are required.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "Calendar Access", isGranted: eventKitManager.isCalendarAuthorized)
            SimplePermissionRow(title: "Reminders Access", isGranted: eventKitManager.isRemindersAuthorized)

            if !eventKitManager.hasAnyPermission {
                Button(action: {
                    Haptics.light()
                    openSettings()
                }) {
                    Label("Open Settings", systemImage: "gear")
                }
                .buttonStyle(.bordered)
                .tint(.warmPrimary)
            }

            Divider()

            Button(action: {
                Haptics.light()
                Task {
                    _ = await eventKitManager.performInitialSync { _ in }
                }
            }) {
                Label("Force Sync Now", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .tint(.warmPrimary)
            .disabled(eventKitManager.isSyncing || !eventKitManager.hasAnyPermission)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("\u{2022} No events: Ensure you have calendars set up in the Calendar app")
                Text("\u{2022} Permission denied: Go to Settings > Privacy > Calendars/Reminders")
                Text("\u{2022} Missing reminders: Both Calendar and Reminders permissions are needed")
            }
            .font(.caption)
            .foregroundColor(.warmForegroundMuted)
        }
    }

    // MARK: - Helpers

    private func bulletPoint(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text("\u{2022}")
            Text(text)
        }
        .font(.subheadline)
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
}

#Preview {
    EventKitInfoView()
}
