//
//  FinanceKitInfoView.swift
//  Virtues
//
//  Info and configuration page for FinanceKit data stream
//

import SwiftUI
import FinanceKit

struct FinanceKitInfoView: View {
    @ObservedObject private var financeKitManager = FinanceKitManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "accounts": [{
        "id": "F8A2...",
        "name": "Apple Card",
        "institution_name": "Apple"
      }],
      "transactions": [{
        "id": "T9B3...",
        "amount": 42.50,
        "currency_code": "USD",
        "merchant_name": "Coffee Shop",
        "credit_debit_indicator": "debit",
        "date": "2025-01-15T10:30:00Z"
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
            .navigationTitle("FinanceKit")
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
            Image(systemName: "creditcard.fill")
                .font(.system(size: 44))
                .foregroundColor(.green)

            VStack(alignment: .leading, spacing: 4) {
                Text("FinanceKit")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Apple Card & Apple Cash transactions")
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

            if let lastSync = financeKitManager.lastSyncDate {
                InfoRow(label: "Last Sync", value: lastSync.formatted(.relative(presentation: .named)))
            }

            InfoRow(label: "Pending Records", value: "\(uploadCoordinator.streamCounts.finance)")
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if financeKitManager.isSyncing {
            return .syncing
        } else if !financeKitManager.isAuthorized {
            return .error("Not authorized")
        } else if financeKitManager.isMonitoring {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream stores your financial transactions from Apple Wallet, including:")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Apple Card transactions")
                bulletPoint("Apple Cash transfers")
                bulletPoint("Apple Savings account activity")
                bulletPoint("Transaction amounts and dates")
                bulletPoint("Merchant names")
            }
            .padding(.vertical, 4)

            Text("Data is synced every 5 minutes. Initial sync fetches up to 10 years of transaction history.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Sync Interval", value: "5 minutes")
                InfoRow(label: "Initial Lookback", value: "10 years")
            }

            Text("Configuration is managed by the server.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample FinanceKit Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Data NOT stored:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Bank account numbers")
                bulletPoint("Credit card numbers")
                bulletPoint("Authentication credentials")
                bulletPoint("Non-Apple financial accounts")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Minimal", valueColor: .warmSuccess)

            Text("FinanceKit data is collected passively from Apple Wallet. No additional sensors or network calls are required.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "FinanceKit Access", isGranted: financeKitManager.isAuthorized)

            if !financeKitManager.isAuthorized {
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
                    _ = await financeKitManager.performInitialSync { _ in }
                }
            }) {
                Label("Force Sync Now", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .tint(.warmPrimary)
            .disabled(financeKitManager.isSyncing || !financeKitManager.isAuthorized)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• No data: Ensure you have Apple Card, Apple Cash, or Apple Savings set up")
                Text("• Permission denied: FinanceKit requires app entitlement from Apple")
                Text("• Stale data: Force sync or wait for next automatic sync")
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
    FinanceKitInfoView()
}
