import HarloKit
import SwiftUI
import UIKit

/// A model's provider as a tile: its logo on a rounded square of its brand's
/// colour. A plain tile with a chip while the model isn't known.
struct ProviderTile: View {
    /// The provider's logo, by its id ("" not known).
    let maker: String
    let size: CGFloat
    let palette: Palette

    /// The app has this provider's logo.
    private var known: Bool { !maker.isEmpty && UIImage(named: "Maker-" + maker) != nil }

    var body: some View {
        let has = known
        let tile = palette.makerTile(has ? maker : "")
        let shape = RoundedRectangle(cornerRadius: size * 0.2, style: .continuous)
        shape
            .fill(ProviderTile.color(tile.fill))
            .frame(width: size, height: size)
            .overlay {
                if !has {
                    Image(systemName: "cpu")
                        .font(.system(size: size * 0.46, weight: .medium))
                        .foregroundStyle(ProviderTile.color(tile.ink ?? palette.foreground))
                } else if let ink = tile.ink {
                    logo(.template).foregroundStyle(ProviderTile.color(ink))
                } else {
                    logo(.original)
                }
            }
            // A black or a white tile still has an edge on any background.
            .overlay { shape.strokeBorder(ProviderTile.color(palette.hairline), lineWidth: 1) }
            .accessibilityElement()
            .accessibilityLabel(maker.isEmpty ? "Model not known yet" : ProviderTile.name(maker))
    }

    private func logo(_ mode: Image.TemplateRenderingMode) -> some View {
        Image("Maker-" + maker)
            .resizable()
            .renderingMode(mode)
            .scaledToFit()
            .frame(width: size * 0.58, height: size * 0.58)
    }

    private static func color(_ c: RGBA) -> Color {
        Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: c.a)
    }

    /// A provider's name, for VoiceOver.
    static func name(_ id: String) -> String {
        switch id {
        case "anthropic": "Anthropic"
        case "bailian-coding-plan": "Alibaba Cloud"
        case "cerebras": "Cerebras"
        case "claude-code": "Claude Code"
        case "deepseek": "DeepSeek"
        case "fireworks": "Fireworks AI"
        case "github-copilot": "GitHub Copilot"
        case "google": "Google"
        case "groq": "Groq"
        case "kimi-for-coding": "Moonshot AI"
        case "lmstudio": "LM Studio"
        case "minimax-coding-plan", "minimax-cn-coding-plan": "MiniMax"
        case "mistral": "Mistral"
        case "ollama": "Ollama"
        case "openai": "OpenAI"
        case "opencode": "OpenCode"
        case "openrouter": "OpenRouter"
        case "together": "Together AI"
        case "xai": "xAI"
        case "zai-coding-plan": "Z.ai"
        case "zhipuai-coding-plan": "Zhipu AI"
        default: id
        }
    }
}
