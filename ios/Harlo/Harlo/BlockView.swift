import HarloKit
import SwiftUI
import UIKit

struct BlockView: View {
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let block: RichBlock

    var body: some View {
        switch block {
        case .paragraph(let spans):
            Text(Style.attributed(spans, p, typeSize: typeSize))
                .lineSpacing(3)
                .textSelection(.enabled)
        case .heading(let level, let spans):
            Text(Style.attributed(spans, p, size: level <= 1 ? 21 : level == 2 ? 19 : 17, typeSize: typeSize))
                .bold()
                .padding(.top, 4)
        case .list(let ordered, let start, let items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(items.indices, id: \.self) { i in
                    HStack(alignment: .firstTextBaseline, spacing: 7) {
                        Text(ordered ? "\(start + i)." : "•")
                            .scaledFont(16).monospacedDigit()
                            .foregroundStyle(Color(p.muted))
                        // (AnyView: lists hold blocks, which hold lists.)
                        AnyView(RichTextView(blocks: items[i]))
                    }
                }
            }
        case .quote(let inner):
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 1.5).fill(Color(p.border)).frame(width: 3)
                AnyView(RichTextView(blocks: inner))
                    .foregroundStyle(Color(p.muted))
            }
        case .code(let spans):
            CodeBlock(spans: spans, language: "")
        case .rule:
            Rectangle().fill(Color(p.border)).frame(height: 1)
        case .picture(let src, let alt):
            ReplyPicture(src: src, alt: alt)
        case .table(let head, let rows):
            ScrollView(.horizontal) {
                Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
                    if !head.isEmpty {
                        GridRow {
                            ForEach(head.indices, id: \.self) { c in
                                Text(Style.attributed(head[c], p, size: 15, typeSize: typeSize)).bold()
                            }
                        }
                        Rectangle().fill(Color(p.border)).frame(height: 1)
                    }
                    ForEach(rows.indices, id: \.self) { r in
                        GridRow {
                            ForEach(rows[r].indices, id: \.self) { c in
                                Text(Style.attributed(rows[r][c], p, size: 15, typeSize: typeSize))
                            }
                        }
                    }
                }
                .padding(.vertical, 4)
            }
            .scrollIndicators(.hidden)
        }
    }
}
