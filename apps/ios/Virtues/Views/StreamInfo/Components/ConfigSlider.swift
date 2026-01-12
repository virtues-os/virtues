//
//  ConfigSlider.swift
//  Virtues
//
//  Configurable value slider for stream settings
//

import SwiftUI

struct ConfigSlider<V: BinaryFloatingPoint>: View where V.Stride: BinaryFloatingPoint {
    let title: String
    @Binding var value: V
    let range: ClosedRange<V>
    var step: V.Stride? = nil
    var format: String = "%.2f"
    var leftLabel: String? = nil
    var rightLabel: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Spacer()
                Text(String(format: format, Double(value)))
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .monospacedDigit()
            }

            if let step = step {
                Slider(value: $value, in: range, step: step)
            } else {
                Slider(value: $value, in: range)
            }

            if leftLabel != nil || rightLabel != nil {
                HStack {
                    if let left = leftLabel {
                        Text(left)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    Spacer()
                    if let right = rightLabel {
                        Text(right)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
    }
}

struct ConfigStepperInt: View {
    let title: String
    @Binding var value: Int
    let range: ClosedRange<Int>
    var unit: String = ""
    var description: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Stepper(value: $value, in: range) {
                HStack {
                    Text(title)
                        .font(.subheadline)
                        .fontWeight(.medium)
                    Spacer()
                    Text("\(value)\(unit.isEmpty ? "" : " \(unit)")")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                        .monospacedDigit()
                }
            }

            if let desc = description {
                Text(desc)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }
}

#Preview {
    VStack(spacing: 24) {
        ConfigSlider(
            title: "Speech Threshold",
            value: .constant(0.5),
            range: 0.3...0.7,
            leftLabel: "More filtering",
            rightLabel: "Save more audio"
        )

        ConfigStepperInt(
            title: "Minimum Speech Duration",
            value: .constant(3),
            range: 2...5,
            unit: "frames",
            description: "Each frame is ~256ms"
        )
    }
    .padding()
}
