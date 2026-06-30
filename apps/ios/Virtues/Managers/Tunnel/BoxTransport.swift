//
//  BoxTransport.swift
//  Virtues
//
//  One choke point for every HTTP call to the box.
//
//  In the relay model the box is reachable at its own HTTPS URL — the blind
//  relay (`https://<boxhash>.boxes.virtues.com`) off-LAN, or the LAN URL on
//  network — and terminates TLS itself with a browser-trusted cert. So box
//  traffic goes over ordinary HTTPS via `URLSession`; there is no in-app tunnel
//  and no plaintext-on-LAN concern (TLS is end-to-end to the box; the relay only
//  ever moves ciphertext). NetworkManager builds requests against the box's
//  `apiEndpoint` and calls `BoxTransport.shared.send(...)`, unchanged.
//
//  NOTE (relay migration): the WireGuard tunnel files under this directory
//  (`VirtuesTunnelManager.swift`, `virtues_tunnel.swift`) and the tunnel UI
//  (`ConnectionSettingsView.swift`) are now unused and should be removed in
//  Xcode. This transport no longer references them.
//

import Foundation

final class BoxTransport {
    static let shared = BoxTransport()
    private init() {}

    /// Send a request to the box over HTTPS. Returns the same shape as
    /// `URLSession.data(for:)`.
    ///
    /// The box terminates TLS with its own browser-trusted cert (obtained via
    /// ACME), so `URLSession`'s default validation against the public CA roots is
    /// the confidentiality + authentication boundary. The relay in the middle is
    /// blind — it only forwards ciphertext.
    func send(_ request: URLRequest, session: URLSession) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NetworkError.unknown(NSError(domain: "Invalid response", code: 0))
        }
        return (data, http)
    }
}
