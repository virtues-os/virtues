import Foundation
import HealthKit

// Shared outbox enqueue (defined in the reach plugin's ffi.rs; one app binary).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

private let iso: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  return f
}()

/// One quantity metric we read, with its box `metric_type` + unit.
private struct QType {
  let id: HKQuantityTypeIdentifier
  let metric: String
  let unit: HKUnit
  let unitLabel: String
}

/// The HealthKit collector: this phone's health data → the shared outbox → box.
///
/// Backfill + incremental are one code path: an `HKAnchoredObjectQuery` with a
/// 3-year predicate. First run pages through all history (persisting the anchor
/// after each page → resumable); later runs return only new samples. Enqueue is
/// idempotent (id = HKSample.uuid), so re-fetching after a crash dedups at the box.
public final class HealthCollector {
  public static let shared = HealthCollector()

  private let store = HKHealthStore()
  private var timer: DispatchSourceTimer?
  private var collecting = false

  /// How far back to backfill (user-expandable later).
  private let backfillYears = 3
  /// Page size for the anchored backfill.
  private let pageLimit = 2000
  private let enabledKey = "virtues.health.enabled"

  private init() {}

  public var isCollecting: Bool { collecting }

  public func isAvailable() -> Bool { HKHealthStore.isHealthDataAvailable() }

  /// HealthKit doesn't expose read-authorization status, so we track an
  /// explicit opt-in flag (set once the user grants via `enable`).
  public func authorized() -> Bool { UserDefaults.standard.bool(forKey: enabledKey) }

  private let quantityTypes: [QType] = [
    QType(id: .heartRate, metric: "heart_rate",
          unit: HKUnit.count().unitDivided(by: .minute()), unitLabel: "bpm"),
    QType(id: .restingHeartRate, metric: "resting_heart_rate",
          unit: HKUnit.count().unitDivided(by: .minute()), unitLabel: "bpm"),
    QType(id: .stepCount, metric: "steps", unit: .count(), unitLabel: "steps"),
    QType(id: .activeEnergyBurned, metric: "active_energy", unit: .kilocalorie(), unitLabel: "kcal"),
    QType(id: .heartRateVariabilitySDNN, metric: "heart_rate_variability",
          unit: HKUnit.secondUnit(with: .milli), unitLabel: "ms"),
    QType(id: .distanceWalkingRunning, metric: "distance", unit: .meter(), unitLabel: "m"),
  ]

  private func readTypes() -> Set<HKObjectType> {
    var s = Set<HKObjectType>()
    for qt in quantityTypes {
      if let t = HKObjectType.quantityType(forIdentifier: qt.id) { s.insert(t) }
    }
    if let sleep = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) { s.insert(sleep) }
    return s
  }

  // MARK: - Lifecycle

  /// Explicit opt-in: prompt for access, then backfill + start collecting.
  public func enable(_ completion: @escaping (Bool) -> Void) {
    guard isAvailable() else { completion(false); return }
    store.requestAuthorization(toShare: nil, read: readTypes()) { [weak self] ok, _ in
      DispatchQueue.main.async {
        if ok { UserDefaults.standard.set(true, forKey: self?.enabledKey ?? "virtues.health.enabled") }
        self?.start()
        completion(ok)
      }
    }
  }

  /// Launch auto-resume: start collecting only if already opted in; no prompt.
  public func resume() {
    guard isAvailable(), authorized() else { return }
    start()
  }

  private func start() {
    if collecting { return }
    collecting = true

    // One-time: an earlier build sent sleep in the wrong shape, so the box
    // dropped it while our anchor advanced past it. Reset the sleep anchor once
    // to re-backfill (the box dedups on HKSample.uuid, so re-fetch is safe).
    let migKey = "virtues.health.sleep_refetch_v1"
    if !UserDefaults.standard.bool(forKey: migKey) {
      UserDefaults.standard.removeObject(forKey: "virtues.health.anchor.HKCategoryTypeIdentifierSleepAnalysis")
      UserDefaults.standard.set(true, forKey: migKey)
    }

    collectAll()  // immediate

    // While the process is alive, poll every 5 min for new samples. Background
    // catch-up comes from BGProcessingTask / any wake calling collectAll().
    let t = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
    t.schedule(deadline: .now() + 300, repeating: 300)
    t.setEventHandler { [weak self] in self?.collectAll() }
    t.resume()
    timer = t
  }

  /// Fetch new samples for every type (safe to call from any wake). No-op if
  /// not authorized.
  public func collectAll() {
    guard authorized() else { return }
    for qt in quantityTypes { collectQuantity(qt) }
    collectSleep()
  }

  // MARK: - Collection

  private func collectQuantity(_ qt: QType) {
    guard let type = HKObjectType.quantityType(forIdentifier: qt.id) else { return }
    runAnchored(type: type) { [weak self] samples in
      guard let self = self else { return }
      for s in samples {
        guard let q = s as? HKQuantitySample else { continue }
        let value = q.quantity.doubleValue(for: qt.unit)
        var rec: [String: Any] = [
          "id": q.uuid.uuidString,
          "timestamp": iso.string(from: q.startDate),
          "metric_type": qt.metric,
          "value": (value * 100).rounded() / 100,
          "unit": qt.unitLabel,
        ]
        if qt.metric == "heart_rate",
          let ctx = q.metadata?[HKMetadataKeyHeartRateMotionContext] as? Int {
          rec["raw_data"] = ["activity_context": ctx == 1 ? "resting" : (ctx == 2 ? "active" : "unknown")]
        }
        self.enqueue(rec)
      }
    }
  }

  private func collectSleep() {
    guard let type = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) else { return }
    runAnchored(type: type) { [weak self] samples in
      guard let self = self else { return }
      for s in samples {
        guard let c = s as? HKCategorySample else { continue }
        // Integer minutes — the box reads duration as i64 (a JSON float fails
        // `as_i64()`); it wants top-level `sleep_duration` + `sleep_stage`.
        let mins = Int((c.endDate.timeIntervalSince(c.startDate) / 60).rounded())
        let stage = self.sleepStateName(c.value)
        self.enqueue([
          "id": c.uuid.uuidString,
          "timestamp": iso.string(from: c.startDate),
          "metric_type": "sleep",
          "unit": "minutes",
          "sleep_duration": mins,
          "sleep_stage": stage,
          "raw_data": ["sleep_state": stage, "duration_minutes": mins],
        ])
      }
    }
  }

  /// Run an anchored query and page through history. Persists the anchor after
  /// each page (optimistic — over-fetch after a crash is deduped at the box), so
  /// an interrupted 3-year backfill resumes instead of restarting.
  private func runAnchored(type: HKSampleType, handle: @escaping ([HKSample]) -> Void) {
    let key = "virtues.health.anchor." + type.identifier
    let start = Calendar.current.date(byAdding: .year, value: -backfillYears, to: Date())
    let predicate = HKQuery.predicateForSamples(withStart: start, end: nil, options: [])

    let query = HKAnchoredObjectQuery(
      type: type, predicate: predicate, anchor: loadAnchor(key), limit: pageLimit
    ) { [weak self] _, samples, _, newAnchor, _ in
      guard let self = self else { return }
      let samples = samples ?? []
      handle(samples)
      if let a = newAnchor { self.saveAnchor(key, a) }
      // A full page means there may be more history — page again.
      if samples.count >= self.pageLimit {
        self.runAnchored(type: type, handle: handle)
      }
    }
    store.execute(query)
  }

  private func enqueue(_ rec: [String: Any]) {
    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }
    let rc = "healthkit".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[Health] enqueue failed rc=%d", rc) }
  }

  // MARK: - Anchor persistence

  private func loadAnchor(_ key: String) -> HKQueryAnchor? {
    guard let data = UserDefaults.standard.data(forKey: key) else { return nil }
    return try? NSKeyedUnarchiver.unarchivedObject(ofClass: HKQueryAnchor.self, from: data)
  }

  private func saveAnchor(_ key: String, _ anchor: HKQueryAnchor) {
    if let data = try? NSKeyedArchiver.archivedData(withRootObject: anchor, requiringSecureCoding: true) {
      UserDefaults.standard.set(data, forKey: key)
    }
  }

  private func sleepStateName(_ v: Int) -> String {
    if v == HKCategoryValueSleepAnalysis.inBed.rawValue { return "in_bed" }
    if v == HKCategoryValueSleepAnalysis.awake.rawValue { return "awake" }
    if #available(iOS 16.0, *) {
      switch v {
      case HKCategoryValueSleepAnalysis.asleepCore.rawValue: return "asleep_core"
      case HKCategoryValueSleepAnalysis.asleepDeep.rawValue: return "asleep_deep"
      case HKCategoryValueSleepAnalysis.asleepREM.rawValue: return "asleep_rem"
      case HKCategoryValueSleepAnalysis.asleepUnspecified.rawValue: return "asleep"
      default: return "unknown"
      }
    }
    if v == HKCategoryValueSleepAnalysis.asleep.rawValue { return "asleep" }
    return "unknown"
  }
}
