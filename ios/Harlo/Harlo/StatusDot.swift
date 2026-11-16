import HarloKit
import SwiftUI
import UIKit

/// A session's state as the desktop's sidebar shows it: a dot, pulsing
/// while it works.
struct StatusDot: View {
    @Environment(\.palette) private var p
    let status: String
    @State private var pulse = false

    var body: some View {
        Circle()
            .fill(Color(p.status(status)))
            .frame(width: 8, height: 8)
            .opacity(status == "busy" && pulse ? 0.35 : 1)
            .animation(status == "busy" ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true) : .default, value: pulse)
            .onAppear { pulse = true }
            .accessibilityLabel(StatusDot.words(status))
    }

    static func words(_ status: String) -> String {
        ["busy": "working", "starting": "starting", "input": "needs you", "unread": "finished, unread", "exited": "stopped"][status] ?? ""
    }
}
