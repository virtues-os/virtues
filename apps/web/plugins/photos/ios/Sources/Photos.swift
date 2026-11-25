import CoreLocation
import Foundation
import Network
import Photos
import UIKit

// The shared outbox and the link to the server (the reach plugin's ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32
@_silgen_name("virtues_outbox_queued")
private func virtues_outbox_queued(_ stream: UnsafePointer<CChar>) -> Int64
@_silgen_name("virtues_pairing_key")
private func virtues_pairing_key(_ buffer: UnsafeMutablePointer<CChar>, _ length: Int) -> Int32
@_silgen_name("virtues_drain_blocking")
private func virtues_drain_blocking(_ timeoutSecs: Int32) -> Int32

// RFC 3339 in UTC, ending in Z: the formatter's default time zone is GMT.
private let iso: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime]
  return f
}()

/// Photo favorites collector: one record per favorite photo or video → shared
/// outbox → server. Nothing is read or sent for a photo that is not a favorite.
///
/// Each record carries the photo's details and a JPEG at most 2048 pixels on
/// its longest side. The original stays on the phone.
///
/// A pass compares the favorites in the library now with the ledger: what this
/// phone has already sent to this server. A favorite the ledger does not have,
/// or has at an older edit, is sent. One the ledger has and the library no
/// longer offers (unfavorited, deleted, hidden, or left out of Limited access)
/// is sent as removed. The ledger names the pairing it belongs to, so a phone
/// paired again sends everything again.
public final class PhotosCollector: NSObject, PHPhotoLibraryChangeObserver {
  public static let shared = PhotosCollector()

  private static let stream = "photos"

  /// The longest side of a favorite's picture, in pixels.
  private static let pictureMaxPixel: CGFloat = 2048
  /// Records waiting in the outbox before a pass stops to let them leave. A
  /// picture is several hundred kilobytes, and the photo library keeps every
  /// one, so there is no reason to hold many copies in the queue.
  private static let maxWaiting: Int64 = 20
  /// Favorites sent between two saves of the ledger.
  private static let saveEvery = 10
  /// How long one picture may take, including its download from iCloud.
  private static let pictureTimeout: TimeInterval = 60

  /// Every pass runs here, one at a time, so a launch, a "Sync now" and a
  /// library change can never read the library twice at once.
  private let queue = DispatchQueue(label: "com.virtues.photos", qos: .utility)
  private let lock = NSLock()
  private var running = false
  private var scheduled = false
  private var observing = false
  private var changeDebounce: DispatchWorkItem?
  private var retry: DispatchWorkItem?

  /// Wi-Fi or another connection that is not metered. A favorite kept only in
  /// iCloud is downloaded only on one of these.
  private let pathMonitor = NWPathMonitor()
  private var onCheapNetwork = false
  /// The last pass left out a favorite whose picture was not available.
  private var picturesMissing = false

  private override init() {}

  // MARK: Status

  public var isCollecting: Bool {
    lock.lock()
    defer { lock.unlock() }
    return running
  }

  private var accessStatus: PHAuthorizationStatus {
    PHPhotoLibrary.authorizationStatus(for: .readWrite)
  }

  /// Full or limited access. Limited access sees only the photos the person
  /// picked in the iOS sheet, and `limited()` says so to the device screen.
  public func authorized() -> Bool {
    let s = accessStatus
    return s == .authorized || s == .limited
  }

  public func limited() -> Bool { accessStatus == .limited }

  public func statusDict() -> [String: Any?] {
    [
      "authorized": authorized(),
      "limited": limited(),
      "collecting": isCollecting,
    ]
  }

  // MARK: Entry points

  /// Explicit opt-in: ask for library access, then start. Reading the library
  /// needs the `.readWrite` level; PhotoKit offers no read-only level and no
  /// favorites-only one.
  public func enable(_ completion: @escaping (Bool) -> Void) {
    PHPhotoLibrary.requestAuthorization(for: .readWrite) { [weak self] status in
      let ok = status == .authorized || status == .limited
      DispatchQueue.main.async {
        if ok { self?.start() }
        completion(ok)
      }
    }
  }

  /// Launch: start only if access was already given. It never prompts.
  public func resume() {
    guard authorized() else { return }
    start()
  }

  /// "Sync now": send whatever changed since the last pass.
  public func collect() {
    guard authorized() else { return }
    start()
  }

  private func start() {
    observe()
    schedule()
  }

  /// Registering before access is given can show the permission prompt, so
  /// this runs only once access exists.
  private func observe() {
    lock.lock()
    let already = observing
    observing = true
    lock.unlock()
    if already { return }
    PHPhotoLibrary.shared().register(self)
    pathMonitor.pathUpdateHandler = { [weak self] path in
      guard let self = self else { return }
      let cheap = path.status == .satisfied && !path.isExpensive && !path.isConstrained
      self.lock.lock()
      let retry = cheap && !self.onCheapNetwork && self.picturesMissing
      self.onCheapNetwork = cheap
      self.lock.unlock()
      if retry { self.schedule() }
    }
    pathMonitor.start(queue: DispatchQueue.global(qos: .utility))
  }

  /// A library change while the app runs. PhotoKit calls this on a background
  /// queue, often several times for one edit, so the pass waits a few seconds
  /// and runs once.
  public func photoLibraryDidChange(_ changeInstance: PHChange) {
    let work = DispatchWorkItem { [weak self] in self?.schedule() }
    lock.lock()
    changeDebounce?.cancel()
    changeDebounce = work
    lock.unlock()
    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 5, execute: work)
  }

  /// Queue one pass. A trigger that arrives while a pass is waiting is folded
  /// into it; one that arrives while a pass runs queues exactly one more, so
  /// a change made mid-pass is still read.
  private func schedule() {
    lock.lock()
    if scheduled {
      lock.unlock()
      return
    }
    scheduled = true
    lock.unlock()
    queue.async { [weak self] in
      guard let self = self else { return }
      self.lock.lock()
      self.scheduled = false
      self.running = true
      self.lock.unlock()
      self.run()
      self.lock.lock()
      self.running = false
      self.lock.unlock()
    }
  }

  private func run() {
    guard authorized(), let pairing = pairingKey() else { return }
    // Ask iOS for time to finish if the app is sent to the background
    // mid-pass. When the time runs out the pass just pauses.
    var task = UIBackgroundTaskIdentifier.invalid
    task = UIApplication.shared.beginBackgroundTask(withName: "virtues.photos") {
      UIApplication.shared.endBackgroundTask(task)
      task = .invalid
    }
    defer {
      if task != .invalid { UIApplication.shared.endBackgroundTask(task) }
    }
    pass(pairing)
  }

  // MARK: The pass

  private func pass(_ pairing: String) {
    lock.lock()
    let startedOnCheapNetwork = onCheapNetwork
    lock.unlock()
    var ledger = loadLedger(pairing)
    let favorites = fetchFavorites()

    if !sendRemovals(favorites, &ledger) { return }

    var sent = 0
    var missing = false
    for index in 0..<favorites.count {
      let asset = favorites.object(at: index)
      let stamp = Self.stamp(asset)
      if ledger[asset.localIdentifier] == stamp { continue }
      guard makeRoom() else {
        // The queue is full and the server did not take any of it. What is
        // left goes on a later pass: the next launch, library change, sync or
        // this retry, whichever comes first.
        saveLedger(ledger, pairing)
        retryLater()
        return
      }
      var ok = true
      autoreleasepool {
        guard let record = record(asset) else {
          missing = true
          return
        }
        ok = enqueue(record)
        if ok { ledger[asset.localIdentifier] = stamp }
      }
      guard ok else {
        saveLedger(ledger, pairing)
        return
      }
      sent += 1
      if sent % Self.saveEvery == 0 { saveLedger(ledger, pairing) }
    }
    saveLedger(ledger, pairing)
    lock.lock()
    picturesMissing = missing
    // The network became unmetered during the pass, after some pictures were
    // already left out.
    let again = missing && onCheapNetwork && !startedOnCheapNetwork
    lock.unlock()
    if again { schedule() }
  }

  /// Run another pass in a few minutes. A pass that asks again replaces the
  /// one waiting, so there is never more than one.
  private func retryLater() {
    let work = DispatchWorkItem { [weak self] in self?.schedule() }
    lock.lock()
    retry?.cancel()
    retry = work
    lock.unlock()
    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 300, execute: work)
  }

  /// Favorite photos and videos, newest first, never the Hidden album.
  private func fetchFavorites() -> PHFetchResult<PHAsset> {
    let opts = PHFetchOptions()
    // PhotoKit's default, set here so that no later edit can turn it on by
    // accident.
    opts.includeHiddenAssets = false
    // The person's own library and photos synced from a computer. Shared
    // albums hold other people's photos, so they stay out.
    opts.includeAssetSourceTypes = [.typeUserLibrary, .typeiTunesSynced]
    opts.predicate = NSPredicate(
      format: "isFavorite == YES AND (mediaType == %d OR mediaType == %d)",
      PHAssetMediaType.image.rawValue, PHAssetMediaType.video.rawValue)
    opts.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
    return PHAsset.fetchAssets(with: opts)
  }

  /// Send each photo the ledger has and the library no longer offers as a
  /// favorite. False when the outbox refused one.
  private func sendRemovals(_ favorites: PHFetchResult<PHAsset>, _ ledger: inout [String: String]) -> Bool {
    if ledger.isEmpty { return true }
    // An empty library means it could not be read (it is still loading, or
    // iCloud Photos was just turned off), so it removes nothing.
    let any = PHFetchOptions()
    any.fetchLimit = 1
    if PHAsset.fetchAssets(with: any).count == 0 { return true }

    var now = Set<String>()
    favorites.enumerateObjects { asset, _, _ in now.insert(asset.localIdentifier) }
    for id in ledger.keys.filter({ !now.contains($0) }) {
      guard enqueue(["local_id": id, "deleted": true]) else { return false }
      ledger[id] = nil
    }
    return true
  }

  /// Wait for the outbox to have room, sending what is in it. False when it
  /// is still full afterward.
  private func makeRoom() -> Bool {
    if waiting() < Self.maxWaiting { return true }
    _ = virtues_drain_blocking(25)
    return waiting() < Self.maxWaiting
  }

  /// Photo records in the outbox. An error reads as full, so a queue that
  /// cannot be counted is not added to.
  private func waiting() -> Int64 {
    let n = Self.stream.withCString { virtues_outbox_queued($0) }
    return n < 0 ? Int64.max : n
  }

  // MARK: The ledger

  private struct Ledger: Codable {
    var pairing: String
    /// PhotoKit's local identifier → the edit that was sent.
    var sent: [String: String]
  }

  private var ledgerURL: URL? {
    try? FileManager.default.url(
      for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
    ).appendingPathComponent("photo-favorites.json")
  }

  /// What was sent to the server this phone is paired with. Empty for another
  /// pairing, so a new server gets every favorite.
  private func loadLedger(_ pairing: String) -> [String: String] {
    guard let url = ledgerURL, let data = try? Data(contentsOf: url),
      let ledger = try? JSONDecoder().decode(Ledger.self, from: data), ledger.pairing == pairing
    else { return [:] }
    return ledger.sent
  }

  private func saveLedger(_ sent: [String: String], _ pairing: String) {
    guard let url = ledgerURL else { return }
    do {
      let data = try JSONEncoder().encode(Ledger(pairing: pairing, sent: sent))
      // Readable after the first unlock: a pass can run while the phone is locked.
      try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    } catch {
      NSLog("[Photos] could not save the ledger: %@", error.localizedDescription)
    }
  }

  /// Which pairing this is, or nil on a phone that is not paired.
  private func pairingKey() -> String? {
    var buffer = [CChar](repeating: 0, count: 256)
    guard virtues_pairing_key(&buffer, buffer.count) == 0 else { return nil }
    return String(cString: buffer)
  }

  /// The edit of a photo as this version of the collector sends it. A change
  /// to the picture's size changes every stamp, so each one is sent again.
  private static func stamp(_ a: PHAsset) -> String {
    let edited = a.modificationDate ?? a.creationDate
    return "\(edited?.timeIntervalSince1970 ?? 0)|\(Int(pictureMaxPixel))"
  }

  // MARK: Records

  private static func kind(_ a: PHAsset) -> String {
    if a.mediaType == .video { return "video" }
    if a.mediaSubtypes.contains(.photoScreenshot) { return "screenshot" }
    if a.mediaSubtypes.contains(.photoLive) { return "live_photo" }
    return "photo"
  }

  /// One favorite's record, or nil when its picture is not available now: it
  /// is only in iCloud and the phone is offline or on a metered connection.
  /// Nothing goes in the ledger then, so the next pass tries again.
  private func record(_ a: PHAsset) -> [String: Any]? {
    guard let created = a.creationDate, let jpeg = picture(a) else { return nil }
    var rec: [String: Any] = [
      "local_id": a.localIdentifier,
      "created_at": iso.string(from: created),
      "kind": Self.kind(a),
      "is_favorite": true,
      "width_px": a.pixelWidth,
      "height_px": a.pixelHeight,
      "image_data": jpeg.base64EncodedString(),
      "image_mime": "image/jpeg",
    ]
    // JSONSerialization raises on NaN or infinity, so only finite, valid
    // coordinates go.
    if let c = a.location?.coordinate, CLLocationCoordinate2DIsValid(c),
      c.latitude.isFinite, c.longitude.isFinite
    {
      rec["latitude"] = c.latitude
      rec["longitude"] = c.longitude
    }
    if a.mediaType == .video, a.duration.isFinite, a.duration > 0 {
      rec["duration_seconds"] = a.duration
    }
    if let modified = a.modificationDate {
      rec["modified_at"] = iso.string(from: modified)
    }
    return rec
  }

  /// A photo as JPEG, at most `pictureMaxPixel` on its longest side. A video
  /// gives its poster frame. The result is delivered on the main thread and
  /// waited for here, so this must run off it, which every pass does.
  private func picture(_ a: PHAsset) -> Data? {
    let w = CGFloat(a.pixelWidth)
    let h = CGFloat(a.pixelHeight)
    let longest = max(w, h)
    guard longest > 0 else { return nil }
    let factor = min(1, Self.pictureMaxPixel / longest)
    let target = CGSize(width: (w * factor).rounded(), height: (h * factor).rounded())

    lock.lock()
    let network = onCheapNetwork
    lock.unlock()
    let opts = PHImageRequestOptions()
    opts.deliveryMode = .highQualityFormat
    opts.resizeMode = .exact
    opts.isNetworkAccessAllowed = network

    let done = DispatchSemaphore(value: 0)
    var image: UIImage?
    let request = PHImageManager.default().requestImage(
      for: a, targetSize: target, contentMode: .aspectFit, options: opts
    ) { result, info in
      // Offline, PhotoKit can hand back a small thumbnail in place of the
      // picture. That is not the picture.
      let degraded = (info?[PHImageResultIsDegradedKey] as? Bool) == true
      let onlyInCloud = !network && (info?[PHImageResultIsInCloudKey] as? Bool) == true
      if !degraded && !onlyInCloud { image = result }
      done.signal()
    }
    if done.wait(timeout: .now() + Self.pictureTimeout) == .timedOut {
      PHImageManager.default().cancelImageRequest(request)
      return nil
    }
    return image?.jpegData(compressionQuality: 0.8)
  }

  /// The outbox keeps one row per `id` and ignores a second record with the
  /// same `id` while the first waits, so each record gets an `id` of its own:
  /// a photo favorited, removed and favorited again is three. The photo's
  /// identifier travels as `local_id`, which the server keys the photo on, and
  /// the outbox sends photo records in the order they were queued.
  private func enqueue(_ record: [String: Any]) -> Bool {
    var rec = record
    rec["id"] = "\(record["local_id"] as? String ?? "")#\(UUID().uuidString)"
    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else {
      // A record that cannot be encoded will never encode; skipping it keeps
      // one bad photo from stopping the pass forever.
      NSLog("[Photos] could not encode a record; skipped")
      return true
    }
    let rc = Self.stream.withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 {
      NSLog("[Photos] enqueue failed rc=%d", rc)
      return false
    }
    return true
  }
}
