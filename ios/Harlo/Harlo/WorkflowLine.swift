import HarloKit
import SwiftUI

/// A line of the activity card above the composer, while the session is
/// part of a workflow run: its stages and what it's doing; tap for the run.
struct WorkflowLine: View {
    @Environment(\.palette) private var p
    let run: WorkflowRun
    var open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                WorkflowStageStrip(stages: run.stages)
                Text(run.liveCheck.map { "\($0.name) running · \($0.seconds) s" } ?? run.statusLine.replacingOccurrences(of: "`", with: ""))
                    .scaledFont(13)
                    .foregroundStyle(Color(run.needsYou ? p.warning : p.foreground))
                    .lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up").scaledFont(10, .semibold).foregroundStyle(Color(p.muted))
            }
            .padding(.vertical, 13)
            .contentShape(Rectangle())
            .padding(.vertical, -13)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(run.heading), \(run.statusLine.replacingOccurrences(of: "`", with: ""))")
        .accessibilityHint("Shows the workflow run")
        .accessibilityIdentifier("workflowLine")
    }
}
