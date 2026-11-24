import HarloKit
import SwiftUI
import UIKit

/// Code, highlighted; a long block shows its first lines until opened.
struct CodeBlock: View {
    @Environment(\.palette) private var p
    let spans: [Span]
    let language: String
    var raw: String = ""
    @State private var whole = false
    @AppStorage(CodeWrap.key) private var wrap = true

    /// Longer than this folds, to its first `shown` lines.
    static let long = 40
    static let shown = 24

    var lines: Int { RichText.lineCount(spans) }
    var folded: Bool { !whole && lines > Self.long }
    var visible: [Span] { folded ? RichText.firstLines(spans, Self.shown) : spans }

    /// "Code, rust, 2 lines".
    var spokenLabel: String {
        ["Code", language.isEmpty ? "" : language, lines == 1 ? "1 line" : "\(lines) lines"].filter { !$0.isEmpty }.joined(separator: ", ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .scaledFont(12.5)
                    .foregroundStyle(Color(p.muted))
                if lines > Self.long {
                    Text("· \(lines) lines")
                        .scaledFont(12.5)
                        .foregroundStyle(Color(p.muted))
                }
                Spacer()
                Button {
                    UIPasteboard.general.string = raw.isEmpty ? RichText.plain(spans) : raw
                } label: {
                    Image(systemName: "doc.on.doc").font(.caption)
                        // (A full-size target in a small header.)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        .padding(.trailing, -12)
                }
                .accessibilityLabel("Copy code")
            }
            .padding(.horizontal, 10)
            Group {
                if wrap {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(CodeLines.lines(visible).enumerated()), id: \.offset) { _, line in
                            WrappedLine(indent: line.indent, font: Style.mono(13)) {
                                Text(Style.attributed(line.spans.isEmpty ? [Span(" ")] : RichText.breakable(line.spans), p, size: 15, inCode: true))
                            }
                        }
                    }
                    // (Not selectable, wrapped: its breaks would be copied;
                    // Copy code above copies it as it is.)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    ScrollView(.horizontal) {
                        Text(Style.attributed(visible, p, size: 15, inCode: true))
                            .textSelection(.enabled)
                            .fixedSize(horizontal: true, vertical: false)
                            .padding(10)
                    }
                    .scrollIndicators(.hidden)
                }
            }
            // Spoken as one: what it is, then the code itself.
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(spokenLabel)
            .accessibilityValue(RichText.plain(visible))
            .mask {
                // Fading out where it's cut.
                LinearGradient(stops: [.init(color: .black, location: 0), .init(color: .black, location: folded ? 0.75 : 1),
                                       .init(color: .black.opacity(folded ? 0.15 : 1), location: 1)],
                               startPoint: .top, endPoint: .bottom)
            }
            if lines > Self.long {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { whole.toggle() }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: whole ? "chevron.up" : "chevron.down").scaledFont(10, .semibold)
                        Text(whole ? "Show less" : "Show all \(lines) lines").scaledFont(13, .medium)
                    }
                    .foregroundStyle(Color(p.primary))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .overlay(alignment: .top) { Rectangle().fill(Color(p.border)).frame(height: 1) }
            }
        }
        .card(p, radius: 10, fill: p.codeBackground)
    }
}
