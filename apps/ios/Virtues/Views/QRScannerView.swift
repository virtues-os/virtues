//
//  QRScannerView.swift
//  Virtues
//
//  QR code scanner for device pairing. Scans a QR code containing
//  the server endpoint and pairing session ID, enabling zero-typing setup.
//

import SwiftUI
import AVFoundation

/// Payload encoded in the Virtues pairing QR code
struct QRPairingPayload: Codable {
    let e: String  // server endpoint URL
    let s: String  // source_id (pairing session)
}

/// SwiftUI view that presents a full-screen camera QR scanner
struct QRScannerView: View {
    let onScanned: (String, String) -> Void  // (endpoint, sourceId)
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
        // Try to parse as Virtues pairing payload
        guard let data = code.data(using: .utf8),
              let payload = try? JSONDecoder().decode(QRPairingPayload.self, from: data),
              !payload.e.isEmpty,
              !payload.s.isEmpty,
              URL(string: payload.e) != nil else {
            // Not a valid Virtues QR code
            invalidQRMessage = "Not a Virtues pairing code"
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                invalidQRMessage = nil
            }
            return
        }

        // Haptic feedback on successful scan
        let generator = UINotificationFeedbackGenerator()
        generator.notificationOccurred(.success)

        onScanned(payload.e, payload.s)
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
