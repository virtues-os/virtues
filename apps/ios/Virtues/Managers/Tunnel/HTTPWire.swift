//
//  HTTPWire.swift
//  Virtues
//
//  Serialize a `URLRequest` to raw HTTP/1.1 bytes and parse a raw HTTP/1.1
//  response back into `(Data, HTTPURLResponse)`, so the app's existing
//  URLSession-shaped call sites can ride over an iroh bi-stream unchanged.
//
//  The box serves each iroh bi-stream as a normal hyper HTTP/1 connection, so we
//  speak plain HTTP/1.1 over the stream. We send `Connection: close` — the box
//  then finishes its send half after the response, which is the EOF the Rust
//  client reads to. This path carries the RPC-style calls that go through
//  `BoxTransport` (uploads, action-ids, health); it does not stream (no SSE/WS).
//

import Foundation

enum HTTPWireError: Error, CustomStringConvertible {
    case noURL
    case malformedResponse(String)

    var description: String {
        switch self {
        case .noURL: return "request has no URL"
        case .malformedResponse(let why): return "malformed HTTP response: \(why)"
        }
    }
}

enum HTTPWire {
    /// Serialize a `URLRequest` into origin-form HTTP/1.1 request bytes.
    static func serialize(_ request: URLRequest) throws -> Data {
        guard let url = request.url else { throw HTTPWireError.noURL }
        let method = request.httpMethod ?? "GET"

        // origin-form target: path + optional query. Default to "/".
        let comps = URLComponents(url: url, resolvingAgainstBaseURL: false)
        var target = comps?.percentEncodedPath ?? url.path
        if target.isEmpty { target = "/" }
        if let q = comps?.percentEncodedQuery, !q.isEmpty { target += "?\(q)" }

        // Host header (required for HTTP/1.1). Include a non-default port.
        var host = url.host ?? "box"
        if let port = url.port {
            let isDefault = (url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443)
            if !isDefault { host += ":\(port)" }
        }

        var head = "\(method) \(target) HTTP/1.1\r\n"
        head += "Host: \(host)\r\n"

        // Caller-supplied headers (Authorization, Content-Type, …). We set
        // Host / Content-Length / Connection ourselves, so skip any dup.
        let reserved: Set<String> = ["host", "content-length", "connection"]
        for (name, value) in request.allHTTPHeaderFields ?? [:] {
            if reserved.contains(name.lowercased()) { continue }
            head += "\(name): \(value)\r\n"
        }

        // Stamp this app's build identity so the box records it on this device's
        // row (shown on the Devices page — update-manifold Phase 1). Only when
        // the caller hasn't set it explicitly.
        if request.value(forHTTPHeaderField: "X-Virtues-Client") == nil {
            head += "X-Virtues-Client: \(AppBuild.clientHeader)\r\n"
        }

        let body = request.httpBody ?? Data()
        head += "Content-Length: \(body.count)\r\n"
        head += "Connection: close\r\n"
        head += "\r\n"

        var out = Data(head.utf8)
        out.append(body)
        return out
    }

    /// Parse raw HTTP/1.1 response bytes into `(body, HTTPURLResponse)`.
    static func parseResponse(_ bytes: Data, url: URL) throws -> (Data, HTTPURLResponse) {
        // Split on the first CRLFCRLF between headers and body.
        let sep = Data([0x0d, 0x0a, 0x0d, 0x0a])
        guard let range = bytes.range(of: sep) else {
            throw HTTPWireError.malformedResponse("no header/body separator")
        }
        let headerData = bytes.subdata(in: bytes.startIndex..<range.lowerBound)
        let bodyStart = range.upperBound
        var body = bytes.subdata(in: bodyStart..<bytes.endIndex)

        guard let headerText = String(data: headerData, encoding: .utf8) else {
            throw HTTPWireError.malformedResponse("headers not UTF-8")
        }
        // Header lines are CRLF-delimited; tolerate a bare LF too.
        let lines = headerText.components(separatedBy: "\r\n").flatMap { $0.components(separatedBy: "\n") }
        guard let statusLine = lines.first else {
            throw HTTPWireError.malformedResponse("empty response")
        }

        // "HTTP/1.1 200 OK" → 200
        let statusParts = statusLine.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: true)
        guard statusParts.count >= 2, let code = Int(statusParts[1]) else {
            throw HTTPWireError.malformedResponse("bad status line: \(statusLine)")
        }

        var headers: [String: String] = [:]
        for line in lines.dropFirst() where !line.isEmpty {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = String(line[line.startIndex..<colon]).trimmingCharacters(in: .whitespaces)
            let value = String(line[line.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
            headers[name] = value
        }

        // Honor Content-Length when present: trim trailing bytes if we over-read,
        // but treat an UNDER-read (fewer bytes than declared) as a truncated
        // response — an error the caller retries, not silently-partial data.
        if let clStr = headers["Content-Length"] ?? headers["content-length"],
           let cl = Int(clStr) {
            if cl <= body.count {
                body = body.subdata(in: body.startIndex..<(body.startIndex + cl))
            } else {
                throw HTTPWireError.malformedResponse(
                    "truncated body: got \(body.count) bytes, Content-Length \(cl)"
                )
            }
        }

        guard let response = HTTPURLResponse(
            url: url, statusCode: code, httpVersion: "HTTP/1.1", headerFields: headers
        ) else {
            throw HTTPWireError.malformedResponse("could not build HTTPURLResponse")
        }
        return (body, response)
    }
}

/// The app's build identity, reported to the box via the `X-Virtues-Client`
/// header so it appears on the Devices page (update-manifold Phase 1). Mirrors
/// the box's `{version, sha, channel}` shape. `sha`/`channel` read from optional
/// Info.plist keys a CI build phase can set (`GitCommit`, `Channel`); until then
/// they degrade to `unknown` / `stable` rather than blocking.
enum AppBuild {
    static var version: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0"
    }
    static var sha: String {
        let v = Bundle.main.infoDictionary?["GitCommit"] as? String
        return (v?.isEmpty == false) ? v! : "unknown"
    }
    static var channel: String {
        let v = Bundle.main.infoDictionary?["Channel"] as? String
        return (v?.isEmpty == false) ? v! : "stable"
    }
    /// The `X-Virtues-Client` header value: `version=…; sha=…; channel=…`.
    static var clientHeader: String {
        "version=\(version); sha=\(sha); channel=\(channel)"
    }
}
