import HarloKit
import SwiftUI

/// Templates saved in harlo: one fills the composer (send it when ready).
struct TemplateStrip: View {
    @Environment(\.palette) private var p
    let templates: [SessionTemplate]
    var apply: (SessionTemplate) -> Void

    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 8) {
                ForEach(templates) { t in
                    Button { apply(t) } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "bookmark").scaledFont(12, .semibold).foregroundStyle(Color(p.primary))
                            Text(t.name).scaledFont(14, .medium).lineLimit(1)
                        }
                        .padding(.horizontal, 12)
                        .frame(minHeight: 32)
                        .background { Capsule().fill(Color(p.raised)) }
                        .overlay { Capsule().strokeBorder(Color(p.hairline), lineWidth: 1) }
                        .foregroundStyle(Color(p.foreground))
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Fills in the agent, model, permissions and message")
                }
            }
            .padding(.horizontal, 2)
        }
        .scrollIndicators(.hidden)
    }
}
