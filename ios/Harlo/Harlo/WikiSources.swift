import HarloKit
import SwiftUI

/// A wiki page's sources: the files it was written from, each with the
/// lines it cites; one opens the file there.
struct WikiSources: View {
    @Environment(\.palette) private var p
    let list: [WikiPageText.Source]
    var open: (WikiPageText.Source) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Sources")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(p.muted))
                .padding(.bottom, 2)
                .accessibilityAddTraits(.isHeader)
            ForEach(list, id: \.path) { s in
                Button {
                    open(s)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(s.path).font(.footnote.monospaced()).foregroundStyle(Color(p.primary.legible(on: p.background, toward: p.foreground, ratio: 5))).multilineTextAlignment(.leading)
                        let about = Self.about(s)
                        if !about.isEmpty {
                            Text(about).font(.caption).foregroundStyle(Color(p.muted)).multilineTextAlignment(.leading)
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("wikiSource")
                .accessibilityHint("Opens the file")
            }
        }
        .padding(.top, 8)
    }

    /// "lines 3–9, 40 · cited twice".
    static func about(_ s: WikiPageText.Source) -> String {
        let lines = s.ranges.map { $0.count == 1 ? "\($0.lowerBound)" : "\($0.lowerBound)–\($0.upperBound)" }
        let cited = s.claims == 0 ? "" : s.claims == 1 ? "cited once" : s.claims == 2 ? "cited twice" : "cited \(s.claims) times"
        return [lines.isEmpty ? "" : "lines " + lines.joined(separator: ", "), cited].filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
