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
    return [
      "authorized": r.authorized(),
      "recording": r.recording,
      "notify": r.notifyEnabled(),
      "silentDropped": r.silentDroppedCount(),
      "quietStart": quiet.start,
      "quietEnd": quiet.end,
    ]
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
