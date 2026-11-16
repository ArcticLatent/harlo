import HarloKit
import SwiftUI
import UIKit

/// harlo's message to the agent (a helper's report, the answer to a
/// split): harlo's mark, folded to its first lines (tap for the rest), and
/// the session it's about to open. Not yours: no bubble.
struct HarloMessage: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image("Logo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: Transcript.tile, height: Transcript.tile)
                Text("harlo")
                    .scaledFont(16.5, .semibold)
                Text("to the agent")
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
            }
            Button {
                withAnimation(.easeOut(duration: 0.2)) { open.toggle() }
            } label: {
                // Its plain line, folded; all of it, as the agent got it,
                // opened.
                VStack(alignment: .leading, spacing: 4) {
                    // Its plain line first; opened, what the agent got
                    // under it, smaller.
                    Text(row.label.isEmpty ? row.text : row.label)
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                        .multilineTextAlignment(.leading)
                        .lineLimit(open ? nil : 2)
                    if open && !row.label.isEmpty {
                        Text("As the agent got it")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color(p.muted))
                            .padding(.top, 6)
                        Text(row.text)
                            .font(.caption)
                            .foregroundStyle(Color(p.muted))
                            .multilineTextAlignment(.leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .buttonStyle(.plain)
            if !row.subtitle.isEmpty, let helper = model.sessions.first(where: { $0.key == row.subtitle }) {
                Button("Open helper") {
                    model.open(helper)
                }
                .scaledFont(14, .medium)
                .foregroundStyle(Color(p.primary))
                .buttonStyle(.plain)
            }
        }
        .padding(.top, 4)
    }
}
