import Foundation

/// The pure rules for sending iMessage/SMS attachments to the box: which ones
/// go, where their file is, what to do when it is not there yet, and when a
/// report of deleted messages can be believed. No I/O here, so every rule is
/// testable (`swift test`); `AttachmentReader` and `Queue` do the touching.
public enum AttachmentPolicy {
    /// Largest attachment sent, measured on the original file. The box refuses
    /// anything larger too.
    public static let maxBytes: Int64 = 10 * 1024 * 1024

    /// Long side, in pixels, an image is scaled down to before it is sent.
    public static let maxPixelLongSide = 2048

    public enum Decision: Equatable {
        case upload
        case skip(String)
    }

    /// Whether an attachment is sent at all, from what chat.db says about it.
    ///
    /// Video and audio never go (they are the bulk of any Messages library and
    /// nothing on the box reads them). Rich-link previews
    /// (`.pluginPayloadAttachment`) are Messages' own cache of a web page's
    /// card, not something anyone sent. Everything else up to `maxBytes` goes.
    /// An unknown size is not a reason to skip: the file's real size is checked
    /// again when it is read.
    public static func decide(
        mime: String?, uti: String?, filename: String?, sizeBytes: Int64?
    ) -> Decision {
        if isVideoOrAudio(mime: mime, uti: uti, filename: filename) {
            return .skip("video or audio")
        }
        if let name = filename?.lowercased(), name.hasSuffix(".pluginpayloadattachment") {
            return .skip("link preview")
        }
        if let size = sizeBytes, size > maxBytes {
            return .skip("over \(maxBytes / 1_048_576) MB")
        }
        return .upload
    }

    private static let videoAudioUTIs: Set<String> = [
        "public.movie", "public.video", "public.audio", "public.mpeg-4",
        "public.mpeg-4-audio", "public.mp3", "public.aiff-audio",
        "com.apple.quicktime-movie", "com.apple.m4v-video", "com.apple.m4a-audio",
        "com.apple.coreaudio-format", "com.apple.protected-mpeg-4-audio",
        "org.3gpp.adaptive-multi-rate-audio", "public.3gpp", "public.avi",
        "com.microsoft.waveform-audio",
    ]

    private static let videoAudioExtensions: Set<String> = [
        "mov", "mp4", "m4v", "avi", "3gp", "caf", "m4a", "mp3", "aac", "amr", "wav",
        "aiff", "aif",
    ]

    static func isVideoOrAudio(mime: String?, uti: String?, filename: String?) -> Bool {
        if let m = mime?.lowercased(), m.hasPrefix("video/") || m.hasPrefix("audio/") {
            return true
        }
        if let u = uti?.lowercased(), videoAudioUTIs.contains(u) {
            return true
        }
        if let ext = filename.map({ ($0 as NSString).pathExtension.lowercased() }),
            videoAudioExtensions.contains(ext)
        {
            return true
        }
        return false
    }

    /// The on-disk path chat.db records, made absolute. chat.db writes most as
    /// `~/Library/Messages/Attachments/…`; some are already absolute. `nil` for
    /// an attachment with no path at all.
    public static func resolvePath(_ raw: String?, home: String) -> String? {
        guard let raw, !raw.isEmpty else { return nil }
        if raw == "~" { return home }
        if raw.hasPrefix("~/") {
            let trimmedHome = home.hasSuffix("/") ? String(home.dropLast()) : home
            return trimmedHome + "/" + raw.dropFirst(2)
        }
        return raw
    }

    /// What is on disk at an attachment's path.
    public enum Availability: Equatable {
        /// A file with bytes in it.
        case present(Int64)
        /// Not downloaded yet. With Messages in iCloud, chat.db keeps the row
        /// and the path while the file (often its whole directory) is absent
        /// until someone opens the conversation. Not an error: try later.
        case notYetDownloaded
    }

    /// Classify a stat of the path. A directory or a zero-byte file is the
    /// same as no file: iCloud leaves both behind before the download.
    public static func availability(exists: Bool, isDirectory: Bool, size: Int64) -> Availability {
        guard exists, !isDirectory, size > 0 else { return .notYetDownloaded }
        return .present(size)
    }

    /// When to look again for a file that was not there: 1h, 2h, 4h … then
    /// daily, forever. Looking is one stat, and a picture may be downloaded
    /// years after it was sent, whenever the owner scrolls back to it.
    public static func retryDelay(afterAttempts attempts: Int) -> TimeInterval {
        let hour: TimeInterval = 3600
        let n = max(attempts, 1)
        if n >= 6 { return 24 * hour }
        return min(hour * pow(2, Double(n - 1)), 24 * hour)
    }

    /// How an image is re-encoded before sending.
    public enum Encoding: Equatable {
        /// Send the file's bytes untouched.
        case asIs
        /// Re-encode as JPEG, at most `maxPixel` on the long side.
        case jpeg(maxPixel: Int)
        /// Re-encode as PNG (keeps transparency), at most `maxPixel`.
        case png(maxPixel: Int)
    }

    /// HEIC/HEIF (and TIFF) become JPEG here, on the Mac, where ImageIO decodes
    /// them for free; the box has no HEIC decoder and a browser mostly cannot
    /// show one. Oversized JPEG/PNG are scaled down in their own format. GIF
    /// and WebP go as they are: GIF may be animated, and ImageIO cannot write
    /// WebP. Anything that is not an image goes as it is.
    public static func encoding(mime: String?, uti: String?, pixelLongSide: Int?) -> Encoding {
        let m = mime?.lowercased() ?? ""
        let u = uti?.lowercased() ?? ""
        let big = (pixelLongSide ?? 0) > maxPixelLongSide
        if m == "image/heic" || m == "image/heif" || u == "public.heic" || u == "public.heif"
            || m == "image/tiff" || u == "public.tiff"
        {
            return .jpeg(maxPixel: maxPixelLongSide)
        }
        if m == "image/jpeg" || u == "public.jpeg" {
            return big ? .jpeg(maxPixel: maxPixelLongSide) : .asIs
        }
        if m == "image/png" || u == "public.png" {
            return big ? .png(maxPixel: maxPixelLongSide) : .asIs
        }
        return .asIs
    }

    /// Whether "these messages are gone from chat.db" is believable enough to
    /// delete their pictures from the box.
    ///
    /// A deletion is irreversible on the box, so a check that says most of
    /// what we sent has vanished at once is read as a broken read (a restored
    /// or swapped chat.db, a migration in progress), not as the owner
    /// deleting hundreds of conversations between two ticks. Small numbers
    /// always pass: deleting one thread is ordinary.
    public static func deletionReportIsTrustworthy(checked: Int, missing: Int) -> Bool {
        guard checked > 0, missing > 0 else { return missing == 0 }
        if missing <= 20 { return true }
        return Double(missing) / Double(checked) < 0.5
    }
}
