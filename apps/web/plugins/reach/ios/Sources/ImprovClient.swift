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
  func discover(seconds: Double, completion: @escaping ([[String: Any]]) -> Void) {
    queue.async {
      self.found.removeAll()
      guard self.central.state == .poweredOn else {
        // Report empty rather than error: the JS treats "nothing found" and
        // "no bluetooth" the same way — fall back to other discovery.
        completion([])
        return
      }
      self.central.scanForPeripherals(
        withServices: [Self.serviceUUID],
        options: [CBCentralManagerScanOptionAllowDuplicatesKey: false])
      self.queue.asyncAfter(deadline: .now() + seconds) {
        self.central.stopScan()
        let list = self.found.map { (id, entry) -> [String: Any] in
          [
            "id": id.uuidString,
            "name": entry.name,
            // Byte 0 of the service data: 0x02 ready, 0x04 already online.
            "improvState": Int(entry.state),
            "rssi": entry.rssi,
          ]
        }
        completion(list.sorted { ($0["rssi"] as! Int) > ($1["rssi"] as! Int) })
      }
    }
  }

  // ─── connect plumbing ─────────────────────────────────────────────────────

  /// Connect to a discovered box and arm notifications. Idempotent: an
  /// already-connected matching target completes immediately.
  private func ensureConnected(id: String, completion: @escaping (String?) -> Void) {
    guard let uuid = UUID(uuidString: id), let entry = found[uuid] else {
      completion("that box is no longer in range — scan again")
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
    failLater(after: 15, message: "couldn't connect to the box over Bluetooth")
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

  /// RPC 0x04: ask the BOX what networks it can see. Streams one packet per
  /// network; an empty packet ends the list.
  func wifiScan(id: String, completion: @escaping ([[String: Any]]?, String?) -> Void) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, err); return }
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
            completion(networks, networks.isEmpty ? "the box didn't answer the scan" : nil)
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
    id: String, ssid: String, password: String,
    onProgress: @escaping (String) -> Void,
    completion: @escaping (String?, String?) -> Void
  ) {
    queue.async {
      self.ensureConnected(id: id) { err in
        if let err { completion(nil, err); return }
        var done = false
        let finish: (String?, String?) -> Void = { url, err in
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
          let msg: String
          switch code {
          case 0x03: msg = "The box couldn't join that network — usually a wrong password."
          default: msg = "Setup failed on the box (error \(code))."
          }
          finish(nil, msg)
        }
        self.onResult = { data in
          if let strings = Self.parseResult(data, command: 0x01) {
            finish(strings.first ?? "", nil)
          }
        }
        var payload: [UInt8] = []
        let ssidBytes = Array(ssid.utf8).prefix(255)
        let pskBytes = Array(password.utf8).prefix(255)
        payload.append(UInt8(ssidBytes.count))
        payload.append(contentsOf: ssidBytes)
        payload.append(UInt8(pskBytes.count))
        payload.append(contentsOf: pskBytes)
        self.write(rpc: Self.buildRPC(command: 0x01, data: payload))
        onProgress("sent")
        // A join is bounded by nmcli's own timeout on the box; add slack.
        self.queue.asyncAfter(deadline: .now() + 45) {
          finish(nil, "Timed out waiting for the box — it may still be joining. Check its screen.")
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
      ready(error?.localizedDescription ?? "couldn't connect to the box")
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
