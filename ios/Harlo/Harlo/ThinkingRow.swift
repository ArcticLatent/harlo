import HarloKit
import SwiftUI
import UIKit

/// What the agent thought: shown open, so its reasoning reads along with
/// the reply; tap "Thought" to fold it.
struct ThinkingRow: View {
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = true

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                open.toggle()
            } label: {
                HStack(spacing: 6) {
                    Text("✦")
                    Text("Thought").italic()
                    Image(systemName: open ? "chevron.down" : "chevron.right").font(.caption2)
                }
                .scaledFont(13.5)
                .foregroundStyle(Color(p.muted))
            }
            .buttonStyle(.plain)
            if open {
                Text(row.text)
                    .font(.footnote)
                    .italic()
                    .foregroundStyle(Color(p.muted))
                    .textSelection(.enabled)
            }
        }
    }
}
