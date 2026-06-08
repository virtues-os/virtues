//
//  QRScannerView.swift
//  Virtues
//
//  QR code scanner for the v1 pair-only flow. Accepts QRs produced by
//  `virtues link` on the box CLI or the "+ Add device" modal in the web
//  Devices page. Both shapes encode a `/pair#t=<token>` URL (or its
//  `virtues://pair?t=<token>` deep-link cousin); the scanner extracts the
//  token + endpoint and hands them to the caller to POST against
//  `/api/pair/consume`.
//
//  Legacy `{"e":..., "s":...}` JSON QRs from pre-v1 pairings are
//  recognized AS legacy and rejected with a clear message — the backend
//  endpoint they targeted (`/api/pairing/complete`) was removed in v1, so
//  there's nothing the app can do with them except tell the user to scan
//  a fresh QR.
//

import SwiftUI
import AVFoundation

/// SwiftUI view that presents a full-screen camera QR scanner
struct QRScannerView: View {
    /// Called when a v1 pair-flow QR is successfully decoded. Hands the
    /// caller `(endpoint, pairToken)` ready for `consumePairToken(...)`.
    let onScanned: (String, String) -> Void
    let onCancel: () -> Void

    @State private var cameraPermissionGranted = false
    @State private var showPermissionDenied = false
    @State private var invalidQRMessage: String?

    var body: some View {
        ZStack {
            if cameraPermissionGranted {
                // Camera preview with QR scanning
                QRCameraPreview(onCodeScanned: handleScannedCode)
                    .ignoresSafeArea()

                // Overlay with scanning frame
                scannerOverlay
            } else if showPermissionDenied {
                permissionDeniedView
            } else {
                Color.black.ignoresSafeArea()
                    .onAppear { checkCameraPermission() }
            }

            // Close button
            VStack {
                HStack {
                    Spacer()
                    Button(action: onCancel) {
                        Image(systemName: "xmark.circle.fill")
                            .font(.title)
                            .foregroundColor(.white)
                            .shadow(radius: 4)
                    }
                    .padding()
                }
                Spacer()
            }

            // Invalid QR toast
            if let message = invalidQRMessage {
                VStack {
                    Spacer()
                    Text(message)
                        .font(.subheadline)
                        .foregroundColor(.white)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                        .background(Color.black.opacity(0.75))
                        .cornerRadius(8)
                        .padding(.bottom, 100)
                        .transition(.opacity)
                }
                .animation(.easeInOut(duration: 0.3), value: invalidQRMessage)
            }
        }
    }

    // MARK: - Scanner Overlay

    private var scannerOverlay: some View {
        GeometryReader { geometry in
            let frameSize: CGFloat = min(geometry.size.width, geometry.size.height) * 0.65
            let frameOrigin = CGPoint(
                x: (geometry.size.width - frameSize) / 2,
                y: (geometry.size.height - frameSize) / 2 - 40
            )

            ZStack {
                // Semi-transparent background
                Color.black.opacity(0.5)
                    .ignoresSafeArea()

                // Clear scanning area
                Rectangle()
                    .frame(width: frameSize, height: frameSize)
                    .position(
                        x: frameOrigin.x + frameSize / 2,
                        y: frameOrigin.y + frameSize / 2
                    )
                    .blendMode(.destinationOut)

                // Corner brackets
                ScannerCorners(size: frameSize)
                    .position(
                        x: frameOrigin.x + frameSize / 2,
                        y: frameOrigin.y + frameSize / 2
                    )

                // Instruction text
                VStack {
                    Spacer()

                    VStack(spacing: 8) {
                        Text("Scan QR Code")
                            .font(.system(size: 20, weight: .semibold))
                            .foregroundColor(.white)

                        Text("Point your camera at the QR code\nshown in the Virtues web app")
                            .font(.subheadline)
                            .foregroundColor(.white.opacity(0.8))
                            .multilineTextAlignment(.center)
                    }
                    .padding(.bottom, 60)
                }
            }
            .compositingGroup()
        }
    }

    // MARK: - Permission Denied

    private var permissionDeniedView: some View {
        VStack(spacing: 20) {
            Image(systemName: "camera.fill")
                .font(.system(size: 48))
                .foregroundColor(.warmForegroundMuted)

            Text("Camera Access Required")
                .font(.headline)

            Text("Virtues needs camera access to scan QR codes for device pairing.")
                .font(.body)
                .foregroundColor(.warmForegroundMuted)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)

            Button("Open Settings") {
                if let url = URL(string: UIApplication.openSettingsURLString) {
                    UIApplication.shared.open(url)
                }
            }
            .padding()
            .background(Color.warmPrimary)
            .foregroundColor(.white)
            .cornerRadius(12)

            Button("Cancel", action: onCancel)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    // MARK: - Logic

    private func checkCameraPermission() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            cameraPermissionGranted = true
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                DispatchQueue.main.async {
                    if granted {
                        cameraPermissionGranted = true
                    } else {
                        showPermissionDenied = true
                    }
                }
            }
        default:
            showPermissionDenied = true
        }
    }

    private func handleScannedCode(_ code: String) {
        guard let parsed = QRScannerView.parsePairURL(code) else {
            // Detect legacy `{"e":..., "s":...}` payloads so we can show a
            // useful error rather than a generic "invalid code" toast.
            if let data = code.data(using: .utf8),
               (try? JSONSerialization.jsonObject(with: data)) is [String: Any] {
                showInvalid(
                    "This QR is from an older Virtues version. " +
                    "Scan a fresh QR from /virtues/devices on your box."
                )
            } else {
                showInvalid("Not a Virtues pair URL")
            }
            return
        }

        let generator = UINotificationFeedbackGenerator()
        generator.notificationOccurred(.success)

        onScanned(parsed.endpoint, parsed.token)
    }

    private func showInvalid(_ message: String) {
        invalidQRMessage = message
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
            invalidQRMessage = nil
        }
    }

    /// Extract `(endpoint, token)` from one of the accepted v1 pair URL
    /// shapes. Returns `nil` for anything else; the caller decides how to
    /// surface the failure to the user.
    ///
    /// Accepted forms:
    ///   - `https://virtues.local/pair#t=<token>`         (browser-friendly URL the box prints)
    ///   - `https://<box-ip>/pair#t=<token>`              (IP fallback for clients without mDNS)
    ///   - `virtues://pair?t=<token>&e=<endpoint>`        (iOS deep-link with explicit endpoint)
    static func parsePairURL(_ raw: String) -> (endpoint: String, token: String)? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: trimmed) else { return nil }

        // virtues://pair?t=...&e=...  — explicit-endpoint deep link
        if url.scheme == "virtues", url.host == "pair",
           let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
           let items = components.queryItems {
            let token = items.first(where: { $0.name == "t" })?.value ?? ""
            let endpoint = items.first(where: { $0.name == "e" })?.value ?? ""
            if !token.isEmpty, !endpoint.isEmpty {
                return (endpoint, token)
            }
            return nil
        }

        // https://<box>/pair#t=<token>  — what `virtues link` prints
        guard let scheme = url.scheme,
              scheme == "http" || scheme == "https",
              url.path == "/pair",
              let host = url.host else {
            return nil
        }
        // The token lives in the fragment as `t=<token>` (URLs sent to
        // browsers; fragments never leave the client).
        guard let fragment = url.fragment else { return nil }
        let token = extractFragmentValue(named: "t", from: fragment)
        guard let token, !token.isEmpty else { return nil }

        // Endpoint is the URL's origin (scheme://host[:port]).
        var endpoint = "\(scheme)://\(host)"
        if let port = url.port {
            endpoint += ":\(port)"
        }
        return (endpoint, token)
    }

    private static func extractFragmentValue(named name: String, from fragment: String) -> String? {
        // Fragments aren't standard query strings, but they typically look
        // like `t=abc` or `t=abc&kind=foo`. Parse defensively.
        for pair in fragment.split(separator: "&") {
            let parts = pair.split(separator: "=", maxSplits: 1).map(String.init)
            if parts.count == 2, parts[0] == name {
                return parts[1].removingPercentEncoding ?? parts[1]
            }
        }
        return nil
    }
}

// MARK: - Scanner Corner Brackets

struct ScannerCorners: View {
    let size: CGFloat
    private let cornerLength: CGFloat = 30
    private let lineWidth: CGFloat = 4

    var body: some View {
        ZStack {
            // Top-left
            cornerBracket(rotation: 0)
                .offset(x: -size / 2, y: -size / 2)
            // Top-right
            cornerBracket(rotation: 90)
                .offset(x: size / 2, y: -size / 2)
            // Bottom-right
            cornerBracket(rotation: 180)
                .offset(x: size / 2, y: size / 2)
            // Bottom-left
            cornerBracket(rotation: 270)
                .offset(x: -size / 2, y: size / 2)
        }
    }

    private func cornerBracket(rotation: Double) -> some View {
        Path { path in
            path.move(to: CGPoint(x: 0, y: cornerLength))
            path.addLine(to: CGPoint(x: 0, y: 0))
            path.addLine(to: CGPoint(x: cornerLength, y: 0))
        }
        .stroke(Color.white, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round, lineJoin: .round))
        .rotationEffect(.degrees(rotation))
    }
}

// MARK: - Camera Preview (UIViewRepresentable)

struct QRCameraPreview: UIViewRepresentable {
    let onCodeScanned: (String) -> Void

    func makeUIView(context: Context) -> UIView {
        let view = UIView(frame: .zero)
        view.backgroundColor = .black

        let captureSession = AVCaptureSession()
        context.coordinator.captureSession = captureSession

        guard let videoCaptureDevice = AVCaptureDevice.default(for: .video),
              let videoInput = try? AVCaptureDeviceInput(device: videoCaptureDevice),
              captureSession.canAddInput(videoInput) else {
            return view
        }

        captureSession.addInput(videoInput)

        let metadataOutput = AVCaptureMetadataOutput()
        guard captureSession.canAddOutput(metadataOutput) else { return view }

        captureSession.addOutput(metadataOutput)
        metadataOutput.setMetadataObjectsDelegate(context.coordinator, queue: DispatchQueue.main)
        metadataOutput.metadataObjectTypes = [.qr]

        let previewLayer = AVCaptureVideoPreviewLayer(session: captureSession)
        previewLayer.videoGravity = .resizeAspectFill
        previewLayer.frame = view.bounds
        view.layer.addSublayer(previewLayer)
        context.coordinator.previewLayer = previewLayer

        DispatchQueue.global(qos: .userInitiated).async {
            captureSession.startRunning()
        }

        return view
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        context.coordinator.previewLayer?.frame = uiView.bounds
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onCodeScanned: onCodeScanned)
    }

    static func dismantleUIView(_ uiView: UIView, coordinator: Coordinator) {
        coordinator.captureSession?.stopRunning()
    }

    class Coordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate {
        let onCodeScanned: (String) -> Void
        var captureSession: AVCaptureSession?
        var previewLayer: AVCaptureVideoPreviewLayer?
        private var hasScanned = false

        init(onCodeScanned: @escaping (String) -> Void) {
            self.onCodeScanned = onCodeScanned
        }

        func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject], from connection: AVCaptureConnection) {
            // Only process the first valid QR code, prevent double-fire
            guard !hasScanned,
                  let metadataObject = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
                  metadataObject.type == .qr,
                  let stringValue = metadataObject.stringValue else {
                return
            }

            hasScanned = true
            onCodeScanned(stringValue)

            // Reset after a delay to allow retry if the QR was invalid
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak self] in
                self?.hasScanned = false
            }
        }
    }
}
