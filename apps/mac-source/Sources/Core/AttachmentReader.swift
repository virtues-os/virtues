import CollectorPolicy
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Reads one queued attachment off disk and makes it ready to send: HEIC
/// becomes JPEG and oversized images are scaled down (see
/// `AttachmentPolicy.encoding`), using ImageIO, which every Mac has — the box
/// has no HEIC decoder, so this is the one place the conversion is free.
enum AttachmentReader {
    enum Outcome {
        /// Bytes to send, with the type OF THOSE BYTES (post-conversion).
        case ready(data: Data, mimeType: String)
        /// No file yet — Messages in iCloud has not downloaded it. Try later.
        case notYetDownloaded
        /// Will never be sendable (over the cap once measured, unreadable).
        case skip(String)
    }

    static func read(_ att: QueuedAttachment) -> Outcome {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        guard let path = AttachmentPolicy.resolvePath(att.path, home: home) else {
            // No path in chat.db at all: the same as not downloaded. A later
            // chat.db read cannot update the queued row, so this retries on the
            // backoff and costs one stat each time.
            return .notYetDownloaded
        }

        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
        let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?
            .int64Value ?? 0
        switch AttachmentPolicy.availability(exists: exists, isDirectory: isDir.boolValue, size: size) {
        case .notYetDownloaded:
            return .notYetDownloaded
        case .present(let actual):
            if actual > AttachmentPolicy.maxBytes {
                return .skip("over \(AttachmentPolicy.maxBytes / 1_048_576) MB on disk")
            }
        }

        let url = URL(fileURLWithPath: path)
        let declaredMime = att.mimeType ?? mimeFromExtension(path) ?? "application/octet-stream"

        let pixelLongSide = imagePixelLongSide(url)
        switch AttachmentPolicy.encoding(mime: declaredMime, uti: att.uti, pixelLongSide: pixelLongSide) {
        case .asIs:
            guard let data = try? Data(contentsOf: url) else {
                // Existed a moment ago; unreadable now (evicted mid-read, or a
                // permission). Try again later rather than give up.
                return .notYetDownloaded
            }
            return .ready(data: data, mimeType: declaredMime)
        case .jpeg(let maxPixel):
            if let data = reencode(url, as: UTType.jpeg, maxPixel: maxPixel) {
                return .ready(data: data, mimeType: "image/jpeg")
            }
            return .skip("could not convert to JPEG")
        case .png(let maxPixel):
            if let data = reencode(url, as: UTType.png, maxPixel: maxPixel) {
                return .ready(data: data, mimeType: "image/png")
            }
            return .skip("could not rescale PNG")
        }
    }

    private static func mimeFromExtension(_ path: String) -> String? {
        let ext = (path as NSString).pathExtension
        guard !ext.isEmpty else { return nil }
        return UTType(filenameExtension: ext)?.preferredMIMEType
    }

    /// The image's long side in pixels, read from its header only. `nil` for
    /// anything ImageIO does not recognise as an image.
    private static func imagePixelLongSide(_ url: URL) -> Int? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
            let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
            let w = props[kCGImagePropertyPixelWidth] as? Int,
            let h = props[kCGImagePropertyPixelHeight] as? Int
        else { return nil }
        return max(w, h)
    }

    /// Decode, apply the EXIF orientation, scale to at most `maxPixel` on the
    /// long side, and encode as `type`. `CGImageSourceCreateThumbnailAtIndex`
    /// with `…FromImageAlways` does all of that in one pass and never scales up.
    private static func reencode(_ url: URL, as type: UTType, maxPixel: Int) -> Data? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else {
            return nil
        }
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, type.identifier as CFString, 1, nil)
        else { return nil }
        var props: [CFString: Any] = [:]
        if type == .jpeg { props[kCGImageDestinationLossyCompressionQuality] = 0.85 }
        CGImageDestinationAddImage(dest, image, props as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return out as Data
    }
}
