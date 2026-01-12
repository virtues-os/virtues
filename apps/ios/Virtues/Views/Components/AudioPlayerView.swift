//
//  AudioPlayerView.swift
//  Virtues
//
//  Audio playback component for recorded chunks
//

import SwiftUI
import AVFoundation

/// View model for audio playback
@MainActor
class AudioPlayerViewModel: ObservableObject {
    @Published var isPlaying = false
    @Published var currentTime: TimeInterval = 0
    @Published var duration: TimeInterval = 0
    @Published var isLoading = false
    @Published var error: String?

    private var audioPlayer: AVAudioPlayer?
    private var displayLink: CADisplayLink?
    private var audioData: Data?

    func loadAudio(from base64String: String) {
        isLoading = true
        error = nil

        guard let data = Data(base64Encoded: base64String) else {
            error = "Failed to decode audio data"
            isLoading = false
            return
        }

        audioData = data

        do {
            // Write to temporary file for playback
            let tempURL = FileManager.default.temporaryDirectory
                .appendingPathComponent("playback_\(UUID().uuidString).m4a")
            try data.write(to: tempURL)

            audioPlayer = try AVAudioPlayer(contentsOf: tempURL)
            audioPlayer?.prepareToPlay()
            duration = audioPlayer?.duration ?? 0
            isLoading = false

            // Clean up temp file after a delay
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                try? FileManager.default.removeItem(at: tempURL)
            }
        } catch {
            self.error = "Failed to load audio: \(error.localizedDescription)"
            isLoading = false
        }
    }

    func play() {
        guard let player = audioPlayer else { return }

        do {
            // Configure audio session for playback
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
            try AVAudioSession.sharedInstance().setActive(true)

            player.play()
            isPlaying = true
            startProgressUpdates()
        } catch {
            self.error = "Failed to play: \(error.localizedDescription)"
        }
    }

    func pause() {
        audioPlayer?.pause()
        isPlaying = false
        stopProgressUpdates()
    }

    func seek(to time: TimeInterval) {
        audioPlayer?.currentTime = time
        currentTime = time
    }

    func togglePlayPause() {
        if isPlaying {
            pause()
        } else {
            play()
        }
    }

    private func startProgressUpdates() {
        // Use a timer for progress updates
        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] timer in
            guard let self = self else {
                timer.invalidate()
                return
            }

            Task { @MainActor in
                if let player = self.audioPlayer {
                    self.currentTime = player.currentTime

                    // Check if playback finished
                    if !player.isPlaying && self.isPlaying {
                        self.isPlaying = false
                        self.currentTime = 0
                        timer.invalidate()
                    }
                } else {
                    timer.invalidate()
                }
            }
        }
    }

    private func stopProgressUpdates() {
        // Timer will stop on next iteration
    }

    deinit {
        audioPlayer?.stop()
    }
}

/// Audio player control view
struct AudioPlayerView: View {
    let base64AudioData: String
    let startTime: Date
    let endTime: Date
    let averageDb: Float?

    @StateObject private var viewModel = AudioPlayerViewModel()

    var body: some View {
        VStack(spacing: 12) {
            // Time info
            HStack {
                Text(timeFormatter.string(from: startTime))
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
                Spacer()
                if let db = averageDb {
                    Text(String(format: "%.0f dB", abs(db)))
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
                Spacer()
                Text(durationText)
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }

            // Progress bar
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    // Background
                    RoundedRectangle(cornerRadius: 4)
                        .fill(Color.warmSurface)

                    // Progress
                    RoundedRectangle(cornerRadius: 4)
                        .fill(Color.warmPrimary)
                        .frame(width: geometry.size.width * progressPercent)
                }
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            let percent = value.location.x / geometry.size.width
                            let seekTime = viewModel.duration * Double(max(0, min(1, percent)))
                            viewModel.seek(to: seekTime)
                        }
                )
            }
            .frame(height: 8)

            // Controls
            HStack {
                Text(formatTime(viewModel.currentTime))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundColor(.warmForegroundMuted)

                Spacer()

                // Play/Pause button
                Button(action: {
                    Haptics.light()
                    viewModel.togglePlayPause()
                }) {
                    Image(systemName: viewModel.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                        .font(.title)
                        .foregroundColor(.warmPrimary)
                }
                .disabled(viewModel.isLoading)

                Spacer()

                Text(formatTime(viewModel.duration))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundColor(.warmForegroundMuted)
            }

            // Error message
            if let error = viewModel.error {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.warmError)
            }
        }
        .padding(12)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(8)
        .onAppear {
            viewModel.loadAudio(from: base64AudioData)
        }
    }

    private var progressPercent: CGFloat {
        guard viewModel.duration > 0 else { return 0 }
        return CGFloat(viewModel.currentTime / viewModel.duration)
    }

    private var durationText: String {
        let duration = endTime.timeIntervalSince(startTime)
        return formatDuration(duration)
    }

    private func formatTime(_ time: TimeInterval) -> String {
        let minutes = Int(time) / 60
        let seconds = Int(time) % 60
        return String(format: "%d:%02d", minutes, seconds)
    }

    private func formatDuration(_ duration: TimeInterval) -> String {
        let minutes = Int(duration) / 60
        let seconds = Int(duration) % 60
        if minutes > 0 {
            return "\(minutes)m \(seconds)s"
        } else {
            return "\(seconds)s"
        }
    }

    private var timeFormatter: DateFormatter {
        let formatter = DateFormatter()
        formatter.dateFormat = "h:mm a"
        return formatter
    }
}

/// Compact audio player for list items
struct CompactAudioPlayer: View {
    let base64AudioData: String
    let timestamp: Date
    let duration: TimeInterval
    let averageDb: Float?
    let isSilent: Bool

    @StateObject private var viewModel = AudioPlayerViewModel()

    var body: some View {
        HStack(spacing: 12) {
            // Play button
            Button(action: {
                Haptics.light()
                viewModel.togglePlayPause()
            }) {
                Image(systemName: viewModel.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                    .font(.title2)
                    .foregroundColor(isSilent ? .warmWarning : .warmPrimary)
            }
            .disabled(viewModel.isLoading || isSilent)

            VStack(alignment: .leading, spacing: 4) {
                // Time
                Text(timeFormatter.string(from: timestamp))
                    .font(.subheadline)
                    .fontWeight(.medium)

                HStack(spacing: 8) {
                    // Duration
                    Text(formatDuration(duration))
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)

                    // dB level
                    if let db = averageDb {
                        Text("•")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                        Text(String(format: "%.0f dB", abs(db)))
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                    }

                    if isSilent {
                        Text("•")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                        Text("Silent")
                            .font(.caption)
                            .foregroundColor(.warmWarning)
                    }
                }
            }

            Spacer()

            // Progress indicator when playing
            if viewModel.isPlaying {
                Text(formatTime(viewModel.currentTime))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundColor(.warmPrimary)
            }
        }
        .padding(12)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(8)
        .onAppear {
            if !isSilent {
                viewModel.loadAudio(from: base64AudioData)
            }
        }
    }

    private func formatTime(_ time: TimeInterval) -> String {
        let minutes = Int(time) / 60
        let seconds = Int(time) % 60
        return String(format: "%d:%02d", minutes, seconds)
    }

    private func formatDuration(_ duration: TimeInterval) -> String {
        let minutes = Int(duration) / 60
        let seconds = Int(duration) % 60
        if minutes > 0 {
            return "\(minutes)m \(seconds)s"
        } else {
            return "\(seconds)s"
        }
    }

    private var timeFormatter: DateFormatter {
        let formatter = DateFormatter()
        formatter.dateFormat = "h:mm a"
        return formatter
    }
}

#Preview {
    VStack(spacing: 16) {
        AudioPlayerView(
            base64AudioData: "",
            startTime: Date().addingTimeInterval(-300),
            endTime: Date(),
            averageDb: -32
        )

        CompactAudioPlayer(
            base64AudioData: "",
            timestamp: Date(),
            duration: 300,
            averageDb: -32,
            isSilent: false
        )

        CompactAudioPlayer(
            base64AudioData: "",
            timestamp: Date().addingTimeInterval(-3600),
            duration: 7200,
            averageDb: -55,
            isSilent: true
        )
    }
    .padding()
    .background(Color.warmBackground)
}
