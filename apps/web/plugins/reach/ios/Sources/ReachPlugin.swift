// The iOS half of the reach plugin: joining a box's setup network.
//
// This is `NEHotspotConfiguration` — Apple's API for JOINING a wifi network.
// Two naming traps have repeatedly derailed conversations about it, so for the
// record: it is NOT Personal Hotspot (no relation to tethering), and it is NOT
// `NEHotspotHelper` (a different NetworkExtension API whose entitlement Apple
// grants case-by-case; ours is the self-serve
// `com.apple.developer.networking.HotspotConfiguration`).
//
// Why this exists at all: onboarding was built around the user manually joining
// `Virtues-XXXX` and iOS then auto-opening a captive portal. On hardware
// (2026-08-10) every OS-mediated step proved flaky in a way we cannot fix: the
// camera-app QR banner sometimes never appears, the captive sheet renders in a
// crippled WebKit and is force-reopened by the OS, and the CNA caches portal
// pages per-SSID across box upgrades. The app joining the network itself
// removes every one of those surfaces from the flow.
//
// Behavior notes, learned from the API's documentation and folklore:
//  * `apply` raises ONE system dialog ("Wants to Join…"). Not silent — but a
//    single tap, inside our own flow, instead of a trip to Settings.
//  * `.alreadyAssociated` is success, not an error: it means the phone is
//    already on the network we asked for.
//  * `joinOnce = true` scopes the join to the app's lifetime, which is exactly
//    a setup session. iOS drops the config afterward on its own, so a failed
//    setup does not leave a dead network pinned in the phone's list.
//  * The prefix initializer (iOS 13+) lets us say "a network starting with
//    `Virtues-`" without knowing the suffix, so the user types only the
//    passphrase shown on the box's screen — never the SSID.

import Tauri
import NetworkExtension

class WifiJoinArgs: Decodable {
  let ssidPrefix: String
  let passphrase: String
}

class WifiForgetArgs: Decodable {
  let ssidPrefix: String
}

class ReachPlugin: Plugin {
  @objc public func wifi_join(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(WifiJoinArgs.self)
    let config = NEHotspotConfiguration(
      ssidPrefix: args.ssidPrefix, passphrase: args.passphrase, isWEP: false)
    config.joinOnce = true
    NEHotspotConfigurationManager.shared.apply(config) { error in
      if let err = error as NSError? {
        if err.domain == NEHotspotConfigurationErrorDomain,
          err.code == NEHotspotConfigurationError.alreadyAssociated.rawValue
        {
          invoke.resolve(["joined": true, "already": true])
          return
        }
        // The message iOS gives is user-meaningful ("could not join", "invalid
        // passphrase") — pass it through rather than rewording.
        invoke.resolve(["joined": false, "error": err.localizedDescription])
        return
      }
      // No error means the association was made (or the user tapped Join).
      invoke.resolve(["joined": true, "already": false])
    }
  }

  @objc public func wifi_forget(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(WifiForgetArgs.self)
    // `joinOnce` configs usually clean up on their own; this is the belt for
    // the case where they do not. Removal needs exact SSIDs, so enumerate.
    NEHotspotConfigurationManager.shared.getConfiguredSSIDs { ssids in
      for ssid in ssids where ssid.hasPrefix(args.ssidPrefix) {
        NEHotspotConfigurationManager.shared.removeConfiguration(forSSID: ssid)
      }
      invoke.resolve(["removed": true])
    }
  }
}

@_cdecl("init_plugin_reach")
func initPlugin() -> Plugin {
  return ReachPlugin()
}
