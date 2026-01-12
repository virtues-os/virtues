//
//  HealthKitInfoView.swift
//  Virtues
//
//  Info and configuration page for HealthKit data stream
//

import SwiftUI
import HealthKit

struct HealthKitInfoView: View {
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "stream_type": "healthkit",
      "metrics": [
        {
          "type": "heart_rate",
          "value": 72,
          "unit": "count/min",
          "start_date": "2025-01-15T10:30:00Z"
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
            .navigationTitle("HealthKit")
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
            Image(systemName: "heart.fill")
                .font(.system(size: 44))
                .foregroundColor(.warmError)

            VStack(alignment: .leading, spacing: 4) {
                Text("HealthKit")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Health & fitness data from Apple Health")
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

            if let lastSync = healthKitManager.lastSyncDate {
                InfoRow(label: "Last Sync", value: lastSync.formatted(.relative(presentation: .named)))
            }

            InfoRow(label: "Pending Records", value: "\(uploadCoordinator.streamCounts.healthkit)")
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if healthKitManager.isSyncing {
            return .syncing
        } else if !healthKitManager.isAuthorized {
            return .error("Not authorized")
        } else if healthKitManager.isMonitoring {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream stores your health and fitness data from Apple Health, including:")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Heart rate measurements")
                bulletPoint("Step count")
                bulletPoint("Active energy burned")
                bulletPoint("Heart rate variability (HRV)")
                bulletPoint("Walking + running distance")
                bulletPoint("Resting heart rate")
                bulletPoint("Sleep analysis")
            }
            .padding(.vertical, 4)

            Text("Data is synced every 5 minutes using incremental sync to avoid duplicates.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Sync Interval", value: "5 minutes")
                InfoRow(label: "Initial Lookback", value: "90 days")
                InfoRow(label: "Batch Size", value: "1000 samples")
            }

            Text("Configuration is managed by the server.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample HealthKit Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Data NOT stored:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Workout routes or GPS data")
                bulletPoint("Body measurements (weight, height)")
                bulletPoint("Reproductive health data")
                bulletPoint("Medical records or conditions")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Minimal", valueColor: .warmSuccess)

            Text("HealthKit data is collected passively from Apple Health. No additional sensors are used.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "HealthKit Access", isGranted: healthKitManager.isAuthorized)

            if !healthKitManager.isAuthorized {
                Button(action: {
                    Haptics.light()
                    openHealthSettings()
                }) {
                    Label("Open Health Settings", systemImage: "heart.circle")
                }
                .buttonStyle(.bordered)
                .tint(.warmPrimary)
            }

            Divider()

            Button(action: {
                Haptics.light()
                Task {
                    await healthKitManager.performSync()
                }
            }) {
                Label("Force Sync Now", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .tint(.warmPrimary)
            .disabled(healthKitManager.isSyncing || !healthKitManager.isAuthorized)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• No data: Check Health app permissions for Virtues")
                Text("• Stale data: Force sync or wait for next automatic sync")
                Text("• Missing types: Some data types require specific devices")
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

    private func openHealthSettings() {
        if let url = URL(string: "x-apple-health://") {
            UIApplication.shared.open(url)
        }
    }
}

#Preview {
    HealthKitInfoView()
}
