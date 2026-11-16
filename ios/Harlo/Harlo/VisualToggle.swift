import HarloKit
import SwiftUI

/// The small link under a visual block that switches it to its code and back.
struct VisualToggle: View {
    @Environment(\.palette) private var p
    let title: String
    let icon: String
    let action: () -> Void

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { action() }
        } label: {
            Label(title, systemImage: icon)
                .font(.footnote.weight(.medium))
                .foregroundStyle(Color(p.primary))
                .padding(.horizontal, 4)
                // (A full-size target, whatever its text.)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
