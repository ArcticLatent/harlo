import HarloKit
import SwiftUI
import UIKit

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
