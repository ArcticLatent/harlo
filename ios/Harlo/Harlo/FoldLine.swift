import SwiftUI

/// A line that shows or hides the rows under it: a project's Settled
/// shelf.
struct FoldLine: View {
    @Environment(\.palette) private var p
    let icon: String
    let title: String
    let open: Bool
    let accessibilityTitle: String
    var toggle: () -> Void

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.2)) { toggle() }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: icon)
                    .scaledFont(13)
                Text(title)
                    .scaledFont(14.5, .medium)
                Image(systemName: "chevron.right")
                    .scaledFont(11, .semibold)
                    .rotationEffect(.degrees(open ? 90 : 0))
                Spacer()
            }
            .foregroundStyle(Color(p.muted))
            .padding(.leading, 6)
            .padding(.trailing, 6)
            .frame(minHeight: 34)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityTitle)
        .accessibilityValue(open ? "Shown" : "Hidden")
        .plainRow(top: 2, bottom: 2)
    }
}
