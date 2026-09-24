import Social
import UIKit
import UniformTypeIdentifiers

/// "Save to Virtues" — the row in the iPhone share sheet.
///
/// It writes and returns. An extension is a separate process with a small
/// memory ceiling and a short life, and it cannot reach the box, which the app
/// serves inside its own process. So this does no network and no image work: it
/// copies what it was handed into the App Group inbox, and the app sends it
/// later (`ShareInbox.swift` in the reach plugin). The picture is copied as raw
/// bytes from a file — decoding an Instagram-sized screenshot in here is how
/// share extensions get killed.
///
/// The optional note is the one line a person types at capture. It is their
/// words, so it rides to the box as the bookmark's `note`, the only field on the
/// row a person authors.
final class ShareViewController: SLComposeServiceViewController {
  /// Must match the App Group in both entitlements files.
  private let appGroup = "group.com.virtues.app"
  private let folderName = "ShareInbox"

  override func viewDidLoad() {
    super.viewDidLoad()
    title = "Save to Virtues"
    placeholder = "Add a note (optional)"
  }

  override func presentationAnimationDidFinish() {
    super.presentationAnimationDidFinish()
    navigationController?.navigationBar.topItem?.rightBarButtonItem?.title = "Save"
  }

  /// A note is optional, so there is nothing to validate: a save with no note
  /// is a whole save.
  override func isContentValid() -> Bool { true }

  override func didSelectPost() {
    let note = contentText.trimmingCharacters(in: .whitespacesAndNewlines)
    let id = UUID().uuidString
    guard let inbox = inboxURL() else {
      // No shared container means the App Group is not set up; nothing can be
      // handed to the app. Close rather than pretend to save.
      extensionContext?.cancelRequest(withError: ShareError.noSharedContainer)
      return
    }

    let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? [])
      .flatMap { $0.attachments ?? [] }

    let group = DispatchGroup()
    let lock = NSLock()
    var url: URL?
    var keptImage = false

    for provider in providers {
      if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
        group.enter()
        provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { item, _ in
          defer { group.leave() }
          guard let found = item as? URL,
            ["http", "https"].contains(found.scheme?.lowercased() ?? "")
          else { return }
          lock.lock()
          if url == nil { url = found }
          lock.unlock()
        }
      } else if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
        group.enter()
        // A file representation, copied inside the callback (the temporary file
        // is gone once it returns). Never loaded into memory here.
        provider.loadFileRepresentation(forTypeIdentifier: UTType.image.identifier) { file, _ in
          defer { group.leave() }
          guard let file else { return }
          lock.lock()
          defer { lock.unlock() }
          guard !keptImage else { return }
          let dest = inbox.appendingPathComponent("\(id).image")
          if (try? FileManager.default.copyItem(at: file, to: dest)) != nil {
            keptImage = true
          }
        }
      }
    }

    group.notify(queue: .global(qos: .userInitiated)) { [weak self] in
      guard let self else { return }
      self.writeShare(id: id, into: inbox, url: url, hasImage: keptImage, note: note)
      self.extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
    }
  }

  /// Write the share's JSON, last and atomically: the app drains only complete
  /// `.json` files, so a share interrupted before this line is never half-sent.
  private func writeShare(id: String, into inbox: URL, url: URL?, hasImage: Bool, note: String) {
    // Something to point at, or nothing is saved — the box would skip it anyway.
    guard url != nil || hasImage else { return }

    var record: [String: Any] = [
      "timestamp": ISO8601DateFormatter().string(from: Date())
    ]
    if let url { record["url"] = url.absoluteString }
    if !note.isEmpty { record["note"] = note }

    guard let data = try? JSONSerialization.data(withJSONObject: record) else { return }
    try? data.write(to: inbox.appendingPathComponent("\(id).json"), options: .atomic)
  }

  private func inboxURL() -> URL? {
    guard
      let container = FileManager.default
        .containerURL(forSecurityApplicationGroupIdentifier: appGroup)
    else { return nil }
    let inbox = container.appendingPathComponent(folderName, isDirectory: true)
    try? FileManager.default.createDirectory(at: inbox, withIntermediateDirectories: true)
    return inbox
  }

  override func configurationItems() -> [Any]! { [] }
}

private enum ShareError: Error {
  case noSharedContainer
}
