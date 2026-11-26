import HarloKit
import SwiftUI

/// A row's words: what it is and where it stands, what it's about, and
/// the start of anything more it has to say.
struct JobRowWords: View {
    @Environment(\.palette) private var p
    let row: JobRow

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Text(row.title)
                    .scaledFont(12, .semibold)
                    .foregroundStyle(Color(p.primary))
                Text(row.note)
                    .scaledFont(12)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
            }
            Text(row.text)
                .scaledFont(13)
                .lineLimit(1)
                .truncationMode(.middle)
            if !row.detail.isEmpty {
                Text(row.detail)
                    .scaledFont(12)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.spoken)
    }
}
