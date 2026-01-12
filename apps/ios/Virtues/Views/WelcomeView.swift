//
//  WelcomeView.swift
//  Virtues
//
//  First-launch welcome screen explaining app functionality
//

import SwiftUI

struct WelcomeView: View {
    let onComplete: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 28) {
                // Header
                VStack(spacing: 8) {
                    Text("Welcome to Virtues")
                        .font(.system(size: 32, weight: .bold, design: .serif))
                        .multilineTextAlignment(.center)

                    Text("Your personal AI memory")
                        .font(.title3)
                        .foregroundColor(.warmForegroundMuted)
                }
                .padding(.top, 50)

                // What is Virtues?
                WelcomeSection(
                    icon: "brain.head.profile",
                    iconColor: .warmPrimary,
                    title: "What is Virtues?"
                ) {
                    Text("Virtues collects personal data from your device to power AI-powered insights about your life.")
                        .font(.subheadline)
                        .foregroundColor(.warmForegroundMuted)

                    Text("Use it standalone to track your day, or sync to a Virtues server for the full AI experience.")
                        .font(.subheadline)
                        .foregroundColor(.warmForegroundMuted)
                        .padding(.top, 4)
                }

                // Location is Essential
                WelcomeSection(
                    icon: "location.fill",
                    iconColor: .warmInfo,
                    title: "Location is Essential"
                ) {
                    VStack(alignment: .leading, spacing: 8) {
                        BulletPoint(text: "Required for background data collection")
                        BulletPoint(text: "Enable \"Always\" for best results")
                        BulletPoint(text: "Without it, data only collects when app is open")
                    }
                }

                // Your Data Streams
                WelcomeSection(
                    icon: "waveform.path.ecg",
                    iconColor: .warmSuccess,
                    title: "Your Data Streams"
                ) {
                    Text("Enable sensors in the Data tab. Each unlocks new AI capabilities:")
                        .font(.subheadline)
                        .foregroundColor(.warmForegroundMuted)
                        .padding(.bottom, 8)

                    VStack(spacing: 10) {
                        DataStreamExample(
                            icon: "location.fill",
                            color: .warmInfo,
                            name: "Location",
                            example: "How many times did I visit the gym?"
                        )
                        DataStreamExample(
                            icon: "heart.fill",
                            color: .warmError,
                            name: "HealthKit",
                            example: "What was my heart rate during the meeting?"
                        )
                        DataStreamExample(
                            icon: "mic.fill",
                            color: .warmSuccess,
                            name: "Audio",
                            example: "What did we discuss at dinner?"
                        )
                        DataStreamExample(
                            icon: "battery.75percent",
                            color: .warmSuccess,
                            name: "Battery",
                            example: "Auto-enabled, no permission needed"
                        )
                    }
                }

                // Server Sync
                WelcomeSection(
                    icon: "server.rack",
                    iconColor: .warmForegroundMuted,
                    title: "Server Sync (Optional)"
                ) {
                    VStack(alignment: .leading, spacing: 8) {
                        BulletPoint(text: "Go to Settings → Connect to Server")
                        BulletPoint(text: "Enter your Virtues server URL")
                        BulletPoint(text: "Device ID is your identifier (no password needed)")
                    }

                    Text("Not required for local tracking!")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                        .italic()
                        .padding(.top, 4)
                }

                Spacer(minLength: 20)

                // CTA Button
                Button(action: {
                    Haptics.medium()
                    onComplete()
                }) {
                    Text("Get Started")
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                        .padding()
                        .background(Color.warmPrimary)
                        .foregroundColor(.white)
                        .cornerRadius(12)
                }
                .padding(.horizontal)
                .padding(.bottom, 40)
            }
            .padding(.horizontal)
        }
        .background(Color.warmBackground)
    }
}

// MARK: - Welcome Section

struct WelcomeSection<Content: View>: View {
    let icon: String
    let iconColor: Color
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.title2)
                    .foregroundColor(iconColor)
                    .frame(width: 28)

                Text(title)
                    .font(.headline)
            }

            content
                .padding(.leading, 38)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

// MARK: - Bullet Point

struct BulletPoint: View {
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text("•")
                .foregroundColor(.warmPrimary)
            Text(text)
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }
}

// MARK: - Data Stream Example

struct DataStreamExample: View {
    let icon: String
    let color: Color
    let name: String
    let example: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .font(.body)
                .foregroundColor(color)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(name)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Text("\"\(example)\"")
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
                    .italic()
            }

            Spacer()
        }
    }
}

#Preview {
    WelcomeView(onComplete: {})
}
