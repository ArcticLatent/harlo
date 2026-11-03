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
                    .font(.system(size: 13))
                    .foregroundStyle(Color(p.primary))
                    .frame(width: 14)
                Text("\(plan.finished)/\(plan.steps.count)")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Color(p.muted))
                    .monospacedDigit()
                Text(plan.current?.title ?? (plan.finished == plan.steps.count ? "All done" : "Plan"))
                    .font(.system(size: 13))
                    .lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color(p.muted))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Plan, \(plan.finished) of \(plan.steps.count) done\(plan.current.map { ", now: \($0.title)" } ?? "")")
        .accessibilityHint("Shows the plan")
    }
}

/// The session's shared plan (every step, to change, skip, reorder or
/// remove, a step to add, each finished step's commit for another agent to
/// review) and its repository's specs, to read.
struct PlanSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    /// A step being changed, or one being added.
    @State private var editing: StepDraft?
    @State private var reading: Spec?
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
                            Text(plan.commits ? "The agent works through the steps and commits each one. Long-press a step to change it." : "The agent works through the steps. Long-press a step to change it.")
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

    @ViewBuilder
    var specs: some View {
        if let s = session.specs, !s.specs.isEmpty {
            Section {
                ForEach(s.specs) { spec in
                    Button { reading = spec } label: {
                        HStack(spacing: 12) {
                            Image(systemName: "doc.text").foregroundStyle(Color(p.primary))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(spec.title.isEmpty ? spec.path : spec.title).font(.system(size: 15.5, weight: .medium))
                                if !spec.summary.isEmpty {
                                    Text(spec.summary).font(.system(size: 13)).foregroundStyle(Color(p.muted)).lineLimit(2)
                                }
                                Text(spec.path).font(Style.mono(11)).foregroundStyle(Color(p.faint)).lineLimit(1).truncationMode(.head)
                            }
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.right").font(.system(size: 12, weight: .semibold)).foregroundStyle(Color(p.muted))
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            } header: {
                Text("Specs")
            } footer: {
                Text(s.enabled ? "The agent reads these before it changes what they describe." : "The repository's specs. harlo's Settings → Projects has the agent read them.")
            }
        }
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
                .font(.system(size: 16))
                .foregroundStyle(Color(color))
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(step.title)
                        .font(.system(size: 15.5, weight: step.status == "active" ? .semibold : .regular))
                        .foregroundStyle(Color(step.status == "skipped" ? p.muted : p.foreground))
                        .strikethrough(step.status == "skipped")
                    if step.editedByUser {
                        Text("yours").font(.system(size: 11.5)).foregroundStyle(Color(p.muted))
                    }
                }
                let note = step.summary.isEmpty ? step.detail : step.summary
                if !note.isEmpty {
                    Text(note).font(.system(size: 13.5)).foregroundStyle(Color(p.muted))
                }
                if !step.commit.isEmpty {
                    Label("Committed \(step.commit.prefix(8))", systemImage: "point.3.filled.connected.trianglepath.dotted")
                        .font(Style.mono(11.5))
                        .foregroundStyle(Color(p.success))
                } else if !step.commitNote.isEmpty {
                    Text(step.commitNote)
                        .font(.system(size: 12))
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
