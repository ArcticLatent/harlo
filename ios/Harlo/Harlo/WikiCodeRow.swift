import HarloKit
import SwiftUI

/// A declaration a wiki search found in the code: its kind and name, its
/// place, and its doc line.
struct WikiCodeRow: View {
    @Environment(\.palette) private var p
    let code: WikiHits.Code

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(code.kind) \(code.name)").font(.body.monospaced().weight(.medium))
            Text(code.place).font(.caption).foregroundStyle(Color(p.muted))
            if !code.doc.isEmpty {
                Text(code.doc).font(.footnote).foregroundStyle(Color(p.muted)).lineLimit(2)
            }
        }
        .padding(.vertical, 2)
    }
}
