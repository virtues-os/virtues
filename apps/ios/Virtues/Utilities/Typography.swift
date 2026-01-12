//
//  Typography.swift
//  Virtues
//
//  Typography modifiers for consistent text styling
//  Design: Serif for headings, Sans-serif (system) for body
//

import SwiftUI

// MARK: - Typography View Modifiers

struct H1Style: ViewModifier {
    func body(content: Content) -> some View {
        content
            .font(.system(.largeTitle, design: .serif))
            .fontWeight(.bold)
    }
}

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

struct BrandStyle: ViewModifier {
    func body(content: Content) -> some View {
        content
            .font(.system(.title, design: .serif))
            .fontWeight(.bold)
    }
}

// MARK: - View Extensions

extension View {
    /// Applies h1 style with serif font (large title, bold)
    func h1Style() -> some View {
        self.modifier(H1Style())
    }

    /// Applies h2 style with serif font (title2, bold)
    func h2Style() -> some View {
        self.modifier(H2Style())
    }

    /// Applies h3 style with serif font (headline, semibold)
    func h3Style() -> some View {
        self.modifier(H3Style())
    }

    /// Applies brand style with serif font (title, bold)
    func brandStyle() -> some View {
        self.modifier(BrandStyle())
    }
}
