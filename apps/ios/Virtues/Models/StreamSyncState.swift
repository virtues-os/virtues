//
//  StreamSyncState.swift
//  Virtues
//
//  Per-stream upload outcome — the device-local half of "is this stream
//  actually getting through." `streamCounts` (pending) only tells you the queue
//  depth; this tells you whether the last upload *landed*, when, and whether
//  it's currently failing. Box-side action-run truth is a separate concern
//  (see the runs fetch — Tier 2B).
//

import Foundation

struct StreamSyncState: Codable, Equatable {
    /// Last time an upload of this stream was attempted (success or failure).
    var lastAttempt: Date?
    /// Last time an upload of this stream succeeded (HTTP 2xx from the webhook).
    var lastSuccess: Date?
    /// Consecutive failures since the last success — drives the "failing" state.
    var consecutiveFailures: Int = 0
    /// Short reason for the most recent failure (nil after a success).
    var lastError: String?
    /// Rolling history of the most recent upload outcomes (true = reached the
    /// box), oldest first / newest last, capped at `maxRecentOutcomes`. Drives
    /// the "last X sends" dot strip on the Data tab — a single latest-state flag
    /// hides a flapping connection; this makes intermittent loss visible.
    var recentOutcomes: [Bool] = []

    /// How many outcomes the rolling buffer retains.
    static let maxRecentOutcomes = 8

    /// True once we've seen a success and aren't currently failing.
    var isHealthy: Bool { consecutiveFailures == 0 && lastSuccess != nil }

    init(lastAttempt: Date? = nil,
         lastSuccess: Date? = nil,
         consecutiveFailures: Int = 0,
         lastError: String? = nil,
         recentOutcomes: [Bool] = []) {
        self.lastAttempt = lastAttempt
        self.lastSuccess = lastSuccess
        self.consecutiveFailures = consecutiveFailures
        self.lastError = lastError
        self.recentOutcomes = recentOutcomes
    }

    private enum CodingKeys: String, CodingKey {
        case lastAttempt, lastSuccess, consecutiveFailures, lastError, recentOutcomes
    }

    // Custom decode so adding `recentOutcomes` doesn't invalidate state that was
    // persisted before the field existed (a missing key would otherwise throw
    // and wipe the whole streamSync map on upgrade).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        lastAttempt = try c.decodeIfPresent(Date.self, forKey: .lastAttempt)
        lastSuccess = try c.decodeIfPresent(Date.self, forKey: .lastSuccess)
        consecutiveFailures = try c.decodeIfPresent(Int.self, forKey: .consecutiveFailures) ?? 0
        lastError = try c.decodeIfPresent(String.self, forKey: .lastError)
        recentOutcomes = try c.decodeIfPresent([Bool].self, forKey: .recentOutcomes) ?? []
    }

    /// Append an outcome to the rolling buffer, trimming to the cap.
    mutating func appendOutcome(_ success: Bool) {
        recentOutcomes.append(success)
        if recentOutcomes.count > Self.maxRecentOutcomes {
            recentOutcomes.removeFirst(recentOutcomes.count - Self.maxRecentOutcomes)
        }
    }
}
