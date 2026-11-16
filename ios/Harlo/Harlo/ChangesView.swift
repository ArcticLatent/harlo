import HarloKit
import SwiftUI

/// The files the session's repository has changed, read-only, each with
/// its diff.
struct ChangesView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    @State private var changes: Changes?
    @State private var loading = true

    var body: some View {
        NavigationStack {
            List {
                if let c = changes, !c.repos.isEmpty {
                    // A folder of repositories: one section each.
                    ForEach(c.repos.filter { $0.changes > 0 }) { repo in
                        Section {
                            ForEach(c.files(in: repo)) { f in
                                NavigationLink {
                                    DiffView(file: f) { dismiss() }
                                } label: {
                                    FileRow(file: f)
                                }
                                .listRowBackground(Color(p.elevated))
                            }
                        } header: {
                            HStack(spacing: 6) {
                                Image(systemName: "shippingbox")
                                Text(repo.name)
                                Text(repo.branch).foregroundStyle(Color(p.muted))
                                if repo.ahead > 0 { Text("↑\(repo.ahead)") }
                                Spacer()
                                Text(repo.changes == 1 ? "1 file" : "\(repo.changes) files")
                            }
                            .font(Style.mono(11.5, .semibold))
                            .foregroundStyle(Color(p.muted))
                            .textCase(nil)
                        }
                    }
                    let clean = c.repos.filter { $0.changes == 0 }
                    if !clean.isEmpty {
                        Section {
                        } footer: {
                            Text((clean.count == 1 ? "1 more repository" : "\(clean.count) more repositories") + " without changes: " + clean.map(\.name).joined(separator: ", "))
                        }
                    }
                } else if let c = changes {
                    Section {
                        ForEach(c.files) { f in
                            NavigationLink {
                                DiffView(file: f) { dismiss() }
                            } label: {
                                FileRow(file: f)
                            }
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        HStack(spacing: 6) {
                            Image(systemName: "arrow.triangle.branch")
                            Text(c.branch.isEmpty ? "detached" : c.branch)
                            if c.ahead > 0 { Text("↑\(c.ahead)") }
                            if c.behind > 0 { Text("↓\(c.behind)") }
                            Spacer()
                            Text(c.files.count == 1 ? "1 file" : "\(c.files.count) files")
                        }
                        .font(Style.mono(11.5, .semibold))
                        .foregroundStyle(Color(p.muted))
                        .textCase(nil)
                    }
                }
            }
            .overlay {
                if loading {
                    ProgressView()
                } else if changes == nil {
                    ContentUnavailableView("No repository", systemImage: "folder.badge.questionmark",
                                           description: Text("This session's folder isn't a git repository."))
                } else if changes?.files.isEmpty == true {
                    ContentUnavailableView("No changes", systemImage: "checkmark.circle",
                                           description: Text("Nothing has changed since the last commit."))
                }
            }
            .refreshable { await load() }
            .screen()
            .navigationTitle("Changed files")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
        .task { await load() }
    }

    func load() async {
        let c = await model.changes()
        // (No repository: the alert said why; the list says it too.)
        if c == nil { model.alert = nil }
        changes = c
        loading = false
    }
}
