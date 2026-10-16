import AVFoundation
import HarloKit
import SwiftUI
import UIKit

/// No harlo yet: scan the QR code harlo shows (Pair a device), or paste
/// its link.
struct PairView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @State private var scanning = false

    var body: some View {
        VStack(spacing: 22) {
            Spacer()
            HarloLogo(height: 54, wordColor: Color(p.foreground))
                .padding(.bottom, 6)
            Text("In harlo on your computer, click the phone button at the bottom of the left rail (or Settings → Devices → Pair a device), then scan the QR code it shows.")
                .multilineTextAlignment(.center)
                .foregroundStyle(Color(p.muted))
                .padding(.horizontal, 28)
            if model.pairing {
                ProgressView("Pairing…")
            } else {
                Button {
                    scanning = true
                } label: {
                    Label("Scan QR code", systemImage: "camera")
                        .frame(maxWidth: 260)
                }
                .buttonStyle(.borderedProminent)
                .foregroundStyle(Color(p.onPrimary))
                .controlSize(.large)
                PasteLinkButton()
                // No harlo yet (or App Review): a look around with sample sessions.
                Button {
                    model.startDemo()
                } label: {
                    Label("Try the demo", systemImage: "sparkles")
                }
                .accessibilityHint("Shows sample sessions; nothing connects to a computer")
            }
            Spacer()
            Label("Everything between this phone and harlo is end-to-end encrypted.", systemImage: "lock")
                .font(.footnote)
                .foregroundStyle(Color(p.muted))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 28)
        }
        .padding(.bottom, 12)
        .frame(maxWidth: .infinity)
        .screen()
        .sheet(isPresented: $scanning) {
            ScanSheet { code in
                scanning = false
                Task { await model.pair(code) }
            }
        }
    }
}

/// Pairs from a link on the clipboard (sent to this phone some other way).
struct PasteLinkButton: View {
    @Environment(HarloModel.self) private var model

    var body: some View {
        Button("Paste a pairing link") {
            guard let text = UIPasteboard.general.string, !text.isEmpty else {
                model.alert = "Copy harlo's pairing link first (the link under the QR code)."
                return
            }
            Task { await model.pair(text) }
        }
    }
}

/// The camera, looking for a QR code (asks for the camera first).
struct ScanSheet: View {
    var onFound: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var access = AVCaptureDevice.authorizationStatus(for: .video)

    var body: some View {
        NavigationStack {
            Group {
                switch access {
                case .authorized:
                    QRScanner(onFound: onFound)
                        .ignoresSafeArea()
                        .overlay {
                            RoundedRectangle(cornerRadius: 24)
                                .strokeBorder(.white.opacity(0.85), lineWidth: 3)
                                .frame(width: 250, height: 250)
                        }
                case .notDetermined:
                    ProgressView()
                        .task {
                            _ = await AVCaptureDevice.requestAccess(for: .video)
                            access = AVCaptureDevice.authorizationStatus(for: .video)
                        }
                default:
                    ContentUnavailableView {
                        Label("No camera access", systemImage: "camera.fill")
                    } description: {
                        Text("Allow harlo to use the camera in Settings to scan the QR code, or paste the pairing link instead.")
                    } actions: {
                        Button("Open Settings") {
                            if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                        }
                    }
                }
            }
            .navigationTitle("Scan harlo's QR code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

struct QRScanner: UIViewControllerRepresentable {
    var onFound: (String) -> Void

    func makeUIViewController(context: Context) -> ScannerController {
        let c = ScannerController()
        c.onFound = onFound
        return c
    }

    func updateUIViewController(_ c: ScannerController, context: Context) {
        c.onFound = onFound
    }
}

final class ScannerController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onFound: ((String) -> Void)?
    private let session = AVCaptureSession()
    private var preview: AVCaptureVideoPreviewLayer?
    private var found = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device),
              session.canAddInput(input)
        else { return }
        session.addInput(input)
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else { return }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        view.layer.addSublayer(layer)
        preview = layer
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        preview?.frame = view.bounds
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        let s = session
        DispatchQueue.global(qos: .userInitiated).async { s.startRunning() }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        let s = session
        DispatchQueue.global(qos: .userInitiated).async { s.stopRunning() }
    }

    // On the main queue (set above).
    nonisolated func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject], from connection: AVCaptureConnection) {
        let code = (metadataObjects.first as? AVMetadataMachineReadableCodeObject)?.stringValue
        MainActor.assumeIsolated { self.scanned(code) }
    }

    private func scanned(_ code: String?) {
        guard !found, let code, !code.isEmpty else { return }
        found = true
        UINotificationFeedbackGenerator().notificationOccurred(.success)
        onFound?(code)
    }
}
