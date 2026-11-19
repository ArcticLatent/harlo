import HarloKit
import SwiftUI
import UIKit

/// What the session is doing besides talking: todo progress, subagents at
/// work (in a card of their own), messages waiting. Tapping opens the
/// details.
struct ActivityBar: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    var open: (SessionSheet) -> Void

    var todo: (done: Int, total: Int, now: String)? {
        let tasks = session.todos.flatMap(\.tasks)
        guard !tasks.isEmpty else { return nil }
        let now = tasks.first { $0.status == "in_progress" }?.content ?? tasks.first { $0.status != "completed" }?.content ?? "All done"
        return (tasks.count(where: { $0.status == "completed" }), tasks.count, now)
    }

    var running: Int { session.subagents.count(where: { $0.status == "running" }) }

    var body: some View {
        let queued = session.queued
        let plan = session.plan.flatMap { $0.shows ? $0 : nil }
        // (A run shows until it's stopped or its worktree is gone.)
        let run = session.workflow.flatMap { $0.state != "stopped" && !$0.removed ? $0 : nil }
        let card = run != nil || plan != nil || todo != nil || !queued.isEmpty
        if card || running > 0 {
            VStack(spacing: 6) {
                if card {
                VStack(alignment: .leading, spacing: 6) {
                    if let run {
                        WorkflowLine(run: run) { open(.workflow(run.id)) }
                    }
                    if let plan {
                        PlanLine(plan: plan) { open(.plan) }
                    }
                    if let t = todo {
                        Button { open(.activity) } label: {
                            HStack(spacing: 8) {
                                ProgressRing(fraction: Double(t.done) / Double(max(t.total, 1)))
                                Text("\(t.done)/\(t.total)")
                                    .scaledFont(13, .semibold)
                                    .foregroundStyle(Color(p.muted))
                                Text(t.now)
                                    .scaledFont(13)
                                    .lineLimit(1)
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.up").scaledFont(10, .semibold).foregroundStyle(Color(p.muted))
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    ForEach(queued) { m in
                        HStack(spacing: 6) {
                            Text(m.isSteering ? "Steer" : "Next")
                                .scaledFont(12, .semibold)
                                .foregroundStyle(Color(p.primary))
                            Text(m.text)
                                .scaledFont(13)
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(1)
                            Spacer(minLength: 4)
                            if session.canUnqueue {
                                // A follow-up can go now, as steering.
                                if !m.isSteering {
                                    Button {
                                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                        Task { await model.sendNow(m) }
                                    } label: {
                                        Text("Send now").scaledFont(12.5, .medium).foregroundStyle(Color(p.primary))
                                    }
                                    .buttonStyle(.plain)
                                }
                                Button {
                                    Task { await model.unqueue(m) }
                                } label: {
                                    Image(systemName: "xmark.circle.fill")
                                        .font(.system(size: 16))
                                        .foregroundStyle(Color(p.muted))
                                        .frame(width: 30, height: 26)
                                        .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .accessibilityLabel("Take back")
                            }
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .card(p, radius: 10)
                }
                // (A card of its own, like the plan's: the subagents at work.)
                if running > 0 {
                    SubagentLine(subagents: session.subagents) { open(.activity) }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .card(p, radius: 10)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 8)
        }
    }
}
