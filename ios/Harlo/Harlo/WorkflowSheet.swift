import HarloKit
import SwiftUI

/// A workflow run in full: its stages, the check running or the last one's
/// output, its log, and what can be done to it (pause, resume, stop; once
/// it's done, merge or push its branch; remove its worktree).
struct WorkflowSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    /// The run's id.
    let id: String
    @State private var asking: WorkflowAction?
    @State private var acting = false

    var run: WorkflowRun? { model.workflowRun(id) }

    var body: some View {
        NavigationStack {
            List {
                if let run {
                    Group {
                        Section {
                            stages(run)
                        } header: {
                            Text(run.heading)
                        } footer: {
                            Text(where_(run))
                        }
                        // What you'd do, before the output (long, it hid
                        // them below a small phone's screen).
                        actions(run)
                        if let live = run.liveCheck {
                            Section {
                                output(live.output, empty: "No output yet")
                                    .accessibilityIdentifier("workflowLiveOutput")
                            } header: {
                                Text("\(live.name) running · \(live.seconds) s").textCase(nil)
                            }
                        } else if let t = run.lastCheck {
                            Section {
                                if !t.tail.isEmpty { output(t.tail, empty: "") }
                            } header: {
                                Text(Self.verdict(t)).textCase(nil)
                            }
                        }
                        if !run.log.isEmpty {
                            Section("Log") {
                                ForEach(Array(Array(run.log.enumerated()).reversed()), id: \.offset) { _, e in
                                    logLine(e)
                                }
                            }
                        }
                    }
                    .listRowBackground(Color(p.elevated))
                } else {
                    Text("This run isn't in harlo any more.")
                        .foregroundStyle(Color(p.muted))
                        .listRowBackground(Color(p.elevated))
                }
            }
            .screen()
            .navigationTitle(run.map { $0.title.isEmpty ? $0.workflow : $0.title } ?? "Workflow run")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
            .confirmationDialog(asking.map(Self.question) ?? "", isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }), titleVisibility: .visible, presenting: asking) { a in
                Button(a == .stop ? "Stop the run" : "Remove worktree", role: .destructive) { act(a) }
                Button("Cancel", role: .cancel) {}
            } message: { a in
                if let run { Text(Self.warning(a, run)) }
            }
        }
        .modifier(Themed())
    }

    /// Each stage: its mark, its name and who or what does it.
    func stages(_ run: WorkflowRun) -> some View {
        ForEach(Array(run.stages.enumerated()), id: \.offset) { _, s in
            HStack(spacing: 10) {
                WorkflowStageMark(status: s.status)
                Text(s.label).scaledFont(15, s.status == "active" || s.status == "paused" ? .semibold : .regular)
                Spacer(minLength: 8)
                let command = s.kind == "check"
                Text(command ? s.what.trimmingCharacters(in: CharacterSet(charactersIn: "`")) : s.what)
                    .scaledFont(command ? 12.5 : 14, design: command ? .monospaced : .default)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .accessibilityElement(children: .combine)
            .accessibilityValue(WorkflowStageStrip.spoken(s.status))
        }
    }

    /// Where it runs, and what it's doing.
    func where_(_ run: WorkflowRun) -> String {
        let place = [run.project.isEmpty ? nil : run.project, run.host.isEmpty ? nil : run.host, run.branch.isEmpty ? nil : "on \(run.branch)"]
            .compactMap(\.self).joined(separator: " · ")
        let line = run.statusLine.replacingOccurrences(of: "`", with: "")
        return [line, place].filter { !$0.isEmpty }.joined(separator: "\n")
    }

    /// A check's output, its last lines in code type.
    func output(_ text: String, empty: String) -> some View {
        let shown = text.trimmingCharacters(in: .newlines)
        return Text(shown.isEmpty ? empty : shown)
            .scaledFont(12, design: .monospaced)
            .foregroundStyle(Color(shown.isEmpty ? p.muted : p.foreground))
            .frame(maxWidth: .infinity, alignment: .leading)
            .textSelection(.enabled)
    }

    static func verdict(_ t: WorkflowCheck) -> String {
        if t.passed { return "\(t.name) passed in \(t.seconds) s" }
        guard let exit = t.exit else { return "\(t.name) couldn't finish" }
        return "\(t.name) failed (exit \(exit)) in \(t.seconds) s"
    }

    /// Pause, resume and stop while it's on; once it's done (or paused), its
    /// work: merge, push, remove.
    @ViewBuilder
    func actions(_ run: WorkflowRun) -> some View {
        let over = run.state == "done" || run.state == "stopped"
        if run.running || run.paused || over {
            Section {
                if run.running || run.paused {
                    button(run.paused ? "Resume" : "Pause", icon: run.paused ? "play.fill" : "pause.fill", id: "workflowPause") {
                        act(run.paused ? .resume : .pause)
                    }
                    button("Stop", icon: "stop.fill", id: "workflowStop", role: .destructive) { asking = .stop }
                }
                if run.finishable && !run.merged {
                    button("Merge into \(run.base.isEmpty ? "its branch" : run.base)", icon: "arrow.triangle.merge", id: "workflowMerge") { act(.merge) }
                    if run.published {
                        Label("Pushed \(run.branch): open its pull request in harlo's Changes panel", systemImage: "arrow.up.circle")
                            .scaledFont(14)
                            .foregroundStyle(Color(p.muted))
                    } else {
                        button("Push for a pull request", icon: "arrow.up.circle", id: "workflowPublish") { act(.publish) }
                    }
                }
                if (over || run.paused) && !run.removed {
                    button("Remove worktree", icon: "trash", id: "workflowRemove", role: .destructive) { asking = .remove }
                }
                if run.merged || run.removed {
                    Text(run.merged && run.removed ? "Merged, worktree removed" : run.merged ? "Merged into \(run.base)" : "Worktree removed")
                        .scaledFont(14)
                        .foregroundStyle(Color(p.muted))
                        .accessibilityIdentifier("workflowFinished")
                }
            }
        }
    }

    func button(_ title: String, icon: String, id: String, role: ButtonRole? = nil, _ action: @escaping () -> Void) -> some View {
        Button(role: role, action: action) {
            Label(title, systemImage: icon)
        }
        .disabled(acting || model.state != .connected)
        .accessibilityIdentifier(id)
    }

    func logLine(_ e: WorkflowLogEntry) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle()
                .fill(Color(e.tone == "bad" ? p.error : e.tone == "good" ? p.success : e.tone == "warn" ? p.warning : p.muted))
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)
            Text(e.text)
                .scaledFont(14)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(Date(timeIntervalSince1970: TimeInterval(e.at)), style: .time)
                .scaledFont(12)
                .foregroundStyle(Color(p.muted))
        }
    }

    func act(_ a: WorkflowAction) {
        guard let run else { return }
        acting = true
        Task {
            await model.workflowAction(run, a)
            acting = false
        }
    }

    static func question(_ a: WorkflowAction) -> String {
        a == .stop ? "Stop the run?" : "Remove the run's worktree?"
    }

    static func warning(_ a: WorkflowAction, _ run: WorkflowRun) -> String {
        if a == .stop { return "It goes no further: harlo sends its sessions nothing more. Its worktree, branch and sessions stay." }
        let over = run.state == "done" || run.state == "stopped"
        return (over ? "" : "The run stops. ") + "Its folder goes, with any changes not committed, and its sessions close. Branch \(run.branch) stays\(run.merged ? " unless it's merged" : "")."
    }
}
