import CryptoKit
import Foundation

/// Reads the browsers' bookmark stores and queues FULL per-browser snapshots.
///
/// Bookmarks are the opposite of history: not an append-only event log but a
/// small mutable document the user edits in place. So there is no cursor —
/// change detection is a content hash of the store file, and when it moves we
/// ship the browser's entire bookmark state. The box reconciles (upsert +
/// tombstone-by-absence), which is why a partial read must never be sent as if
/// it were complete: absence IS the delete signal.
///
/// Two families:
///
///   Chromium (Dia, Arc, Chrome, Brave, Edge, …)
///     `Bookmarks` JSON beside the profile's History file.
///     `date_added` = MICROSECONDS since 1601-01-01, as a string (WebKit epoch —
///     the same trap as history; conversion mirrors BrowserKind.chromium).
///
///   Safari
///     ~/Library/Safari/Bookmarks.plist (binary plist, Full Disk Access — the
///     same grant as History.db, so Health's `full_disk_access` already covers
///     it). Plain bookmarks carry NO date; Reading List items do
///     (`ReadingList.DateAdded`, a real plist Date — no epoch math), plus a
///     WebKit-generated `PreviewText`.
///
/// Firefox keeps bookmarks inside places.sqlite; not read yet — it needs the
/// same copy-with-sidecars dance as BrowserMonitor and is rare enough to wait.
/// Arc caveat: Arc's real "bookmarks" are its sidebar (StorableSidebar.json,
/// a private format); the Chromium `Bookmarks` file we read exists but may be
/// vestigial there, so expect little from Arc until that format is reversed.
final class BookmarkMonitor {
    private let queue: Queue
    private var timer: DispatchSourceTimer?

    /// Same cadence as the other monitors; almost every tick is a no-op hash
    /// check on a handful of small files.
    private let syncInterval: TimeInterval = 300

    private static let webkitEpochOffset: Double = 11_644_473_600  // 1601 → 1970

    /// Bookmarks can be old, but not older than the web, and not from the
    /// future. An implausible date means the epoch conversion is wrong — drop
    /// the DATE (the box treats it as unknown), never the bookmark.
    private static func plausible(_ date: Date) -> Bool {
        let floor = Date(timeIntervalSince1970: 788_918_400)  // 1995-01-01
        return date > floor && date < Date().addingTimeInterval(86_400)
    }

    init(queue: Queue) {
        self.queue = queue
    }

    func start() {
        print("Starting bookmark monitor...")
        DispatchQueue.global(qos: .background).async { [weak self] in self?.sync() }

        let t = DispatchSource.makeTimerSource(queue: .global(qos: .background))
        t.schedule(deadline: .now() + syncInterval, repeating: syncInterval)
        t.setEventHandler { [weak self] in self?.sync() }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    // MARK: - Sync

    private func sync() {
        if FileManager.default.fileExists(
            atPath: Config.configDir.appendingPathComponent("paused").path)
        {
            return
        }

        for source in BookmarkStore.installed() {
            do {
                let data = try Data(contentsOf: source.path)

                // Hash BEFORE parsing, advance AFTER queueing: a parse failure
                // (e.g. a torn read mid-write by the browser) leaves the hash
                // untouched, so the next tick simply retries.
                let hash = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
                guard hash != lastHash(for: source.id) else { continue }

                let records = try source.kind.parse(data)
                let json = try JSONSerialization.data(withJSONObject: records)
                try queue.replacePendingBookmarkSnapshot(
                    browser: source.id,
                    recordsJSON: String(decoding: json, as: UTF8.self))
                setLastHash(hash, for: source.id)
                print("✓ bookmarks[\(source.id)]: snapshot queued (\(records.count) records)")
            } catch {
                print("⚠️ bookmarks[\(source.id)] sync failed: \(error)")
            }
        }
    }

    // MARK: - Change detection

    private func lastHash(for id: String) -> String? {
        UserDefaults.standard.string(forKey: "virtues.bookmarks.\(id).hash")
    }

    private func setLastHash(_ hash: String, for id: String) {
        UserDefaults.standard.set(hash, forKey: "virtues.bookmarks.\(id).hash")
    }
}

// MARK: - Stores

struct BookmarkStore {
    let id: String
    let path: URL
    let kind: BookmarkStoreKind

    /// Discovered, not asserted — the same doctrine (and the same two Chromium
    /// layouts) as BrowserSource.installed(). The `Bookmarks` JSON sits beside
    /// the profile's `History`, so discovery walks the identical directories.
    static func installed() -> [BookmarkStore] {
        let fm = FileManager.default
        let home = fm.homeDirectoryForCurrentUser
        let appSupport = home.appendingPathComponent("Library/Application Support")

        var found: [BookmarkStore] = []

        let chromium: [(String, String)] = [
            ("dia", "Dia"),
            ("chrome", "Google/Chrome"),
            ("chrome-beta", "Google/Chrome Beta"),
            ("chrome-canary", "Google/Chrome Canary"),
            ("arc", "Arc"),
            ("brave", "BraveSoftware/Brave-Browser"),
            ("edge", "Microsoft Edge"),
            ("vivaldi", "Vivaldi"),
            ("opera", "com.operasoftware.Opera"),
            ("chromium", "Chromium"),
        ]

        for (id, dir) in chromium {
            let root = appSupport.appendingPathComponent(dir)
            for base in [root, root.appendingPathComponent("User Data")] {
                guard let entries = try? fm.contentsOfDirectory(atPath: base.path) else { continue }
                for profile in entries where profile == "Default" || profile.hasPrefix("Profile ") {
                    let bookmarks = base.appendingPathComponent(profile)
                        .appendingPathComponent("Bookmarks")
                    guard fm.fileExists(atPath: bookmarks.path) else { continue }
                    let sourceId = profile == "Default" ? id : "\(id):\(profile.lowercased())"
                    found.append(BookmarkStore(id: sourceId, path: bookmarks, kind: .chromium))
                }
            }
        }

        let safari = home.appendingPathComponent("Library/Safari/Bookmarks.plist")
        if fm.fileExists(atPath: safari.path) {
            found.append(BookmarkStore(id: "safari", path: safari, kind: .safari))
        }

        return found
    }
}

enum BookmarkStoreKind {
    case chromium
    case safari

    /// Store bytes → webhook record dicts:
    /// `{guid, url, title, folder_path: [..], date_added?: ISO8601,
    ///   kind: "bookmark"|"reading_list", preview?}`
    func parse(_ data: Data) throws -> [[String: Any]] {
        switch self {
        case .chromium: return try Self.parseChromium(data)
        case .safari: return try Self.parseSafari(data)
        }
    }

    // MARK: Chromium

    private static func parseChromium(_ data: Data) throws -> [[String: Any]] {
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let roots = root["roots"] as? [String: Any]
        else { throw BookmarkError.malformedStore }

        var out: [[String: Any]] = []
        // Root display names match what the browser shows, so the folder path
        // reads like the user's own bookmarks UI.
        let rootNames = [
            ("bookmark_bar", "Bookmarks Bar"), ("other", "Other Bookmarks"),
            ("synced", "Mobile Bookmarks"),
        ]
        for (key, label) in rootNames {
            guard let node = roots[key] as? [String: Any] else { continue }
            walkChromium(node, path: [], rootLabel: label, into: &out)
        }
        return out
    }

    private static func walkChromium(
        _ node: [String: Any], path: [String], rootLabel: String, into out: inout [[String: Any]]
    ) {
        let type = node["type"] as? String
        if type == "folder" || (node["children"] != nil && type != "url") {
            // The root node contributes its display label ("Bookmarks Bar");
            // child folders contribute their user-given names.
            let label = path.isEmpty ? rootLabel : (node["name"] as? String ?? "")
            let newPath = label.isEmpty ? path : path + [label]
            for child in node["children"] as? [[String: Any]] ?? [] {
                walkChromium(child, path: newPath, rootLabel: rootLabel, into: &out)
            }
            return
        }
        guard type == "url",
            let guid = node["guid"] as? String,
            let url = node["url"] as? String
        else { return }

        var rec: [String: Any] = [
            "guid": guid,
            "url": url,
            "folder_path": path,
            "kind": "bookmark",
        ]
        if let name = node["name"] as? String, !name.isEmpty { rec["title"] = name }

        // date_added: µs since 1601, serialized as a string. Same epoch trap as
        // history (BrowserKind.chromium); an implausible conversion drops the
        // date, not the bookmark.
        if let raw = node["date_added"] as? String, let micros = Double(raw), micros > 0 {
            let date = Date(timeIntervalSince1970: micros / 1_000_000 - 11_644_473_600)
            if date > Date(timeIntervalSince1970: 788_918_400),
                date < Date().addingTimeInterval(86_400)
            {
                rec["date_added"] = ISO8601DateFormatter().string(from: date)
            }
        }
        out.append(rec)
    }

    // MARK: Safari

    private static func parseSafari(_ data: Data) throws -> [[String: Any]] {
        guard
            let root = try PropertyListSerialization.propertyList(from: data, format: nil)
                as? [String: Any]
        else { throw BookmarkError.malformedStore }

        var out: [[String: Any]] = []
        walkSafari(root, path: [], into: &out)
        return out
    }

    private static func walkSafari(
        _ node: [String: Any], path: [String], into out: inout [[String: Any]]
    ) {
        let type = node["WebBookmarkType"] as? String

        if type == "WebBookmarkTypeLeaf" {
            guard let guid = node["WebBookmarkUUID"] as? String,
                let url = node["URLString"] as? String
            else { return }

            let readingList = node["ReadingList"] as? [String: Any]
            var rec: [String: Any] = [
                "guid": guid,
                "url": url,
                "folder_path": path,
                "kind": readingList != nil ? "reading_list" : "bookmark",
            ]
            if let title = (node["URIDictionary"] as? [String: Any])?["title"] as? String,
                !title.isEmpty
            {
                rec["title"] = title
            }
            if let added = readingList?["DateAdded"] as? Date {
                rec["date_added"] = ISO8601DateFormatter().string(from: added)
            }
            if let preview = readingList?["PreviewText"] as? String, !preview.isEmpty {
                rec["preview"] = preview
            }
            out.append(rec)
            return
        }

        // Everything else with Children is a list/folder (including the root,
        // which has no type). Proxy nodes (History) have no Children and fall
        // through harmlessly.
        guard let children = node["Children"] as? [[String: Any]] else { return }
        let title = node["Title"] as? String
        let label = Self.safariFolderLabel(title)
        // Reading List lives in a folder literally titled com.apple.ReadingList;
        // its items are typed by the ReadingList dict above, so the folder name
        // itself is noise — contribute no path segment for it or the root.
        let newPath = label.map { path + [$0] } ?? path
        for child in children {
            walkSafari(child, path: newPath, into: &out)
        }
    }

    private static func safariFolderLabel(_ title: String?) -> String? {
        switch title {
        case nil, "", "com.apple.ReadingList": return nil
        // Safari's UI calls BookmarksBar "Favorites"; keep the user's mental model.
        case "BookmarksBar": return "Favorites"
        case "BookmarksMenu": return "Bookmarks Menu"
        default: return title
        }
    }
}

enum BookmarkError: Error {
    case malformedStore
}
