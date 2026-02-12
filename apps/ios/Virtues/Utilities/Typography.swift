//
//  Typography.swift
//  Virtues
//
//  Typography modifiers for consistent text styling
//  Design: Serif for headings, Sans-serif (system) for body
//

import SwiftUI

// MARK: - Typography View Modifiers

struct H2Style: ViewModifier {
    func body(content: Content) -> some View {
        content
            .font(.system(.title2, design: .serif))
            .fontWeight(.bold)
    }
}

struct H3Style: ViewModifier {
    func body(content: Content) -> some View {
        content
            .font(.system(.headline, design: .serif))
            .fontWeight(.semibold)
    }
}

// MARK: - View Extensions

extension View {
    /// Applies h2 style with serif font (title2, bold)
    func h2Style() -> some View {
        self.modifier(H2Style())
    }

    /// Applies h3 style with serif font (headline, semibold)
    func h3Style() -> some View {
        self.modifier(H3Style())
    }
}
