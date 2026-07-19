//
//  ManualPairView.swift
//  Virtues
//
//  Type the box address + pairing code by hand — the counterpart to scanning a
//  QR. This is the path for pairing over Tailscale (enter the box's tailnet IP)
//  or whenever a camera scan isn't handy, mirroring `virtues-client pair
//  "http://<host>:8000/pair#t=<code>"` on the Mac CLI. On submit it hands
//  `(endpoint, code)` back to the caller, which runs the SAME
//  `consumePairToken` path as a scanned QR.
//

import SwiftUI

struct ManualPairView: View {
    @Environment(\.dismiss) var dismiss

    /// `(endpoint, code)` — the caller runs the consume/pair flow.
    let onPair: (String, String) -> Void

    @State private var endpoint = ""
    @State private var code = ""

    /// A dialable box address (host present) + a 6-digit code. Parse the
    /// *normalized* endpoint, not the raw text: a bare `IP:port` (the common
    /// Tailscale/LAN entry) has no scheme, so `URL(_:).host` is nil on the raw
    /// string and the button would never enable — `normalizedEndpoint` prepends
    /// `http://` first, matching what the caller actually dials.
    private var canPair: Bool {
        let host = URL(string: Self.normalizedEndpoint(endpoint))?.host
        return (host?.isEmpty == false) && code.filter(\.isNumber).count >= 6
    }

    /// Normalize the typed box address: trim, default the scheme to `http`, and
    /// default the port to the box's web port `8000` when the user omits it (the
    /// common mistake — the box serves HTTP on :8000, not :80).
    static func normalizedEndpoint(_ raw: String) -> String {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if !s.contains("://") { s = "http://\(s)" }
        guard let u = URL(string: s), let scheme = u.scheme, let host = u.host else { return s }
        if u.port == nil { return "\(scheme)://\(host):8000" }
        return s
    }

    var body: some View {
        NavigationView {
            Form {
                Section(
                    header: Text("Box address"),
                    footer: Text("The box's address on your network — e.g. its Tailscale IP `http://100.x.y.z:8000`, or `http://192.168.1.5:8000` on the same wifi.")
                ) {
                    TextField("http://100.x.y.z:8000", text: $endpoint)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                        .keyboardType(.URL)
                        .font(.system(.body, design: .monospaced))
                }

                Section(
                    header: Text("Pairing code"),
                    footer: Text("Run `virtues pair` on the box (or open its Devices page) to get the 6-digit code.")
                ) {
                    TextField("123456", text: $code)
                        .keyboardType(.numberPad)
                        .font(.system(.body, design: .monospaced))
                }
            }
            .scrollContentBackground(.hidden)
            .background(Color.warmBackground)
            .navigationTitle("Enter Pairing Code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("Cancel") {
                        Haptics.light()
                        dismiss()
                    }
                    .foregroundColor(.warmForegroundMuted)
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Pair") {
                        Haptics.light()
                        let ep = Self.normalizedEndpoint(endpoint)
                        let c = code.filter(\.isNumber)
                        dismiss()
                        onPair(ep, c)
                    }
                    .disabled(!canPair)
                    .foregroundColor(canPair ? .warmPrimary : .warmForegroundDisabled)
                }
            }
        }
    }
}
