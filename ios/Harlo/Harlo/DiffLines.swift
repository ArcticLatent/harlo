import HarloKit
import SwiftUI
import UIKit

/// An edit's or a write's lines, tinted as the desktop's diffs.
struct DiffLines: View {
    @Environment(\.palette) private var p
    let diff: String
    /// Inside a card already (no card of its own).
    var bare = false
    /// The most lines shown.
    var limit = 400
    @AppStorage(CodeWrap.key) private var wrap = true

    var body: some View {
        CodeScroll(wrap: wrap) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(diff.split(separator: "\n", omittingEmptySubsequences: false).prefix(limit).enumerated()), id: \.offset) { _, line in
                    let tint: RGBA? = line.hasPrefix("+") ? (p.token(9) ?? p.success) : line.hasPrefix("-") ? (p.token(10) ?? p.error) : nil
                    Group {
                        if wrap, let first = line.first {
                            let (n, rest) = CodeLines.indent(line.dropFirst())
                            WrappedLine(lead: String(first), indent: n, font: Style.mono(11.5)) {
                                Text(rest.isEmpty ? " " : RichText.breakable(String(rest))).font(Style.mono(11.5))
                                    .accessibilityLabel(String(rest))
                            }
                        } else {
                            Text(line.isEmpty ? " " : String(line))
                                .font(Style.mono(11.5))
                                .fixedSize(horizontal: true, vertical: false)
                        }
                    }
                        .padding(.horizontal, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(tint.map { Color($0).opacity(0.14) } ?? Color.clear)
                }
            }
            .padding(.vertical, 6)
        }
        .background(bare ? Color(p.codeBackground) : Color.clear)
        .modifier(CardUnless(bare: bare, radius: Corner.small, fill: p.codeBackground))
    }
}
