import HarloKit
import SwiftUI

/// A block of a wiki page: its text (with its links to other pages and to
/// code kept: the page opens them), code, or a diagram.
struct WikiBlock: View {
    @Environment(HarloModel.self) private var model
    let block: WikiPageText.Block
    /// The page's first block (its title is in the bar, so not again here).
    var first = false

    var body: some View {
        let b = block
        switch b.kind {
        case "mermaid" where model.drawsDiagrams:
            DiagramBlock(source: b.code, code: CodeBlock(spans: [Span(b.code, code: true)], language: "mermaid", raw: b.code))
        case "mermaid", "code":
            CodeBlock(spans: codeSpans, language: b.lang, raw: b.code)
        default:
            RichTextView(blocks: RichText.parse(first ? WikiPageText.withoutTitle(b.html) : b.html, keepingAnyLink: true))
        }
    }

    var codeSpans: [Span] {
        if case .code(let spans)? = RichText.parse(block.html).first { return spans }
        return [Span(block.code, code: true)]
    }
}
