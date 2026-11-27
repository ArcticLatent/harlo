import HarloKit
import SwiftUI
import UIKit

/// What the session is doing besides talking: what it has in the
/// background and its goal, todo progress, messages waiting. Tapping the
/// todos opens the list.
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

    var body: some View {
        let queued = session.queued
        let card = todo != nil || !queued.isEmpty || !session.jobs.isEmpty
        if card {
            VStack(alignment: .leading, spacing: 6) {
                JobRows(rows: session.jobs)
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
            .card(p, radius: Corner.small)
            .padding(.horizontal, 12)
            .padding(.top, 8)
        }
    }
}
