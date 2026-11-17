import HarloKit
import SwiftUI
import UIKit

/// A session as a tile: its model's maker's logo on a rounded square of the
/// maker's colour, with the agent that runs it as a small tag on the lower
/// left. The agent's own mark when the model isn't known yet, or when it's
/// the agent's own maker's (Claude Code on Claude).
struct AgentTile: View {
    @Environment(\.palette) private var p
    let harness: String
    /// The model's maker, a logo's id ("" not known, or an agent alone).
    var maker = ""
    var size: CGFloat = 44
    /// What's behind it (the tag's ring cuts into it).
    var ring: RGBA?

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
            .overlay(alignment: .bottomLeading) {
                if modelFirst {
                    // The agent as a small badge in its own tile's colours,
                    // ringed in what's behind so it cuts into the corner.
                    let tag = p.agentTile(harness)
                    Image(AgentTile.known.contains(harness) ? "Mark-" + harness : "Mark-demo")
                        .resizable()
                        .renderingMode(.template)
                        .scaledToFit()
                        .frame(width: size * 0.22, height: size * 0.22)
                        .foregroundStyle(Color(tag.ink))
                        .frame(width: size * 0.4, height: size * 0.4)
                        .background(Color(tag.fill), in: .circle)
                        .padding(size * 0.05)
                        .background(Color(ring ?? p.background), in: .circle)
                        .offset(x: -size * 0.1, y: size * 0.1)
                }
            }
            .accessibilityElement()
            .accessibilityLabel(modelFirst ? Style.harnessLabel(harness) + ", " + AgentTile.makerName(maker) : Style.harnessLabel(harness))
    }

    static let known = ["harlo", "claude", "demo"]

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
