import AVFoundation
import Foundation

/// The two grants call recording needs: System Audio Recording for the far
/// side and Microphone for the owner's side.
///
/// macOS has no public API that reports System Audio Recording. A denied or
/// unanswered grant does not fail: the tap delivers silence with `noErr`, which
/// looks exactly like a quiet call. So the collector asks TCC directly through
/// its private `TCCAccessPreflight` / `TCCAccessRequest`, the same calls
/// AudioCap and Hyprnote use. If a future macOS removes them, `status` reports
/// nil (unknown) and recording still runs.
enum CallAudioPermission {
    /// true granted, false denied, nil not asked yet or unknown.
    static var systemAudio: Bool? {
        guard let preflight = tcc.preflight else { return nil }
        switch preflight(audioCaptureService, nil) {
        case 0: return true
        case 1: return false
        default: return nil
        }
    }

    /// true granted, false denied, nil not asked yet.
    static var microphone: Bool? {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return true
        case .denied, .restricted: return false
        default: return nil
        }
    }

    /// Ask for whichever of the two has not been asked yet. Called at daemon
    /// start, which runs during Setup, so the prompts appear then and never at
    /// the start of a call: an unanswered prompt holds capture until it is
    /// answered.
    static func requestIfNeeded() {
        if systemAudio == nil, let request = tcc.request {
            request(audioCaptureService, nil) { granted in
                print("System Audio Recording: \(granted ? "granted" : "DENIED (calls will record only your side)")")
            }
        }
        if microphone == nil {
            AVCaptureDevice.requestAccess(for: .audio) { granted in
                print("Microphone: \(granted ? "granted" : "DENIED (calls will record only the other side)")")
            }
        }
    }

    private static let audioCaptureService = "kTCCServiceAudioCapture" as CFString

    private typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int
    private typealias Request = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void

    private static let tcc: (preflight: Preflight?, request: Request?) = {
        guard let handle = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW) else {
            return (nil, nil)
        }
        return (
            dlsym(handle, "TCCAccessPreflight").map { unsafeBitCast($0, to: Preflight.self) },
            dlsym(handle, "TCCAccessRequest").map { unsafeBitCast($0, to: Request.self) }
        )
    }()
}
