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

    /// True once we've seen a success and aren't currently failing.
    var isHealthy: Bool { consecutiveFailures == 0 && lastSuccess != nil }
}
