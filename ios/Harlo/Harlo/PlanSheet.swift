import HarloKit
import SwiftUI

/// The session's shared plan (every step, to change, skip, reorder or
/// remove, a step to add, each finished step's commit for another agent to
/// review, and a run with helpers) and its repository's wiki, to read.
struct PlanSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    /// A step being changed, or one being added.
    @State private var editing: StepDraft?
    @State private var notice: String?

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
            .alert(notice ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
                Button("OK", role: .cancel) {}
            }
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

    func row(_ step: PlanStep, index: Int, count: Int, plan: Plan) -> some View {
        PlanStepRow(step: step, reviewed: plan.run?.reviewing[step.id] != nil)
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
