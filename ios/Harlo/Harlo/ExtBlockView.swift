import HarloKit
import SwiftUI

/// One block of an extension's view (see `ExtBlocks`).
struct ExtBlockView: View {
    @Environment(\.palette) private var p
    let block: ExtBlock

    var body: some View {
        switch block {
        case .lines(let lines):
            // (A Box's blank rows around Markdown or a picture: left out.)
            if lines.contains(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty }) {
                ExtLines(lines: lines)
            }
        case .markdown(let parts):
            ForEach(Array(parts.enumerated()), id: \.offset) { _, part in
                if part.isCode {
                    CodeBlock(spans: Self.codeSpans(part), language: part.lang, raw: part.code)
                } else {
                    RichTextView(blocks: RichCache.blocks(part.html))
                }
            }
        case .picture(let data, _):
            ExtPicture(data: data)
        case .pictureTooBig:
            Label("A picture, too big to send to the phone", systemImage: "photo")
                .scaledFont(13.5)
                .foregroundStyle(Color(p.muted))
        }
    }

    /// A code part's highlighted lines (its text, when it has none).
    static func codeSpans(_ part: ExtMarkdownPart) -> [Span] {
        if case .code(let spans)? = RichCache.blocks(part.html).first { return spans }
        return [Span(part.code, code: true)]
    }
}
