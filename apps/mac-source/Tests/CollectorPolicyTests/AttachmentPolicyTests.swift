import XCTest
@testable import CollectorPolicy

final class AttachmentPolicyTests: XCTestCase {
    func testPhotosAndDocumentsGoVideoAndAudioDoNot() {
        XCTAssertEqual(
            AttachmentPolicy.decide(mime: "image/heic", uti: "public.heic",
                                    filename: "IMG_0001.HEIC", sizeBytes: 2_000_000),
            .upload)
        XCTAssertEqual(
            AttachmentPolicy.decide(mime: "application/pdf", uti: "com.adobe.pdf",
                                    filename: "plans.pdf", sizeBytes: 300_000),
            .upload)
        XCTAssertNotEqual(
            AttachmentPolicy.decide(mime: "video/quicktime", uti: nil,
                                    filename: "IMG_0002.MOV", sizeBytes: 1_000),
            .upload)
        // chat.db often has no mime for a voice memo; the UTI still says audio.
        XCTAssertNotEqual(
            AttachmentPolicy.decide(mime: nil, uti: "com.apple.coreaudio-format",
                                    filename: "Audio Message.caf", sizeBytes: 50_000),
            .upload)
        // Neither mime nor UTI: the extension decides.
        XCTAssertNotEqual(
            AttachmentPolicy.decide(mime: nil, uti: nil, filename: "clip.mp4", sizeBytes: nil),
            .upload)
    }

    func testTheCapIsInclusiveAndAnUnknownSizeIsNotASkip() {
        let cap = AttachmentPolicy.maxBytes
        XCTAssertEqual(
            AttachmentPolicy.decide(mime: "image/jpeg", uti: nil, filename: nil, sizeBytes: cap),
            .upload)
        XCTAssertNotEqual(
            AttachmentPolicy.decide(mime: "image/jpeg", uti: nil, filename: nil, sizeBytes: cap + 1),
            .upload)
        XCTAssertEqual(
            AttachmentPolicy.decide(mime: "image/jpeg", uti: nil, filename: nil, sizeBytes: nil),
            .upload)
    }

    func testLinkPreviewsAreNotSent() {
        XCTAssertNotEqual(
            AttachmentPolicy.decide(mime: nil, uti: "dyn.ah62d4rv4ge80",
                                    filename: "ABC.pluginPayloadAttachment", sizeBytes: 900),
            .upload)
    }

    func testPathsAreExpandedAgainstHome() {
        XCTAssertEqual(
            AttachmentPolicy.resolvePath("~/Library/Messages/Attachments/ab/01/X/IMG.HEIC",
                                         home: "/Users/nick"),
            "/Users/nick/Library/Messages/Attachments/ab/01/X/IMG.HEIC")
        XCTAssertEqual(
            AttachmentPolicy.resolvePath("~/a.png", home: "/Users/nick/"), "/Users/nick/a.png")
        XCTAssertEqual(
            AttachmentPolicy.resolvePath("/private/var/folders/x/a.png", home: "/Users/nick"),
            "/private/var/folders/x/a.png")
        XCTAssertNil(AttachmentPolicy.resolvePath("", home: "/Users/nick"))
        XCTAssertNil(AttachmentPolicy.resolvePath(nil, home: "/Users/nick"))
    }

    func testAnEmptyDirectoryOrEmptyFileIsNotDownloadedYet() {
        XCTAssertEqual(
            AttachmentPolicy.availability(exists: false, isDirectory: false, size: 0),
            .notYetDownloaded)
        XCTAssertEqual(
            AttachmentPolicy.availability(exists: true, isDirectory: true, size: 96),
            .notYetDownloaded)
        XCTAssertEqual(
            AttachmentPolicy.availability(exists: true, isDirectory: false, size: 0),
            .notYetDownloaded)
        XCTAssertEqual(
            AttachmentPolicy.availability(exists: true, isDirectory: false, size: 42), .present(42))
    }

    func testRetriesBackOffToDailyAndNeverStop() {
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 0), 3600)
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 1), 3600)
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 2), 7200)
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 5), 16 * 3600)
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 6), 24 * 3600)
        XCTAssertEqual(AttachmentPolicy.retryDelay(afterAttempts: 500), 24 * 3600)
    }

    func testHeicBecomesJpegAndBigImagesShrink() {
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "image/heic", uti: "public.heic", pixelLongSide: 800),
            .jpeg(maxPixel: 2048))
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: nil, uti: "public.heic", pixelLongSide: nil),
            .jpeg(maxPixel: 2048))
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "image/jpeg", uti: nil, pixelLongSide: 4032),
            .jpeg(maxPixel: 2048))
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "image/jpeg", uti: nil, pixelLongSide: 1200), .asIs)
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "image/png", uti: nil, pixelLongSide: 2732),
            .png(maxPixel: 2048))
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "image/gif", uti: nil, pixelLongSide: 4000), .asIs)
        XCTAssertEqual(
            AttachmentPolicy.encoding(mime: "application/pdf", uti: nil, pixelLongSide: nil), .asIs)
    }

    func testAMassVanishingIsNotBelievedButAThreadIs() {
        XCTAssertTrue(AttachmentPolicy.deletionReportIsTrustworthy(checked: 1000, missing: 0))
        XCTAssertTrue(AttachmentPolicy.deletionReportIsTrustworthy(checked: 1000, missing: 12))
        XCTAssertTrue(AttachmentPolicy.deletionReportIsTrustworthy(checked: 15, missing: 15))
        XCTAssertTrue(AttachmentPolicy.deletionReportIsTrustworthy(checked: 1000, missing: 300))
        XCTAssertFalse(AttachmentPolicy.deletionReportIsTrustworthy(checked: 1000, missing: 900))
    }
}
