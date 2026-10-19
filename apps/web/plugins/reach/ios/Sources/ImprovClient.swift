// CoreBluetooth client for the Improv Wi-Fi service the box serves while
// unclaimed (virtues-core `maintenance::ble_provision`).
//
// This is the app's half of the BLE setup path — the one that replaced SoftAP.
// The shape mirrors what the connect screen actually does, as three verbs:
//
//   discover()               → which unclaimed boxes are in radio range?
//   wifiScan(id)             → what networks can THAT BOX see? (RPC 0x04)
//   provision(id, ssid, psk) → put it on one, and watch it happen (RPC 0x01)
//
// One operation in flight at a time, by design. Setup is a single conversation
// with a single box; a queue would only add states nobody is in.
//
// Protocol notes (Improv, improv-wifi.com/ble):
//   RPC packet  = [command, data_len, data…, checksum(low byte of sum)]
//   strings     = length-prefixed inside data
//   wifi scan   = one result packet per network [ssid, rssi, "YES"/"NO"],
//                 terminated by an empty-data packet
//   provision   = state notifications 0x03 (provisioning) → 0x04 (provisioned),
//                 then a result packet carrying the box's URL; failure arrives
//                 as an error notification instead (0x03 = unable to connect)

import CoreBluetooth
import Foundation
import UIKit

/// Why a call failed, as a class a screen can branch on. `code` is one of the
/// `BoxRadioErrorCode`s in `src/lib/tauri/boxRadio.ts`, the same strings the
/// desktop client's `FailureKind` produces, so one screen handles both.
typealias ImprovFailure = (code: String, message: String)

final class ImprovClient: NSObject {
  static let shared = ImprovClient()

  static let serviceUUID = CBUUID(string: "00467768-6228-2272-4663-277478268000")
  static let stateUUID = CBUUID(string: "00467768-6228-2272-4663-277478268001")
  static let errorUUID = CBUUID(string: "00467768-6228-2272-4663-277478268002")
  static let rpcUUID = CBUUID(string: "00467768-6228-2272-4663-277478268003")
  static let resultUUID = CBUUID(string: "00467768-6228-2272-4663-277478268004")

  private var central: CBCentralManager!
  private let queue = DispatchQueue(label: "improv-client")

  // Discovery accumulates here; connect looks peripherals up by identifier.
  private var found: [UUID: (peripheral: CBPeripheral, name: String, state: UInt8, rssi: Int)] = [:]

  // The single in-flight operation's plumbing.
  private var target: CBPeripheral?
  private var rpcChar: CBCharacteristic?
  private var resultChar: CBCharacteristic?
  private var stateChar: CBCharacteristic?
  private var errorChar: CBCharacteristic?
  private var onReady: ((String?) -> Void)?         // connection + notify setup done (err?)
  private var onResult: ((Data) -> Void)?           // each result-characteristic packet
  private var onStateChange: ((UInt8) -> Void)?     // each state notification
  private var onImprovError: ((UInt8) -> Void)?     // each nonzero error notification
  private var pendingWaits: [DispatchWorkItem] = []

  private override init() {
    super.init()
    central = CBCentralManager(delegate: self, queue: queue)
  }

  // ─── discover ─────────────────────────────────────────────────────────────

  /// Scan for Improv boxes for `seconds`, then hand back what was heard.
  /// Filtered on the service UUID, so only Virtues boxes (and other Improv
  /// devices — fine, the name disambiguates) ever appear.
  ///
  /// Completes with `(boxes, reason)`. `reason` is set only when Bluetooth
  /// itself can't scan (off, not allowed, not on this phone), in words for the
  /// screen. Until 2026-09-28 this reported an empty list for all of those, so
  /// "no server here" and "this phone can't look" were the same picture on a
  /// screen that keeps searching. It also answered the FIRST scan after launch
  /// with nothing: the central is created on first use and reports `.unknown`
  /// until CoreBluetooth calls back, so this now waits for it to settle.
  func discover(seconds: Double, completion: @escaping ([[String: Any]], String?) -> Void) {
    queue.async {
      self.found.removeAll()
      self.whenSettled(timeout: 3) { state in
        guard state == .poweredOn else {
          completion([], Self.describe(state))
          return
        }
        self.scan(seconds: seconds) { completion($0, nil) }
      }
    }
  }

  /// Wait (on `queue`) until the central has left `.unknown`/`.resetting`, or
  /// `timeout` passes, then hand over its state.
  private func whenSettled(timeout: Double, _ then: @escaping (CBManagerState) -> Void) {
    let state = central.state
    if state != .unknown && state != .resetting || timeout <= 0 {
      then(state)
      return
    }
    queue.asyncAfter(deadline: .now() + 0.1) {
      self.whenSettled(timeout: timeout - 0.1, then)
    }
  }

  /// Why this phone can't scan, for a person. `nil` when it can.
  static func describe(_ state: CBManagerState) -> String? {
    switch state {
    case .poweredOn: return nil
    case .poweredOff: return "Bluetooth is off. Turn it on in Control Center or Settings, then look again."
    case .unauthorized:
      return "Virtues isn't allowed to use Bluetooth. Turn it on in Settings, then Privacy & Security, then Bluetooth."
    case .unsupported: return "This phone can't use Bluetooth."
    default: return "Bluetooth isn't ready yet. Look again in a moment."
    }
  }

  /// The scan itself, on `queue`, once the central is powered on.
  private func scan(seconds: Double, completion: @escaping ([[String: Any]]) -> Void) {
    central.scanForPeripherals(
      withServices: [Self.serviceUUID],
      options: [CBCentralManagerScanOptionAllowDuplicatesKey: false])
    queue.asyncAfter(deadline: .now() + seconds) {
      self.central.stopScan()
      let list = self.found.map { (id, entry) -> [String: Any] in
        [
          "id": id.uuidString,
          "name": entry.name,
          // Byte 0 of the service data: 0x02 ready, 0x04 already online,
          // 0x01 a claimed server that lost its network and wants its owner.
          "improvState": Int(entry.state),
          "rssi": entry.rssi,
        ]
      }
      completion(list.sorted { ($0["rssi"] as! Int) > ($1["rssi"] as! Int) })
    }
  }

  // ─── connect plumbing ─────────────────────────────────────────────────────

  /// Connect to a discovered box and arm notifications. Idempotent: an
  /// already-connected matching target completes immediately.
  private func ensureConnected(id: String, completion: @escaping (String?) -> Void) {
    guard let uuid = UUID(uuidString: id), let entry = found[uuid] else {
      completion("Your server went out of Bluetooth range. Move closer to it and scan again.")
      return
    }
    if let t = target, t.identifier == uuid, t.state == .connected, rpcChar != nil {
      completion(nil)
      return
    }
    disconnectLocked()
    target = entry.peripheral
    onReady = completion
    entry.peripheral.delegate = self
    central.connect(entry.peripheral, options: nil)
    // A connect that goes nowhere must not hang the UI's promise forever.
    failLater(after: 15, message: "Your phone couldn't reach your server over Bluetooth. Move closer and try again.")
  }

  private func failLater(after: Double, message: String) {
    let work = DispatchWorkItem { [weak self] in
      guard let self, let ready = self.onReady else { return }
      self.onReady = nil
      self.disconnectLocked()
      ready(message)
    }
    pendingWaits.append(work)
    queue.asyncAfter(deadline: .now() + after, execute: work)
  }

  private func cancelWaits() {
    for w in pendingWaits { w.cancel() }
    pendingWaits.removeAll()
  }

  func disconnect() {
    queue.async { self.disconnectLocked() }
  }

  private func disconnectLocked() {
    cancelWaits()
    if let t = target {
      central.cancelPeripheralConnection(t)
    }
    target = nil
    rpcChar = nil
    resultChar = nil
    stateChar = nil
    errorChar = nil
    onReady = nil
    onResult = nil
    onStateChange = nil
    onImprovError = nil
  }

  // ─── the two RPCs the connect screen uses ─────────────────────────────────

  /// RPC 0x86: claim the setup session with the box's four-word phrase.
  ///
  /// Must succeed before wifi, the account grant, or pairing — an unclaimed box
  /// advertises to everyone in radio range, and radio range passes through
  /// walls. The phrase is printed on the box's own panel, so having it proves
  /// line of sight, which is the bar we actually want.
  ///
  /// `label` is this device's name. Not security — the box shows it on its
  /// panel in place of the phrase, so the owner can see their words landed here.
  ///
  /// Completes with `(gated, error)`. `gated == false` means the box's firmware
  /// predates 0x86 and asked for nothing — every released box is in that state,
  /// so treating it as a failure would make the app unable to set one up.
  func claimSetup(
    id: String, phrase: String, label: String, completion: @escaping (Bool, ImprovFailure?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(true, ("not-found", err)); return }
        var done = false
        let finish: (Bool, ImprovFailure?) -> Void = { gated, e in
          guard !done else { return }
          done = true
          self.onResult = nil
          self.onImprovError = nil
          completion(gated, e)
        }
        self.onImprovError = { code in
          // 0x02 UnknownCommand: older firmware, no gate. Not a refusal.
          if code == 0x02 {
            finish(false, nil)
            return
          }
          // One message for wrong words AND a spent attempt budget: the box
          // refuses to distinguish them, and neither do we, so a guesser
          // learns nothing from the shape of the refusal.
          finish(true, ("refused", "Those words don't match. Check the words on your server's screen."))
        }
        self.onResult = { data in
          if Self.parseResult(data, command: 0x86) != nil { finish(true, nil) }
        }
        var payload: [UInt8] = []
        for s in [phrase, label.isEmpty ? UIDevice.current.name : label] {
          let bytes = Array(s.utf8).prefix(255)
          payload.append(UInt8(bytes.count))
          payload.append(contentsOf: bytes)
        }
        self.write(rpc: Self.buildRPC(command: 0x86, data: payload))
        self.queue.asyncAfter(deadline: .now() + 20) {
          finish(true, ("timeout", "Your server didn't answer. Move closer to it and try again."))
        }
      }
    }
  }

  /// RPC 0x82: hand the box an account grant so it can link itself.
  ///
  /// The inversion that made 0x84 unnecessary. Rather than the app reading a
  /// user_code off the box and carrying it to atlas, the already-signed-in app
  /// mints a short-lived grant and injects it here; the box redeems it on its
  /// own outbound poll. The box stays outbound-only and atlas never gains a
  /// path in.
  ///
  /// Session-authorized: 0x86 must have succeeded on this connection.
  ///
  /// The box ACKs with `0x82 ["accepted"]` as soon as it has STORED the grant,
  /// deliberately before redeeming it — redemption may outlive this Bluetooth
  /// session, and grant-then-join is as legal as join-then-grant. So this
  /// completing means the box holds the grant, not that the account is linked.
  ///
  /// This did not exist until 2026-08-28. `improv_grant` was declared in
  /// build.rs, permitted by the ACL, and forwarded by commands.rs to a Swift
  /// method nobody had written — so iOS setup reached the account hand-off and
  /// died on "No command improv_grant found for plugin reach". Lockstep diffs
  /// Rust's generate_handler! against COMMANDS and cannot see this file.
  func claimGrant(id: String, grant: String, completion: @escaping (ImprovFailure?) -> Void) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(("not-found", err)); return }
        var done = false
        let finish: (ImprovFailure?) -> Void = { e in
          guard !done else { return }
          done = true
          self.onResult = nil
          self.onImprovError = nil
          completion(e)
        }
        self.onImprovError = { code in
          // 0x04 NotAuthorized carries two meanings here and the box will not
          // distinguish them: no setup session, or ALREADY LINKED. The second
          // is the one a person actually meets — re-running setup against a box
          // that kept its key — and it is not a failure of theirs, so say what
          // is true of both without alarming.
          if code == 0x04 {
            finish(("refused", "This server is already linked to an account."))
            return
          }
          finish(("failed", "Your server couldn't take the account hand-off (error \(code))."))
        }
        self.onResult = { data in
          if Self.parseResult(data, command: 0x82) != nil { finish(nil) }
        }
        var payload: [UInt8] = []
        let bytes = Array(grant.utf8).prefix(255)
        payload.append(UInt8(bytes.count))
        payload.append(contentsOf: bytes)
        self.write(rpc: Self.buildRPC(command: 0x82, data: payload))
        self.queue.asyncAfter(deadline: .now() + 20) {
          finish(("timeout", "Your server didn't answer. Move closer to it and try again."))
        }
      }
    }
  }

  /// RPC 0x04: ask the BOX what networks it can see. Streams one packet per
  /// network; an empty packet ends the list.
  func wifiScan(id: String, completion: @escaping ([[String: Any]]?, ImprovFailure?) -> Void) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, ("not-found", err)); return }
        var networks: [[String: Any]] = []
        var finished = false
        self.onResult = { data in
          guard !finished, let strings = Self.parseResult(data, command: 0x04) else { return }
          if strings.isEmpty {
            finished = true
            self.onResult = nil
            completion(networks, nil)
            return
          }
          if strings.count >= 3 {
            networks.append([
              "ssid": strings[0],
              "signal": Int(strings[1]) ?? 0,
              // "ENT" is our 802.1X extension to Improv's YES/NO — those
              // networks need a username the BLE protocol can't carry, so the
              // UI routes them to a different path.
              "secured": strings[2] == "YES" || strings[2] == "ENT",
              "enterprise": strings[2] == "ENT",
            ])
          }
        }
        self.write(rpc: Self.buildRPC(command: 0x04, data: []))
        // The box's scan can take a few seconds; cap the whole exchange.
        self.queue.asyncAfter(deadline: .now() + 20) {
          if !finished {
            finished = true
            self.onResult = nil
            completion(networks, networks.isEmpty ? ("timeout", "Your server didn't answer the Wi-Fi scan. Try again.") : nil)
          }
        }
      }
    }
  }

  /// RPC 0x01: send credentials, then watch the join happen. Resolves with the
  /// box's URL on success, or the failure in words on error. This living
  /// progress is the entire reason the BLE path exists — contrast the SoftAP
  /// flow's "the socket died, go and look".
  func provision(
    id: String, ssid: String, password: String, identity: String?,
    onProgress: @escaping (String) -> Void,
    completion: @escaping (String?, ImprovFailure?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, ("not-found", err)); return }
        var done = false
        let finish: (String?, ImprovFailure?) -> Void = { url, err in
          guard !done else { return }
          done = true
          self.onStateChange = nil
          self.onImprovError = nil
          self.onResult = nil
          completion(url, err)
        }
        self.onStateChange = { state in
          if state == 0x03 { onProgress("joining") }
          // 0x04 alone is not success — wait for the result packet with the
          // URL, which follows it. But surface the milestone.
          if state == 0x04 { onProgress("joined") }
        }
        self.onImprovError = { code in
          switch code {
          case 0x03:
            finish(nil, ("join-failed", "Your server couldn't join that network. Check the Wi-Fi password."))
          case 0x04:
            finish(nil, ("refused", "Your server refused the request."))
          default:
            finish(nil, ("failed", "Your server couldn't finish that step (error \(code))."))
          }
        }
        // 0x81 = our enterprise extension (ssid, identity, password); 0x01 =
        // stock Improv (ssid, password). The result echoes whichever we sent.
        let command: UInt8 = identity == nil ? 0x01 : 0x81
        self.onResult = { data in
          if let strings = Self.parseResult(data, command: command) {
            finish(strings.first ?? "", nil)
          }
        }
        var payload: [UInt8] = []
        func pushString(_ v: String) {
          let bytes = Array(v.utf8).prefix(255)
          payload.append(UInt8(bytes.count))
          payload.append(contentsOf: bytes)
        }
        pushString(ssid)
        if let identity { pushString(identity) }
        pushString(password)
        self.write(rpc: Self.buildRPC(command: command, data: payload))
        onProgress("sent")
        // A join is bounded by nmcli's own timeout on the box; add slack.
        self.queue.asyncAfter(deadline: .now() + 45) {
          finish(nil, ("timeout", "Your server didn't answer in time. It may still be joining."))
        }
      }
    }
  }

  /// RPC 0x83 (our extension): pair THROUGH the box's Bluetooth, for LANs
  /// that block peer-to-peer (office client isolation).
  ///
  /// CODELESS. The box fetches its OWN standing code and redeems it against
  /// its own consume endpoint over loopback, then streams the response back.
  /// This command is authorized by the setup SESSION — opened by the
  /// four-word phrase on the box's panel (0x86) — so a code here would prove
  /// nothing that has not already been proven.
  ///
  /// The wire is `[kind, source, label, endpoint_id]` — FOUR strings, and the
  /// box rejects a fifth. It carried a leading 6-digit code until 2026-08-24;
  /// this client kept sending it, so `parse_rpc` failed every packet on
  /// `!rest.is_empty()` and Bluetooth pairing could not succeed at all. If you
  /// change this list, change `Command::PairConsume` in
  /// crates/virtues-improv/src/protocol.rs with it — nothing compiles the two
  /// together, which is exactly how they drifted.
  ///
  /// The response is chunked (Improv frames cap at 255 data bytes): JSON
  /// chunks until an empty terminator; a body starting `error:` is a refusal.
  func pair(
    id: String, label: String, endpointId: String,
    completion: @escaping (String?, ImprovFailure?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, ("not-found", err)); return }
        var body = ""
        var done = false
        let finish: (String?, ImprovFailure?) -> Void = { json, err in
          guard !done else { return }
          done = true
          self.onResult = nil
          completion(json, err)
        }
        self.onResult = { data in
          guard let strings = Self.parseResult(data, command: 0x83) else { return }
          if strings.isEmpty {
            // Terminator: the body is complete.
            if body.hasPrefix("error:") {
              let code = String(body.dropFirst("error:".count))
              let msg: String
              // No code is typed on this path any more, so none of these may
              // tell the owner to check one — they'd be hunting a screen that
              // shows nothing to hunt. The failures here are the BOX's own
              // internal redemption, which the owner can only answer by
              // starting setup again.
              switch code {
              case "invalid_or_expired_token":
                msg = "Your server's setup code expired before pairing finished. Start setup again."
              case "too_many_attempts":
                msg = "Too many pairing attempts on your server. Wait a few minutes and try again."
              default:
                msg = "Your server couldn't complete pairing (\(code))."
              }
              finish(nil, ("refused", msg))
            } else {
              finish(body, nil)
            }
            return
          }
          body += strings.joined()
        }
        var payload: [UInt8] = []
        func pushString(_ v: String) {
          let bytes = Array(v.utf8).prefix(255)
          payload.append(UInt8(bytes.count))
          payload.append(contentsOf: bytes)
        }
        // Exactly four, in this order. `source = "ios"` because the phone IS a
        // collector (HealthKit, location, calendar), so it earns the ios ingest
        // fan-out — unlike the desktop app, which sends "" deliberately and is
        // filed as `__device__`.
        pushString("mobile_app")
        pushString("ios")
        pushString(label)
        pushString(endpointId)
        self.write(rpc: Self.buildRPC(command: 0x83, data: payload))
        // The server may spend up to 45s finishing its reach ticket, then up to
        // 15s on its own loopback consume, before it answers. This was 25s
        // until 2026-09-27 and reported failures the server then completed.
        // Same bound as PAIR_TIMEOUT in crates/virtues-improv/src/client.rs.
        self.queue.asyncAfter(deadline: .now() + 70) {
          finish(nil, ("timeout", "Pairing over Bluetooth timed out. Try again."))
        }
      }
    }
  }

  // ─── the moved box: prove this phone is one of its own ────────────────────

  /// RPC 0x88: ask a CLAIMED, offline server for a one-time owner challenge.
  /// Completes with the nonce as hex, or `(code, message)` on failure, where
  /// `code` is a `BoxRadioErrorCode` (`src/lib/tauri/boxRadio.ts`).
  ///
  /// Signing happens in Rust, beside the key (`virtues_reach_client::owner`):
  /// this file only carries the nonce out and the signature back in, so the
  /// seed never crosses into Swift.
  func ownerChallenge(
    id: String, completion: @escaping (String?, ImprovFailure?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, ("not-found", err)); return }
        var done = false
        let finish: (String?, ImprovFailure?) -> Void = { nonce, e in
          guard !done else { return }
          done = true
          self.onResult = nil
          self.onImprovError = nil
          completion(nonce, e)
        }
        self.onImprovError = { code in
          if code == 0x02 {
            finish(nil, ("unsupported", "Your server needs an update before you can reconnect it over Bluetooth."))
          } else if code == 0x04 {
            finish(nil, ("refused", "This server isn't asking for its owner."))
          } else {
            // A garbled packet is a failure to talk, not an answer about ownership.
            finish(nil, ("failed", "Your server couldn't read that request. Try again."))
          }
        }
        self.onResult = { data in
          if let strings = Self.parseResult(data, command: 0x88) {
            finish(strings.first ?? "", nil)
          }
        }
        self.write(rpc: Self.buildRPC(command: 0x88, data: []))
        self.queue.asyncAfter(deadline: .now() + 20) {
          finish(nil, ("timeout", "Your server didn't answer. Try again."))
        }
      }
    }
  }

  /// RPC 0x89: present the signed challenge — `[endpoint_id, signature]`, both
  /// hex, exactly as `Command::OwnerProve` in crates/virtues-improv parses it.
  /// Success opens a wifi-only session on this connection.
  func ownerProve(
    id: String, endpointId: String, signature: String,
    completion: @escaping (ImprovFailure?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(("not-found", err)); return }
        var done = false
        let finish: (ImprovFailure?) -> Void = { e in
          guard !done else { return }
          done = true
          self.onResult = nil
          self.onImprovError = nil
          completion(e)
        }
        // Only 0x04 NotAuthorized means "not yours" (or a stale challenge,
        // which the caller retries once): the server gives one answer for
        // both, deliberately. A garbled packet is a failure to talk, and
        // calling it a refusal made /reconnect hide the owner's own server.
        self.onImprovError = { code in
          if code == 0x04 {
            finish(("refused", "This server doesn't recognize this phone."))
          } else {
            finish(("failed", "Your server couldn't read that request. Try again."))
          }
        }
        self.onResult = { data in
          if Self.parseResult(data, command: 0x89) != nil { finish(nil) }
        }
        var payload: [UInt8] = []
        for v in [endpointId, signature] {
          let bytes = Array(v.utf8).prefix(255)
          payload.append(UInt8(bytes.count))
          payload.append(contentsOf: bytes)
        }
        self.write(rpc: Self.buildRPC(command: 0x89, data: payload))
        self.queue.asyncAfter(deadline: .now() + 20) {
          finish(("timeout", "Your server didn't answer. Try again."))
        }
      }
    }
  }

  private func write(rpc: Data) {
    guard let t = target, let c = rpcChar else { return }
    t.writeValue(rpc, for: c, type: .withResponse)
  }

  // ─── framing ──────────────────────────────────────────────────────────────

  static func buildRPC(command: UInt8, data: [UInt8]) -> Data {
    var packet: [UInt8] = [command, UInt8(data.count)]
    packet.append(contentsOf: data)
    let ck = packet.reduce(UInt8(0)) { $0 &+ $1 }
    packet.append(ck)
    return Data(packet)
  }

  /// Parse a result packet for `command`; nil if malformed or another
  /// command's result. Empty array = the empty terminator packet.
  static func parseResult(_ data: Data, command: UInt8) -> [String]? {
    let bytes = [UInt8](data)
    guard bytes.count >= 3, bytes[0] == command else { return nil }
    let body = bytes.dropLast()
    let ck = body.reduce(UInt8(0)) { $0 &+ $1 }
    guard ck == bytes[bytes.count - 1] else { return nil }
    let dataLen = Int(bytes[1])
    guard body.count == 2 + dataLen else { return nil }
    var strings: [String] = []
    var i = 2
    while i < 2 + dataLen {
      let len = Int(bytes[i])
      guard i + 1 + len <= 2 + dataLen else { return nil }
      let s = String(bytes: bytes[(i + 1)..<(i + 1 + len)], encoding: .utf8) ?? ""
      strings.append(s)
      i += 1 + len
    }
    return strings
  }
}

// ─── CoreBluetooth delegates ──────────────────────────────────────────────────

extension ImprovClient: CBCentralManagerDelegate {
  func centralManagerDidUpdateState(_ central: CBCentralManager) {
    // Discovery reads .state at call time; nothing to do eagerly.
  }

  func centralManager(
    _ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
    advertisementData: [String: Any], rssi RSSI: NSNumber
  ) {
    let name = peripheral.name
      ?? (advertisementData[CBAdvertisementDataLocalNameKey] as? String)
      ?? "Virtues box"
    // Improv advertisement carries [state, capabilities, …] as service data
    // under the 16-bit UUID 0x4677.
    var state: UInt8 = 0
    if let sd = advertisementData[CBAdvertisementDataServiceDataKey] as? [CBUUID: Data],
      let d = sd[CBUUID(string: "4677")], !d.isEmpty
    {
      state = d[d.startIndex]
    }
    found[peripheral.identifier] = (peripheral, name, state, RSSI.intValue)
  }

  func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
    peripheral.discoverServices([Self.serviceUUID])
  }

  func centralManager(
    _ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?
  ) {
    if let ready = onReady {
      onReady = nil
      cancelWaits()
      ready(error?.localizedDescription ?? "Your phone couldn't reach your server.")
    }
  }

  func centralManager(
    _ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?
  ) {
    // Mid-operation disconnect surfaces as the operation's timeout; the next
    // ensureConnected reconnects fresh.
    if peripheral.identifier == target?.identifier {
      rpcChar = nil
      resultChar = nil
      stateChar = nil
      errorChar = nil
    }
  }
}

extension ImprovClient: CBPeripheralDelegate {
  func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
    guard let service = peripheral.services?.first(where: { $0.uuid == Self.serviceUUID }) else {
      if let ready = onReady {
        onReady = nil
        cancelWaits()
        ready("that device doesn't offer Virtues setup")
      }
      return
    }
    peripheral.discoverCharacteristics(
      [Self.stateUUID, Self.errorUUID, Self.rpcUUID, Self.resultUUID], for: service)
  }

  func peripheral(
    _ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?
  ) {
    for c in service.characteristics ?? [] {
      switch c.uuid {
      case Self.rpcUUID: rpcChar = c
      case Self.resultUUID:
        resultChar = c
        peripheral.setNotifyValue(true, for: c)
      case Self.stateUUID:
        stateChar = c
        peripheral.setNotifyValue(true, for: c)
      case Self.errorUUID:
        errorChar = c
        peripheral.setNotifyValue(true, for: c)
      default: break
      }
    }
    if rpcChar != nil, resultChar != nil, let ready = onReady {
      onReady = nil
      cancelWaits()
      ready(nil)
    }
  }

  func peripheral(
    _ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?
  ) {
    guard let data = characteristic.value else { return }
    switch characteristic.uuid {
    case Self.resultUUID:
      onResult?(data)
    case Self.stateUUID:
      if let b = data.first { onStateChange?(b) }
    case Self.errorUUID:
      if let b = data.first, b != 0 { onImprovError?(b) }
    default: break
    }
  }
}
