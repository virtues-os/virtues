//
//  BatteryInfoView.swift
//  Virtues
//
//  Info and configuration page for Battery data stream
//

import SwiftUI
import UIKit

struct BatteryInfoView: View {
    @ObservedObject private var batteryManager = BatteryManager.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "metrics": [
        {
          "timestamp": "2025-01-15T10:30:00Z",
          "level": 0.85,
          "state": "unplugged",
          "is_low_power_mode": false
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
            .navigationTitle("Battery")
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
            Image(systemName: batteryIcon)
                .font(.system(size: 44))
                .foregroundColor(batteryColor)

            VStack(alignment: .leading, spacing: 4) {
                Text("Battery")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Battery level and charging state")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()
        }
        .padding(.horizontal, 4)
    }

    private var batteryIcon: String {
        switch batteryManager.batteryState {
        case .charging, .full:
            return "battery.100.bolt"
        case .unplugged:
            if batteryManager.batteryLevel > 0.5 {
                return "battery.75"
            } else if batteryManager.batteryLevel > 0.25 {
                return "battery.50"
            } else {
                return "battery.25"
            }
        default:
            return "battery.50"
        }
    }

    private var batteryColor: Color {
        if batteryManager.batteryState == .charging || batteryManager.batteryState == .full {
            return .warmSuccess
        } else if batteryManager.batteryLevel < 0.2 {
            return .warmError
        } else if batteryManager.batteryLevel < 0.5 {
            return .warmWarning
        } else {
            return .warmSuccess
        }
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

            InfoRow(label: "Battery Level", value: "\(Int(batteryManager.batteryLevel * 100))%")
            InfoRow(label: "Charging State", value: stateString)
            InfoRow(label: "Low Power Mode", value: ProcessInfo.processInfo.isLowPowerModeEnabled ? "On" : "Off")
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var stateString: String {
        switch batteryManager.batteryState {
        case .charging: return "Charging"
        case .full: return "Full"
        case .unplugged: return "On Battery"
        case .unknown: return "Unknown"
        @unknown default: return "Unknown"
        }
    }

    private var currentStatus: StreamStatus {
        if batteryManager.isMonitoring {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream tracks your device's battery status to help understand your usage patterns and optimize sync behavior.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("Data stored:")
                .font(.subheadline)
                .fontWeight(.medium)
                .padding(.top, 4)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Battery percentage (0-100%)")
                bulletPoint("Charging state (charging/full/unplugged)")
                bulletPoint("Low Power Mode status")
                bulletPoint("Timestamp")
            }
            .padding(.vertical, 4)

            Text("Battery data is collected every 5 minutes when monitoring is active.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Collection Interval", value: "5 minutes")
                InfoRow(label: "Background Mode", value: "Passive")
            }

            Text("Battery data is collected passively with minimal impact.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample Battery Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Purpose:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Optimize sync timing based on battery level")
                bulletPoint("Reduce activity when battery is low")
                bulletPoint("Track device usage patterns")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Minimal", valueColor: .warmSuccess)

            Text("This stream has virtually no battery impact - it only reads existing system values.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "Battery Monitoring", isGranted: true)

            Text("No special permissions required. Battery monitoring uses standard system APIs.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)

            Divider()

            Button(action: {
                Haptics.medium()
                if batteryManager.isMonitoring {
                    batteryManager.stopMonitoring()
                } else {
                    batteryManager.startMonitoring()
                }
            }) {
                Label(
                    batteryManager.isMonitoring ? "Stop Monitoring" : "Start Monitoring",
                    systemImage: batteryManager.isMonitoring ? "stop.circle" : "play.circle"
                )
            }
            .buttonStyle(.bordered)
            .tint(batteryManager.isMonitoring ? .warmError : .warmPrimary)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• Unknown state: Occurs briefly when battery monitoring initializes")
                Text("• -1 level: Battery level temporarily unavailable")
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
}

#Preview {
    BatteryInfoView()
}
