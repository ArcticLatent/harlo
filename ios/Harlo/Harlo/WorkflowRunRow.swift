import HarloKit
import SwiftUI

/// A workflow run in a list: its task, project, stages and state.
struct WorkflowRunRow: View {
    @Environment(\.palette) private var p
    let run: WorkflowRun

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(run.title.isEmpty ? run.workflow : run.title)
                    .scaledFont(15, .semibold)
                    .lineLimit(2)
                Spacer(minLength: 4)
                WorkflowStageStrip(stages: run.stages)
            }
            Text([run.project.isEmpty ? nil : (run.host.isEmpty ? run.project : "\(run.project) · \(run.host)"), run.heading]
                .compactMap(\.self).joined(separator: " · "))
                .scaledFont(13)
                .foregroundStyle(Color(p.muted))
                .lineLimit(1)
            let line = run.statusLine.replacingOccurrences(of: "`", with: "")
            if !line.isEmpty {
                Text(line)
                    .scaledFont(13)
                    .foregroundStyle(Color(run.needsYou ? p.warning : p.foreground))
                    .lineLimit(2)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(p, radius: 16)
        .contentShape(RoundedRectangle(cornerRadius: 16))
    }
}
