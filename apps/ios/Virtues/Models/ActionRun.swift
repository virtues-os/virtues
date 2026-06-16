//
//  ActionRun.swift
//  Virtues
//
//  One row of `app_action_runs` from the box, as returned by
//  `GET /api/devices/actions/{id}/runs`. This is the *server-side* truth about
//  whether an action actually ran and processed the upload — distinct from the
//  device-local "did the POST return 2xx" (StreamSyncState / 2A).
//
//  Decoded with `.convertFromSnakeCase`; dates are RFC3339 strings on the wire
//  (the box's `Timestamp` serializes via `to_rfc3339`). Only the fields the app
//  displays are declared; extra fields the box sends are ignored. All optional
//  so schema drift never breaks decoding.
//

import Foundation

struct ActionRun: Codable, Identifiable {
    let id: String
    let status: String
    let startedAt: String?
    let completedAt: String?
    let createdAt: String?
    let recordsProcessed: Int?
    let error: String?
    let resultSummary: String?

    /// Best timestamp to display, parsed from the RFC3339 strings.
    var timestamp: Date? {
        for s in [completedAt, startedAt, createdAt].compactMap({ $0 }) {
            if let d = Self.parseBoxTimestamp(s) { return d }
        }
        return nil
    }

    /// Tolerant RFC3339 parse: the box's `to_rfc3339()` can carry 1–9 fractional
    /// digits and a `+00:00` (or `Z`) offset; ISO8601DateFormatter only accepts
    /// up to 3 fractional digits, so fall back to stripping the fraction.
    static func parseBoxTimestamp(_ s: String) -> Date? {
        if let d = ISO8601DateFormatter.virtuesPlain.date(from: s) { return d }
        if let d = ISO8601DateFormatter.virtuesFractional.date(from: s) { return d }
        let stripped = s.replacingOccurrences(
            of: #"\.\d+"#, with: "", options: .regularExpression
        )
        return ISO8601DateFormatter.virtuesPlain.date(from: stripped)
    }

    enum Outcome { case success, failure, running, other }

    /// Map the box's run-status strings to a small UI-facing set.
    var outcome: Outcome {
        switch status.lowercased() {
        case "success", "completed", "ok": return .success
        case "error", "failed", "cancelled": return .failure
        case "running", "pending", "queued": return .running
        default: return .other
        }
    }
}

extension ISO8601DateFormatter {
    static let virtuesPlain: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
    static let virtuesFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
