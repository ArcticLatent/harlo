import HarloKit
import SwiftUI
import UIKit

/// A session as a tile: its model's maker's logo on a rounded square of the
/// maker's colour; the agent's own mark while the model isn't known yet.
struct AgentTile: View {
    @Environment(\.palette) private var p
    let harness: String
    /// The model's maker, a logo's id ("" not known, or an agent alone).
    var maker = ""
    var size: CGFloat = 44

    private var modelFirst: Bool { Palette.modelFirst(harness: harness, maker: maker) && UIImage(named: "Maker-" + maker) != nil }

    var body: some View {
        let t = modelFirst ? p.makerTile(maker) : p.agentTile(harness)
        RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
            .fill(Color(t.fill))
            .frame(width: size, height: size)
            .overlay {
                Image(modelFirst ? "Maker-" + maker : AgentTile.known.contains(harness) ? "Mark-" + harness : "Mark-demo")
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: size * 0.5, height: size * 0.5)
                    .foregroundStyle(Color(t.ink))
            }
            .accessibilityElement()
            .accessibilityLabel(modelFirst ? AgentTile.makerName(maker) : Style.harnessLabel(harness))
    }

    static let known = ["harlo", "demo"]

    /// A maker's name, for VoiceOver.
    static func makerName(_ id: String) -> String {
        switch id {
        case "anthropic": "Anthropic"
        case "openai": "OpenAI"
        case "google": "Google"
        case "zai-coding-plan": "Z.ai"
        case "kimi-for-coding": "Moonshot AI"
        case "deepseek": "DeepSeek"
        case "xai": "xAI"
        case "minimax-coding-plan": "MiniMax"
        case "mistral": "Mistral"
        case "opencode": "OpenCode"
        default: id
        }
    }
}
