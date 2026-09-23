import Foundation
import UIKit

// Shared outbox enqueue (this plugin's own ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

/// The app's half of the share sheet: move what the Share extension dropped in
/// the App Group inbox onto the outbox, as `bookmark` records.
///
/// The extension cannot send anything itself. It is a separate process with a
/// small memory ceiling and a short life, and the box is served inside THIS
/// process, so the extension writes and returns and the app sends
/// (agents/plan/bookmarks-plan.md §5). From the outbox a share rides the same
/// reach drain as every other stream to the box's `ios_ingest`, whose bookmark
/// arm keeps the picture in Drive for the image pass.
///
/// Until the App Group is registered and the app carries the entitlement, the
/// shared container is nil and `drain` does nothing — which is why this can ship
/// ahead of the extension without touching signing.
///
/// Inbox format, written by the extension: `{id}.json` (url, note, timestamp)
/// and, for a picture, `{id}.image` (the raw bytes as handed over). The JSON is
/// written LAST and atomically, so a half-written share is never drained.
enum ShareInbox {
  /// Must match the App Group in both the app's and the extension's
  /// entitlements.
  static let appGroup = "group.com.virtues.app"
  static let folderName = "ShareInbox"

  /// Longest side after downscaling. A screenshot's words stay legible to the
  /// image pass at this size, and the upload stays well inside the box's 10MB
  /// image cap. The decode happens HERE, in the app, never in the extension —
  /// the app has the memory headroom the extension does not.
  static let maxPixel: CGFloat = 2048
  static let jpegQuality: CGFloat = 0.82

  /// Serializes drains: a foreground wake and a background task arriving
  /// together would otherwise both read the same inbox file.
  private static let queue = DispatchQueue(label: "com.virtues.share-inbox")
  private static var observing = false

  static var inbox: URL? {
    FileManager.default
      .containerURL(forSecurityApplicationGroupIdentifier: appGroup)?
      .appendingPathComponent(folderName, isDirectory: true)
  }

  /// Drain whenever the app comes forward, and once now for shares made while
  /// it was closed. Called from `init_plugin_reach` at launch.
  static func observeForeground() {
    guard !observing else { return }
    observing = true
    NotificationCenter.default.addObserver(
      forName: UIApplication.didBecomeActiveNotification, object: nil, queue: nil
    ) { _ in
      queue.async { drain() }
    }
    queue.async { drain() }
  }

  /// Serialized entry point for callers outside this queue (the background task).
  static func drainSerialized() -> Int {
    queue.sync { drain() }
  }

  /// Move every complete share onto the outbox. Returns how many moved.
  ///
  /// A share's files are deleted only after the outbox accepted it. The outbox
  /// keys on the record `id` (the share's own UUID), so a crash between the
  /// enqueue and the delete re-enqueues the same id, and the outbox dedups it.
  @discardableResult
  private static func drain() -> Int {
    guard let inbox = inbox,
      let names = try? FileManager.default.contentsOfDirectory(atPath: inbox.path)
    else { return 0 }

    var moved = 0
    for name in names where name.hasSuffix(".json") {
      let id = String(name.dropLast(".json".count))
      let jsonURL = inbox.appendingPathComponent(name)
      let imageURL = inbox.appendingPathComponent("\(id).image")

      guard let data = try? Data(contentsOf: jsonURL),
        var record = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
      else {
        // It will never parse; retrying it forever would wedge nothing but
        // itself. Remove it and move on.
        NSLog("[ShareInbox] unreadable share %@ removed", id)
        try? FileManager.default.removeItem(at: jsonURL)
        try? FileManager.default.removeItem(at: imageURL)
        continue
      }
      record["id"] = id

      if let raw = try? Data(contentsOf: imageURL) {
        if let jpeg = downscaledJPEG(raw) {
          record["image_data"] = jpeg.base64EncodedString()
          record["image_mime"] = "image/jpeg"
        } else {
          // A format UIKit cannot decode: send the bytes as they came. The box
          // types an image by its own bytes, so no label is needed.
          record["image_data"] = raw.base64EncodedString()
        }
      }

      guard let out = try? JSONSerialization.data(withJSONObject: record),
        let json = String(data: out, encoding: .utf8)
      else { continue }

      let rc = "bookmark".withCString { stream in
        json.withCString { body in virtues_enqueue(stream, body) }
      }
      if rc == 0 {
        try? FileManager.default.removeItem(at: jsonURL)
        try? FileManager.default.removeItem(at: imageURL)
        moved += 1
      } else {
        // Left in place; the next drain tries again.
        NSLog("[ShareInbox] enqueue failed rc=%d for share %@", rc, id)
      }
    }
    if moved > 0 { NSLog("[ShareInbox] moved %d share(s) to the outbox", moved) }
    sweepOrphans(in: inbox, names: names)
    return moved
  }

  /// A picture whose JSON never arrived — the extension was killed between
  /// copying the file and writing the record — is never drained, because only
  /// `.json` files are. Left alone it sits in the shared container forever.
  ///
  /// Only past `orphanAge`: the extension writes the picture FIRST, so for a few
  /// seconds a share in progress looks exactly like an orphan, and deleting it
  /// would lose a save that was about to complete.
  static let orphanAge: TimeInterval = 24 * 60 * 60

  private static func sweepOrphans(in inbox: URL, names: [String]) {
    let fm = FileManager.default
    let now = Date()
    for name in names where name.hasSuffix(".image") {
      let id = String(name.dropLast(".image".count))
      guard !fm.fileExists(atPath: inbox.appendingPathComponent("\(id).json").path) else {
        continue
      }
      let url = inbox.appendingPathComponent(name)
      guard let modified = (try? fm.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date,
        now.timeIntervalSince(modified) > orphanAge
      else { continue }
      try? fm.removeItem(at: url)
      NSLog("[ShareInbox] removed an orphaned picture %@", id)
    }
  }

  /// Decode, shrink to `maxPixel` on the longest side, and re-encode as JPEG.
  /// Flattened onto white: a transparent PNG would otherwise come out black.
  static func downscaledJPEG(_ data: Data) -> Data? {
    guard let image = UIImage(data: data) else { return nil }
    let longest = max(image.size.width, image.size.height)
    guard longest > 0 else { return nil }
    let factor = min(1, maxPixel / longest)
    let size = CGSize(
      width: (image.size.width * factor).rounded(),
      height: (image.size.height * factor).rounded())

    let format = UIGraphicsImageRendererFormat.default()
    format.scale = 1
    format.opaque = true
    let rendered = UIGraphicsImageRenderer(size: size, format: format).image { ctx in
      UIColor.white.setFill()
      ctx.fill(CGRect(origin: .zero, size: size))
      image.draw(in: CGRect(origin: .zero, size: size))
    }
    return rendered.jpegData(compressionQuality: jpegQuality)
  }
}

/// For the background task (the health plugin's `BackgroundSync`), which runs
/// before the outbox drain so a share made while the app was closed leaves on
/// the same wake. Same C-symbol pattern the plugins already use to reach the
/// Rust drain.
@_cdecl("virtues_share_inbox_drain")
public func virtues_share_inbox_drain() -> Int32 {
  Int32(ShareInbox.drainSerialized())
}
