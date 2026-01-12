//
//  CollapsibleSection.swift
//  Virtues
//
//  UI components for stream info pages
//

import SwiftUI

/// Static section header for wiki-style info pages
struct SectionHeader: View {
    let title: String
    var icon: String? = nil

    var body: some View {
        HStack(spacing: 10) {
            if let icon = icon {
                Image(systemName: icon)
                    .font(.system(size: 16, weight: .medium))
                    .foregroundColor(.warmPrimary)
            }

            Text(title)
                .font(.title3)
                .fontWeight(.semibold)
        }
    }
}

/// Card-style section wrapper for wiki-style info pages
struct InfoSection<Content: View>: View {
    let title: String
    var icon: String? = nil
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            SectionHeader(title: title, icon: icon)
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

struct CollapsibleSection<Content: View>: View {
    let title: String
    var icon: String? = nil
    @ViewBuilder let content: () -> Content

    @State private var isExpanded: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button(action: {
                withAnimation(.easeInOut(duration: 0.2)) {
                    isExpanded.toggle()
                }
            }) {
                HStack {
                    if let icon = icon {
                        Image(systemName: icon)
                            .foregroundColor(.warmPrimary)
                            .frame(width: 24)
                    }

                    Text(title)
                        .font(.subheadline)
                        .fontWeight(.medium)
                        .foregroundColor(.warmForeground)

                    Spacer()

                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                }
                .padding(.vertical, 12)
            }
            .buttonStyle(PlainButtonStyle())

            if isExpanded {
                VStack(alignment: .leading, spacing: 12) {
                    content()
                }
                .padding(.leading, icon != nil ? 24 : 0)
                .padding(.bottom, 12)
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }
}

/// Pre-styled row for use in collapsible sections
struct InfoRow: View {
    let label: String
    let value: String
    var valueColor: Color = .warmForegroundMuted

    var body: some View {
        HStack {
            Text(label)
                .font(.subheadline)
            Spacer()
            Text(value)
                .font(.subheadline)
                .foregroundColor(valueColor)
        }
    }
}

/// Simple permission status row for info views (distinct from OnboardingView's PermissionRow)
struct SimplePermissionRow: View {
    let title: String
    let isGranted: Bool

    var body: some View {
        HStack {
            Text(title)
                .font(.subheadline)
            Spacer()
            HStack(spacing: 4) {
                Image(systemName: isGranted ? "checkmark.circle.fill" : "xmark.circle.fill")
                Text(isGranted ? "Granted" : "Denied")
            }
            .font(.subheadline)
            .foregroundColor(isGranted ? .warmSuccess : .warmError)
        }
    }
}

#Preview {
    VStack(spacing: 0) {
        CollapsibleSection(title: "About This Stream", icon: "info.circle") {
            Text("This stream collects heart rate data from Apple Health.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }

        Divider()

        CollapsibleSection(title: "Configuration", icon: "slider.horizontal.3") {
            InfoRow(label: "Sync Interval", value: "5 minutes")
            InfoRow(label: "Lookback Days", value: "90 days")
        }

        Divider()

        CollapsibleSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "HealthKit Access", isGranted: true)
            SimplePermissionRow(title: "Background Refresh", isGranted: false)

            Button(action: {}) {
                Label("Force Sync Now", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
        }
    }
    .padding()
}
