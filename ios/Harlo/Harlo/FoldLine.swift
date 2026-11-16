import SwiftUI

/// A line that shows or hides the rows under it: a project's Settled
/// shelf, or a session's helpers ("Helpers · 3", and whether one works or
/// waits for you).
struct FoldLine: View {
    @Environment(\.palette) private var p
    let icon: String
    let title: String
    let open: Bool
    /// Set in under a session's card (its helpers).
    var nested = false
    var waiting = false
    var working = false
    let accessibilityTitle: String
    var toggle: () -> Void

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.2)) { toggle() }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: icon)
                    .scaledFont(nested ? 12 : 13)
                Text(title)
                    .scaledFont(14.5, .medium)
                Image(systemName: "chevron.right")
                    .scaledFont(11, .semibold)
                    .rotationEffect(.degrees(open ? 90 : 0))
                Spacer()
                if waiting {
                    Text("Waiting for you")
                        .scaledFont(13, .medium)
                        .foregroundStyle(Color(p.warning))
                } else if working {
                    Text("Working")
                        .scaledFont(13, .medium)
                        .foregroundStyle(Color(p.primary))
                }
            }
            .foregroundStyle(Color(p.muted))
            .padding(.leading, nested ? 26 : 6)
            .padding(.trailing, 6)
            .frame(minHeight: nested ? 30 : 34)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityTitle)
        .accessibilityValue(open ? "Shown" : "Hidden")
        .plainRow(top: nested ? 0 : 2, bottom: 2)
    }
}
