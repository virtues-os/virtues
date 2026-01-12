//
//  VUMeterView.swift
//  Virtues
//
//  Real-time audio level visualization
//

import SwiftUI

/// A VU (Volume Unit) meter that displays real-time audio levels
struct VUMeterView: View {
    /// Current dB level (AVAudioRecorder scale: -160 to 0)
    let dbLevel: Float

    /// Orientation of the meter
    var orientation: Orientation = .horizontal

    enum Orientation {
        case horizontal
        case vertical
    }

    // Normalize dB to 0-1 range for display
    // AVAudioRecorder returns -160 (silence) to 0 (max)
    private var normalizedLevel: CGFloat {
        let clamped = max(min(dbLevel, 0), -60)  // Clamp to -60 to 0 range
        return CGFloat((clamped + 60) / 60)  // Convert to 0-1
    }

    private var meterColor: Color {
        if dbLevel > -10 {
            return .warmError  // Very loud / clipping
        } else if dbLevel > -30 {
            return .warmSuccess  // Good level
        } else {
            return .warmInfo  // Quiet but audible
        }
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: orientation == .horizontal ? .leading : .bottom) {
                // Background track
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color.warmSurface)

                // Level indicator
                RoundedRectangle(cornerRadius: 4)
                    .fill(meterColor)
                    .frame(
                        width: orientation == .horizontal
                            ? geometry.size.width * normalizedLevel
                            : geometry.size.width,
                        height: orientation == .vertical
                            ? geometry.size.height * normalizedLevel
                            : geometry.size.height
                    )
                    .animation(.easeOut(duration: 0.1), value: normalizedLevel)
            }
        }
    }
}

/// Compact VU meter with label for inline display
struct LabeledVUMeter: View {
    @ObservedObject var audioManager: AudioManager

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Audio Level")
                    .font(.subheadline)
                    .fontWeight(.medium)

                Spacer()

                Text(dbDisplayText)
                    .font(.caption)
                    .fontWeight(.medium)
                    .foregroundColor(levelColor)
                    .monospacedDigit()
            }

            VUMeterView(dbLevel: audioManager.currentDbLevel)
                .frame(height: 12)

            HStack {
                Text("Silent")
                    .font(.caption2)
                    .foregroundColor(.warmForegroundMuted)
                Spacer()
                Text("Loud")
                    .font(.caption2)
                    .foregroundColor(.warmForegroundMuted)
            }
        }
        .padding(12)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(8)
    }

    private var dbDisplayText: String {
        let db = audioManager.currentDbLevel
        if db <= -160 {
            return "Silent"
        } else {
            return String(format: "%.0f dB", abs(db))
        }
    }

    private var levelColor: Color {
        if audioManager.currentDbLevel > -10 {
            return .warmError
        } else if audioManager.currentDbLevel > -30 {
            return .warmSuccess
        } else {
            return .warmInfo
        }
    }
}

#Preview {
    VStack(spacing: 20) {
        VUMeterView(dbLevel: -30)
            .frame(height: 20)
            .padding()

        VUMeterView(dbLevel: -50)
            .frame(height: 20)
            .padding()

        LabeledVUMeter(audioManager: AudioManager.shared)
            .padding()
    }
    .background(Color.warmBackground)
}
