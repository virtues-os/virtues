import Foundation

/// One choke point for every HTTP call the collector makes to the box.
///
/// In the iroh model the box has no public URL: it's an iroh `Endpoint` reached
/// by its Ed25519 EndpointId — LAN-direct, hole-punched, or via the relay. This
/// holds a warm `IrohTransport` (the uniffi/Rust client, from
/// VirtuesIrohMac.xcframework), dialed once from the pairing reach ticket
/// (`{boxNodeId, relayUrl}` + this device's iroh seed) and reused. `send()`
/// serializes a `URLRequest` to HTTP/1 bytes over a fresh bi-stream and parses
/// the reply back into `(Data, HTTPURLResponse)`.
///
/// Auth is the transport itself: iroh enforces the box's EndpointId allowlist,
/// so the collector's allowlisted key IS its credential — there is no bearer.
actor BoxTransport {
    static let shared = BoxTransport()
    private init() {}

    private var transport: IrohTransport?
    private var ticket: (boxNodeId: String, relayUrl: String, seed: String)?

    /// Install the reach ticket (call once after `Config.load`). Redials on change.
    func configure(boxNodeId: String, relayUrl: String, seed: String) {
        let changed = ticket?.boxNodeId != boxNodeId
            || ticket?.relayUrl != relayUrl
            || ticket?.seed != seed
        ticket = (boxNodeId, relayUrl, seed)
        if changed { transport = nil }
    }

    /// Send a request to the box over iroh. Same shape as `URLSession.data(for:)`.
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        guard let url = request.url else { throw TransportError.invalidURL }
        let client = try await transportOrDial()
        let reqBytes = try HTTPWire.serialize(request)
        do {
            let respBytes = try await client.request(rawHttp: reqBytes, background: false)
            return try HTTPWire.parseResponse(respBytes, url: url)
        } catch {
            // Drop the cached connection so the next call redials a fresh one
            // (box restart / network change / relay hiccup).
            transport = nil
            throw error
        }
    }

    private func transportOrDial() async throws -> IrohTransport {
        if let t = transport { return t }
        guard let ticket else { throw TransportError.notConfigured }
        let t = try await IrohTransport.dial(
            relayUrl: ticket.relayUrl,
            boxIdHex: ticket.boxNodeId,
            deviceSeedHex: ticket.seed,
            background: false
        )
        transport = t
        return t
    }

    enum TransportError: Error, CustomStringConvertible {
        case invalidURL
        case notConfigured
        var description: String {
            switch self {
            case .invalidURL: return "invalid URL"
            case .notConfigured: return "collector has no iroh reach ticket — run `init`"
            }
        }
    }
}
