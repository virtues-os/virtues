// Version information for virtues-mac
struct Version {
    static let current = "1.0.0"
    static let buildDate = "2025-12-03"
    static let gitCommit = "unknown" // Will be set during CI build

    static var full: String {
        return "\(current) (\(buildDate))"
    }

    static var userAgent: String {
        return "virtues-mac/\(current)"
    }
}