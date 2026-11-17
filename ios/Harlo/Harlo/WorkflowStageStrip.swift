import HarloKit
import SwiftUI

/// A run's stages on one line: each one's mark, joined.
struct WorkflowStageStrip: View {
    @Environment(\.palette) private var p
    let stages: [WorkflowStage]
    var size: CGFloat = 12

    var body: some View {
        HStack(spacing: 3) {
            ForEach(Array(stages.enumerated()), id: \.offset) { i, s in
                if i > 0 {
                    Rectangle()
                        .fill(Color(s.status == "todo" ? p.hairline : p.success))
                        .frame(width: size * 0.6, height: 1.5)
                }
                WorkflowStageMark(status: s.status, size: size)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(stages.map { "\($0.label) \(Self.spoken($0.status))" }.joined(separator: ", "))
    }

    static func spoken(_ status: String) -> String {
        switch status {
        case "done": "done"
        case "active": "under way"
        case "paused": "paused"
        default: "to come"
        }
    }
}
