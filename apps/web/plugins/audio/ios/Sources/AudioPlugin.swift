import Tauri
import UIKit

class AudioPlugin: Plugin {
  /// The one status payload every command resolves. Rust's AudioStatus REQUIRES
  /// authorized/recording (no serde defaults — a partial payload rejects the
  /// whole invoke, which is how set_notify shipped broken once), and the Svelte
  /// screen assigns the whole response to its `audio` state — so omitting the
  /// quiet fields wipes the quiet-hours UI until the next full load. One
  /// builder, so no resolve site can drift from the shape.
  private static func fullStatus() -> [String: Any] {
    let r = AudioRecorder.shared
    let quiet = r.quietHours()
    var status: [String: Any] = [
      "authorized": r.authorized(),
      "recording": r.recording,
      "notify": r.notifyEnabled(),
      "silentDropped": r.silentDroppedCount(),
      "quietStart": quiet.start,
      "quietEnd": quiet.end,
      // Present on every build that has them: the Svelte editor keys on
      // presence to tell this native build from one that predates the policy.
      "schedule": r.scheduleJSON(),
      "places": r.placesJSON(),
    ]
    if let why = r.mutedBy() { status["mutedBy"] = why }
    // Absent when there is no reason. Rust's Option<String> would decode a
    // JSON null just as well; absent keeps the common payload identical to
    // what shipped before the field existed.
    if let reason = r.pausedReason() { status["pausedReason"] = reason }
    return status
  }

  /// Explicit "Enable": prompt for microphone access, then start recording.
  @objc public func enable(_ invoke: Invoke) throws {
    AudioRecorder.shared.enable { _ in
      invoke.resolve(AudioPlugin.fullStatus())
    }
  }

  /// Toggle off / pause: finalize the current chunk and stop.
  @objc public func disable(_ invoke: Invoke) throws {
    AudioRecorder.shared.disable()
    invoke.resolve(AudioPlugin.fullStatus())
  }

  /// Launch auto-resume: record only if already authorized + left enabled.
  @objc public func resume(_ invoke: Invoke) throws {
    AudioRecorder.shared.resume()
    invoke.resolve(AudioPlugin.fullStatus())
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve(AudioPlugin.fullStatus())
  }

  /// Toggle the "notify me if recording stops" gap-nudge (default on).
  @objc public func setNotify(_ invoke: Invoke) throws {
    let on = (try? invoke.parseArgs(NotifyArgs.self))?.enabled ?? true
    AudioRecorder.shared.setNotifyEnabled(on)
    invoke.resolve(AudioPlugin.fullStatus())
  }

  /// Set the quiet-hours window (minutes since local midnight; -1/-1 = off).
  /// Mute-don't-release: the mic stays hot, chunks stop being written.
  @objc public func setQuietHours(_ invoke: Invoke) throws {
    let args = (try? invoke.parseArgs(QuietHoursArgs.self))
    AudioRecorder.shared.setQuietHours(start: args?.start ?? -1, end: args?.end ?? -1)
    invoke.resolve(AudioPlugin.fullStatus())
  }

  /// Replace the weekly mute schedule. Mute-don't-release, like quiet hours.
  /// A malformed document is rejected rather than read as "no schedule" — an
  /// unparseable edit must not silently turn recording back on everywhere.
  @objc public func setSchedule(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ScheduleArgs.self)
    AudioRecorder.shared.setSchedule(json: args.schedule.json)
    invoke.resolve(AudioPlugin.fullStatus())
  }

  /// Replace the cached muted places (the SPA copies the box's rows here).
  @objc public func setPlaces(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PlacesArgs.self)
    AudioRecorder.shared.setPlaces(json: args.places.map { $0.json })
    invoke.resolve(AudioPlugin.fullStatus())
  }
}

struct ScheduleArgs: Decodable {
  let schedule: ScheduleDoc
}

/// The schedule document as the SPA sends it (see the plugin's models.rs).
struct ScheduleDoc: Decodable {
  let default_muted: Bool?
  let days: [String: [[Int]]]?
  var json: [String: Any] {
    ["default_muted": default_muted ?? false, "days": days ?? [:]]
  }
}

struct PlacesArgs: Decodable {
  let places: [PlaceArg]
}

struct PlaceArg: Decodable {
  let id: String
  let name: String?
  let lat: Double
  let lon: Double
  let radiusM: Double?
  var json: [String: Any] {
    ["id": id, "name": name ?? "", "lat": lat, "lon": lon, "radiusM": radiusM ?? 100]
  }
}

struct NotifyArgs: Decodable {
  let enabled: Bool
}

struct QuietHoursArgs: Decodable {
  let start: Int
  let end: Int
}

@_cdecl("init_plugin_audio")
func initPlugin() -> Plugin {
  return AudioPlugin()
}

/// C-ABI so the always-on location plugin can re-arm audio on a background
/// location update (the piggyback: location keeps the process alive and gives us
/// a heartbeat to retry the mic). No-op unless enabled + authorized; idempotent.
@_cdecl("virtues_ensure_recording")
func virtues_ensure_recording() {
  AudioRecorder.shared.ensureRecording(reason: "location")
}

/// C-ABI: the location plugin hands every fix here for the place gate. Any
/// thread; the recorder locks. A negative accuracy is CoreLocation's "invalid".
@_cdecl("virtues_audio_location")
func virtues_audio_location(_ lat: Double, _ lon: Double, _ accuracyM: Double) {
  AudioRecorder.shared.updateLocation(lat: lat, lon: lon, accuracy: accuracyM)
}

/// C-ABI: a geofence verdict for one muted place (1 = inside, 0 = outside).
@_cdecl("virtues_audio_region")
func virtues_audio_region(_ placeId: UnsafePointer<CChar>, _ inside: Int32) {
  AudioRecorder.shared.regionEvent(placeId: String(cString: placeId), inside: inside != 0)
}
