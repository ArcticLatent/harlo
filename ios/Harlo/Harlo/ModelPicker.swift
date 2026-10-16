import HarloKit
import SwiftUI

/// The models the session's agent offers, by provider, searchable; the
/// current one checked.
struct ModelPicker: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    var title = "Model"
    /// Instead of switching the session's model (Retry with).
    var onPick: ((ModelChoice) -> Void)? = nil
    @State private var models: [ModelChoice] = []
    @State private var loading = true
    @State private var search = ""

    var providers: [(String, [ModelChoice])] {
        let q = search.lowercased()
        let shown = models.filter { q.isEmpty || $0.name.lowercased().contains(q) || $0.id.lowercased().contains(q) }
        var out: [(String, [ModelChoice])] = []
        for m in shown {
            if let i = out.firstIndex(where: { $0.0 == m.provider }) { out[i].1.append(m) } else { out.append((m.provider, [m])) }
        }
        return out
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(providers, id: \.0) { provider, list in
                    Section {
                        ForEach(list) { m in
                            Button {
                                UISelectionFeedbackGenerator().selectionChanged()
                                if let onPick { onPick(m) } else { Task { await model.setModel(m) } }
                                dismiss()
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(m.name).font(.system(size: 15))
                                        Text(m.modelId).font(Style.mono(11)).foregroundStyle(Color(p.muted))
                                    }
                                    Spacer()
                                    if m.reasoning {
                                        Image(systemName: "brain").font(.caption).foregroundStyle(Color(p.muted))
                                    }
                                    if m.id == session.info.modelId {
                                        Image(systemName: "checkmark").foregroundStyle(Color(p.primary))
                                    }
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        Text(provider).font(Style.mono(11.5, .semibold)).foregroundStyle(Color(p.muted)).textCase(nil)
                    }
                }
            }
            .overlay {
                if loading {
                    ProgressView()
                } else if models.isEmpty {
                    ContentUnavailableView("No models", systemImage: "cpu", description: Text("The agent didn't list any models."))
                }
            }
            .searchable(text: $search, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search models")
            .screen()
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
        .task {
            models = await model.models()
            loading = false
        }
    }
}
