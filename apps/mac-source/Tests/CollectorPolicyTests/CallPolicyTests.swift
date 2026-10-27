import XCTest
@testable import CollectorPolicy

final class CallPolicyTests: XCTestCase {
    private let zoom = CallPolicy.app(forBundleID: "us.zoom.xos")!
    private let chrome = CallPolicy.app(forBundleID: "com.google.Chrome")!
    private let t0 = Date(timeIntervalSinceReferenceDate: 0)

    func testHelpersBelongToTheirApp() {
        XCTAssertEqual(CallPolicy.app(forBundleID: "com.google.Chrome.helper")?.name, "Chrome")
        XCTAssertEqual(CallPolicy.app(forBundleID: "company.thebrowser.browser.helper")?.name, "Arc or Dia")
        XCTAssertEqual(CallPolicy.app(forBundleID: "com.microsoft.teams2")?.name, "Microsoft Teams")
        XCTAssertEqual(CallPolicy.app(forBundleID: "com.apple.avconferenced")?.name, "FaceTime")
    }

    func testAppsOffTheListAreIgnored() {
        // Siri's listener and a dictation app hold the mic with no call.
        XCTAssertNil(CallPolicy.app(forBundleID: "com.apple.CoreSpeech"))
        XCTAssertNil(CallPolicy.app(forBundleID: "com.apple.corespeechd_system"))
        XCTAssertNil(CallPolicy.app(forBundleID: "com.apple.VoiceMemos"))
        XCTAssertNil(CallPolicy.app(forBundleID: ""))
    }

    func testACallStartsAtOnceAndEndsAfterTheGrace() {
        var tracker = CallPolicy.Tracker()
        XCTAssertEqual(tracker.observe([zoom], at: t0), [.started(zoom)])
        XCTAssertEqual(tracker.observe([zoom], at: t0 + 5), [])
        // Muted for a moment: inside the grace, still one call.
        XCTAssertEqual(tracker.observe([], at: t0 + 6), [])
        XCTAssertEqual(tracker.observe([zoom], at: t0 + 9), [])
        XCTAssertEqual(tracker.observe([], at: t0 + 18), [])
        XCTAssertEqual(tracker.observe([], at: t0 + 19), [.ended(zoom)])
        XCTAssertNil(tracker.current)
    }

    func testTheAppBeingRecordedKeepsItWhenAnotherOpensTheMic() {
        var tracker = CallPolicy.Tracker()
        _ = tracker.observe([chrome], at: t0)
        // Zoom is earlier in the list, but Chrome's call is already running.
        XCTAssertEqual(tracker.observe([chrome, zoom], at: t0 + 1), [])
        XCTAssertEqual(tracker.current, chrome)
    }

    func testOneCallHandsOverToAnotherAfterTheGrace() {
        var tracker = CallPolicy.Tracker()
        _ = tracker.observe([chrome], at: t0)
        XCTAssertEqual(tracker.observe([zoom], at: t0 + 1), [])
        XCTAssertEqual(tracker.observe([zoom], at: t0 + 10), [.ended(chrome), .started(zoom)])
    }
}
