import Contacts
import CryptoKit
import Foundation
import UIKit

// Shared outbox enqueue (defined in the reach plugin's ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

/// Contacts collector: a full address-book snapshot → shared outbox → box.
///
/// No time dimension — it's a snapshot on enable / launch / "Sync now". id = the
/// contact's stable `identifier`, so re-scans dedup at the box (one row per
/// contact). The Rust side is thin; this does the collection.
public final class ContactsCollector {
  public static let shared = ContactsCollector()

  private let store = CNContactStore()
  private var collecting = false

  private init() {}

  public var isCollecting: Bool { collecting }

  public func authorized() -> Bool {
    let status = CNContactStore.authorizationStatus(for: .contacts)
    if #available(iOS 18.0, *) { return status == .authorized || status == .limited }
    return status == .authorized
  }

  public func enable(_ completion: @escaping (Bool) -> Void) {
    store.requestAccess(for: .contacts) { [weak self] granted, _ in
      DispatchQueue.main.async {
        if granted { self?.start() }
        completion(granted)
      }
    }
  }

  public func resume() {
    guard authorized() else { return }
    start()
  }

  private func start() {
    collecting = true
    scan()
  }

  /// Re-snapshot all contacts (safe to call from any wake / "Sync now").
  public func collectAll() {
    guard authorized() else { return }
    scan()
  }

  private func scan() {
    DispatchQueue.global(qos: .utility).async { [weak self] in
      guard let self = self else { return }
      let keys: [CNKeyDescriptor] = [
        CNContactIdentifierKey as CNKeyDescriptor,
        CNContactGivenNameKey as CNKeyDescriptor,
        CNContactMiddleNameKey as CNKeyDescriptor,
        CNContactFamilyNameKey as CNKeyDescriptor,
        CNContactPreviousFamilyNameKey as CNKeyDescriptor,
        CNContactNicknameKey as CNKeyDescriptor,
        CNContactRelationsKey as CNKeyDescriptor,
        CNContactOrganizationNameKey as CNKeyDescriptor,
        CNContactPhoneNumbersKey as CNKeyDescriptor,
        CNContactEmailAddressesKey as CNKeyDescriptor,
        CNContactBirthdayKey as CNKeyDescriptor,
        CNContactImageDataAvailableKey as CNKeyDescriptor,
        CNContactThumbnailImageDataKey as CNKeyDescriptor,
      ]
      let request = CNContactFetchRequest(keysToFetch: keys)
      do {
        try self.store.enumerateContacts(with: request) { contact, _ in
          self.enqueue(contact)
        }
      } catch {
        NSLog("[Contacts] fetch failed: %@", error.localizedDescription)
      }
    }
  }

  private func enqueue(_ c: CNContact) {
    let phones = c.phoneNumbers.map { ["number": $0.value.stringValue] }
    let emails = c.emailAddresses.map { ["address": $0.value as String] }

    var rec: [String: Any] = [
      "id": c.identifier,
      "identifier": c.identifier,
      "givenName": c.givenName,
      "familyName": c.familyName,
      "phones": phones,
      "emails": emails,
    ]
    if !c.middleName.isEmpty { rec["middleName"] = c.middleName }
    if !c.previousFamilyName.isEmpty { rec["previousFamilyName"] = c.previousFamilyName }
    if !c.nickname.isEmpty { rec["nickname"] = c.nickname }
    // Related names ("partner", "mother") as the owner wrote them on the card.
    // The box records a relationship only from here or from the owner's own edit.
    let relations: [[String: String]] = c.contactRelations.compactMap {
      (rel: CNLabeledValue<CNContactRelation>) -> [String: String]? in
      let name = rel.value.name.trimmingCharacters(in: CharacterSet.whitespaces)
      guard !name.isEmpty else { return nil }
      let label = rel.label.map { CNLabeledValue<CNContactRelation>.localizedString(forLabel: $0) } ?? ""
      return ["label": label, "name": name]
    }
    if !relations.isEmpty { rec["relations"] = relations }
    if !c.organizationName.isEmpty { rec["organizationName"] = c.organizationName }
    if let b = c.birthday, let y = b.year, let m = b.month, let d = b.day {
      rec["birthday"] = String(format: "%04d-%02d-%02d", y, m, d)
    }
    // The card's photo becomes the person's picture on the box, unless the
    // owner chose one there. The hash lets the box check the bytes arrived
    // whole and tell a new photo from the one it already has.
    if c.imageDataAvailable, let thumb = c.thumbnailImageData, let jpeg = Self.photoJPEG(thumb) {
      rec["photo"] = jpeg.base64EncodedString()
      rec["photoSha256"] = SHA256.hash(data: jpeg).map { String(format: "%02x", $0) }.joined()
    }

    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }
    let rc = "contacts".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[Contacts] enqueue failed rc=%d", rc) }
  }

  /// Every launch re-sends the whole address book, so a photo travels as the
  /// card's thumbnail, never the full image. A thumbnail is a few kilobytes;
  /// one over this cap is shrunk, and one still over it after that is not sent.
  static let photoCapBytes = 64 * 1024
  /// The longest side a shrunk photo keeps, in pixels: plenty for a picture
  /// the app draws at 72 points.
  static let photoMaxPixel: CGFloat = 256

  /// The thumbnail as JPEG within `photoCapBytes`. A JPEG already under the cap
  /// is sent byte for byte, so the same photo always has the same hash. Anything
  /// else is re-encoded at `photoMaxPixel`; nil when even that is over the cap.
  static func photoJPEG(_ data: Data) -> Data? {
    let isJPEG = data.starts(with: [0xFF, 0xD8, 0xFF])
    if isJPEG && data.count <= photoCapBytes { return data }
    guard let shrunk = shrunkJPEG(data), shrunk.count <= photoCapBytes else { return nil }
    return shrunk
  }

  /// Decode, shrink to `photoMaxPixel` on the longest side, and re-encode as
  /// JPEG, flattened onto white so a transparent PNG doesn't come out black.
  /// The same steps as the reach plugin's `ShareInbox.downscaledJPEG`, which
  /// lives in another Swift package this one can't import.
  private static func shrunkJPEG(_ data: Data) -> Data? {
    guard let image = UIImage(data: data) else { return nil }
    let longest = max(image.size.width, image.size.height)
    guard longest > 0 else { return nil }
    let factor = min(1, photoMaxPixel / longest)
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
    return rendered.jpegData(compressionQuality: 0.8)
  }
}
