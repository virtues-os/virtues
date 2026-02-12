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

// MARK: - Toggle Style Extensions

extension ToggleStyle where Self == WarmToggleStyle {
    static var warm: WarmToggleStyle { WarmToggleStyle() }
}
