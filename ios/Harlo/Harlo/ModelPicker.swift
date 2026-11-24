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

    /// The models shown, by provider (its id, its name, its models).
    var providers: [(String, String, [ModelChoice])] {
        let q = search.trimmingCharacters(in: .whitespaces)
        let shown = models.filter { q.isEmpty || $0.name.localizedStandardContains(q) || $0.id.localizedStandardContains(q) || $0.providerName.localizedStandardContains(q) }
        var out: [(String, String, [ModelChoice])] = []
        for m in shown {
            if let i = out.firstIndex(where: { $0.0 == m.provider }) { out[i].2.append(m) } else { out.append((m.provider, m.providerName, [m])) }
        }
        return out
    }

    /// Where more models come from: keys added on the computer.
    var keysNote: LocalizedStringKey? {
        switch session.harness {
        case "pi":
            return "Other providers' models show here once their key is added in harlo on your computer: Settings → Models."
        default:
            return nil
        }
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(providers, id: \.0) { _, providerName, list in
                    Section {
                        ForEach(list) { m in
                            Button {
                                UISelectionFeedbackGenerator().selectionChanged()
                                if let onPick { onPick(m) } else { Task { await model.setModel(m) } }
                                dismiss()
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(m.name).scaledFont(15)
                                        Text(m.modelId).font(Style.mono(11)).foregroundStyle(Color(p.muted))
                                        if !m.stoppedServer.isEmpty {
                                            HStack(spacing: 4) {
                                                Image(systemName: "pause.circle")
                                                Text("\(m.stoppedServer) is stopped: it starts with your next message")
                                            }
                                            .scaledFont(12)
                                            .foregroundStyle(Color(p.warning))
                                        }
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
                        Text(providerName).scaledFont(13, .semibold).foregroundStyle(Color(p.muted)).textCase(nil)
                    }
                }
                // Keys stay on the computer: they're added there.
                if !loading, let note = keysNote {
                    Section {
                    } footer: {
                        Text(note)
                            .scaledFont(12)
                            .foregroundStyle(Color(p.muted))
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
