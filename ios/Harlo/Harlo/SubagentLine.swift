import HarloKit
import SwiftUI

/// The subagents at work, as a line in a card above the composer, drawn
/// like the plan's: how many, and what the latest one is doing; tap for
/// the whole list.
struct SubagentLine: View {
    @Environment(\.palette) private var p
    let subagents: [Subagent]
    var open: () -> Void

    var body: some View {
        let running = subagents.filter { $0.status == "running" }
        let now = running.last { !$0.description.isEmpty }?.description
        Button(action: open) {
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(Color(p.primary))
                    .frame(width: 14)
                Text(running.count == 1 ? "1 subagent" : "\(running.count) subagents")
                    .scaledFont(13, .semibold)
                    .foregroundStyle(Color(p.muted))
                    .monospacedDigit()
                Text(now ?? "working")
                    .scaledFont(13)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up").scaledFont(10, .semibold).foregroundStyle(Color(p.muted))
            }
            // (A full-size target: it reaches 13 pt above and below the
            // line without making the card any taller.)
            .padding(.vertical, 13)
            .contentShape(Rectangle())
            .padding(.vertical, -13)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(running.count == 1 ? "1 subagent" : "\(running.count) subagents") working\(now.map { ", now: \($0)" } ?? "")")
        .accessibilityHint("Shows the todos and subagents")
    }
}
