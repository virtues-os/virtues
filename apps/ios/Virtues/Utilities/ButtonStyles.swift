//
//  ButtonStyles.swift
//  Virtues
//
//  Custom button styles for consistent UI polish
//

import SwiftUI

// MARK: - Primary Button Style

/// Warm orange primary button with scale animation and haptic feedback
struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundColor(.white)
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(isEnabled ? Color.warmPrimary : Color.warmBorder)
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.7), value: configuration.isPressed)
            .opacity(isEnabled ? 1.0 : 0.6)
            .onChange(of: configuration.isPressed) { _, isPressed in
                if isPressed {
                    Haptics.light()
                }
            }
    }
}

// MARK: - Secondary Button Style

/// Elevated surface button with primary text color
struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .medium))
            .foregroundColor(isEnabled ? .warmForeground : .warmForegroundDisabled)
            .frame(maxWidth: .infinity)
            .frame(height: 48)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(configuration.isPressed ? Color.warmBorder : Color.warmSurfaceElevated)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.warmBorder, lineWidth: 1)
            )
            .scaleEffect(configuration.isPressed ? 0.98 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.7), value: configuration.isPressed)
            .opacity(isEnabled ? 1.0 : 0.6)
            .onChange(of: configuration.isPressed) { _, isPressed in
                if isPressed {
                    Haptics.light()
                }
            }
    }
}

// MARK: - Destructive Button Style

/// Red destructive button for dangerous actions
struct DestructiveButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundColor(.white)
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(configuration.isPressed ? Color.warmError.opacity(0.8) : Color.warmError)
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.7), value: configuration.isPressed)
            .opacity(isEnabled ? 1.0 : 0.6)
            .onChange(of: configuration.isPressed) { _, isPressed in
                if isPressed {
                    Haptics.warning()
                }
            }
    }
}

// MARK: - Ghost Button Style

/// Transparent button with primary text color
struct GhostButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .medium))
            .foregroundColor(isEnabled ? .warmForeground : .warmForegroundDisabled)
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(configuration.isPressed ? Color.warmPrimarySubtle : Color.clear)
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.7), value: configuration.isPressed)
            .opacity(isEnabled ? 1.0 : 0.6)
            .onChange(of: configuration.isPressed) { _, isPressed in
                if isPressed {
                    Haptics.light()
                }
            }
    }
}

// MARK: - Icon Button Style

/// Circular icon button with scale animation
struct IconButtonStyle: ButtonStyle {
    var size: CGFloat = 44

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(width: size, height: size)
            .contentShape(Circle())
            .scaleEffect(configuration.isPressed ? 0.9 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.6), value: configuration.isPressed)
            .onChange(of: configuration.isPressed) { _, isPressed in
                if isPressed {
                    Haptics.light()
                }
            }
    }
}

// MARK: - Row Button Style

/// Subtle button style for list rows
struct RowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(configuration.isPressed ? Color.warmBorder.opacity(0.5) : Color.clear)
            )
            .animation(.easeInOut(duration: 0.15), value: configuration.isPressed)
    }
}

// MARK: - Button Style Extensions

extension ButtonStyle where Self == PrimaryButtonStyle {
    static var primary: PrimaryButtonStyle { PrimaryButtonStyle() }
}

extension ButtonStyle where Self == SecondaryButtonStyle {
    static var secondary: SecondaryButtonStyle { SecondaryButtonStyle() }
}

extension ButtonStyle where Self == DestructiveButtonStyle {
    static var destructive: DestructiveButtonStyle { DestructiveButtonStyle() }
}

extension ButtonStyle where Self == GhostButtonStyle {
    static var ghost: GhostButtonStyle { GhostButtonStyle() }
}

extension ButtonStyle where Self == IconButtonStyle {
    static var icon: IconButtonStyle { IconButtonStyle() }
}

extension ButtonStyle where Self == RowButtonStyle {
    static var row: RowButtonStyle { RowButtonStyle() }
}

// MARK: - Preview

#Preview {
    VStack(spacing: 20) {
        Button("Primary Button") {}
            .buttonStyle(.primary)

        Button("Secondary Button") {}
            .buttonStyle(.secondary)

        Button("Destructive Button") {}
            .buttonStyle(.destructive)

        Button("Ghost Button") {}
            .buttonStyle(.ghost)

        Button {
        } label: {
            Image(systemName: "info.circle")
                .font(.title2)
                .foregroundColor(.warmForegroundMuted)
        }
        .buttonStyle(.icon)

        Button("Disabled Primary") {}
            .buttonStyle(.primary)
            .disabled(true)
    }
    .padding()
}
