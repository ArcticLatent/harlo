import HarloKit
import SwiftUI

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
            Text("In harlo on your computer, click the phone button at the bottom of the left rail (or Settings → Phone → Pair a device), then scan the QR code it shows.")
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
