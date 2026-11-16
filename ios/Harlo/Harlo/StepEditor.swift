import HarloKit
import SwiftUI

/// Changing a plan's step, or a new one: its outcome, and how.
struct StepEditor: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var p
    @State private var draft: PlanSheet.StepDraft
    var done: (PlanSheet.StepDraft) -> Void
    @FocusState private var focused: Bool

    init(draft: PlanSheet.StepDraft, done: @escaping (PlanSheet.StepDraft) -> Void) {
        _draft = State(initialValue: draft)
        self.done = done
    }

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
