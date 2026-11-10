import HarloKit
import SwiftUI

/// The first line of the activity card above the composer, while the
/// session has a shared plan: how far along it is and the step in hand;
/// tap for the whole plan.
struct PlanLine: View {
    @Environment(\.palette) private var p
    let plan: Plan
    var open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Image(systemName: "list.bullet.clipboard")
                    .scaledFont(13)
                    .foregroundStyle(Color(p.primary))
                    .frame(width: 14)
                Text("\(plan.finished)/\(plan.steps.count)")
                    .scaledFont(13, .semibold)
                    .foregroundStyle(Color(p.muted))
                    .monospacedDigit()
                Text(plan.current?.title ?? (plan.finished == plan.steps.count ? "All done" : "Plan"))
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
        .accessibilityLabel("Plan, \(plan.finished) of \(plan.steps.count) done\(plan.current.map { ", now: \($0.title)" } ?? "")")
        .accessibilityHint("Shows the plan")
    }
}

/// The session's shared plan (every step, to change, skip, reorder or
/// remove, a step to add, each finished step's commit for another agent to
/// review, and a run with helpers) and its repository's specs, to read.
struct PlanSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    /// A step being changed, or one being added.
    @State private var editing: StepDraft?
    @State private var reading: Spec?
    @State private var notice: String?
    @State private var specsGraph = false

    struct StepDraft: Identifiable {
        /// nil: a new step (after `after`, else at the end).
        var step: Int64?
        var after: Int64?
        var title = ""
        var detail = ""
        var id: String { "\(step ?? -1)|\(after ?? -1)" }
    }

    var plan: Plan? { session.plan }

    var body: some View {
        NavigationStack {
            List {
                Group {
                    if let plan, plan.shows {
                        Section {
                            ForEach(Array(plan.steps.enumerated()), id: \.element.id) { i, step in
                                row(step, index: i, count: plan.steps.count, plan: plan)
                            }
                        } header: {
                            Text("\(plan.finished) of \(plan.steps.count) done")
                        } footer: {
                            Text(plan.run != nil ? "harlo gives each step to a helper session. Long-press a step to change it." : plan.commits ? "The agent works through the steps and commits each one. Long-press a step to change it." : "The agent works through the steps. Long-press a step to change it.")
                        }
                        if model.runsPlans && (plan.canRun || plan.run != nil) {
                            runSection(plan)
                        }
                    } else {
                        Section {
                            Text(plan?.enabled == true || plan == nil
                                 ? "No plan yet. With the plan on, the agent writes its steps here as it starts."
                                 : "The plan is off for this session's project (harlo's Settings → Projects turns it on).")
                                .foregroundStyle(Color(p.muted))
                        } header: {
                            Text("Plan")
                        }
                    }
                    specs
                }
                .listRowBackground(Color(p.elevated))
            }
            .screen()
            .navigationTitle("Plan")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                if plan?.enabled == true {
                    ToolbarItem(placement: .primaryAction) {
                        Button { editing = StepDraft() } label: { Image(systemName: "plus") }
                            .accessibilityLabel("Add a step")
                    }
                }
            }
            .sheet(item: $editing) { d in StepEditor(draft: d) { done in save(done) } }
            .sheet(item: $reading) { s in FileView(path: s.path, line: 0) }
            .alert(notice ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
                Button("OK", role: .cancel) {}
            }
            .task { await model.loadSpecs() }
        }
        .modifier(Themed())
    }

    /// Running it with helpers: a helper session per step, stage by stage.
    func runSection(_ plan: Plan) -> some View {
        Section {
            if let run = plan.run, !run.paused.isEmpty {
                Label(run.paused, systemImage: "pause.circle")
                    .scaledFont(14)
                    .foregroundStyle(Color(p.warning))
            } else if let run = plan.run {
                Label("Running with \(run.agentName) helpers", systemImage: "person.2")
                    .scaledFont(14)
                    .foregroundStyle(Color(p.primary))
            }
            if plan.canRun && !plan.reviewers.isEmpty {
                Menu {
                    // The project's Team (its agent and model, and who
                    // reviews), or another agent for this run.
                    if let team = plan.team {
                        Section(team.custom ? "The project's team" : "This session's agent") {
                            Button {
                                UISelectionFeedbackGenerator().selectionChanged()
                                Task { await model.runPlan(agent: "") }
                            } label: {
                                Label(team.label, systemImage: "person.2")
                            }
                        }
                    }
                    Section(plan.team == nil ? "" : "Or this run's helpers") {
                        ForEach(plan.helperAgents(for: session.harness)) { a in
                            Button(a.id == session.harness ? "\(a.label) (this session's)" : a.label) {
                                UISelectionFeedbackGenerator().selectionChanged()
                                Task { await model.runPlan(agent: a.id) }
                            }
                        }
                    }
                } label: {
                    Label(plan.run == nil ? "Run with helpers" : "Run again", systemImage: "play.circle")
                }
            }
            if plan.run != nil {
                Button(role: .destructive) {
                    Task { await model.stopPlanRun() }
                } label: {
                    Label("Stop the run", systemImage: "stop.circle")
                }
            }
        } header: {
            Text("Helpers")
        } footer: {
            Text(plan.run == nil
                 ? "harlo gives each step to a helper session, in order. Steps marked to run with the one before run at the same time, each in a worktree."
                 : "No more steps start once you stop; helpers at work finish theirs.")
        }
    }

    @ViewBuilder
    var specs: some View {
        if let s = session.specs, !s.specs.isEmpty {
            Section {
                if !s.mermaid.isEmpty && s.specs.count > 1 {
                    Picker("Show", selection: $specsGraph) {
                        Text("Tree").tag(false)
                        Text("Graph").tag(true)
                    }
                    .pickerStyle(.segmented)
                    .accessibilityIdentifier("specsTreeGraph")
                }
                if specsGraph && !s.mermaid.isEmpty {
                    SpecsGraph(source: s.mermaid)
                } else {
                    ForEach(s.specs) { spec in specRow(spec) }
                }
                if !s.problems.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(s.problems.count == 1 ? "1 problem in the specs' links" : "\(s.problems.count) problems in the specs' links")
                            .font(.subheadline.weight(.medium)).foregroundStyle(Color(p.warning))
                        ForEach(s.problems.prefix(4), id: \.self) { problem in
                            // (Its `names` in the code font; it opens the spec it's in.)
                            Button { reading = s.spec(of: problem) } label: {
                                Text(.init(problem))
                                    .font(.footnote)
                                    .foregroundStyle(Color(p.muted))
                                    .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .disabled(s.spec(of: problem) == nil)
                        }
                        if !session.liveKey.isEmpty {
                            Button("Ask the agent to fix them") {
                                let ask = "Fix the problems in this project's specs' links (spec_validate lists them): " + s.problems.joined(separator: "; ") + ". Point each link at the spec it means, or take it out if there's none."
                                Task { if await model.send(ask) { notice = "Sent to the agent." } }
                            }
                            .font(.subheadline.weight(.medium))
                            .frame(minHeight: 44)
                        }
                    }
                }
            } header: {
                Text("Specs")
            } footer: {
                Text(specsGraph
                     ? "A line joins a spec to the ones under it; a dashed arrow is a link (depends on, references, implements). Tap it to zoom; the tree opens each one."
                     : s.enabled ? "The agent looks up the spec that governs its work and keeps it true." : "The repository's specs. harlo's Settings → Projects has the agent read them.")
            }
        }
    }

    func specRow(_ spec: Spec) -> some View {
        Button { reading = spec } label: {
            HStack(spacing: 12) {
                Image(systemName: "doc.text").foregroundStyle(Color(p.primary))
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(spec.title.isEmpty ? spec.path : spec.title).font(.body.weight(.medium))
                        // Its status, when it isn't simply in force; its type.
                        if !spec.status.isEmpty && spec.status != "active" {
                            Text(spec.status).font(.caption).foregroundStyle(Color(spec.status == "stale" ? p.warning : p.muted))
                                .strikethrough(spec.status == "deprecated")
                        }
                        if !spec.kind.isEmpty {
                            Text(spec.kind).font(.caption).foregroundStyle(Color(p.muted))
                        }
                    }
                    if !spec.summary.isEmpty {
                        Text(spec.summary).font(.footnote).foregroundStyle(Color(p.muted)).lineLimit(2)
                    }
                    Text(spec.path).font(Style.mono(11)).foregroundStyle(Color(p.muted)).lineLimit(1).truncationMode(.head)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Color(p.muted))
            }
            // (Under its parent.)
            .padding(.leading, CGFloat(min(spec.depth, 4)) * 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    func mark(_ status: String) -> (String, RGBA) {
        switch status {
        case "done": return ("checkmark.circle.fill", p.success)
        case "active": return ("arrow.right.circle.fill", p.primary)
        case "skipped": return ("xmark.circle", p.muted)
        default: return ("circle", p.muted)
        }
    }

    func row(_ step: PlanStep, index: Int, count: Int, plan: Plan) -> some View {
        let (icon, color) = mark(step.status)
        return HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .scaledFont(16)
                .foregroundStyle(Color(color))
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(step.title)
                        .scaledFont(15.5, step.status == "active" ? .semibold : .regular)
                        .foregroundStyle(Color(step.status == "skipped" ? p.muted : p.foreground))
                        .strikethrough(step.status == "skipped")
                    if step.editedByUser {
                        Text("yours").scaledFont(11.5).foregroundStyle(Color(p.muted))
                    }
                }
                let note = step.summary.isEmpty ? step.detail : step.summary
                if !note.isEmpty {
                    Text(note).scaledFont(13.5).foregroundStyle(Color(p.muted))
                }
                if step.alongside {
                    Label("With the step before", systemImage: "rectangle.split.2x1")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.info))
                }
                if plan.run?.reviewing[step.id] != nil {
                    Label("Done, being reviewed", systemImage: "checkmark.seal")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.info))
                        .lineLimit(1)
                } else if !step.helper.isEmpty {
                    Label((step.status == "active" ? "A helper is on it" : "Done by a helper") + (step.branch.isEmpty ? "" : " · \(step.branch)"), systemImage: "person.2")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.muted))
                        .lineLimit(1)
                }
                // Its own helper (chosen on the desktop's plan card).
                if !step.agent.isEmpty, step.open {
                    Label("Done by \(Style.harnessLabel(step.agent))" + (step.model.isEmpty ? "" : " · \(step.model.split(separator: "/").last ?? "")"), systemImage: "person.crop.circle")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.muted))
                        .lineLimit(1)
                }
                if !step.commit.isEmpty {
                    Label("Committed \(step.commit.prefix(8))", systemImage: "point.3.filled.connected.trianglepath.dotted")
                        .font(Style.mono(11.5))
                        .foregroundStyle(Color(p.success))
                } else if !step.commitNote.isEmpty {
                    Text(step.commitNote)
                        .scaledFont(12)
                        .foregroundStyle(Color(step.commitNote.hasPrefix("Not committed") ? p.warning : p.muted))
                }
            }
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .combine)
        .accessibilityValue(step.status == "active" ? "In hand" : step.status == "done" ? "Done" : step.status == "skipped" ? "Skipped" : "To do")
        .swipeActions(edge: .trailing) {
            if step.status == "pending" {
                Button(role: .destructive) { edit(.remove(id: step.id)) } label: { Label("Remove", systemImage: "trash") }
            }
            if step.status != "done" {
                Button { edit(.skip(id: step.id, skipped: step.status != "skipped")) } label: {
                    Label(step.status == "skipped" ? "Don't skip" : "Skip", systemImage: step.status == "skipped" ? "arrow.uturn.backward" : "forward")
                }
                .tint(Color(p.muted))
            }
        }
        .contextMenu {
            if step.open {
                Button { editing = StepDraft(step: step.id, title: step.title, detail: step.detail) } label: { Label("Change it", systemImage: "pencil") }
                Button { editing = StepDraft(after: step.id) } label: { Label("Add a step after it", systemImage: "plus") }
            }
            if let reviewer = plan.run?.reviewing[step.id], model.sessions.contains(where: { $0.key == reviewer }) {
                Button { dismiss(); model.openHelper(reviewer) } label: { Label("Open its review", systemImage: "checkmark.seal") }
            } else if !step.helper.isEmpty, step.status == "active", model.sessions.contains(where: { $0.key == step.helper }) {
                Button { dismiss(); model.openHelper(step.helper) } label: { Label("Open its helper", systemImage: "arrow.up.forward.app") }
            }
            if index > 0 && step.status == "pending" && model.runsPlans {
                Button { edit(.alongside(id: step.id, alongside: !step.alongside)) } label: {
                    Label(step.alongside ? "Run it after the step before" : "Run it with the step before", systemImage: "rectangle.split.2x1")
                }
            }
            if index > 0 {
                Button { edit(.shift(id: step.id, by: -1)) } label: { Label("Move up", systemImage: "arrow.up") }
            }
            if index < count - 1 {
                Button { edit(.shift(id: step.id, by: 1)) } label: { Label("Move down", systemImage: "arrow.down") }
            }
            if step.status != "done" {
                Button { edit(.skip(id: step.id, skipped: step.status != "skipped")) } label: {
                    Label(step.status == "skipped" ? "Don't skip it" : "Skip it", systemImage: step.status == "skipped" ? "arrow.uturn.backward" : "forward")
                }
            }
            if !step.commit.isEmpty && !plan.reviewers.isEmpty {
                Section("Review its commit") {
                    ForEach(plan.reviewers(for: session.harness)) { a in
                        Button { review(step, a) } label: { Label("Review with \(a.label)", systemImage: "checkmark.bubble") }
                    }
                }
            }
            if step.status == "pending" {
                Button(role: .destructive) { edit(.remove(id: step.id)) } label: { Label("Remove it", systemImage: "trash") }
            }
        }
    }

    func edit(_ e: PlanEdit) {
        UISelectionFeedbackGenerator().selectionChanged()
        Task { await model.editPlan(e) }
    }

    func save(_ d: StepDraft) {
        let title = d.title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        let detail = d.detail.trimmingCharacters(in: .whitespacesAndNewlines)
        edit(d.step.map { .edit(id: $0, title: title, detail: detail) } ?? .add(title: title, detail: detail, after: d.after))
    }

    func review(_ step: PlanStep, _ agent: Agent) {
        Task {
            if await model.reviewStep(step, with: agent.id) {
                notice = "\(agent.label) is reviewing the commit. Its review comes back to this session."
            }
        }
    }
}

/// Changing a plan's step, or a new one: its outcome, and how.
struct StepEditor: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var p
    @State var draft: PlanSheet.StepDraft
    var done: (PlanSheet.StepDraft) -> Void
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("The step's outcome", text: $draft.title, axis: .vertical)
                        .focused($focused)
                    TextField("How, or what to watch for (optional)", text: $draft.detail, axis: .vertical)
                        .lineLimit(2...6)
                }
                .listRowBackground(Color(p.elevated))
            }
            .screen()
            .navigationTitle(draft.step == nil ? "New step" : "Change the step")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(draft.step == nil ? "Add" : "Save") {
                        done(draft)
                        dismiss()
                    }
                    .disabled(draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium])
        .modifier(Themed())
    }
}
