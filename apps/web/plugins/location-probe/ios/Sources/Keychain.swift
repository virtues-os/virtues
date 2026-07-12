import Foundation
import Security

// Keychain-backed storage for the paired-box record (the Ed25519 seed + box info).
//
// Why here: the reach plugin (which owns pairing) has no iOS Swift package, and the
// whole app links into one binary so `@_cdecl` symbols resolve globally — the same
// trick `virtues_enqueue` uses (defined in reach's Rust ffi.rs, called from Swift).
// So the reach Rust store calls these to persist the pairing in the Keychain, which
// — unlike a Documents file — SURVIVES app deletion/reinstall. Cleared explicitly by
// reach `forget` (the Unpair/Reset button).

private let kcService = "com.virtues.app"
private let kcAccount = "paired-box"

private func kcBaseQuery() -> [String: Any] {
  [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: kcService,
    kSecAttrAccount as String: kcAccount,
  ]
}

/// Store the paired-box JSON, replacing any existing item. Returns 0 on success,
/// else the OSStatus.
@_cdecl("virtues_keychain_save")
func virtues_keychain_save(_ json: UnsafePointer<CChar>) -> Int32 {
  let data = Data(String(cString: json).utf8)
  SecItemDelete(kcBaseQuery() as CFDictionary)  // replace-if-present
  var add = kcBaseQuery()
  add[kSecValueData as String] = data
  // AfterFirstUnlock: the background drain (sig-loc / BGTask wakes) must read the
  // seed while the phone is locked. ThisDeviceOnly: the seed never syncs to iCloud
  // or restores onto a different device (a pairing is device-specific).
  add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
  return Int32(SecItemAdd(add as CFDictionary, nil))
}

/// Load the paired-box JSON as a malloc'd C string (caller frees via
/// `virtues_keychain_free`), or NULL if absent.
@_cdecl("virtues_keychain_load")
func virtues_keychain_load() -> UnsafeMutablePointer<CChar>? {
  var q = kcBaseQuery()
  q[kSecReturnData as String] = true
  q[kSecMatchLimit as String] = kSecMatchLimitOne
  var out: AnyObject?
  let status = SecItemCopyMatching(q as CFDictionary, &out)
  guard status == errSecSuccess, let data = out as? Data,
    let str = String(data: data, encoding: .utf8)
  else { return nil }
  return strdup(str)
}

/// Delete the paired-box item. Returns 0 on success or if already absent.
@_cdecl("virtues_keychain_delete")
func virtues_keychain_delete() -> Int32 {
  let status = SecItemDelete(kcBaseQuery() as CFDictionary)
  return (status == errSecSuccess || status == errSecItemNotFound) ? 0 : Int32(status)
}

/// Free a C string returned by `virtues_keychain_load`.
@_cdecl("virtues_keychain_free")
func virtues_keychain_free(_ ptr: UnsafeMutablePointer<CChar>?) {
  if let ptr = ptr { free(ptr) }
}
