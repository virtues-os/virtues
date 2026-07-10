import Tauri
import UIKit

class AudioPlugin: Plugin {
  /// Explicit "Enable": prompt for microphone access, then start recording.
  @objc public func enable(_ invoke: Invoke) throws {
    AudioRecorder.shared.enable { granted in
      invoke.resolve(["authorized": granted, "recording": AudioRecorder.shared.recording])
    }
  }

  /// Toggle off / pause: finalize the current chunk and stop.
  @objc public func disable(_ invoke: Invoke) throws {
    AudioRecorder.shared.disable()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
    ])
  }

  /// Launch auto-resume: record only if already authorized + left enabled.
  @objc public func resume(_ invoke: Invoke) throws {
    AudioRecorder.shared.resume()
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
    ])
  }

  @objc public func status(_ invoke: Invoke) throws {
    invoke.resolve([
      "authorized": AudioRecorder.shared.authorized(),
      "recording": AudioRecorder.shared.recording,
    ])
  }
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
