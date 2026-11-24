import HarloKit
import SwiftUI

/// Undoing a turn: what goes back as it was before your message, what goes,
/// what stays; then how it went.
struct UndoSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let row: String
    @State private var plan: UndoPlan?
    @State private var loading = true
    @State private var undoing = false
    @State private var done: UndoPlan?

    var body: some View {
        NavigationStack {
            List {
                if loading {
                    HStack { ProgressView(); Text("Checking what changed…").foregroundStyle(Color(p.muted)) }
                } else if let d = done {
                    Label(d.changes == 0 ? "Nothing to undo: the files are as they were." : "Undone: \(d.changes) \(d.changes == 1 ? "file is" : "files are") back as before this message.",
                          systemImage: "checkmark.circle.fill")
                        .foregroundStyle(Color(p.success))
                } else if let plan {
                    Section {
                        Text(plan.changes == 0 ? "Nothing to undo: the files are as they were before this message."
                             : "The folder's files go back to how they were before this message" + (plan.later > 0 ? ": this turn and the \(plan.later == 1 ? "one" : String(plan.later)) after it." : "."))
                    }
                    if !plan.restore.isEmpty {
                        Section("Brought back (\(plan.restore.count))") {
                            ForEach(plan.restore.prefix(12), id: \.self) { Text($0).font(Style.mono(13)).lineLimit(1).truncationMode(.middle) }
                            if plan.restore.count > 12 { Text("and \(plan.restore.count - 12) more").foregroundStyle(Color(p.muted)) }
                        }
                    }
                    if !plan.remove.isEmpty {
                        Section("Removed, new since (\(plan.remove.count))") {
                            ForEach(plan.remove.prefix(12), id: \.self) { Text($0).font(Style.mono(13)).lineLimit(1).truncationMode(.middle) }
                            if plan.remove.count > 12 { Text("and \(plan.remove.count - 12) more").foregroundStyle(Color(p.muted)) }
                        }
                    }
                    if plan.changes > 0 {
                        Section {
                            Text("Edits made in these files since are undone too. What's there now is kept in git on your computer, so this can be reversed."
                                 + (plan.headMoved ? " Commits made since stay; only the files change." : "")
                                 + " Things outside the folder (installed packages, a database, a push) stay as they are.")
                                .font(.footnote)
                                .foregroundStyle(Color(p.muted))
                        }
                    }
                } else {
                    Text(model.alert ?? "Couldn't check what changed.").foregroundStyle(Color(p.error))
                }
            }
            .screen()
            .navigationTitle((plan?.later ?? 0) > 0 ? "Undo from here" : "Undo this turn")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(done == nil ? "Cancel" : "Close") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if done == nil, let plan, plan.changes > 0 {
                        Button {
                            undoing = true
                            Task {
                                done = await model.undo(row)
                                undoing = false
                            }
                        } label: {
                            if undoing { ProgressView() } else { Text("Undo").bold() }
                        }
                        .disabled(undoing)
                    }
                }
            }
        }
        .modifier(Themed())
        .task {
            plan = await model.undoPlan(row)
            loading = false
        }
    }
}
