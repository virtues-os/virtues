//
//  ToggleStyles.swift
//  Virtues
//
//  Custom toggle styles using the warm color palette
//

import SwiftUI

// MARK: - Warm Toggle Style

/// Custom toggle with warmPrimary tint instead of iOS default green
struct WarmToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack {
            configuration.label

            Spacer()

            ZStack {
                // Track
                Capsule()
                    .fill(configuration.isOn ? Color.warmPrimary : Color.warmBorder)
                    .frame(width: 51, height: 31)

                // Thumb
                Circle()
                    .fill(Color.white)
                    .shadow(color: .black.opacity(0.15), radius: 2, x: 0, y: 1)
                    .frame(width: 27, height: 27)
                    .offset(x: configuration.isOn ? 10 : -10)
            }
            .onTapGesture {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) {
                    configuration.isOn.toggle()
                }
                Haptics.light()
            }
        }
    }
}

// MARK: - Compact Toggle Style

/// Smaller toggle for inline use
struct CompactToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack {
            configuration.label

            Spacer()

            ZStack {
                // Track
                Capsule()
                    .fill(configuration.isOn ? Color.warmPrimary : Color.warmBorder)
                    .frame(width: 44, height: 26)

                // Thumb
                Circle()
                    .fill(Color.white)
                    .shadow(color: .black.opacity(0.15), radius: 1.5, x: 0, y: 1)
                    .frame(width: 22, height: 22)
                    .offset(x: configuration.isOn ? 9 : -9)
            }
            .onTapGesture {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) {
                    configuration.isOn.toggle()
                }
                Haptics.light()
            }
        }
    }
}

// MARK: - Toggle Style Extensions

extension ToggleStyle where Self == WarmToggleStyle {
    static var warm: WarmToggleStyle { WarmToggleStyle() }
}

extension ToggleStyle where Self == CompactToggleStyle {
    static var compact: CompactToggleStyle { CompactToggleStyle() }
}

// MARK: - Preview

#Preview {
    VStack(spacing: 30) {
        Toggle("Warm Toggle", isOn: .constant(true))
            .toggleStyle(.warm)

        Toggle("Warm Toggle Off", isOn: .constant(false))
            .toggleStyle(.warm)

        Toggle("Compact Toggle", isOn: .constant(true))
            .toggleStyle(.compact)

        Toggle("System Toggle (for comparison)", isOn: .constant(true))
    }
    .padding()
}
