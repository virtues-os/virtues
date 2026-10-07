import AppIntents
import SwiftUI
import WidgetKit

/// The Virtues control: pause recording from Control Center, the Lock Screen,
/// or the Action button, and record anyway while your hours or a place mute it.
///
/// The toggle answers one question, "is Virtues keeping audio right now?", and
/// a tap flips it for the length chosen when the control was added (an hour,
/// or until you end it). Off from recording is a pause; on from a mute is
/// "record anyway". Either way the next tap goes back to your hours and places.
///
/// It overrides, it never stops. Both are mute reasons in the recorder: the
/// mic stays on, so ending one works from here without opening the app. iOS
/// will not let a backgrounded app turn the mic back on, so a control that
/// really switched it off could not switch it on again. The orange dot stays
/// lit while paused, which is why this says "Paused" and never "Mic off". The
/// real off is the switch in the app.
///
/// This extension runs in its own process. It writes the override into the App
/// Group's defaults and posts a Darwin note; the recorder re-reads on that note
/// and mirrors back what the label needs (MARK: Override in the audio plugin's
/// Audio.swift).
enum RecordingShared {
  /// These must match `AudioRecorder` in Audio.swift, and the group must match
  /// both entitlements files.
  static let appGroup = "group.com.virtues.app"
  static let overrideModeKey = "virtues.audio.override"
  static let overrideUntilKey = "virtues.audio.overrideUntil"
  static let enabledKey = "virtues.audio.enabled"
  static let ruleKey = "virtues.audio.ruleMute"
  static let overrideChangedNote = "com.virtues.audio.override-changed"
  static let kind = "com.virtues.app.recording"
}

@main
struct VirtuesControls: WidgetBundle {
  var body: some Widget {
    RecordingControl()
  }
}

/// How long one tap lasts, chosen when the control is added.
enum OverrideLength: String, AppEnum {
  case oneHour
  case untilEnded

  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Length"
  static let caseDisplayRepresentations: [OverrideLength: DisplayRepresentation] = [
    .oneHour: "1 hour",
    .untilEnded: "Until I resume",
  ]

  var minutes: Int? { self == .oneHour ? 60 : nil }
}

struct RecordingControlConfig: ControlConfigurationIntent {
  static let title: LocalizedStringResource = "Pause recording"

  @Parameter(title: "Pause for", default: .oneHour)
  var length: OverrideLength
}

struct RecordingState {
  /// The in-app switch is on. Off, there is nothing to pause.
  let enabled: Bool
  /// "silence" or "record", only while it is still running.
  let override: String?
  let until: Date?
  /// What your hours and places say: "schedule", "place", or nil to record.
  let rule: String?
  let length: OverrideLength

  /// The toggle's On: Virtues is keeping audio right now.
  var keeping: Bool {
    guard enabled else { return false }
    switch override {
    case "silence": return false
    case "record": return true
    default: return rule == nil
    }
  }

  static func read(length: OverrideLength, now: Date = Date()) -> RecordingState {
    let d = UserDefaults(suiteName: RecordingShared.appGroup)
    let rule = d?.string(forKey: RecordingShared.ruleKey)
    var mode = d?.string(forKey: RecordingShared.overrideModeKey)
    let until = (d?.object(forKey: RecordingShared.overrideUntilKey) as? NSNumber)
      .map { Date(timeIntervalSince1970: $0.doubleValue) }
    // The same ends the recorder enforces, so a label read before the app
    // has cleared a finished override does not show it as running.
    if let u = until, now >= u { mode = nil }
    if mode == "record", rule == nil { mode = nil }
    return RecordingState(
      enabled: d?.bool(forKey: RecordingShared.enabledKey) ?? false,
      override: mode, until: mode == nil ? nil : until, rule: rule, length: length)
  }
}

struct RecordingValue: AppIntentControlValueProvider {
  func previewValue(configuration: RecordingControlConfig) -> RecordingState {
    RecordingState(
      enabled: true, override: nil, until: nil, rule: nil, length: configuration.length)
  }

  func currentValue(configuration: RecordingControlConfig) async throws -> RecordingState {
    RecordingState.read(length: configuration.length)
  }
}

struct RecordingControl: ControlWidget {
  var body: some ControlWidgetConfiguration {
    AppIntentControlConfiguration(kind: RecordingShared.kind, provider: RecordingValue()) { state in
      ControlWidgetToggle(
        "Virtues", isOn: state.keeping, action: SetRecordingIntent(length: state.length)
      ) { _ in
        RecordingLabel(state: state)
      }
    }
    .displayName("Pause recording")
    .description(
      "Pause recording for an hour, or until you resume. The mic stays on while it's paused. When your hours or a place mute recording, tap to record anyway."
    )
  }
}

/// Always the ∴ mark (a custom symbol in this extension's asset catalog);
/// iOS tints it when the toggle is on, and the words say which state.
struct RecordingLabel: View {
  let state: RecordingState

  var body: some View {
    Label { text } icon: { Image("virtues.mark") }
  }

  private var text: Text {
    if !state.enabled { return Text("Off") }
    switch state.override {
    case "silence": return untilText("Paused")
    case "record": return untilText("Recording")
    default: break
    }
    switch state.rule {
    case "schedule": return Text("Muted by your hours")
    case "place": return Text("Muted at this place")
    default: return Text("Recording")
    }
  }

  private func untilText(_ what: String) -> Text {
    guard let u = state.until else { return Text(what) }
    return Text("\(what) until \(u, format: .dateTime.hour().minute())")
  }
}

/// One tap: flip whether Virtues keeps audio, for the configured length.
struct SetRecordingIntent: SetValueIntent {
  static let title: LocalizedStringResource = "Pause or resume recording"

  @Parameter(title: "Recording")
  var value: Bool

  @Parameter(title: "Pause for", default: .oneHour)
  var length: OverrideLength

  init() {}

  init(length: OverrideLength) {
    self.length = length
  }

  func perform() async throws -> some IntentResult {
    let s = RecordingState.read(length: length)
    // Recording is off in the app: there is nothing to pause or record. The
    // control reloads after this and reads Off again.
    guard s.enabled, let d = UserDefaults(suiteName: RecordingShared.appGroup) else {
      return .result()
    }
    if value {
      if s.override == "silence" {
        clear(d)
      } else if s.rule != nil {
        set(d, mode: "record")
      }
    } else {
      if s.override == "record" {
        clear(d)
      } else {
        set(d, mode: "silence")
      }
    }
    CFNotificationCenterPostNotification(
      CFNotificationCenterGetDarwinNotifyCenter(),
      CFNotificationName(RecordingShared.overrideChangedNote as CFString), nil, nil, true)
    return .result()
  }

  private func set(_ d: UserDefaults, mode: String) {
    if let m = length.minutes {
      d.set(Date().addingTimeInterval(Double(m) * 60).timeIntervalSince1970,
            forKey: RecordingShared.overrideUntilKey)
    } else {
      d.removeObject(forKey: RecordingShared.overrideUntilKey)
    }
    d.set(mode, forKey: RecordingShared.overrideModeKey)
  }

  private func clear(_ d: UserDefaults) {
    d.removeObject(forKey: RecordingShared.overrideModeKey)
    d.removeObject(forKey: RecordingShared.overrideUntilKey)
  }
}
