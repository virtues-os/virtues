//
//  ConnectionSettingsView.swift
//  Virtues
//
//  The "set up WG in settings" surface. Shows the in-app tunnel's status, the
//  box's WireGuard endpoint + internal address, and the SPKI fingerprint the
//  user can compare to what's printed on the box. Also lets the user test the
//  tunnel and forget its credentials (forcing a re-pair).
//

import SwiftUI

struct ConnectionSettingsView: View {
    @State private var tunnelStatus: String = VirtuesTunnelManager.shared.statusString()
    @State private var isTesting = false
    @State private var testResult: String?
    @State private var showForgetConfirm = false

    private var bundle: BoxBundle? { VirtuesTunnelManager.shared.boxBundle() }
    private var fingerprint: String? { VirtuesTunnelManager.shared.spkiFingerprint() }

    var body: some View {
        List {
            Section(header: Text("Tunnel")) {
                DetailRow(label: "Status", value: tunnelStatus.capitalized)

                if VirtuesTunnelManager.shared.canBringUp {
                    Button(action: testTunnel) {
                        HStack {
                            if isTesting {
                                ProgressView().scaleEffect(0.8)
                                Text("Testing…")
                            } else {
                                Label("Test Tunnel", systemImage: "bolt.horizontal.circle")
                            }
                        }
                        .foregroundColor(.warmPrimary)
                    }
                    .disabled(isTesting)

                    if let testResult {
                        Text(testResult)
                            .font(.caption)
                            .foregroundColor(testResult.hasPrefix("✓") ? .warmSuccess : .warmError)
                    }
                } else {
                    Text("No tunnel credentials. Pair the device to enable off-network access.")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }

            if let bundle {
                Section(header: Text("Box Address"),
                        footer: Text("Off the local network, the app reaches the box directly over WireGuard at this endpoint. The tunnel runs inside the app — it does not take over your device VPN.")) {
                    DetailRow(label: "WG Endpoint", value: bundle.wg.serverEndpoint)
                    DetailRow(label: "Internal", value: "\(bundle.internalIp):\(bundle.httpPort)")
                }

                if let fingerprint {
                    Section(header: Text("Identity (SPKI)"),
                            footer: Text("Compare this fingerprint to the one shown on the box to verify you're connected to the right machine.")) {
                        Text(fingerprint)
                            .font(.system(.caption, design: .monospaced))
                            .textSelection(.enabled)
                            .foregroundColor(.warmForegroundMuted)
                    }
                }

                Section {
                    Button(role: .destructive) {
                        showForgetConfirm = true
                    } label: {
                        Label("Forget Tunnel Credentials", systemImage: "trash")
                    }
                }
            }
        }
        .navigationTitle("Connection")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { tunnelStatus = VirtuesTunnelManager.shared.statusString() }
        .confirmationDialog(
            "Forget the WireGuard credentials? You'll need to re-pair to reach the box off your local network.",
            isPresented: $showForgetConfirm,
            titleVisibility: .visible
        ) {
            Button("Forget", role: .destructive) {
                KeychainStore.shared.wipeAll()
                VirtuesTunnelManager.shared.teardown()
                tunnelStatus = VirtuesTunnelManager.shared.statusString()
                testResult = nil
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    /// Bring the tunnel up and dial the box's internal HTTP port to prove
    /// reachability, updating the status line. Runs off the main thread (the FFI
    /// dial blocks until the handshake completes or times out).
    private func testTunnel() {
        isTesting = true
        testResult = nil
        Task.detached {
            var result: String
            do {
                let handle = try VirtuesTunnelManager.shared.bringUp()
                if let b = VirtuesTunnelManager.shared.boxBundle() {
                    _ = try handle.dial(ip: b.internalIp, port: b.httpPort)
                    result = "✓ Tunnel up — box reachable"
                } else {
                    result = "No bundle to dial"
                }
            } catch {
                result = "✗ \(error.localizedDescription)"
            }
            let status = VirtuesTunnelManager.shared.statusString()
            await MainActor.run {
                self.testResult = result
                self.tunnelStatus = status
                self.isTesting = false
            }
        }
    }
}
