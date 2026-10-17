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

/// The phone's own calendar day. Cycle samples are all-day, and only the phone
/// knows which day a midnight belongs to; the box cannot recover it from UTC.
private let localDay: DateFormatter = {
  let f = DateFormatter()
  f.calendar = Calendar(identifier: .gregorian)
  f.locale = Locale(identifier: "en_US_POSIX")
  f.timeZone = .current
  f.dateFormat = "yyyy-MM-dd"
  return f
}()

/// A cycle-log category type and how its values read on the box. `fixed` is
/// the value for a type whose samples carry none of their own (Apple's
/// deviation flags); a type with neither sends no value (spotting).
private struct CycleType {
  let id: HKCategoryTypeIdentifier
  let kind: String
  var values: [Int: String] = [:]
  var fixed: String? = nil
}

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
  /// Cycle tracking is its own opt-in with its own Health sheet, never part
  /// of the general one.
  private let cycleEnabledKey = "virtues.health.cycle.enabled"

  private init() {}

  public var isCollecting: Bool { collecting }

  public func isAvailable() -> Bool { HKHealthStore.isHealthDataAvailable() }

  /// The person went through `enable`. Not a grant: HealthKit never tells an
  /// app whether read access was allowed (a refusal reads back as an empty
  /// store), so this flag is all there is to gate collection on.
  public func optedIn() -> Bool { UserDefaults.standard.bool(forKey: enabledKey) }

  /// The person turned on cycle tracking. Not a grant, as with `optedIn`.
  public func cycleOptedIn() -> Bool { UserDefaults.standard.bool(forKey: cycleEnabledKey) }

  /// Whether the Health sheet has been shown for every type we read, which is
  /// the most iOS will say: `requested`, `not_requested`, `unavailable` (no
  /// HealthKit on this device) or `unknown` (iOS could not tell). `requested`
  /// says nothing about what the person allowed.
  public func permission(_ done: @escaping (String) -> Void) {
    guard isAvailable() else { done("unavailable"); return }
    store.getRequestStatusForAuthorization(toShare: [], read: readTypes()) { status, _ in
      switch status {
      case .unnecessary: done("requested")
      case .shouldRequest: done("not_requested")
      default: done("unknown")
      }
    }
  }

  /// The same as `permission`, for the cycle sheet.
  public func cyclePermission(_ done: @escaping (String) -> Void) {
    guard isAvailable() else { done("unavailable"); return }
    store.getRequestStatusForAuthorization(toShare: [], read: cycleReadTypes()) { status, _ in
      switch status {
      case .unnecessary: done("requested")
      case .shouldRequest: done("not_requested")
      default: done("unknown")
      }
    }
  }

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

  // MARK: - Cycle types
  //
  // Never requested: sexual activity, pregnancy and progesterone tests, and
  // (until their own switch) pregnancy and lactation. Not asking is what keeps
  // them on the phone. See agents/plan/cycle-tracking-plan.md.

  private func cycleTypes() -> [CycleType] {
    var t = [
      CycleType(id: .menstrualFlow, kind: "flow",
                values: [1: "unspecified", 2: "light", 3: "medium", 4: "heavy", 5: "none"]),
      CycleType(id: .intermenstrualBleeding, kind: "spotting"),
      CycleType(id: .ovulationTestResult, kind: "ovulation_test",
                values: [1: "negative", 2: "lh_surge", 3: "indeterminate", 4: "estrogen_surge"]),
      CycleType(id: .cervicalMucusQuality, kind: "cervical_mucus",
                values: [1: "dry", 2: "sticky", 3: "creamy", 4: "watery", 5: "egg_white"]),
    ]
    if #available(iOS 14.3, *) {
      t.append(CycleType(id: .contraceptive, kind: "contraceptive",
                         values: [1: "unspecified", 2: "implant", 3: "injection", 4: "iud",
                                  5: "ring", 6: "oral", 7: "patch"]))
    }
    if #available(iOS 16.0, *) {
      t.append(CycleType(id: .irregularMenstrualCycles, kind: "deviation", fixed: "irregular"))
      t.append(CycleType(id: .infrequentMenstrualCycles, kind: "deviation", fixed: "infrequent"))
      t.append(CycleType(id: .prolongedMenstrualPeriods, kind: "deviation", fixed: "prolonged"))
      t.append(CycleType(id: .persistentIntermenstrualBleeding, kind: "deviation",
                         fixed: "persistent_spotting"))
    }
    return t
  }

  /// Every HealthKit symptom type, by the name the box stores.
  private let symptomTypes: [(HKCategoryTypeIdentifier, String)] = [
    (.abdominalCramps, "abdominal_cramps"), (.acne, "acne"),
    (.appetiteChanges, "appetite_changes"), (.bladderIncontinence, "bladder_incontinence"),
    (.bloating, "bloating"), (.breastPain, "breast_pain"),
    (.chestTightnessOrPain, "chest_tightness_or_pain"), (.chills, "chills"),
    (.constipation, "constipation"), (.coughing, "coughing"), (.diarrhea, "diarrhea"),
    (.dizziness, "dizziness"), (.drySkin, "dry_skin"), (.fainting, "fainting"),
    (.fatigue, "fatigue"), (.fever, "fever"), (.generalizedBodyAche, "generalized_body_ache"),
    (.hairLoss, "hair_loss"), (.headache, "headache"), (.heartburn, "heartburn"),
    (.hotFlashes, "hot_flashes"), (.lossOfSmell, "loss_of_smell"),
    (.lossOfTaste, "loss_of_taste"), (.lowerBackPain, "lower_back_pain"),
    (.memoryLapse, "memory_lapse"), (.moodChanges, "mood_changes"), (.nausea, "nausea"),
    (.nightSweats, "night_sweats"), (.pelvicPain, "pelvic_pain"),
    (.rapidPoundingOrFlutteringHeartbeat, "rapid_pounding_or_fluttering_heartbeat"),
    (.runnyNose, "runny_nose"), (.shortnessOfBreath, "shortness_of_breath"),
    (.sinusCongestion, "sinus_congestion"), (.skippedHeartbeat, "skipped_heartbeat"),
    (.sleepChanges, "sleep_changes"), (.soreThroat, "sore_throat"),
    (.vaginalDryness, "vaginal_dryness"), (.vomiting, "vomiting"), (.wheezing, "wheezing"),
  ]

  /// A symptom's value names, which follow its HealthKit enum. Zero means
  /// "unspecified" for severity but "present" for presence, so the raw int
  /// never travels alone.
  private func symptomValue(_ name: String, _ raw: Int) -> String? {
    switch name {
    case "mood_changes", "sleep_changes":
      return [0: "present", 1: "not_present"][raw]
    case "appetite_changes":
      return [0: "unspecified", 1: "no_change", 2: "decreased", 3: "increased"][raw]
    default:
      return [0: "unspecified", 1: "not_present", 2: "mild", 3: "moderate", 4: "severe"][raw]
    }
  }

  private func cycleReadTypes() -> Set<HKObjectType> {
    var s = Set<HKObjectType>()
    for c in cycleTypes() {
      if let t = HKObjectType.categoryType(forIdentifier: c.id) { s.insert(t) }
    }
    for (id, _) in symptomTypes {
      if let t = HKObjectType.categoryType(forIdentifier: id) { s.insert(t) }
    }
    if let t = HKObjectType.quantityType(forIdentifier: .basalBodyTemperature) { s.insert(t) }
    if #available(iOS 16.0, *),
      let t = HKObjectType.quantityType(forIdentifier: .appleSleepingWristTemperature) {
      s.insert(t)
    }
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

  /// Cycle tracking's own opt-in: its own Health sheet, then a backfill of
  /// the whole history for these types. The app turns this on only for a box
  /// that can store them (`api_version` 5); a box below that would drop them
  /// while the anchors move past.
  public func enableCycle(_ completion: @escaping (Bool) -> Void) {
    guard isAvailable() else { completion(false); return }
    store.requestAuthorization(toShare: nil, read: cycleReadTypes()) { [weak self] ok, _ in
      DispatchQueue.main.async {
        guard let self = self else { return }
        if ok { UserDefaults.standard.set(true, forKey: self.cycleEnabledKey) }
        self.start()
        self.collectAll()
        completion(ok)
      }
    }
  }

  /// Launch auto-resume: start collecting only if already opted in; no prompt.
  public func resume() {
    guard isAvailable(), optedIn() || cycleOptedIn() else { return }
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

    // Ask iOS for a periodic background window (stationary catch-up); the
    // handler self-chains thereafter.
    BackgroundSync.shared.schedule()

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
    if optedIn() {
      for qt in quantityTypes { collectQuantity(qt) }
      collectSleep()
    }
    if cycleOptedIn() { collectCycle() }
  }

  // MARK: - Collection

  private func collectQuantity(_ qt: QType) {
    guard let type = HKObjectType.quantityType(forIdentifier: qt.id) else { return }
    runAnchored(type: type) { [weak self] samples -> Bool in
      guard let self = self else { return false }
      var allEnqueued = true
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
        if !self.enqueue(rec) { allEnqueued = false }
      }
      return allEnqueued
    }
  }

  private func collectSleep() {
    guard let type = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) else { return }
    runAnchored(type: type) { [weak self] samples -> Bool in
      guard let self = self else { return false }
      var allEnqueued = true
      for s in samples {
        guard let c = s as? HKCategorySample else { continue }
        // Integer minutes — the box reads duration as i64 (a JSON float fails
        // `as_i64()`); it wants top-level `sleep_duration` + `sleep_stage`.
        let mins = Int((c.endDate.timeIntervalSince(c.startDate) / 60).rounded())
        let stage = self.sleepStateName(c.value)
        if !self.enqueue([
          "id": c.uuid.uuidString,
          "timestamp": iso.string(from: c.startDate),
          "metric_type": "sleep",
          "unit": "minutes",
          "sleep_duration": mins,
          "sleep_stage": stage,
          "raw_data": ["sleep_state": stage, "duration_minutes": mins],
        ]) { allEnqueued = false }
      }
      return allEnqueued
    }
  }

  // MARK: - Cycle collection

  private func collectCycle() {
    for c in cycleTypes() {
      guard let type = HKObjectType.categoryType(forIdentifier: c.id) else { continue }
      runAnchored(type: type, fullHistory: true) { [weak self] samples -> Bool in
        guard let self = self else { return false }
        var allEnqueued = true
        for s in samples {
          guard let cs = s as? HKCategorySample else { continue }
          var rec = self.daySample(cs, metric: "cycle")
          rec["kind"] = c.kind
          if let v = c.fixed ?? c.values[cs.value] {
            rec["value"] = v
          } else if !c.values.isEmpty {
            // A value this build does not know (a newer iOS case). Skipping is
            // safe for the anchor: the box would refuse it anyway.
            NSLog("[Health] unknown %@ value %d, skipped", c.kind, cs.value)
            continue
          }
          if c.kind == "flow",
            let start = cs.metadata?[HKMetadataKeyMenstrualCycleStart] as? NSNumber {
            rec["is_cycle_start"] = start.boolValue
          }
          if !self.enqueue(rec) { allEnqueued = false }
        }
        return allEnqueued
      }
    }

    for (id, name) in symptomTypes {
      guard let type = HKObjectType.categoryType(forIdentifier: id) else { continue }
      runAnchored(type: type, fullHistory: true) { [weak self] samples -> Bool in
        guard let self = self else { return false }
        var allEnqueued = true
        for s in samples {
          guard let cs = s as? HKCategorySample,
            let value = self.symptomValue(name, cs.value)
          else { continue }
          var rec = self.daySample(cs, metric: "symptom")
          rec["symptom"] = name
          rec["value"] = value
          if !self.enqueue(rec) { allEnqueued = false }
        }
        return allEnqueued
      }
    }

    if let type = HKObjectType.quantityType(forIdentifier: .basalBodyTemperature) {
      runAnchored(type: type, fullHistory: true) { [weak self] samples -> Bool in
        guard let self = self else { return false }
        var allEnqueued = true
        for s in samples {
          guard let q = s as? HKQuantitySample else { continue }
          var rec = self.daySample(q, metric: "cycle")
          rec["kind"] = "basal_temperature"
          rec["temperature_celsius"] = celsius(q)
          if !self.enqueue(rec) { allEnqueued = false }
        }
        return allEnqueued
      }
    }

    if #available(iOS 16.0, *),
      let type = HKObjectType.quantityType(forIdentifier: .appleSleepingWristTemperature) {
      runAnchored(type: type, fullHistory: true) { [weak self] samples -> Bool in
        guard let self = self else { return false }
        var allEnqueued = true
        for s in samples {
          guard let q = s as? HKQuantitySample else { continue }
          // The night a reading belongs to is the day it began, counting a
          // start before noon as the night before. That is a definition, not
          // an inference: a night that starts after midnight is still last
          // night.
          let night = q.startDate.addingTimeInterval(-12 * 3600)
          if !self.enqueue([
            "id": q.uuid.uuidString,
            "metric_type": "wrist_temperature",
            "temperature_celsius": celsius(q),
            "local_start_date": localDay.string(from: night),
            "timestamp": iso.string(from: q.startDate),
            "end_timestamp": iso.string(from: q.endDate),
            "raw_data": ["hk_type": type.identifier],
          ]) { allEnqueued = false }
        }
        return allEnqueued
      }
    }
  }

  /// A temperature to the hundredth of a degree. The °F → °C conversion
  /// otherwise leaves float noise (97.7 °F arrives as 36.50000000000006).
  private func celsius(_ q: HKQuantitySample) -> Double {
    (q.quantity.doubleValue(for: .degreeCelsius()) * 100).rounded() / 100
  }

  /// The fields every cycle and symptom record shares: the instants, and the
  /// phone's calendar days. Health logs a day as an instant at local noon, so
  /// a zero-length sample's last day is its first; a span's last day is the
  /// one just before its end.
  ///
  /// A cycle factor still running ("Ended: Not Yet") carries `distantFuture`
  /// as its end. It is sent with no end at all, which the box stores as an
  /// open interval, rather than as the year 4000.
  private func daySample(_ s: HKSample, metric: String) -> [String: Any] {
    let open = s.endDate >= Date.distantFuture.addingTimeInterval(-86_400 * 366)
    let last = s.endDate > s.startDate ? s.endDate.addingTimeInterval(-1) : s.startDate
    // Only JSON types in here: a boxed nil makes JSONSerialization raise an
    // Objective-C exception, which no `try?` catches.
    var raw: [String: Any] = ["hk_type": s.sampleType.identifier]
    if let c = s as? HKCategorySample { raw["hk_value"] = c.value }
    var rec: [String: Any] = [
      "id": s.uuid.uuidString,
      "metric_type": metric,
      "local_start_date": localDay.string(from: s.startDate),
      "timestamp": iso.string(from: s.startDate),
      "raw_data": raw,
    ]
    if !open {
      rec["local_end_date"] = localDay.string(from: last)
      rec["end_timestamp"] = iso.string(from: s.endDate)
    }
    return rec
  }

  /// Run an anchored query and page through history. Persists the anchor after
  /// each page (over-fetch after a crash is deduped at the box), so an
  /// interrupted 3-year backfill resumes instead of restarting.
  ///
  /// The anchor advances ONLY when `handle` reports that every sample in the
  /// page reached the outbox. An `HKQueryAnchor` cannot be rewound: once it
  /// moves past a sample, the only recovery is resetting it to nil and
  /// re-backfilling everything. So an anchor that advances past a failed
  /// enqueue is permanent, silent data loss — which has already happened once
  /// here (see the `sleep_refetch_v1` migration key below: an earlier build
  /// sent sleep in a shape the box dropped while the anchor advanced through
  /// it, and recovery required shipping code). Paging also stops on failure —
  /// pulling more history we cannot store just widens the hole.
  private func runAnchored(
    type: HKSampleType, fullHistory: Bool = false, handle: @escaping ([HKSample]) -> Bool
  ) {
    let key = "virtues.health.anchor." + type.identifier
    // Cycle types take the whole history: years of cycles are what make a
    // person's own pattern, and the rows are small.
    let predicate: NSPredicate? = fullHistory
      ? nil
      : HKQuery.predicateForSamples(
          withStart: Calendar.current.date(byAdding: .year, value: -backfillYears, to: Date()),
          end: nil, options: [])

    let query = HKAnchoredObjectQuery(
      type: type, predicate: predicate, anchor: loadAnchor(key), limit: pageLimit
    ) { [weak self] _, samples, deleted, newAnchor, _ in
      guard let self = self else { return }
      let samples = samples ?? []
      let deleted = deleted ?? []
      // Deletions ride the same anchor as samples, so they count toward
      // whether the page is durable. Without them a sample deleted in Health
      // stays on the box for good.
      let durable = handle(samples) && self.enqueueDeletions(deleted)
      guard durable else {
        NSLog("[Health] enqueue failed for %@ — anchor HELD, will retry this page",
              type.identifier)
        return
      }
      if let a = newAnchor { self.saveAnchor(key, a) }
      // A full page means there may be more history — page again.
      if samples.count + deleted.count >= self.pageLimit {
        self.runAnchored(type: type, fullHistory: fullHistory, handle: handle)
      }
    }
    store.execute(query)
  }

  /// Tell the box which samples were deleted in Health. The box tombstones
  /// the row on whichever table holds it. The record's own `id` cannot be the
  /// sample's: the outbox ignores an id it has already queued, and the sample
  /// may still be in it.
  private func enqueueDeletions(_ deleted: [HKDeletedObject]) -> Bool {
    var allEnqueued = true
    for d in deleted {
      let uuid = d.uuid.uuidString
      if !enqueue([
        "id": "deleted:" + uuid,
        "metric_type": "deleted",
        "deleted_id": uuid,
        "timestamp": iso.string(from: Date()),
      ]) { allEnqueued = false }
    }
    return allEnqueued
  }

  /// Hand one record to the outbox. Returns whether it was actually taken —
  /// the caller uses this to decide if the anchor may advance past it.
  ///
  /// A serialization failure counts as NOT durable. It is our bug rather than
  /// the outbox's, but the sample is equally lost either way, and reporting it
  /// as stored is what turns a bug into missing history.
  @discardableResult
  private func enqueue(_ rec: [String: Any]) -> Bool {
    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else {
      NSLog("[Health] record failed to serialize — treating as not durable")
      return false
    }
    let rc = "healthkit".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[Health] enqueue failed rc=%d", rc) }
    return rc == 0
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
