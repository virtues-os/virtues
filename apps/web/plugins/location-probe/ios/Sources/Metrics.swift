import Foundation
import MetricKit

/// MetricKit subscriber — the battery/network/location attribution feed.
///
/// iOS collects these metrics system-wide whether or not anyone subscribes;
/// registering only means we RECEIVE the ~daily digest (one `MXMetricPayload`
/// per 24h window, delivered on the OS's schedule, usually shortly after
/// midnight or next launch). Near-zero runtime cost. The payloads carry what
/// the battery work needs to be attributable instead of vibes:
///   • MXLocationActivityMetric — cumulative time at each accuracy bucket
///     (directly measures the precise-vs-coarse split the power modes control)
///   • MXNetworkTransferMetric — cellular vs Wi-Fi bytes up/down (directly
///     measures the drain-cadence work)
///   • MXCPUMetric / MXAppRunTimeMetric — the always-resident baseline
///
/// Storage: raw payload JSON into the diagnostics SQLite the location probe
/// already owns (device-local, surfaced later by an iOS-only view; deliberately
/// NOT shipped to the box for now).
final class Metrics: NSObject, MXMetricManagerSubscriber {
  static let shared = Metrics()

  private override init() { super.init() }

  func start() {
    MXMetricManager.shared.add(self)
    NSLog("[Metrics] MetricKit subscriber registered")
  }

  func didReceive(_ payloads: [MXMetricPayload]) {
    for p in payloads {
      let json = String(data: p.jsonRepresentation(), encoding: .utf8) ?? "{}"
      LocationProbe.shared.writeMetric(kind: "metrics", json: json)
      NSLog("[Metrics] stored daily payload (%d bytes)", json.utf8.count)
    }
  }

  /// Diagnostics (crashes, hangs, disk-write exceptions) ride the same channel;
  /// keep them — a hang or crash digest is exactly what a field report needs.
  func didReceive(_ payloads: [MXDiagnosticPayload]) {
    for p in payloads {
      let json = String(data: p.jsonRepresentation(), encoding: .utf8) ?? "{}"
      LocationProbe.shared.writeMetric(kind: "diagnostics", json: json)
      NSLog("[Metrics] stored diagnostic payload (%d bytes)", json.utf8.count)
    }
  }
}
