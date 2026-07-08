//
//  BoxTransport.swift
//  Virtues
//
//  One choke point for every HTTP call to the box.
//
//  In the iroh model the box has no public URL: it's an iroh `Endpoint` reached
//  by its Ed25519 EndpointId — LAN-direct, hole-punched, or via our relay. This
//  transport holds a warm `IrohTransport` (the uniffi/Rust client, from
//  VirtuesIroh.xcframework), dialed once from the pairing reach ticket
//  (`{box_node_id, relay_url}` + this device's iroh seed) and reused across the
//  5-minute upload timer and background bursts — a cold dial won't fit the ~30s
//  background budget. `send()` serializes the caller's `URLRequest` to HTTP/1
//  bytes, sends them over a fresh bi-stream, and parses the reply back into
//  `(Data, HTTPURLResponse)`, so NetworkManager and BatchUploadCoordinator are
//  unchanged above this line.
//
//  Auth is layered: iroh enforces the box's EndpointId allowlist at the
//  transport; the app-layer `Authorization: Bearer <token>` remains the
//  authorization keystone on top (unchanged).
//

import Foundation
import UIKit

/// Serializes dialing + guards the one warm connection. An `actor` so concurrent
/// callers (upload timer, health check, action-id refetch) share a single dial
/// and never race on the cached transport.
actor BoxTransport {
    static let shared = BoxTransport()
    private init() {}

    /// The warm iroh client, dialed lazily on first use and reused. Dropped on
    /// any transport error so the next call redials (box restart / network
    /// change / relay hiccup).
    private var transport: IrohTransport?

    /// Send a request to the box over iroh. Returns the same shape as
    /// `URLSession.data(for:)`. The `session` argument is ignored (kept so the
    /// call sites that used to pass a `URLSession` compile unchanged).
    func send(_ request: URLRequest, session: URLSession = .shared) async throws -> (Data, HTTPURLResponse) {
        _ = session
        guard let url = request.url else { throw NetworkError.invalidURL }

        let bg = await Self.isBackground()
        let client = try await transportOrDial(background: bg)
        let reqBytes = try HTTPWire.serialize(request)
        do {
            let respBytes = try await client.request(rawHttp: reqBytes, background: bg)
            return try HTTPWire.parseResponse(respBytes, url: url)
        } catch {
            // Drop the cached connection so the next call redials a fresh one.
            transport = nil
            throw Self.mapTransportError(error)
        }
    }

    /// Force the next `send` to redial — e.g. after the reach ticket changes at
    /// (re)pair, or on explicit reset.
    func reset() {
        transport = nil
    }

    // MARK: - Dialing

    private func transportOrDial(background: Bool) async throws -> IrohTransport {
        if let t = transport { return t }
        guard let ticket = DeviceManager.currentReachTicket() else {
            // Not paired for iroh reach (no ticket) — surface as an auth-ish
            // config error so the UI prompts a (re)pair rather than retrying.
            throw NetworkError.invalidToken
        }
        do {
            let t = try await IrohTransport.dial(
                boxIdHex: ticket.boxNodeId,
                deviceSeedHex: ticket.deviceSeed,
                relayUrl: ticket.relayUrl,
                directAddrs: ticket.directAddrs,
                background: background
            )
            transport = t
            return t
        } catch {
            throw Self.mapTransportError(error)
        }
    }

    /// Are we running in an iOS background task? Determines the dial/request
    /// budget so a cold background wake bails fast instead of being force-killed.
    private static func isBackground() async -> Bool {
        await MainActor.run { UIApplication.shared.applicationState != .active }
    }

    /// Map an iroh/FFI error to the app's `NetworkError` so the upload queue's
    /// existing retry/circuit-breaker logic treats it as a transient network
    /// failure (keep the data, retry next cycle).
    private static func mapTransportError(_ error: Error) -> NetworkError {
        if let ne = error as? NetworkError { return ne }
        return .noConnection
    }
}
