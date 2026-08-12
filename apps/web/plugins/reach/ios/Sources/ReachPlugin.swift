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

class ImprovDiscoverArgs: Decodable {
  let seconds: Double?
}

class ImprovTargetArgs: Decodable {
  let id: String
}

class ImprovClaimArgs: Decodable {
  let id: String
  let phrase: String
}

class ImprovProvisionArgs: Decodable {
  let id: String
  let ssid: String
  let password: String
  /// Present = 802.1X; `password` is then the account password.
  let identity: String?
}

class ImprovPairArgs: Decodable {
  let id: String
  let code: String
  let label: String?
  /// This device's iroh EndpointId — the box allowlists it at enrollment.
  let endpointId: String?
}

class ReachPlugin: Plugin {
  // ─── Improv BLE setup (see ImprovClient.swift) ─────────────────────────────

  @objc public func improv_discover(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ImprovDiscoverArgs.self)
    ImprovClient.shared.discover(seconds: args.seconds ?? 4.0) { boxes in
      invoke.resolve(["boxes": boxes])
    }
  }

  @objc public func improv_claim(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ImprovClaimArgs.self)
    ImprovClient.shared.claimSetup(id: args.id, phrase: args.phrase) { err in
      if let err {
        invoke.resolve(["ok": false, "error": err])
      } else {
        invoke.resolve(["ok": true])
      }
    }
  }

  @objc public func improv_wifi_scan(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ImprovTargetArgs.self)
    ImprovClient.shared.wifiScan(id: args.id) { networks, err in
      if let err {
        invoke.resolve(["networks": [] as [[String: Any]], "error": err])
      } else {
        invoke.resolve(["networks": networks ?? []])
      }
    }
  }

  @objc public func improv_provision(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ImprovProvisionArgs.self)
    ImprovClient.shared.provision(
      id: args.id, ssid: args.ssid, password: args.password, identity: args.identity,
      onProgress: { [weak self] stage in
        self?.trigger("improv-progress", data: ["stage": stage])
      },
      completion: { url, err in
        if let err {
          invoke.resolve(["ok": false, "error": err])
        } else {
          invoke.resolve(["ok": true, "url": url ?? ""])
        }
      })
  }

  @objc public func improv_pair(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ImprovPairArgs.self)
    ImprovClient.shared.pair(
      id: args.id, code: args.code, label: args.label ?? "", endpointId: args.endpointId ?? ""
    ) { json, err in
      if let err {
        invoke.resolve(["ok": false, "error": err])
      } else {
        // The consume response verbatim, as a string — the JS parses it with
        // the same code the LAN path uses.
        invoke.resolve(["ok": true, "response": json ?? ""])
      }
    }
  }

  @objc public func improv_disconnect(_ invoke: Invoke) throws {
    ImprovClient.shared.disconnect()
    invoke.resolve()
  }

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
