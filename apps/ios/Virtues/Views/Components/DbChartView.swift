//
//  DbChartView.swift
//  Virtues
//
//  Historical audio level visualization using Swift Charts
//

import SwiftUI
import Charts

/// Data point for the dB chart
struct DbSample: Identifiable {
    let id = UUID()
    let timestamp: Date
    let dbLevel: Float
    let isSilent: Bool

    /// Normalized dB for display (0-100 scale)
    var displayLevel: Double {
        if isSilent {
            return 0
        }
        // Convert from -60..0 to 0..100
        let normalized = Double((dbLevel + 60) / 60) * 100
        return max(0, min(100, normalized))
    }
}

/// Historical dB line chart view
struct DbChartView: View {
    let samples: [DbSample]
    let threshold: Float

    /// Optional: callback when user taps a time period
    var onTapTime: ((Date) -> Void)?

    private var normalizedThreshold: Double {
        Double((threshold + 60) / 60) * 100
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Audio Levels")
                .font(.headline)

            if samples.isEmpty {
                emptyState
            } else {
                chart
            }
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "waveform")
                .font(.largeTitle)
                .foregroundColor(.warmForegroundMuted)

            Text("No audio data yet")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("Audio levels will appear here as you record")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .multilineTextAlignment(.center)
        }
        .frame(height: 200)
        .frame(maxWidth: .infinity)
        .background(Color.warmSurface)
        .cornerRadius(12)
    }

    private var chart: some View {
        Chart {
            // dB level line
            ForEach(samples) { sample in
                LineMark(
                    x: .value("Time", sample.timestamp),
                    y: .value("Level", sample.displayLevel)
                )
                .foregroundStyle(
                    sample.isSilent
                        ? Color.warmWarning.opacity(0.5)
                        : Color.warmPrimary
                )
                .interpolationMethod(.catmullRom)
            }

            // Area fill under the line
            ForEach(samples) { sample in
                AreaMark(
                    x: .value("Time", sample.timestamp),
                    y: .value("Level", sample.displayLevel)
                )
                .foregroundStyle(
                    LinearGradient(
                        colors: [
                            sample.isSilent
                                ? Color.warmWarning.opacity(0.3)
                                : Color.warmPrimary.opacity(0.3),
                            Color.clear
                        ],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                .interpolationMethod(.catmullRom)
            }

            // Threshold line
            RuleMark(y: .value("Threshold", normalizedThreshold))
                .foregroundStyle(Color.warmForegroundMuted.opacity(0.5))
                .lineStyle(StrokeStyle(lineWidth: 1, dash: [5, 5]))
                .annotation(position: .trailing, alignment: .leading) {
                    Text("Threshold")
                        .font(.caption2)
                        .foregroundColor(.warmForegroundMuted)
                }
        }
        .chartYScale(domain: 0...100)
        .chartYAxis {
            AxisMarks(position: .leading) { value in
                if let intValue = value.as(Int.self) {
                    AxisValueLabel {
                        // Convert back to dB for display
                        let db = Int((Double(intValue) / 100.0 * 60) - 60)
                        Text("\(abs(db))")
                            .font(.caption2)
                    }
                }
            }
        }
        .chartXAxis {
            AxisMarks { value in
                if let date = value.as(Date.self) {
                    AxisValueLabel {
                        Text(formatTime(date))
                            .font(.caption2)
                    }
                }
            }
        }
        .frame(height: 200)
        .padding(.vertical, 8)
    }

    private func formatTime(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "h:mm a"
        return formatter.string(from: date)
    }
}

/// Mini chart for inline display (no axes, just the line)
struct MiniDbChart: View {
    let samples: [DbSample]

    var body: some View {
        if samples.isEmpty {
            Rectangle()
                .fill(Color.warmSurface)
                .frame(height: 40)
                .cornerRadius(4)
        } else {
            Chart {
                ForEach(samples) { sample in
                    LineMark(
                        x: .value("Time", sample.timestamp),
                        y: .value("Level", sample.displayLevel)
                    )
                    .foregroundStyle(
                        sample.isSilent
                            ? Color.warmWarning
                            : Color.warmPrimary
                    )
                    .interpolationMethod(.catmullRom)
                }
            }
            .chartYScale(domain: 0...100)
            .chartXAxis(.hidden)
            .chartYAxis(.hidden)
            .frame(height: 40)
        }
    }
}

/// Card view wrapper for the dB chart
struct DbChartCard: View {
    let samples: [DbSample]
    let threshold: Float
    var onTapTime: ((Date) -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            DbChartView(
                samples: samples,
                threshold: threshold,
                onTapTime: onTapTime
            )

            // Legend
            HStack(spacing: 16) {
                LegendItem(color: .warmPrimary, label: "Recording")
                LegendItem(color: .warmWarning, label: "Silent")
            }
            .font(.caption)
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

private struct LegendItem: View {
    let color: Color
    let label: String

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
            Text(label)
                .foregroundColor(.warmForegroundMuted)
        }
    }
}

#Preview {
    let now = Date()
    let samples: [DbSample] = (0..<50).map { i in
        DbSample(
            timestamp: now.addingTimeInterval(Double(i) * 60),
            dbLevel: Float.random(in: -50...(-20)),
            isSilent: i > 20 && i < 30
        )
    }

    return ScrollView {
        VStack(spacing: 20) {
            DbChartCard(
                samples: samples,
                threshold: -35
            )

            DbChartCard(
                samples: [],
                threshold: -35
            )

            MiniDbChart(samples: samples)
                .padding()
                .background(Color.warmSurfaceElevated)
                .cornerRadius(8)
        }
        .padding()
    }
    .background(Color.warmBackground)
}
