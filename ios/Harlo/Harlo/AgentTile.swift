import HarloKit
import SwiftUI
import UIKit

/// An agent as a tile: its mark, dark, on a rounded square of its colour.
struct AgentTile: View {
    @Environment(\.palette) private var p
    let harness: String
    var size: CGFloat = 44

    var body: some View {
        let t = p.agentTile(harness)
        RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
            .fill(Color(t.fill))
            .frame(width: size, height: size)
            .overlay {
                Image(AgentTile.known.contains(harness) ? "Mark-" + harness : "Mark-demo")
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: size * 0.5, height: size * 0.5)
                    .foregroundStyle(Color(t.ink))
            }
            .accessibilityLabel(Style.harnessLabel(harness))
    }

    static let known = ["harlo", "claude", "demo"]
}
