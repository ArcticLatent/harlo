import HarloKit
import SwiftUI

/// The session's todo list (by phase), the subagents it started, and its
/// helpers (sessions of their own in harlo).
struct ActivityView: View {
    @Environment(HarloModel.self) private var model
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
                                    .scaledFont(15)
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
                                        Text(s.description).scaledFont(14).foregroundStyle(Color(p.muted)).lineLimit(3)
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
                if !session.helpers.isEmpty {
                    Section {
                        ForEach(session.helpers) { h in
                            VStack(alignment: .leading, spacing: 6) {
                                Button {
                                    dismiss()
                                    model.openHelper(h.id)
                                } label: {
                                    HStack(alignment: .top, spacing: 10) {
                                        StatusDot(status: h.needsSomeone ? "input" : h.status == "running" ? "busy" : h.status == "completed" ? "idle" : "exited")
                                            .padding(.top, 6)
                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(h.name.isEmpty ? Style.harnessLabel(h.agent) : h.name).scaledFont(15, .medium)
                                            Text(h.summary.isEmpty ? Style.harnessLabel(h.agent) : h.summary)
                                                .scaledFont(13.5)
                                                .foregroundStyle(Color(p.muted))
                                                .lineLimit(3)
                                            // Its agent and model, and what it used.
                                            Text(([Style.harnessLabel(h.agent), h.modelLabel, h.used?.label ?? ""].filter { !$0.isEmpty }).joined(separator: " · "))
                                                .font(Style.mono(11))
                                                .foregroundStyle(Color(p.muted))
                                                .lineLimit(1)
                                            if let ask = h.asks {
                                                Text(ask.text)
                                                    .scaledFont(13.5)
                                                    .foregroundStyle(Color(p.warning))
                                                    .lineLimit(4)
                                            }
                                        }
                                        Spacer(minLength: 0)
                                        Text(helperState(h.status))
                                            .scaledFont(12)
                                            .foregroundStyle(Color(h.needsSomeone ? p.warning : h.status == "failed" ? p.error : p.muted))
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .disabled(!model.sessions.contains { $0.key == h.id })
                                // A tool's approval, answered right here.
                                if let ask = h.asks, ask.approval, model.answersHelpers {
                                    HStack(spacing: 8) {
                                        ForEach(ask.options.isEmpty ? ["Approve", "Deny"] : ask.options, id: \.self) { o in
                                            Button(o) {
                                                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                                Task { await model.answerHelper(h, value: o) }
                                            }
                                            .buttonStyle(.bordered)
                                            .tint(o == "Deny" ? Color(p.error) : Color(p.primary))
                                            .controlSize(.small)
                                        }
                                    }
                                    .padding(.leading, 20)
                                }
                            }
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        HStack {
                            Text("Helpers")
                            Spacer()
                            // All of them together.
                            Text(Helper.Used.total(session.helpers.map(\.used)).label)
                        }
                        .font(Style.mono(11.5, .semibold)).foregroundStyle(Color(p.muted)).textCase(nil)
                    } footer: {
                        Text("Sessions this one started in harlo. Tap one to open it.")
                    }
                }
            }
            .screen()
            .navigationTitle(session.helpers.isEmpty ? "Todos and subagents" : "Todos and helpers")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
    }

    func helperState(_ status: String) -> String {
        switch status {
        case "asking": return "Asks its parent"
        case "waiting": return "Needs you"
        case "completed": return "Done"
        case "failed": return "Failed"
        case "ended": return "Ended"
        default: return "Working"
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
