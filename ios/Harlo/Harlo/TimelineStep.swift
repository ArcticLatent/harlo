import HarloKit
import SwiftUI
import UIKit

/// A step of a run: its mark on the timeline (under the agent's tile), and
/// the step itself (a tool call's card, a thought).
struct TimelineStep: View {
    @Environment(\.palette) private var p
    let row: Row
    var first = false
    var last = false
    /// Where a thought's mark sits: on its line, which grows with the text
    /// size (a tool card's mark sits on its card's first line, at 15).
    @ScaledMetric(relativeTo: .body) private var thoughtTop: CGFloat = 7

    var body: some View {
        Group {
            if row.kind == "tool" {
                ToolCard(row: row)
            } else {
                RowView(row: row)
                    .padding(.vertical, 8)
            }
        }
        .padding(.leading, Transcript.step)
        .padding(.bottom, last ? 0 : 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .topLeading) {
            VStack(spacing: 0) {
                line.frame(height: row.kind == "tool" ? 15 : thoughtTop).opacity(first ? 0 : 1)
                StepMark(row: row)
                line.frame(maxHeight: .infinity).opacity(last ? 0 : 1)
            }
            .frame(width: Transcript.rail)
        }
    }

    var line: some View {
        Rectangle().fill(Color(p.success).opacity(0.35)).frame(width: 2)
    }
}
