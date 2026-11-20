import HarloKit
import SwiftUI

/// A pi extension's view: its Markdown drawn as a reply's, its pictures,
/// and the rest of its lines in the code font; just its lines when it
/// sends no blocks.
struct ExtBlocks: View {
    let lines: [String]
    let blocks: [ExtBlock]

    var body: some View {
        if blocks.isEmpty {
            ExtLines(lines: lines)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                    ExtBlockView(block: block)
                }
            }
        }
    }
}
