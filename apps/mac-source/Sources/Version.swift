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

    /// `X-Virtues-Client` value for every request to the box. For a collector,
    /// `version` IS the binary's release (there is no separate UI bundle), so
    /// the box's Devices page finally gets to say what build this daemon is —
    /// it read "version unknown" forever because this identity, stamped in CI
    /// since Phase 1, was transmitted to nobody.
    static var clientHeader: String {
        let sha = gitCommit == "unknown" ? "dev" : String(gitCommit.prefix(7))
        return "version=\(current); sha=\(sha)"
    }
}