import AppIntents
import SwiftUI
import WidgetKit

/// The Virtues widget: one sentence about recording, how current your server
/// is, and today as a strip.
///
/// It reads like a log entry, not a dashboard: a serif line ("Recording since
/// 7:02"), times instead of counts ("Up to date through 3:38", because a count
/// of queued records means nothing to a person), and at most one action. On
/// the Lock Screen a ring shows how much of today is accounted for, kept or
/// muted by choice, so only a gap leaves it short.
///
/// It runs in this extension's process and never touches the box or the
/// outbox. It reads what the app writes into the App Group: the recorder's
/// state, heartbeat and today's stretches (defaults; MARK: Override and
/// MARK: Today's stretches in the audio plugin's Audio.swift) and the upload
/// snapshot (`uploads.json`, reach/src/widget.rs).
///
/// A recorder that dies cannot say so, so each timeline carries an entry
/// `staleAfter` past the last heartbeat that reads "No word since 3:40".
/// While the app is alive the heartbeat moves every minute and the widget's
/// own refresh replaces that entry before it shows. Tapping the widget opens
/// the app, and opening it is the one thing that can restart the mic.
struct RecordingWidget: Widget {
  var body: some WidgetConfiguration {
    StaticConfiguration(kind: RecordingShared.widgetKind, provider: StatusProvider()) { entry in
      StatusView(entry: entry)
    }
    .configurationDisplayName("Recording")
    .description("See whether Virtues is recording and how current your server is, with today as a strip.")
    .supportedFamilies([
      .systemSmall, .systemMedium, .systemLarge,
      .accessoryCircular, .accessoryRectangular, .accessoryInline,
    ])
  }
}

// MARK: - Timeline

struct StatusEntry: TimelineEntry {
  let date: Date
  let state: RecordingState
  let uploads: UploadSnapshot?
  /// Today's stretches, clipped to today.
  let day: [Stretch]
  /// Nothing from the recorder for `staleAfter`: it may have stopped.
  let stale: Bool
}

struct StatusProvider: TimelineProvider {
  /// Long enough that a widget refresh the system runs late does not cry
  /// wolf over a recorder that is fine (its heartbeat moves every minute).
  static let staleAfter: TimeInterval = 45 * 60
  /// Asked, not promised: the system spaces refreshes to its daily budget.
  static let refreshEvery: TimeInterval = 15 * 60

  func placeholder(in context: Context) -> StatusEntry { .preview }

  func getSnapshot(in context: Context, completion: @escaping (StatusEntry) -> Void) {
    completion(context.isPreview ? .preview : entry(at: Date()))
  }

  func getTimeline(in context: Context, completion: @escaping (Timeline<StatusEntry>) -> Void) {
    let now = Date()
    let s = RecordingState.read(length: .oneHour, now: now)
    var dates = [now]
    // The moment an override runs out, and the moment silence turns stale.
    if let u = s.until, u > now { dates.append(u) }
    if let beat = s.heartbeat, beat + Self.staleAfter > now { dates.append(beat + Self.staleAfter) }
    let entries = dates.sorted().map { entry(at: $0) }
    completion(Timeline(entries: entries, policy: .after(now + Self.refreshEvery)))
  }

  private func entry(at date: Date) -> StatusEntry {
    let s = RecordingState.read(length: .oneHour, now: date)
    let silent = s.heartbeat.map { date.timeIntervalSince($0) >= Self.staleAfter } ?? false
    let stale = s.enabled && !s.carPlay && silent
    return StatusEntry(
      date: date, state: s, uploads: UploadSnapshot.read(),
      day: Stretch.today(at: date, heartbeat: stale ? nil : s.heartbeat), stale: stale)
  }
}

// MARK: - Today

/// One run of kept or muted audio. A gap is the absence of one.
struct Stretch: Identifiable {
  let start: Date
  let end: Date
  let kept: Bool
  var id: Double { start.timeIntervalSince1970 }

  /// Today's stretches from the recorder's log, clipped to local midnight.
  /// The open stretch (the one the heartbeat is still extending) is drawn up
  /// to `date`, because the recorder writes it at most once a minute.
  static func today(at date: Date, heartbeat: Date?) -> [Stretch] {
    let midnight = Calendar.current.startOfDay(for: date)
    guard
      let str = UserDefaults(suiteName: RecordingShared.appGroup)?
        .string(forKey: RecordingShared.stretchesKey),
      let data = str.data(using: .utf8),
      let rows = (try? JSONSerialization.jsonObject(with: data)) as? [[Double]]
    else { return [] }
    var out: [Stretch] = []
    for (i, r) in rows.enumerated() where r.count == 3 {
      var end = Date(timeIntervalSince1970: r[1])
      if i == rows.count - 1, let beat = heartbeat, abs(beat.timeIntervalSince(end)) < 120 {
        end = max(end, date)
      }
      let start = max(Date(timeIntervalSince1970: r[0]), midnight)
      end = min(end, date)
      if end > start { out.append(Stretch(start: start, end: end, kept: r[2] == 1)) }
    }
    return out
  }
}

extension Array where Element == Stretch {
  var keptSeconds: TimeInterval { filter(\.kept).reduce(0) { $0 + $1.end.timeIntervalSince($1.start) } }
  var mutedSeconds: TimeInterval { filter { !$0.kept }.reduce(0) { $0 + $1.end.timeIntervalSince($1.start) } }
}

// MARK: - Uploads

/// `uploads.json`: per stream, what is queued and when the box last took it.
struct UploadSnapshot {
  struct Stream: Identifiable {
    let id: String
    let queued: Int
    let oldest: Date?
    let delivered: Date?

    /// The device screen's names for the outbox streams.
    var name: String {
      switch id {
      case "location": return "Location"
      case "healthkit": return "Health"
      case "eventkit": return "Calendar"
      case "contacts": return "Contacts"
      case "financekit": return "Finance"
      case "microphone": return "Audio"
      case "bookmark": return "Saved links"
      default: return id.prefix(1).uppercased() + id.dropFirst()
      }
    }
  }

  /// When the snapshot was written; an empty queue is current to here.
  let at: Date
  /// The box has given no answer at all since then.
  let unreachableSince: Date?
  let streams: [Stream]

  /// Your server has everything this stream recorded before this moment.
  func through(_ s: Stream) -> Date { s.queued > 0 ? (s.oldest ?? at) : at }

  /// The least current stream sets how current your record is.
  var through: Date { streams.map(through).min() ?? at }

  static func read() -> UploadSnapshot? {
    guard
      let url = FileManager.default
        .containerURL(forSecurityApplicationGroupIdentifier: RecordingShared.appGroup)?
        .appendingPathComponent("uploads.json"),
      let data = try? Data(contentsOf: url),
      let doc = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    else { return nil }
    let date = { (v: Any?) -> Date? in
      guard let n = v as? NSNumber, n.doubleValue > 0 else { return nil }
      return Date(timeIntervalSince1970: n.doubleValue)
    }
    let rows = (doc["streams"] as? [[String: Any]] ?? []).compactMap { r -> Stream? in
      guard let id = r["stream"] as? String else { return nil }
      return Stream(
        id: id,
        queued: (r["queued"] as? NSNumber)?.intValue ?? 0,
        oldest: date(r["oldest"]),
        delivered: date(r["delivered"]))
    }
    return UploadSnapshot(
      at: date(doc["at"]) ?? Date(),
      unreachableSince: date(doc["unreachableSince"]),
      streams: rows.sorted { $0.name < $1.name })
  }
}

// MARK: - Intent

enum OverrideAction: String, AppEnum {
  case pauseHour
  case recordHour
  case end

  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Recording change"
  static let caseDisplayRepresentations: [OverrideAction: DisplayRepresentation] = [
    .pauseHour: "Pause for 1 hour",
    .recordHour: "Record for 1 hour",
    .end: "Back to your hours and places",
  ]
}

/// The widget's one action. An hour, the control's default; the control is
/// where "until I resume" lives.
struct OverrideIntent: AppIntent {
  static let title: LocalizedStringResource = "Change recording"
  static let isDiscoverable = false

  @Parameter(title: "Change")
  var action: OverrideAction

  init() {}

  init(_ action: OverrideAction) {
    self.action = action
  }

  func perform() async throws -> some IntentResult {
    guard RecordingState.read(length: .oneHour).enabled else { return .result() }
    switch action {
    case .pauseHour: OverrideStore.set("silence", minutes: 60)
    case .recordHour: OverrideStore.set("record", minutes: 60)
    case .end: OverrideStore.clear()
    }
    return .result()
  }
}

// MARK: - Views

/// The app's own faces, bundled in this extension (UIAppFonts).
private enum Face {
  static func serif(_ size: CGFloat, _ style: Font.TextStyle) -> Font {
    .custom("EBGaramond-Regular", size: size, relativeTo: style)
  }
  static func mono(_ size: CGFloat, _ style: Font.TextStyle) -> Font {
    .custom("IBMPlexMono-Regular", size: size, relativeTo: style)
  }
}

/// A server out of reach this long is worth a sentence; shorter is a blip.
private let unreachableNotice: TimeInterval = 30 * 60
/// A stream this far behind gets named.
private let behindNotice: TimeInterval = 2 * 3600

struct StatusView: View {
  @Environment(\.widgetFamily) private var family
  let entry: StatusEntry

  var body: some View {
    content.containerBackground(.background, for: .widget)
  }

  @ViewBuilder private var content: some View {
    switch family {
    case .accessoryCircular: circular
    case .accessoryRectangular: rectangular
    case .accessoryInline: Label { line } icon: { Image("virtues.mark") }
    case .systemSmall: small
    case .systemMedium: medium
    default: large
    }
  }

  // MARK: Lock Screen

  private var circular: some View {
    Gauge(value: coverage) {
      EmptyView()
    } currentValueLabel: {
      Image("virtues.mark").font(.title3)
    }
    .gaugeStyle(.accessoryCircularCapacity)
    .widgetAccentable()
  }

  private var rectangular: some View {
    VStack(alignment: .leading, spacing: 1) {
      Label { line } icon: { Image("virtues.mark") }
        .font(.headline)
        .widgetAccentable()
      uploadLine.font(.caption).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  // MARK: Home Screen

  private var small: some View {
    VStack(alignment: .leading, spacing: 0) {
      if let until = countdownEnd {
        countdown(until)
      } else {
        HStack { Spacer(); mark }
        Spacer(minLength: 4)
        sentence(22)
        if let d = detail { d.font(.caption).foregroundStyle(.secondary).padding(.top, 4) }
        Spacer(minLength: 6)
        // Stale needs its room for the sentence; uploads are not the news.
        if let b = actionButton {
          b
        } else if !entry.stale {
          uploadLine.font(.caption).foregroundStyle(.secondary)
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
  }

  private var medium: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(alignment: .firstTextBaseline) {
        if let until = countdownEnd {
          Text("Paused").font(Face.serif(22, .title2))
          Text(timerInterval: entry.date...until, countsDown: true)
            .font(Face.mono(15, .subheadline)).foregroundStyle(.secondary)
        } else {
          sentence(22)
        }
        Spacer(minLength: 8)
        mark
      }
      (detail ?? totals).font(.caption).foregroundStyle(.secondary).padding(.top, 2)
      Spacer(minLength: 8)
      DayView(day: entry.day, now: entry.date)
      HStack(alignment: .firstTextBaseline) {
        uploadLine.font(.caption).foregroundStyle(.secondary).lineLimit(1)
        Spacer(minLength: 8)
        actionButton
      }
      .padding(.top, 6)
    }
  }

  private var large: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(alignment: .firstTextBaseline) {
        if let until = countdownEnd {
          Text("Paused").font(Face.serif(24, .title2))
          Text(timerInterval: entry.date...until, countsDown: true)
            .font(Face.mono(16, .subheadline)).foregroundStyle(.secondary)
        } else {
          sentence(24)
        }
        Spacer(minLength: 8)
        mark
      }
      (detail ?? totals).font(.caption).foregroundStyle(.secondary).padding(.top, 3)
      DayView(day: entry.day, now: entry.date).padding(.top, 14)
      ledger.padding(.top, 16)
      Spacer(minLength: 6)
      HStack {
        if let notice = behindSentence {
          notice.font(.caption).foregroundStyle(.secondary).lineLimit(2)
        }
        Spacer(minLength: 8)
        actionButton
      }
    }
  }

  // MARK: Pieces

  private var mark: some View {
    Image("virtues.mark")
      .font(.body)
      .foregroundStyle(entry.stale ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary))
      .opacity(entry.state.keeping || entry.stale ? 1 : 0.35)
      .widgetAccentable()
  }

  private func sentence(_ size: CGFloat) -> some View {
    line
      .font(Face.serif(size, .title2))
      .lineLimit(2)
      .minimumScaleFactor(0.8)
  }

  /// The one sentence. Fragments, so no period.
  private var line: Text {
    let s = entry.state
    if entry.stale, let beat = s.heartbeat { return Text("No word since \(when(beat))") }
    if !s.enabled { return Text("Recording is off") }
    if s.carPlay { return Text("Paused for CarPlay") }
    switch s.override {
    case "silence":
      return s.until.map { Text("Paused until \(when($0))") } ?? Text("Paused")
    case "record":
      return s.until.map { Text("Recording until \(when($0))") } ?? Text("Recording anyway")
    default: break
    }
    switch s.rule {
    case "schedule": return Text("Muted by your hours")
    case "place": return Text("Muted at this place")
    default: return recordingSince.map { Text("Recording since \(when($0))") } ?? Text("Recording")
    }
  }

  private var detail: Text? {
    let s = entry.state
    if entry.stale { return Text("Open Virtues to start recording again") }
    if !s.enabled { return Text("Turn it on in Virtues") }
    if s.carPlay { return Text("Recording picks up again after CarPlay disconnects") }
    if s.override == "silence", s.until == nil { return Text("Until you resume") }
    if s.override == "record" { return Text("Through your hours and places") }
    return nil
  }

  /// A running pause with an end: shown as a countdown, which ticks with no
  /// refreshes at all.
  private var countdownEnd: Date? {
    let s = entry.state
    guard !entry.stale, s.enabled, !s.carPlay, s.override == "silence",
          let u = s.until, u > entry.date else { return nil }
    return u
  }

  private func countdown(_ until: Date) -> some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack {
        Text("Paused").font(.caption).foregroundStyle(.secondary)
        Spacer()
        mark
      }
      Spacer(minLength: 4)
      Text(timerInterval: entry.date...until, countsDown: true)
        .font(Face.mono(26, .title))
        .lineLimit(1)
        .minimumScaleFactor(0.7)
      Text("until \(when(until))").font(.caption).foregroundStyle(.secondary)
      Spacer(minLength: 6)
      actionButton
    }
  }

  /// When the current kept stretch began, while it is still running.
  private var recordingSince: Date? {
    guard let last = entry.day.last, last.kept,
          entry.date.timeIntervalSince(last.end) < 5 * 60 else { return nil }
    return last.start
  }

  private var totals: Text {
    let kept = entry.day.keptSeconds, muted = entry.day.mutedSeconds
    if kept < 60 && muted < 60 { return Text("Nothing kept yet today") }
    let k = Text("\(span(kept)) kept today")
    return muted >= 60 ? Text("\(k) · \(span(muted)) muted") : k
  }

  /// Share of today accounted for, kept or muted by choice. Only gaps cost.
  private var coverage: Double {
    let elapsed = entry.date.timeIntervalSince(Calendar.current.startOfDay(for: entry.date))
    guard elapsed > 0 else { return 0 }
    return Swift.min(1, (entry.day.keptSeconds + entry.day.mutedSeconds) / elapsed)
  }

  private var uploadLine: Text {
    guard let u = entry.uploads, !u.streams.isEmpty else {
      return Text("Upload status shows after the next sync")
    }
    if let since = u.unreachableSince, entry.date.timeIntervalSince(since) >= unreachableNotice {
      return Text("Can't reach your server since \(when(since))")
    }
    // One stream hours behind would drag "through" back for everything, so
    // the line names it instead and the rest stay current.
    let behind = u.streams.filter { entry.date.timeIntervalSince(u.through($0)) >= behindNotice }
    if let worst = behind.min(by: { u.through($0) < u.through($1) }) {
      let since = when(u.through(worst))
      return behind.count == 1
        ? Text("\(worst.name) behind since \(since)")
        : Text("\(behind.count) streams behind since \(since)")
    }
    return Text("Up to date through \(when(u.through))")
  }

  private var ledger: some View {
    VStack(alignment: .leading, spacing: 7) {
      Text("On your server through").font(.caption).foregroundStyle(.secondary)
      Divider()
      if let u = entry.uploads, !u.streams.isEmpty {
        ForEach(u.streams.prefix(7)) { s in
          let t = u.through(s)
          HStack {
            Text(s.name).font(.subheadline)
            Spacer()
            when(t)
              .font(Face.mono(13, .subheadline))
              .foregroundStyle(
                entry.date.timeIntervalSince(t) >= behindNotice
                  ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
          }
        }
      } else {
        Text("Upload status shows after the next sync").font(.caption).foregroundStyle(.secondary)
      }
    }
  }

  /// The furthest-behind stream, named, when it is hours behind.
  private var behindSentence: Text? {
    guard let u = entry.uploads else { return nil }
    if let since = u.unreachableSince, entry.date.timeIntervalSince(since) >= unreachableNotice {
      return Text("Can't reach your server since \(when(since)).")
    }
    guard let worst = u.streams.min(by: { u.through($0) < u.through($1) }) else { return nil }
    let t = u.through(worst)
    guard entry.date.timeIntervalSince(t) >= behindNotice else { return nil }
    return Text("\(worst.name) hasn't reached your server since \(when(t)).")
  }

  /// The one action, as plain text: Pause, Resume, Record, or Mute again.
  private var actionButton: AnyView? {
    let s = entry.state
    guard !entry.stale, s.enabled, !s.carPlay else { return nil }
    let (action, title): (OverrideAction, String)
    switch (s.override, s.rule) {
    case ("silence", _): (action, title) = (.end, "Resume")
    case ("record", _): (action, title) = (.end, "Mute again")
    case (_, .some): (action, title) = (.recordHour, "Record 1 hr")
    default:
      // Small has no room for Pause beside the sentence; the control has it.
      if family == .systemSmall { return nil }
      (action, title) = (.pauseHour, "Pause 1 hr")
    }
    return AnyView(
      Button(intent: OverrideIntent(action)) {
        Text(title).font(.caption.weight(.semibold))
      }
      .buttonStyle(.plain)
      .foregroundStyle(.tint))
  }

  /// A time today, or a date for anything older.
  private func when(_ d: Date) -> Text {
    Calendar.current.isDate(d, inSameDayAs: entry.date)
      ? Text(d, format: .dateTime.hour().minute())
      : Text(d, format: .dateTime.month(.abbreviated).day())
  }

  private func span(_ seconds: TimeInterval) -> String {
    Duration.seconds(seconds).formatted(
      .units(allowed: [.hours, .minutes], width: .narrow, maximumUnitCount: 2))
  }
}

/// Today from midnight to midnight: kept is solid, muted is faint, a gap is
/// empty, and a hairline marks now.
struct DayView: View {
  let day: [Stretch]
  let now: Date

  var body: some View {
    let start = Calendar.current.startOfDay(for: now)
    let end = Calendar.current.date(byAdding: .day, value: 1, to: start) ?? start + 86_400
    let span = end.timeIntervalSince(start)
    VStack(spacing: 5) {
      GeometryReader { g in
        let w = g.size.width
        ZStack(alignment: .leading) {
          Capsule().strokeBorder(.primary.opacity(0.15), lineWidth: 0.5)
          ForEach(day) { s in
            Rectangle()
              .fill(s.kept ? AnyShapeStyle(.primary) : AnyShapeStyle(.primary.opacity(0.16)))
              .frame(width: Swift.max(1, w * s.end.timeIntervalSince(s.start) / span))
              .offset(x: w * s.start.timeIntervalSince(start) / span)
          }
          .widgetAccentable()
          Rectangle()
            .fill(.orange)
            .frame(width: 1.5)
            .offset(x: w * now.timeIntervalSince(start) / span)
        }
        .clipShape(Capsule())
      }
      .frame(height: 6)
      HStack {
        ForEach(0..<5) { i in
          if i > 0 { Spacer(minLength: 0) }
          Text(verbatim: hour(start.addingTimeInterval(Double(i) * 6 * 3600)))
        }
      }
      .font(.custom("IBMPlexMono-Regular", size: 10, relativeTo: .caption2))
      .foregroundStyle(.secondary)
    }
  }

  /// "12a", "6p": the narrow hour, without the space the formatter puts in.
  private func hour(_ d: Date) -> String {
    d.formatted(.dateTime.hour(.defaultDigits(amPM: .narrow)))
      .filter { !$0.isWhitespace }
  }
}

extension StatusEntry {
  static var preview: StatusEntry {
    let now = Date()
    let midnight = Calendar.current.startOfDay(for: now)
    return StatusEntry(
      date: now,
      state: RecordingState(
        enabled: true, override: nil, until: nil, rule: nil, length: .oneHour,
        heartbeat: now),
      uploads: UploadSnapshot(
        at: now, unreachableSince: nil,
        streams: [
          .init(id: "microphone", queued: 0, oldest: nil, delivered: now),
          .init(id: "location", queued: 0, oldest: nil, delivered: now),
          .init(id: "healthkit", queued: 0, oldest: nil, delivered: now),
        ]),
      day: [
        Stretch(start: midnight, end: midnight + 7 * 3600, kept: false),
        Stretch(start: midnight + 7 * 3600, end: now, kept: true),
      ],
      stale: false)
  }
}
