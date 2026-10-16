import HarloKit
import SwiftUI

/// The session's todo list (by phase) and the subagents it started.
struct ActivityView: View {
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession

    var body: some View {
        NavigationStack {
            List {
                ForEach(session.todos) { phase in
                    Section {
                        ForEach(phase.tasks) { t in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Image(systemName: icon(t.status))
                                    .foregroundStyle(Color(color(t.status)))
                                Text(t.content)
                                    .font(.system(size: 15))
                                    .strikethrough(t.status == "completed", color: Color(p.muted))
                                    .foregroundStyle(Color(t.status == "completed" ? p.muted : p.foreground))
                            }
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        HStack {
                            Text(phase.name)
                            Spacer()
                            Text("\(phase.done)/\(phase.tasks.count)")
                        }
                        .font(Style.mono(11.5, .semibold))
                        .foregroundStyle(Color(p.muted))
                        .textCase(nil)
                    }
                }
                if !session.subagents.isEmpty {
                    Section {
                        ForEach(session.subagents) { s in
                            HStack(alignment: .top, spacing: 10) {
                                StatusDot(status: s.status == "running" ? "busy" : s.status == "completed" ? "idle" : "exited")
                                    .padding(.top, 6)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(s.agent.isEmpty ? s.id : s.agent).font(Style.mono(13, .semibold))
                                    if !s.description.isEmpty {
                                        Text(s.description).font(.system(size: 14)).foregroundStyle(Color(p.muted)).lineLimit(3)
                                    }
                                }
                                Spacer(minLength: 0)
                                Text(s.status).font(Style.mono(11)).foregroundStyle(Color(p.muted))
                            }
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        Text("Subagents").font(Style.mono(11.5, .semibold)).foregroundStyle(Color(p.muted)).textCase(nil)
                    }
                }
            }
            .screen()
            .navigationTitle("Todos and subagents")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
    }

    func icon(_ status: String) -> String {
        switch status {
        case "completed": return "checkmark.circle.fill"
        case "in_progress": return "circle.dotted.circle"
        case "blocked": return "exclamationmark.circle"
        default: return "circle"
        }
    }

    func color(_ status: String) -> RGBA {
        switch status {
        case "completed": return p.success
        case "in_progress": return p.primary
        case "blocked": return p.warning
        default: return p.muted
        }
    }
}
