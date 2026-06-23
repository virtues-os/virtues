//
//  ActionLinkRow.swift
//  Virtues
//
//  Everything about a stream's server linkage + outcome, in one row group:
//
//   * **Linkage** — is the stream wired to a server-side action? Uploads route
//     to `POST /webhook/{action_id}` from the map the box returns at pair time
//     (`DeviceConfiguration.actionIds`). No entry → `BatchUploadCoordinator`
//     silently skips the stream and its pending count climbs forever; we surface
//     that + the fix (re-pair), and show the action identity when set.
//   * **Local outcome (2A)** — did the last upload POST land?
//     `BatchUploadCoordinator.streamSync` tracks last-success / failing per stream.
//   * **Server outcome (2B)** — did the box's action actually *run*? Fetched
//     on-demand from `GET /api/devices/actions/{id}/runs` when the page appears.
//
//  `streamKey` is the canonical backend function name (e.g. "ios_healthkit").
//

import SwiftUI

struct ActionLinkRow: View {
    let streamKey: String

    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared

    @State private var serverRun: ActionRun?
    @State private var loadingRun = false
    @State private var runUnavailable = false

    private var canonical: String { DeviceConfiguration.canonicalStreamName(streamKey) }
    /// All iOS streams now share one backend action (`ios_ingest`); the
    /// per-stream "Server Action" row points at it. Local sync status stays
    /// keyed per stream.
    private var actionId: String? { deviceManager.configuration.actionIds["ios_ingest"] }
    private var sync: StreamSyncState? { uploadCoordinator.streamSync[canonical] }

    var body: some View {
        Group {
            if let actionId {
                VStack(spacing: 10) {
                    InfoRow(label: "Server Action", value: shortId(actionId))
                    localOutcomeRow
                    serverRunRow
                }
                .task(id: actionId) { await loadServerRun(actionId) }
            } else {
                notLinked
            }
        }
    }

    // MARK: - Local outcome (2A)

    @ViewBuilder
    private var localOutcomeRow: some View {
        if let sync {
            if sync.consecutiveFailures > 0 {
                InfoRow(label: "Last Upload",
                        value: "Failing (\(sync.consecutiveFailures)×)",
                        valueColor: .warmError)
            } else if let last = sync.lastSuccess {
                InfoRow(label: "Last Upload",
                        value: last.formatted(.relative(presentation: .named)),
                        valueColor: .warmSuccess)
            } else {
                InfoRow(label: "Last Upload", value: "—")
            }
        } else {
            InfoRow(label: "Last Upload", value: "No uploads yet")
        }
    }

    // MARK: - Server outcome (2B)

    @ViewBuilder
    private var serverRunRow: some View {
        if loadingRun && serverRun == nil {
            InfoRow(label: "Server Run", value: "Checking…")
        } else if let run = serverRun {
            let when = run.timestamp?.formatted(.relative(presentation: .named))
            switch run.outcome {
            case .success:
                InfoRow(label: "Server Run", value: when.map { "Ran \($0)" } ?? "Succeeded",
                        valueColor: .warmSuccess)
            case .failure:
                VStack(alignment: .leading, spacing: 4) {
                    InfoRow(label: "Server Run", value: when.map { "Failed \($0)" } ?? "Failed",
                            valueColor: .warmError)
                    if let err = run.error ?? run.resultSummary {
                        Text(err)
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            case .running:
                InfoRow(label: "Server Run", value: "Running…", valueColor: .warmInfo)
            case .other:
                InfoRow(label: "Server Run", value: run.status.capitalized)
            }
        } else if runUnavailable {
            InfoRow(label: "Server Run", value: "Unavailable")
        } else {
            InfoRow(label: "Server Run", value: "No server runs yet")
        }
    }

    private func loadServerRun(_ actionId: String) async {
        loadingRun = true
        runUnavailable = false
        defer { loadingRun = false }
        do {
            serverRun = try await NetworkManager.shared.fetchActionRuns(actionId: actionId, limit: 1).first
        } catch {
            // Couldn't reach the box (off-LAN with no tunnel, etc.) — show
            // "Unavailable" rather than an error; the local outcome still stands.
            runUnavailable = true
        }
    }

    // MARK: - Not linked

    private var notLinked: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundColor(.warmWarning)
                Text("Not linked to a server action")
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .foregroundColor(.warmWarning)
            }
            Text("Queued data for this stream can't upload until it's linked. "
                + "Re-pair this device (Settings → Server) to fix.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// action_ids look like `action_ios_healthkit_cred_abc123`; show a readable tail.
    private func shortId(_ id: String) -> String {
        id.count > 16 ? "…" + id.suffix(14) : id
    }
}
