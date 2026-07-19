import Contacts
import Foundation

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
        CNContactFamilyNameKey as CNKeyDescriptor,
        CNContactOrganizationNameKey as CNKeyDescriptor,
        CNContactPhoneNumbersKey as CNKeyDescriptor,
        CNContactEmailAddressesKey as CNKeyDescriptor,
        CNContactBirthdayKey as CNKeyDescriptor,
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
    if !c.organizationName.isEmpty { rec["organizationName"] = c.organizationName }
    if let b = c.birthday, let y = b.year, let m = b.month, let d = b.day {
      rec["birthday"] = String(format: "%04d-%02d-%02d", y, m, d)
    }

    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }
    let rc = "contacts".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[Contacts] enqueue failed rc=%d", rc) }
  }
}
