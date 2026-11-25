import HarloKit
import SwiftUI
import UIKit

struct RowView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        // (Your pictures are in your bubble, a tool's in its card, a reply's
        // in their place; the rest, files it names, under it.)
        let rest = row.kind == "user" || row.kind == "tool" || !model.showsPictures ? [] : under
        if !rest.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                content
                PictureStrip(paths: rest)
            }
        } else {
            content
        }
    }

    /// The row's pictures its reply doesn't show in their place (harlo
    /// sends a reply's pictures as a row of their own after its text, with
    /// the same message key: `m44:0:pics` after `m44:0:0`).
    var under: [String] {
        guard !row.images.isEmpty else { return [] }
        let message = row.key.split(separator: ":").dropLast().joined(separator: ":")
        let reply = row.kind == "images" && !message.isEmpty
            ? (model.session?.rows ?? []).filter { $0.key.hasPrefix(message + ":") }
            : [row]
        let placed = reply.flatMap { RichText.pictures(RichCache.blocks($0.html)) }.compactMap { PictureNames.match($0, in: row.images) }
        return row.images.filter { !placed.contains($0) }
    }

    @ViewBuilder var content: some View {
        switch row.kind {
        case "user":
            UserBubble(row: row)
        case "markdown":
            RichTextView(blocks: RichCache.blocks(row.html))
        // A long code block's later pieces: its first shows it.
        case "codePart":
            EmptyView()
        case "code":
            CodeBlock(spans: codeSpans, language: row.title, raw: row.text)
        case "thinking":
            ThinkingRow(row: row)
        case "tool":
            ToolCard(row: row)
        case "notice" where row.fromHarlo:
            HarloMessage(row: row)
        case "images":
            // (Its pictures only: under it, the ones its reply doesn't show.)
            EmptyView()
        default:
            Text(row.text)
                .font(.footnote)
                .foregroundStyle(Color(row.title == "error" ? p.error : row.title == "warning" ? p.warning : p.muted))
                .textSelection(.enabled)
        }
    }

    var codeSpans: [Span] {
        if case .code(let spans)? = RichCache.blocks(row.html).first { return spans }
        return [Span(row.text, code: true)]
    }
}
