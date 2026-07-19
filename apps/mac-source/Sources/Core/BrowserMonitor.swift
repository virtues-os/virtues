import Foundation
import SQLite3

private let SQLITE_TRANSIENT_BM = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

/// Reads page visits out of the browsers' own history databases.
///
/// This is the difference between knowing you had Dia focused for 104 hours and
/// knowing what you actually read. Window titles give a page title at the moment
/// of focus; history gives every URL, with a domain and a time.
///
/// Two families, and they agree on nothing:
///
///   Chromium (Dia, Chrome, Arc, Brave, Edge)
///     ~/Library/Application Support/<Browser>/User Data/<Profile>/History
///     `visits` JOIN `urls`; visit_time = MICROSECONDS since 1601-01-01 (WebKit epoch)
///
///   Safari
///     ~/Library/Safari/History.db          (Full Disk Access required)
///     `history_visits` JOIN `history_items`; visit_time = SECONDS since 2001-01-01
///     (Core Data epoch)
///
/// Getting an epoch wrong doesn't fail loudly — it files your browsing history in
/// 1601, or 2001, or next century — so both conversions are pinned in one place and
/// unit-tested against known values.
///
/// The DB is COPIED before reading: the browser holds its history open, and a
/// live-locked read fails (or, worse, blocks). A copy of a rollback-journal SQLite
/// file may miss the last uncommitted transaction; that visit simply arrives on the
/// next sync, since the cursor only advances past what we actually read.
final class BrowserMonitor {
    private let queue: Queue
    private var timer: DispatchSourceTimer?

    /// Warn once per browser, not once per visit.
    private var warnedImplausible: Set<String> = []

    /// The window a real page visit can fall in. The web is younger than this floor,
    /// and a visit cannot be from tomorrow.
    private static func plausible(_ date: Date) -> Bool {
        let floor = Date(timeIntervalSince1970: 788_918_400)  // 1995-01-01
        return date > floor && date < Date().addingTimeInterval(86_400)
    }

    /// Re-scan every 5 minutes, matching the uploader's cadence.
    private let syncInterval: TimeInterval = 300

    /// How far back to reach on a first run, when there's no cursor yet.
    private let initialBackfillDays = 30

    init(queue: Queue) {
        self.queue = queue
    }

    func start() {
        print("Starting browser monitor...")
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
        // Same pause gate as the other collectors.
        if FileManager.default.fileExists(
            atPath: Config.configDir.appendingPathComponent("paused").path)
        {
            return
        }

        for source in BrowserSource.installed() {
            do {
                let since = cursor(for: source.id)
                let visits = try read(source: source, since: since)
                guard !visits.isEmpty else { continue }

                let queued = try queue.addBrowserVisits(visits)

                // Advance the cursor only over visits we actually read. Anything the
                // browser writes after this copy is picked up next tick.
                if let newest = visits.map(\.rawTime).max() {
                    setCursor(newest, for: source.id)
                }
                print("✓ browser[\(source.id)]: \(visits.count) visits read, \(queued) new")
            } catch {
                print("⚠️ browser[\(source.id)] sync failed: \(error)")
            }
        }
    }

    private func read(source: BrowserSource, since: Date?) throws -> [RawVisit] {
        let copy = try snapshot(source.path)
        // Remove the whole snapshot directory — the sidecars live beside the db.
        defer { try? FileManager.default.removeItem(at: copy.deletingLastPathComponent()) }

        var db: OpaquePointer?
        defer { if db != nil { sqlite3_close(db) } }
        guard sqlite3_open_v2(copy.path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else {
            throw BrowserError.cannotOpen(source.path.path)
        }

        let floor = since ?? Calendar.current.date(
            byAdding: .day, value: -initialBackfillDays, to: Date())!

        var statement: OpaquePointer?
        defer { if statement != nil { sqlite3_finalize(statement) } }
        guard sqlite3_prepare_v2(db, source.kind.query, -1, &statement, nil) == SQLITE_OK else {
            throw BrowserError.cannotQuery(String(cString: sqlite3_errmsg(db)))
        }
        sqlite3_bind_double(statement, 1, source.kind.toNative(floor))

        var out: [RawVisit] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let urlPtr = sqlite3_column_text(statement, 0) else { continue }
            let url = String(cString: urlPtr)
            guard !url.isEmpty else { continue }

            let title: String? = sqlite3_column_type(statement, 1) != SQLITE_NULL
                ? String(cString: sqlite3_column_text(statement, 1)) : nil
            let native = sqlite3_column_double(statement, 2)
            let when = source.kind.toDate(native)

            // EPOCH GUARD. Three browsers, three epochs (1601 µs / 2001 s / 1970 µs),
            // and getting one wrong doesn't throw — it just files your browsing in the
            // 17th century, or 56,000 years from now, and nothing ever tells you. Any
            // visit outside a plausible window means the conversion is wrong, not that
            // the data is weird, so say so loudly instead of writing it.
            guard Self.plausible(when) else {
                if !warnedImplausible.contains(source.id) {
                    warnedImplausible.insert(source.id)
                    print(
                        "⚠️ browser[\(source.id)]: visit converts to \(when) — implausible, "
                            + "so the epoch for this browser is wrong. Skipping its visits.")
                }
                continue
            }

            // A visit strictly at the cursor was already queued; `>` in SQL would be
            // brittle across float rounding, so filter here too.
            if let since, when <= since { continue }

            out.append(
                RawVisit(
                    url: url,
                    title: title?.isEmpty == true ? nil : title,
                    date: when,
                    browser: source.id))
        }
        return out
    }

    /// Copy the history DB before reading it — the browser keeps its own copy open.
    ///
    /// The SIDECARS have to come too. Safari's History.db runs in WAL mode, and a
    /// WAL database is not self-contained: opening a copy of just the main file
    /// fails outright with "unable to open database file", and even where it opens,
    /// every recent visit still living in the -wal would be invisible. Copy whatever
    /// sidecars exist (-wal/-shm for WAL, -journal for rollback) and the snapshot is
    /// a real, complete database.
    private func snapshot(_ path: URL) throws -> URL {
        let fm = FileManager.default
        let dir = fm.temporaryDirectory
            .appendingPathComponent("virtues-browser-\(UUID().uuidString)")
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)

        let dest = dir.appendingPathComponent("history.db")
        try fm.copyItem(at: path, to: dest)

        for suffix in ["-wal", "-shm", "-journal"] {
            let sidecar = URL(fileURLWithPath: path.path + suffix)
            guard fm.fileExists(atPath: sidecar.path) else { continue }
            try? fm.copyItem(at: sidecar, to: URL(fileURLWithPath: dest.path + suffix))
        }
        return dest
    }

    // MARK: - Cursors

    private func cursor(for id: String) -> Date? {
        UserDefaults.standard.object(forKey: "virtues.browser.\(id).lastVisit") as? Date
    }

    private func setCursor(_ date: Date, for id: String) {
        UserDefaults.standard.set(date, forKey: "virtues.browser.\(id).lastVisit")
    }
}

// MARK: - Sources

struct BrowserSource {
    let id: String
    let path: URL
    let kind: BrowserKind

    /// Everything here is DISCOVERED, not asserted. A browser that isn't installed
    /// contributes nothing; a browser that lays its profiles out differently than we
    /// expected still gets found.
    ///
    /// That matters because the Chromium layout is not uniform on macOS. `User Data`
    /// is the *Windows* convention, and only some ports keep it:
    ///
    ///   Dia, Arc     ~/Library/Application Support/Dia/User Data/Default/History
    ///   Chrome       ~/Library/Application Support/Google/Chrome/Default/History
    ///   Brave, Edge  …/BraveSoftware/Brave-Browser/Default/History   (no User Data)
    ///
    /// Hardcoding `User Data` therefore finds Dia and Arc and silently reports Chrome,
    /// Brave and Edge as "not installed" — a stream that contributes zero rows forever
    /// and never errors. So probe both layouts and take whichever exists.
    static func installed() -> [BrowserSource] {
        let fm = FileManager.default
        let home = fm.homeDirectoryForCurrentUser
        let appSupport = home.appendingPathComponent("Library/Application Support")

        var found: [BrowserSource] = []

        // (id, Application Support subdirectory)
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
            // Both layouts: <root>/<profile>/History and <root>/User Data/<profile>/History.
            for base in [root, root.appendingPathComponent("User Data")] {
                guard let entries = try? fm.contentsOfDirectory(atPath: base.path) else { continue }
                for profile in entries where profile == "Default" || profile.hasPrefix("Profile ") {
                    let history = base.appendingPathComponent(profile)
                        .appendingPathComponent("History")
                    guard fm.fileExists(atPath: history.path) else { continue }
                    let sourceId = profile == "Default" ? id : "\(id):\(profile.lowercased())"
                    found.append(BrowserSource(id: sourceId, path: history, kind: .chromium))
                }
            }
        }

        // Firefox family: profiles are hash-named (`abc123.default-release`), so the
        // profile directory has to be enumerated rather than guessed.
        // Deliberately NOT Tor Browser: it exists to not remember, and collecting the
        // history it tries hard not to keep is the wrong default.
        let firefox: [(String, String)] = [
            ("firefox", "Firefox"),
            ("zen", "zen"),
            ("librewolf", "LibreWolf"),
        ]

        for (id, dir) in firefox {
            let profiles = appSupport.appendingPathComponent(dir)
                .appendingPathComponent("Profiles")
            guard let entries = try? fm.contentsOfDirectory(atPath: profiles.path) else { continue }
            for profile in entries {
                let places = profiles.appendingPathComponent(profile)
                    .appendingPathComponent("places.sqlite")
                guard fm.fileExists(atPath: places.path) else { continue }
                let sourceId = entries.count > 1 ? "\(id):\(profile)" : id
                found.append(BrowserSource(id: sourceId, path: places, kind: .firefox))
            }
        }

        let safari = home.appendingPathComponent("Library/Safari/History.db")
        if fm.fileExists(atPath: safari.path) {
            found.append(BrowserSource(id: "safari", path: safari, kind: .safari))
        }

        return found
    }
}

/// The three history schemas that exist on a Mac. They agree on nothing — not the
/// table names, not the column names, and not even the epoch:
///
///   Chromium  MICROSECONDS since 1601-01-01  (WebKit/Windows FILETIME epoch)
///   Safari    SECONDS      since 2001-01-01  (Core Data epoch)
///   Firefox   MICROSECONDS since 1970-01-01  (Unix epoch, but in µs)
///
/// A wrong epoch does not fail — it just files your browsing history in 1601, or
/// 2001, or 56,000 years from now. So the conversions live here, together, where
/// they can be compared, and each is checked against real data before shipping.
enum BrowserKind {
    case chromium
    case safari
    case firefox

    private static let webkitEpochOffset: Double = 11_644_473_600  // 1601 → 1970
    private static let coreDataEpochOffset: Double = 978_307_200  // 2001 → 1970

    var query: String {
        switch self {
        case .chromium:
            return """
                SELECT u.url, u.title, v.visit_time
                FROM visits v JOIN urls u ON u.id = v.url
                WHERE v.visit_time > ?
                ORDER BY v.visit_time ASC
                LIMIT 2000
                """
        case .safari:
            return """
                SELECT i.url, v.title, v.visit_time
                FROM history_visits v JOIN history_items i ON i.id = v.history_item
                WHERE v.visit_time > ?
                ORDER BY v.visit_time ASC
                LIMIT 2000
                """
        case .firefox:
            return """
                SELECT p.url, p.title, v.visit_date
                FROM moz_historyvisits v JOIN moz_places p ON p.id = v.place_id
                WHERE v.visit_date > ?
                ORDER BY v.visit_date ASC
                LIMIT 2000
                """
        }
    }

    func toDate(_ native: Double) -> Date {
        switch self {
        case .chromium:
            return Date(timeIntervalSince1970: native / 1_000_000 - Self.webkitEpochOffset)
        case .safari:
            return Date(timeIntervalSince1970: native + Self.coreDataEpochOffset)
        case .firefox:
            return Date(timeIntervalSince1970: native / 1_000_000)
        }
    }

    func toNative(_ date: Date) -> Double {
        switch self {
        case .chromium:
            return (date.timeIntervalSince1970 + Self.webkitEpochOffset) * 1_000_000
        case .safari:
            return date.timeIntervalSince1970 - Self.coreDataEpochOffset
        case .firefox:
            return date.timeIntervalSince1970 * 1_000_000
        }
    }
}

enum BrowserError: Error {
    case cannotOpen(String)
    case cannotQuery(String)
}

/// A visit before it's handed to the queue — keeps the parsed `Date` so the cursor
/// can advance without re-parsing the ISO string.
struct RawVisit {
    let url: String
    let title: String?
    let date: Date
    let browser: String

    var rawTime: Date { date }
}

extension Queue {
    func addBrowserVisits(_ raw: [RawVisit]) throws -> Int {
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime]
        return try addBrowserVisits(
            raw.map {
                BrowserVisit(
                    url: $0.url, title: $0.title,
                    timestamp: iso.string(from: $0.date), browser: $0.browser)
            })
    }
}
