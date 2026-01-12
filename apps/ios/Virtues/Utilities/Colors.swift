//
//  Colors.swift
//  Virtues
//
//  Warm color palette matching the web app design
//

import SwiftUI

// MARK: - Warm Palette Colors

extension Color {
    // Primary / Accent
    static let warmPrimary = Color(hex: "EB5601")
    static let warmPrimaryHover = Color(hex: "FF6B1A")
    static let warmPrimaryActive = Color(hex: "D14A00")
    static let warmPrimarySubtle = Color(hex: "FFF0E6")

    // Backgrounds
    static let warmBackground = Color(hex: "F7F7F4")
    static let warmSurface = Color(hex: "FFFFFF")
    static let warmSurfaceElevated = Color(hex: "F0EFE9")

    // Text / Foreground
    static let warmForeground = Color(hex: "26251E")
    static let warmForegroundMuted = Color(hex: "3D3B33")
    static let warmForegroundSubtle = Color(hex: "78716c")
    static let warmForegroundDisabled = Color(hex: "a8a29e")

    // Borders
    static let warmBorder = Color(hex: "e7e5e4")
    static let warmBorderSubtle = Color(hex: "f0eeeb")
    static let warmBorderStrong = Color(hex: "a8a29e")

    // Status Colors
    static let warmSuccess = Color(hex: "16a34a")
    static let warmSuccessSubtle = Color(hex: "dcfce7")
    static let warmWarning = Color(hex: "ea580c")
    static let warmWarningSubtle = Color(hex: "ffedd5")
    static let warmError = Color(hex: "dc2626")
    static let warmErrorSubtle = Color(hex: "fee2e2")
    static let warmInfo = Color(hex: "0284c7")
    static let warmInfoSubtle = Color(hex: "e0f2fe")

    // Highlight
    static let warmHighlight = Color(hex: "fffbeb")
}

// MARK: - Hex Color Initializer

extension Color {
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let a, r, g, b: UInt64
        switch hex.count {
        case 3: // RGB (12-bit)
            (a, r, g, b) = (255, (int >> 8) * 17, (int >> 4 & 0xF) * 17, (int & 0xF) * 17)
        case 6: // RGB (24-bit)
            (a, r, g, b) = (255, int >> 16, int >> 8 & 0xFF, int & 0xFF)
        case 8: // ARGB (32-bit)
            (a, r, g, b) = (int >> 24, int >> 16 & 0xFF, int >> 8 & 0xFF, int & 0xFF)
        default:
            (a, r, g, b) = (255, 0, 0, 0)
        }
        self.init(
            .sRGB,
            red: Double(r) / 255,
            green: Double(g) / 255,
            blue: Double(b) / 255,
            opacity: Double(a) / 255
        )
    }
}
