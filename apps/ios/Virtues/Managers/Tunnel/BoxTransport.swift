//
//  BoxTransport.swift
//  Virtues
//
//  One choke point for every HTTP call to the box. A paired device ALWAYS goes
//  through the in-app WireGuard tunnel — on the LAN and off — and never sends
//  box traffic in cleartext (matching the desktop daemon's posture). WG, keyed
//  by the pinned SPKI server key, is the sole confidentiality + auth boundary;
//  the box serves plain HTTP/1.1 behind it. If the tunnel can't come up the
//  error propagates so the caller queues + retries — there is no plaintext
//  fallback.
//
//  This keeps NetworkManager's request-building unchanged: it builds the same
//  `webhook/{actionId}` URLs and calls `BoxTransport.shared.send(...)` instead
//  of `session.data(for:)`.
//

import Foundation

final class BoxTransport {
    static let shared = BoxTransport()
    private init() {}

    /// Send a request to the box. Returns the same shape as
    /// `URLSession.data(for:)`.
    ///
    /// A **paired** device ALWAYS reaches the box through the WG tunnel — never
    /// in cleartext on the LAN. WireGuard (keyed by the pinned SPKI server key)
    /// is the only confidentiality + authentication boundary; the box serves
    /// plain HTTP behind it. If the tunnel can't be established (e.g. an
    /// IPv4-only network with no route to the box's IPv6 endpoint), the error is
    /// surfaced so the caller queues the data (SQLite) and retries later — we
    /// deliberately do **not** fall back to plaintext.
    ///
    /// The direct `URLSession` path is used only when the device has no tunnel
    /// bundle at all. The only traffic that reaches `BoxTransport` is post-pair
    /// box API calls (uploads, action-id refetch), so this branch is effectively
    /// "a box that never provisioned WG" (dev / no-WG host) — there is no bundle
    /// or long-lived secret to protect over that link yet. (Pairing itself runs
    /// over a separate direct call in `NetworkManager`, gated by the SPKI
    /// fingerprint + TOFU checks.)
    func send(_ request: URLRequest, session: URLSession) async throws -> (Data, HTTPURLResponse) {
        if VirtuesTunnelManager.shared.canBringUp {
            return try await sendViaTunnel(request)
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NetworkError.unknown(NSError(domain: "Invalid response", code: 0))
        }
        return (data, http)
    }

    // MARK: - Tunnel path

    /// Run the blocking FFI exchange off the cooperative pool via a continuation.
    private func sendViaTunnel(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    cont.resume(returning: try Self.tunnelExchange(request))
                } catch {
                    cont.resume(throwing: error)
                }
            }
        }
    }

    /// Blocking: bring up the tunnel, dial the box ULA, write the request, read
    /// and parse the response.
    private static func tunnelExchange(_ request: URLRequest) throws -> (Data, HTTPURLResponse) {
        guard let bundle = VirtuesTunnelManager.shared.boxBundle() else {
            throw TunnelSetupError.notPaired
        }
        let handle = try VirtuesTunnelManager.shared.bringUp()
        let stream = try handle.dial(ip: bundle.internalIp, port: bundle.httpPort)

        let wire = try serializeRequest(request, host: bundle.internalHost)

        // Write the whole request (write accepts everything; loop defensively).
        var offset = 0
        let bytes = [UInt8](wire)
        while offset < bytes.count {
            let n = try stream.write(data: Data(bytes[offset...]))
            if n == 0 { throw NetworkError.unknown(NSError(domain: "tunnel write stalled", code: 0)) }
            offset += Int(n)
        }

        // Read until the server closes (we sent Connection: close).
        var raw = Data()
        while true {
            let chunk = try stream.read(maxLen: 65536)
            if chunk.isEmpty { break }
            raw.append(chunk)
            if raw.count > 64 * 1024 * 1024 { break } // hard cap, defensive
        }

        return try parseResponse(raw, url: request.url)
    }

    /// Build an HTTP/1.1 request. Path+query come from the original URL; the body
    /// and the auth/content headers carry over. Host is the box's internal host.
    private static func serializeRequest(_ request: URLRequest, host: String) throws -> Data {
        guard let url = request.url,
              let comps = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else {
            throw NetworkError.invalidURL
        }
        let method = request.httpMethod ?? "GET"
        var pathQuery = comps.percentEncodedPath.isEmpty ? "/" : comps.percentEncodedPath
        if let q = comps.percentEncodedQuery { pathQuery += "?\(q)" }

        let body = request.httpBody ?? Data()

        var head = "\(method) \(pathQuery) HTTP/1.1\r\n"
        head += "Host: \(host)\r\n"
        for (k, v) in request.allHTTPHeaderFields ?? [:] {
            // Skip hop-by-hop / length headers we set ourselves.
            let lower = k.lowercased()
            if lower == "host" || lower == "content-length" || lower == "connection" { continue }
            head += "\(k): \(v)\r\n"
        }
        head += "Content-Length: \(body.count)\r\n"
        head += "Connection: close\r\n"
        head += "\r\n"

        var out = Data(head.utf8)
        out.append(body)
        return out
    }

    /// Parse a raw HTTP/1.1 response into (body, HTTPURLResponse).
    private static func parseResponse(_ raw: Data, url: URL?) throws -> (Data, HTTPURLResponse) {
        let separator = Data("\r\n\r\n".utf8)
        guard let sep = raw.range(of: separator) else {
            throw NetworkError.unknown(NSError(domain: "malformed response (no header terminator)", code: 0))
        }
        let headerData = raw.subdata(in: raw.startIndex..<sep.lowerBound)
        var body = raw.subdata(in: sep.upperBound..<raw.endIndex)

        let headerText = String(decoding: headerData, as: UTF8.self)
        let lines = headerText.components(separatedBy: "\r\n")
        guard let statusLine = lines.first else {
            throw NetworkError.unknown(NSError(domain: "empty response", code: 0))
        }
        // "HTTP/1.1 200 OK"
        let parts = statusLine.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: true)
        guard parts.count >= 2, let code = Int(parts[1]) else {
            throw NetworkError.unknown(NSError(domain: "bad status line: \(statusLine)", code: 0))
        }

        var headers: [String: String] = [:]
        for line in lines.dropFirst() where !line.isEmpty {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let key = String(line[line.startIndex..<colon]).trimmingCharacters(in: .whitespaces)
            let value = String(line[line.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
            headers[key] = value
        }

        // If Content-Length is present and smaller than what we read, trim.
        if let lenStr = headers["Content-Length"] ?? headers["content-length"],
           let len = Int(lenStr), len <= body.count {
            body = body.subdata(in: body.startIndex..<body.index(body.startIndex, offsetBy: len))
        }

        guard let response = HTTPURLResponse(
            url: url ?? URL(string: "http://box.invalid")!,
            statusCode: code,
            httpVersion: "HTTP/1.1",
            headerFields: headers
        ) else {
            throw NetworkError.unknown(NSError(domain: "could not build HTTPURLResponse", code: 0))
        }
        return (body, response)
    }
}
