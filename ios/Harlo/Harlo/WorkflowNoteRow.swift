import HarloKit
import SwiftUI

/// A workflow's message to its agent: harlo's mark and what it's for in a
/// line (tap for the message itself). Not yours: no bubble.
struct WorkflowNoteRow: View {
    @Environment(\.palette) private var p
    let note: String
    let text: String
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image("Logo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: Transcript.tile, height: Transcript.tile)
                    .accessibilityHidden(true)
                Text("harlo")
                    .scaledFont(16.5, .semibold)
                Text("workflow")
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
            }
            Button {
                withAnimation(.easeOut(duration: 0.2)) { open.toggle() }
            } label: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(note)
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                        .multilineTextAlignment(.leading)
                    if open {
                        Text("As the agent got it")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color(p.muted))
                            .padding(.top, 6)
                        Text(text)
                            .font(.caption)
                            .foregroundStyle(Color(p.muted))
                            .multilineTextAlignment(.leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint(open ? "Hides the message" : "Shows the message as the agent got it")
        }
        .padding(.top, 4)
    }
}
