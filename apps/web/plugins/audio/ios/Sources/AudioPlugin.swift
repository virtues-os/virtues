import Tauri
import UIKit

class AudioPlugin: Plugin {
  /// Explicit "Enable": prompt for microphone access, then start recording.
  @objc public func enable(_ invoke: Invoke) throws {
    AudioRecorder.shared.enable { granted in
      invoke.resolve([
        "authorized": granted,
        "recording": AudioRecorder.shared.recording,
        "notify": AudioRecorder.shared.notifyEnabled(),
      ])
    }
  }

  /// Toggle off / pause: finalize the current chunk and stop.
  @objc public func disable(_ invoke: Invoke) throws {
    AudioRecorder.shared.disable()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
      "notify": AudioRecorder.shared.notifyEnabled(),
    ])
  }

  /// Launch auto-resume: record only if already authorized + left enabled.
  @objc public func resume(_ invoke: Invoke) throws {
    AudioRecorder.shared.resume()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
      "notify": AudioRecorder.shared.notifyEnabled(),
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    let quiet = AudioRecorder.shared.quietHours()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
      "notify": AudioRecorder.shared.notifyEnabled(),
      "silentDropped": AudioRecorder.shared.silentDroppedCount(),
      "quietStart": quiet.start,
      "quietEnd": quiet.end,
    ])
  }

  /// Toggle the "notify me if recording stops" gap-nudge (default on).
  @objc public func setNotify(_ invoke: Invoke) throws {
    let on = (try? invoke.parseArgs(NotifyArgs.self))?.enabled ?? true
    AudioRecorder.shared.setNotifyEnabled(on)
    invoke.resolve(["notify": on])
  }

  /// Set the quiet-hours window (minutes since local midnight; -1/-1 = off).
  /// Mute-don't-release: the mic stays hot, chunks stop being written.
  @objc public func setQuietHours(_ invoke: Invoke) throws {
    let args = (try? invoke.parseArgs(QuietHoursArgs.self))
    AudioRecorder.shared.setQuietHours(start: args?.start ?? -1, end: args?.end ?? -1)
    // Resolve the same shape as `status` so the Rust side's AudioStatus parses.
    let quiet = AudioRecorder.shared.quietHours()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
      "notify": AudioRecorder.shared.notifyEnabled(),
      "quietStart": quiet.start,
      "quietEnd": quiet.end,
    ])
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
