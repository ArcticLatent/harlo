import HarloKit
import SwiftUI

/// The model: its provider's tile and its name; tap to pick another.
struct ModelChip: View {
    @Environment(\.palette) private var p
    let session: OpenSession
    /// The largest text sizes: its name isn't cut.
    let big: Bool
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 6) {
                AgentTile(maker: session.info.maker, size: 20)
                Text(session.model.isEmpty ? "Model" : session.model)
                    .scaledFont(13.5, .medium)
                    .lineLimit(1)
                    .frame(maxWidth: big ? nil : 120, alignment: .leading)
                    .fixedSize(horizontal: !big, vertical: false)
                Image(systemName: "chevron.down").scaledFont(9, .bold).foregroundStyle(Color(p.muted))
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .frame(minHeight: 34)
            .background { RoundedRectangle(cornerRadius: Corner.small).fill(Color(p.raised.mix(p.foreground, 0.04))) }
            .overlay { RoundedRectangle(cornerRadius: Corner.small).strokeBorder(Color(p.hairline), lineWidth: 1) }
            .contentShape(RoundedRectangle(cornerRadius: Corner.small))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Model: \(session.model.isEmpty ? "default" : session.model)")
    }
}
