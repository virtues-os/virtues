// The iOS half of the reach plugin: the Tauri bridge to the CoreBluetooth
// Improv client (`ImprovClient.swift`) — discovering an unclaimed box,
// presenting the four-word phrase (0x86), streaming its wifi scan, watching a
// join, and pairing, all over BLE while the phone never leaves its own network.
//
// This file used to be the NEHotspotConfiguration wifi-join half of the SoftAP
// flow (put the phone on `Virtues-XXXX` programmatically, then talk HTTP).
// That flow and its `com.apple.developer.networking.HotspotConfiguration`
// entitlement were deleted 2026-08-18 — BLE removed the need to move the phone
// between networks at all. See `src/commands.rs` for the tombstone.

import Foundation
import Tauri

class ImprovDiscoverArgs: Decodable {
  let seconds: Double?
}

class ImprovTargetArgs: Decodable {
  let id: String
}

class ImprovClaimArgs: Decodable {
  let id: String
  let phrase: String
  /// This device's name, for the box's panel. Cosmetic, and optional.
  let label: String?
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
    ImprovClient.shared.claimSetup(id: args.id, phrase: args.phrase, label: args.label ?? "") {
      gated, err in
      if let err {
        invoke.resolve(["ok": false, "error": err])
      } else {
        invoke.resolve(["ok": true, "gated": gated])
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
      id: args.id, label: args.label ?? "", endpointId: args.endpointId ?? ""
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
}

@_cdecl("init_plugin_reach")
func initPlugin() -> Plugin {
  return ReachPlugin()
}
