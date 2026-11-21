import CoreLocation
import Foundation
import Photos
import UIKit

// Shared outbox enqueue (defined in the reach plugin's ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

// RFC 3339 in UTC, ending in Z: the formatter's default time zone is GMT.
private let iso: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime]
  return f
}()

/// Photo library collector: one record per photo or video → shared outbox → server.
///
/// The first pass sends every photo's metadata, oldest first, and remembers the
/// creation date it reached so a pass cut short by the app being killed picks
/// up where it stopped. After that, only changes go: PhotoKit's persistent
/// change history says which photos were added, edited or deleted since the
/// last token. A favorite also carries a JPEG about 1024 pixels on its longest
/// side; other photos carry metadata only.
///
/// id = PhotoKit's `localIdentifier`, so a photo sent again updates its row on
/// the server, and a favorite sent later brings its picture.
public final class PhotosCollector: NSObject, PHPhotoLibraryChangeObserver {
  public static let shared = PhotosCollector()

  private static let stream = "photos"
  private static let firstPassDoneKey = "virtues.photos.firstPassDone"
  private static let cursorKey = "virtues.photos.firstPassCursor"
  private static let tokenKey = "virtues.photos.changeToken"
  /// Whether the finished first pass ran with Limited access, so only the
  /// photos the person picked were sent.
  private static let firstPassLimitedKey = "virtues.photos.firstPassLimited"

  /// The longest side of a favorite's picture, in pixels.
  private static let pictureMaxPixel: CGFloat = 1024
  /// How many assets one batch reads before the first pass saves its place.
  private static let batchSize = 500

  /// Every pass runs here, one at a time, so a launch, a "Sync now" and a
  /// library change can never read the library twice at once.
  private let queue = DispatchQueue(label: "com.virtues.photos", qos: .utility)
  private let lock = NSLock()
  private var running = false
  private var scheduled = false
  private var observing = false
  private var changeDebounce: DispatchWorkItem?

  private let defaults = UserDefaults.standard

  private override init() {}

  // MARK: Status

  public var isCollecting: Bool {
    lock.lock()
    defer { lock.unlock() }
    return running
  }

  public var firstPassDone: Bool { defaults.bool(forKey: Self.firstPassDoneKey) }

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
      "firstPassDone": firstPassDone,
    ]
  }

  // MARK: Entry points

  /// Explicit opt-in: ask for library access, then start. Reading the library
  /// needs the `.readWrite` level; PhotoKit has no read-only level for the
  /// library itself, and this is how the "Full Access" choice is offered.
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
    if !already { PHPhotoLibrary.shared().register(self) }
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
    guard authorized() else { return }
    // Ask iOS for time to finish if the app is sent to the background
    // mid-pass. When the time runs out the pass just pauses; the first pass
    // has saved its place and the change token only moves after a full read.
    var task = UIBackgroundTaskIdentifier.invalid
    task = UIApplication.shared.beginBackgroundTask(withName: "virtues.photos") {
      UIApplication.shared.endBackgroundTask(task)
      task = .invalid
    }
    defer {
      if task != .invalid { UIApplication.shared.endBackgroundTask(task) }
    }

    if !firstPassDone {
      firstPass()
    } else if defaults.bool(forKey: Self.firstPassLimitedKey) && !limited() {
      // The person moved from Limited to Full Access. The photos they had
      // not picked are not changes in PhotoKit's history, so read the whole
      // library again; the server skips the rows that have not changed.
      restartFirstPass()
    } else {
      sendChanges()
    }
  }

  // MARK: First pass

  /// What every fetch shares: photos and videos only, never the Hidden album.
  private func fetchOptions() -> PHFetchOptions {
    let opts = PHFetchOptions()
    // Never read the Hidden album. This is PhotoKit's default, set here so
    // that no later edit can turn it on by accident.
    opts.includeHiddenAssets = false
    // The person's own library and photos synced from a computer. Shared
    // albums hold other people's photos, so they stay out.
    opts.includeAssetSourceTypes = [.typeUserLibrary, .typeiTunesSynced]
    return opts
  }

  private func firstPass() {
    let library = PHPhotoLibrary.shared()
    // Take the change token before reading, and keep the one from the start
    // of an interrupted pass. Changes made while the pass runs are then read
    // by the first change pass after it.
    if loadToken() == nil { saveToken(library.currentChangeToken) }

    let opts = fetchOptions()
    let media = NSPredicate(
      format: "mediaType == %d OR mediaType == %d",
      PHAssetMediaType.image.rawValue, PHAssetMediaType.video.rawValue)
    if let cursor = defaults.object(forKey: Self.cursorKey) as? Double {
      // `>=` sends again the photos taken in the same second as the last one
      // sent, so a photo that shares that timestamp is never skipped.
      let since = NSPredicate(format: "creationDate >= %@", Date(timeIntervalSince1970: cursor) as NSDate)
      opts.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [media, since])
    } else {
      opts.predicate = media
    }
    opts.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: true)]

    let result = PHAsset.fetchAssets(with: opts)
    let total = result.count
    var start = 0
    while start < total {
      let end = min(start + Self.batchSize, total)
      var ok = true
      var last: Date?
      autoreleasepool {
        for asset in result.objects(at: IndexSet(integersIn: start..<end)) {
          guard send(asset) else {
            ok = false
            return
          }
          if let created = asset.creationDate { last = created }
        }
      }
      // Save the place up to the last record the outbox took, so a record it
      // refused is read again next time.
      if let last = last {
        defaults.set(last.timeIntervalSince1970, forKey: Self.cursorKey)
      }
      guard ok else {
        NSLog("[Photos] first pass stopped at %ld of %ld; it resumes on the next pass", start, total)
        return
      }
      start = end
    }

    defaults.set(true, forKey: Self.firstPassDoneKey)
    defaults.set(limited(), forKey: Self.firstPassLimitedKey)
    defaults.removeObject(forKey: Self.cursorKey)
    NSLog("[Photos] first pass sent %ld assets", total)
    // Catch up on anything that changed while the pass ran. A pass that
    // just finished never starts itself over, so lost history cannot loop.
    sendChanges(restartIfHistoryLost: false)
  }

  /// Start the first pass over: the change history could not be read, or
  /// access widened from Limited to Full.
  private func restartFirstPass() {
    defaults.set(false, forKey: Self.firstPassDoneKey)
    defaults.removeObject(forKey: Self.firstPassLimitedKey)
    defaults.removeObject(forKey: Self.cursorKey)
    defaults.removeObject(forKey: Self.tokenKey)
    firstPass()
  }

  // MARK: Changes

  private func sendChanges(restartIfHistoryLost: Bool = true) {
    guard let token = loadToken() else {
      if restartIfHistoryLost { restartFirstPass() }
      return
    }
    let library = PHPhotoLibrary.shared()

    var inserted = Set<String>()
    var updated = Set<String>()
    var deleted = Set<String>()
    var latest: PHPersistentChangeToken?
    do {
      for change in try library.fetchPersistentChanges(since: token) {
        let details = try change.changeDetails(for: .asset)
        inserted.formUnion(details.insertedLocalIdentifiers)
        updated.formUnion(details.updatedLocalIdentifiers)
        deleted.formUnion(details.deletedLocalIdentifiers)
        latest = change.changeToken
      }
    } catch {
      handleHistoryError(error, restart: restartIfHistoryLost)
      return
    }
    guard let newToken = latest else { return }  // nothing changed

    // Read every changed photo as it is now. One that can no longer be
    // fetched was deleted, or was moved to the Hidden album, and either way
    // the server marks it gone.
    let ids = Array(inserted.union(updated).union(deleted))
    var found = Set<String>()
    var start = 0
    while start < ids.count {
      let chunk = Array(ids[start..<min(start + Self.batchSize, ids.count)])
      var ok = true
      autoreleasepool {
        let result = PHAsset.fetchAssets(withLocalIdentifiers: chunk, options: fetchOptions())
        for asset in result.objects(at: IndexSet(integersIn: 0..<result.count)) {
          guard asset.mediaType == .image || asset.mediaType == .video, !asset.isHidden else { continue }
          found.insert(asset.localIdentifier)
          guard send(asset) else {
            ok = false
            return
          }
        }
      }
      // The token moves only after every change is in the outbox, so a
      // refused record is read again next time.
      guard ok else { return }
      start += Self.batchSize
    }

    // A photo only inserted and already gone never reached the server, so
    // it gets no deletion either.
    for id in deleted.union(updated) where !found.contains(id) {
      guard enqueue(["id": id, "deleted": true]) else { return }
    }
    saveToken(newToken)
  }

  private func handleHistoryError(_ error: Error, restart: Bool) {
    if restart, let e = error as? PHPhotosError,
      e.code == .persistentChangeTokenExpired || e.code == .persistentChangeDetailsUnavailable
    {
      // The history no longer reaches back to the saved token, so read the
      // whole library again. The server updates rows it already has.
      NSLog("[Photos] change history unavailable (%ld); sending the library again", e.code.rawValue)
      restartFirstPass()
      return
    }
    NSLog("[Photos] reading the change history failed: %@", error.localizedDescription)
  }

  private func loadToken() -> PHPersistentChangeToken? {
    guard let data = defaults.data(forKey: Self.tokenKey) else { return nil }
    return try? NSKeyedUnarchiver.unarchivedObject(ofClass: PHPersistentChangeToken.self, from: data)
  }

  private func saveToken(_ token: PHPersistentChangeToken) {
    do {
      let data = try NSKeyedArchiver.archivedData(withRootObject: token, requiringSecureCoding: true)
      defaults.set(data, forKey: Self.tokenKey)
    } catch {
      NSLog("[Photos] could not save the change token: %@", error.localizedDescription)
    }
  }

  // MARK: Records

  private static func kind(_ a: PHAsset) -> String {
    if a.mediaType == .video { return "video" }
    if a.mediaSubtypes.contains(.photoScreenshot) { return "screenshot" }
    if a.mediaSubtypes.contains(.photoLive) { return "live_photo" }
    return "photo"
  }

  /// Send one asset. False when the outbox refused it, so the caller stops
  /// and keeps its place. An asset with no creation date is skipped and
  /// counts as sent: the server has no use for it.
  private func send(_ a: PHAsset) -> Bool {
    guard !a.isHidden, let created = a.creationDate else { return true }
    var rec: [String: Any] = [
      "id": a.localIdentifier,
      "created_at": iso.string(from: created),
      "kind": Self.kind(a),
      "is_favorite": a.isFavorite,
      "width_px": a.pixelWidth,
      "height_px": a.pixelHeight,
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
    if a.isFavorite {
      if let jpeg = picture(a) {
        rec["image_data"] = jpeg.base64EncodedString()
        rec["image_mime"] = "image/jpeg"
      } else {
        NSLog("[Photos] no picture for a favorite; sending its metadata alone")
      }
    }
    return enqueue(rec)
  }

  /// A favorite's picture as JPEG, at most `pictureMaxPixel` on its longest
  /// side. A video gives its poster frame. Network access is allowed because
  /// a favorite may be stored only in iCloud. Synchronous, so it must run off
  /// the main thread, which every pass does.
  private func picture(_ a: PHAsset) -> Data? {
    let w = CGFloat(a.pixelWidth)
    let h = CGFloat(a.pixelHeight)
    let longest = max(w, h)
    guard longest > 0 else { return nil }
    let factor = min(1, Self.pictureMaxPixel / longest)
    let target = CGSize(width: (w * factor).rounded(), height: (h * factor).rounded())

    let opts = PHImageRequestOptions()
    opts.deliveryMode = .highQualityFormat
    opts.resizeMode = .exact
    opts.isNetworkAccessAllowed = true
    opts.isSynchronous = true

    var jpeg: Data?
    autoreleasepool {
      _ = PHImageManager.default().requestImage(
        for: a, targetSize: target, contentMode: .aspectFit, options: opts
      ) { image, _ in
        jpeg = image?.jpegData(compressionQuality: 0.8)
      }
    }
    return jpeg
  }

  /// The outbox keeps one row per `id` and ignores a second record with the
  /// same `id` while the first is still waiting. So each version of a photo
  /// gets its own `id` (the photo's identifier, then its modification date,
  /// favorite flag or deletion), and the photo's identifier travels as
  /// `local_id`, which is what the server keys the photo on. A later change
  /// is then never dropped behind an earlier one still in the queue, and the
  /// same version sent twice is still one row.
  private static func versioned(_ rec: [String: Any]) -> [String: Any] {
    guard let localId = rec["id"] as? String else { return rec }
    var out = rec
    let version: String
    if rec["deleted"] as? Bool == true {
      version = "deleted"
    } else {
      let stamp = (rec["modified_at"] as? String) ?? (rec["created_at"] as? String) ?? ""
      let favorite = (rec["is_favorite"] as? Bool) == true ? "f" : "n"
      let picture = rec["image_data"] == nil ? "" : "p"
      version = "\(stamp)|\(favorite)\(picture)"
    }
    out["local_id"] = localId
    out["id"] = "\(localId)#\(version)"
    return out
  }

  private func enqueue(_ record: [String: Any]) -> Bool {
    let rec = Self.versioned(record)
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
