//
//  ContactsInfoView.swift
//  Virtues
//
//  Info and configuration page for Contacts data stream
//

import SwiftUI

struct ContactsInfoView: View {
    @ObservedObject private var contactsManager = ContactsManager.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "contacts": [
        {
          "given_name": "John",
          "family_name": "Doe",
          "organization": "Acme Corp",
          "phones": ["+1234567890"],
          "emails": ["john@acme.com"]
        }
      ]
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
            .navigationTitle("Contacts")
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
            Image(systemName: "person.crop.circle.fill")
                .font(.system(size: 44))
                .foregroundColor(.warmPrimary)

            VStack(alignment: .leading, spacing: 4) {
                Text("Contacts")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Address book sync for person identification")
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

            if let lastSync = contactsManager.lastSyncDate {
                InfoRow(label: "Last Sync", value: lastSync.formatted(.relative(presentation: .named)))
            }

            InfoRow(label: "Contacts Synced", value: "\(contactsManager.contactCount)")
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if contactsManager.isSyncing {
            return .syncing
        } else if !contactsManager.isAuthorized {
            return .error("Not authorized")
        } else if contactsManager.isEnabled {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream syncs your contacts to help identify people mentioned in your conversations and recordings.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("Data stored:")
                .font(.subheadline)
                .fontWeight(.medium)
                .padding(.top, 4)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("First and last names")
                bulletPoint("Organization/company names")
                bulletPoint("Phone numbers with labels")
                bulletPoint("Email addresses with labels")
                bulletPoint("Birthdays and important dates")
            }
            .padding(.vertical, 4)

            Text("Contacts are synced when changes are detected in the Contacts app.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Sync Mode", value: "Full sync")
                InfoRow(label: "Auto Sync", value: "Every 24 hours")
                InfoRow(label: "Real-time Updates", value: "Enabled")
            }

            Text("Contacts automatically sync when changes are detected.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample Contact Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Data NOT stored:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Contact photos")
                bulletPoint("Physical addresses")
                bulletPoint("Personal notes")
                bulletPoint("Social media profiles")
                bulletPoint("Relationships/linked contacts")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Minimal", valueColor: .warmSuccess)

            Text("Contacts sync only when changes occur. No continuous background activity.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "Contacts Access", isGranted: contactsManager.isAuthorized)

            if !contactsManager.isAuthorized {
                Button(action: {
                    Haptics.light()
                    openSettings()
                }) {
                    Label("Open Settings", systemImage: "person.crop.circle")
                }
                .buttonStyle(.bordered)
                .tint(.warmPrimary)
            }

            Divider()

            Button(action: {
                Haptics.light()
                Task {
                    await contactsManager.performSync()
                }
            }) {
                Label("Force Sync Now", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .tint(.warmPrimary)
            .disabled(contactsManager.isSyncing || !contactsManager.isAuthorized)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• No contacts: Check Contacts permissions")
                Text("• Missing contacts: Some may be in iCloud-only accounts")
                Text("• Stale data: Force sync to refresh")
            }
            .font(.caption)
            .foregroundColor(.warmForegroundMuted)
        }
    }

    // MARK: - Helpers

    private func bulletPoint(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text("•")
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
    ContactsInfoView()
}
