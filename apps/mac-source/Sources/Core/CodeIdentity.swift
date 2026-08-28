import Foundation
import Security

/// What macOS will use to recognise this binary across rebuilds.
///
/// This type exists because of a three-day iMessage outage. TCC stores each
/// grant as an authorization value *and* a `csreq` — a code requirement
/// captured when the grant was made. System Settings draws the toggle from the
/// authorization; the kernel enforces the requirement. For a Developer ID
/// signed binary the requirement is an identity ("this signing identifier, this
/// team"), which survives any number of rebuilds. For an ad-hoc signed one
/// there is no identity to name, so the requirement degrades to *this exact
/// cdhash* — and the next `swift build` produces a different one.
///
/// The failure that follows is the worst shape available: the switch stays
/// blue, `tccutil` still lists the grant, and every read returns "Operation not
/// permitted". Toggling off and on does not repair it, because that rewrites
/// the authorization and not the requirement — the entry has to be removed and
/// re-added. Meanwhile the collector keeps running and keeps uploading, so the
/// only symptom is that iMessages, Safari history and window titles quietly
/// stop arriving while app focus events (which need no grant at all) continue.
enum CodeIdentity {
    /// `true` when the binary at `path` is ad-hoc signed, `false` when it
    /// carries a real signing identity, `nil` when we could not tell.
    ///
    /// `nil` is deliberately distinct from `false`: a probe that failed must
    /// never be read as "this one is fine". Callers block on `true` only, so an
    /// unreadable signature degrades to permitting the install rather than to
    /// asserting a safety it never established.
    static func isAdHocSigned(path: String) -> Bool? {
        var staticCode: SecStaticCode?
        let url = URL(fileURLWithPath: path) as CFURL
        guard SecStaticCodeCreateWithPath(url, [], &staticCode) == errSecSuccess,
              let code = staticCode
        else { return nil }

        var info: CFDictionary?
        let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
        guard SecCodeCopySigningInformation(code, flags, &info) == errSecSuccess,
              let dict = info as? [String: Any],
              let signatureFlags = dict[kSecCodeInfoFlags as String] as? UInt32
        else { return nil }

        return signatureFlags & Self.adhocFlag != 0
    }

    /// `kSecCodeSignatureAdhoc` — the CS_ADHOC bit of the code directory.
    /// Spelled out because CSCommon.h declares it in an anonymous enum that
    /// does not survive the bridge into Swift. It is ABI, so it is stable; the
    /// same bit is what `codesign -dvvv` prints as `flags=0x2(adhoc)`.
    private static let adhocFlag: UInt32 = 0x0000_0002

    /// The signing identifier, for diagnostics. `nil` when unsigned/unreadable.
    static func signingIdentifier(path: String) -> String? {
        var staticCode: SecStaticCode?
        let url = URL(fileURLWithPath: path) as CFURL
        guard SecStaticCodeCreateWithPath(url, [], &staticCode) == errSecSuccess,
              let code = staticCode
        else { return nil }

        var info: CFDictionary?
        let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
        guard SecCodeCopySigningInformation(code, flags, &info) == errSecSuccess,
              let dict = info as? [String: Any]
        else { return nil }

        return dict[kSecCodeInfoIdentifier as String] as? String
    }
}
