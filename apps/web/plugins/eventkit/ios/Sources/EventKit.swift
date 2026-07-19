import EventKit
import Foundation

// Shared outbox enqueue (defined in the reach plugin's ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

private let iso: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime]
  return f
}()

/// Calendar collector: events from 3 years back to 3 years forward → shared
/// outbox → box. Re-scans on enable/launch/"Sync now" (calendar has no
/// background delivery); enqueue is idempotent so re-scans dedup at the box.
///
/// `predicateForEvents` caps at a 4-year span, so the 6-year window is walked in
/// yearly chunks. Recurring events share one `eventIdentifier`, so the id is
/// keyed by identifier + occurrence start to keep occurrences distinct.
public final class EventKitCollector {
  public static let shared = EventKitCollector()

  private let store = EKEventStore()
  private var collecting = false
  private let backYears = 3
  private let forwardYears = 3

  private init() {}

  public var isCollecting: Bool { collecting }

  public func authorized() -> Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) { return status == .fullAccess }
    return status == .authorized
  }

  /// Explicit opt-in: prompt, then scan.
  public func enable(_ completion: @escaping (Bool) -> Void) {
    let handler: (Bool, Error?) -> Void = { [weak self] granted, _ in
      DispatchQueue.main.async {
        if granted { self?.start() }
        completion(granted)
      }
    }
    if #available(iOS 17.0, *) {
      store.requestFullAccessToEvents(completion: handler)
    } else {
      store.requestAccess(to: .event, completion: handler)
    }
  }

  /// Launch auto-resume: scan only if already authorized; no prompt.
  public func resume() {
    guard authorized() else { return }
    start()
  }

  private func start() {
    collecting = true
    scan()
  }

  /// Re-scan the whole window (safe to call from any wake / "Sync now").
  public func collectAll() {
    guard authorized() else { return }
    scan()
  }

  private func scan() {
    // `events(matching:)` is blocking — do it off the main thread.
    DispatchQueue.global(qos: .utility).async { [weak self] in
      guard let self = self else { return }
      let cal = Calendar.current
      let now = Date()
      guard
        let windowStart = cal.date(byAdding: .year, value: -self.backYears, to: now),
        let windowEnd = cal.date(byAdding: .year, value: self.forwardYears, to: now)
      else { return }

      // Walk in ≤1-year chunks (predicateForEvents caps at a 4-year span).
      var chunkStart = windowStart
      while chunkStart < windowEnd {
        let chunkEnd = min(cal.date(byAdding: .year, value: 1, to: chunkStart) ?? windowEnd, windowEnd)
        let predicate = self.store.predicateForEvents(withStart: chunkStart, end: chunkEnd, calendars: nil)
        for ev in self.store.events(matching: predicate) {
          self.enqueue(ev)
        }
        chunkStart = chunkEnd
      }
    }
  }

  private func enqueue(_ ev: EKEvent) {
    let startStr = iso.string(from: ev.startDate)
    // Per-occurrence id (recurring events share eventIdentifier).
    let base = ev.eventIdentifier ?? UUID().uuidString
    var rec: [String: Any] = [
      "record_type": "event",
      "id": base + "_" + startStr,
      "title": ev.title ?? "",
      "startDate": startStr,
      "endDate": iso.string(from: ev.endDate),
      "isAllDay": ev.isAllDay,
    ]
    if let cal = ev.calendar {
      rec["calendarTitle"] = cal.title
      rec["calendarId"] = cal.calendarIdentifier
    }
    if let loc = ev.location, !loc.isEmpty { rec["location"] = loc }
    if let notes = ev.notes, !notes.isEmpty { rec["notes"] = notes }
    if let url = ev.url?.absoluteString { rec["url"] = url }
    if let mod = ev.lastModifiedDate { rec["lastModified"] = iso.string(from: mod) }

    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }
    let rc = "eventkit".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[EventKit] enqueue failed rc=%d", rc) }
  }
}
