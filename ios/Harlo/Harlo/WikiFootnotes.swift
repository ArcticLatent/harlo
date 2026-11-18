import HarloKit
import SwiftUI

/// A wiki page's footnotes: the code each claim comes from, at its lines
/// now, and what's wrong with it; one opens its file there.
struct WikiFootnotes: View {
    @Environment(\.palette) private var p
    let list: [WikiPageText.Footnote]
    var open: (WikiPageText.Footnote) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Footnotes")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(p.muted))
                .padding(.bottom, 2)
                .accessibilityAddTraits(.isHeader)
            ForEach(list) { f in
                Button {
                    open(f)
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(f.n)").font(.footnote.monospacedDigit()).foregroundStyle(Color(p.muted))
                        VStack(alignment: .leading, spacing: 2) {
                            Text(f.place).font(.footnote.monospaced())
                                .foregroundStyle(Color(f.state == "gone" ? p.muted : p.primary.legible(on: p.background, toward: p.foreground, ratio: 5)))
                                .multilineTextAlignment(.leading)
                            if !f.lines.isEmpty || !f.note.isEmpty {
                                about(f).font(.caption).multilineTextAlignment(.leading)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(f.state == "gone")
                .accessibilityIdentifier("wikiFootnote")
                .accessibilityHint(f.state == "gone" ? "" : "Opens the code")
            }
        }
        .padding(.top, 8)
    }

    /// "lines 1–3 · its code changed since the page was written", the
    /// note in the warning colour.
    func about(_ f: WikiPageText.Footnote) -> Text {
        let note = Text(f.note).foregroundStyle(Color(p.warning))
        if f.note.isEmpty { return Text(f.lines).foregroundStyle(Color(p.muted)) }
        if f.lines.isEmpty { return note }
        return Text("\(Text(f.lines).foregroundStyle(Color(p.muted))) · \(note)")
    }
}
