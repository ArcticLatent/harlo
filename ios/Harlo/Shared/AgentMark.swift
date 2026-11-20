import HarloKit
import SwiftUI

/// An agent's tile: its mark, dark, on a rounded square of its colour.
struct AgentMark: View {
    let harness: String
    let size: CGFloat
    let palette: Palette

    var body: some View {
        let t = palette.agentTile(harness)
        RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
            .fill(Color(.sRGB, red: t.fill.r, green: t.fill.g, blue: t.fill.b, opacity: t.fill.a))
            .frame(width: size, height: size)
            .overlay {
                Image(harness == "pi" ? "Mark-pi" : "Mark-demo")
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: size * 0.5, height: size * 0.5)
                    .foregroundStyle(Color(.sRGB, red: t.ink.r, green: t.ink.g, blue: t.ink.b, opacity: t.ink.a))
            }
    }
}
