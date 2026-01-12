//
//  AudioInfoView.swift
//  Virtues
//
//  Info and configuration page for Audio/Microphone data stream
//

import SwiftUI

struct AudioInfoView: View {
    @ObservedObject private var audioManager = AudioManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "chunk_id": "chunk_001",
      "duration_seconds": 300,
      "sample_rate": 16000,
      "average_db_level": -32.5,
      "is_silent": false,
      "timestamp": "2025-01-15T10:30:00Z"
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

                    // Recording Settings
                    recordingSettingsSection

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
            .navigationTitle("Audio")
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
            Image(systemName: "mic.fill")
                .font(.system(size: 44))
                .foregroundColor(.warmSuccess)

            VStack(alignment: .leading, spacing: 4) {
                Text("Audio Recording")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Microphone input with speech detection")
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

            InfoRow(label: "Pending Chunks", value: "\(uploadCoordinator.streamCounts.audio)")

            if audioManager.isRecording {
                InfoRow(label: "Recording", value: "Active", valueColor: .warmSuccess)

                // Real-time dB level
                HStack {
                    Text("Audio Level")
                        .font(.subheadline)
                    Spacer()
                    Text(dbLevelText)
                        .font(.subheadline)
                        .foregroundColor(dbLevelColor)
                }
            }
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var dbLevelText: String {
        let db = audioManager.currentDbLevel
        if db <= -160 {
            return "Silent"
        } else {
            // Convert from negative dB scale to positive for display
            let displayDb = abs(db)
            return String(format: "%.0f dB", displayDb)
        }
    }

    private var dbLevelColor: Color {
        let db = audioManager.currentDbLevel
        if db > -10 {
            return .warmError  // Very loud
        } else if db > -30 {
            return .warmSuccess  // Good level
        } else {
            return .warmInfo  // Quiet
        }
    }

    private var currentStatus: StreamStatus {
        if !audioManager.hasPermission {
            return .error("Permission denied")
        } else if audioManager.isRecording {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream continuously records audio from your device's microphone and uploads it for server-side transcription.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("How it works:")
                .font(.subheadline)
                .fontWeight(.medium)
                .padding(.top, 4)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Records continuous 5-minute audio chunks")
                bulletPoint("Audio is transcribed to text on the server")
                bulletPoint("Raw audio is deleted after transcription")
                bulletPoint("Average dB level stored for activity analysis")
            }
            .padding(.vertical, 4)
        }
    }

    private var recordingSettingsSection: some View {
        InfoSection(title: "Recording Settings", icon: "speaker.wave.2") {
            InfoRow(label: "Input", value: "iPhone Microphone")

            Divider()

            VStack(spacing: 10) {
                InfoRow(label: "Sample Rate", value: "16 kHz")
                InfoRow(label: "Chunk Duration", value: "5 minutes")
                InfoRow(label: "Format", value: "AAC (16 kbps)")
            }

            Text("Uses the built-in iPhone microphone only. Bluetooth audio devices are not used to avoid conflicts.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample Audio Record", jsonString: sampleJSON)

            Text("Note: The actual audio data is base64 encoded. It is transcribed server-side and the raw audio is not permanently stored.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Privacy measures:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Audio is transcribed to text, raw audio is deleted")
                bulletPoint("Recording indicator always visible when active")
                bulletPoint("You control when recording is enabled")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Moderate", valueColor: .warmWarning)

            Text("Continuous recording uses battery. Disable when not needed.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "Microphone Access", isGranted: audioManager.hasPermission)

            if !audioManager.hasPermission {
                Button(action: {
                    Haptics.light()
                    openSettings()
                }) {
                    Label("Open Settings", systemImage: "mic.circle")
                }
                .buttonStyle(.bordered)
                .tint(.warmPrimary)
            }

            Divider()

            Button(action: {
                Haptics.medium()
                if audioManager.isRecording {
                    audioManager.stopRecording()
                } else {
                    audioManager.startRecording()
                }
            }) {
                Label(
                    audioManager.isRecording ? "Stop Recording" : "Start Recording",
                    systemImage: audioManager.isRecording ? "stop.circle" : "record.circle"
                )
            }
            .buttonStyle(.bordered)
            .tint(audioManager.isRecording ? .warmError : .warmPrimary)
            .disabled(!audioManager.hasPermission)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• No audio: Check microphone permissions")
                Text("• Interrupted: Recording pauses during calls/Siri")
                Text("• App closed: Enable Location for background recording")
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
    AudioInfoView()
}
