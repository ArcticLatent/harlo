import HarloKit
import SwiftUI

/// Above the composer, near where the agent compacts the conversation on
/// its own: how many tokens are left.
struct CompactLine: View {
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        let note = session.info.compactNote
        if !note.isEmpty {
            HStack(spacing: 8) {
                Image(systemName: "arrow.down.right.and.arrow.up.left")
                    .scaledFont(12)
                    .foregroundStyle(Color(p.warning))
                Text(note)
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }
}
