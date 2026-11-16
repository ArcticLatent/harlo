import HarloKit
import SwiftUI

/// What the session ran and touched (harlo's action log): every command,
/// every file it read or changed, other tools too, newest first, with how
/// each went. A file opens; a command copies.
struct ActionLogView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    @State private var log: ActionLog?
    @State private var loading = true
    @State private var filter = "all"

    var shown: [ActionLog.Entry] {
        guard let log else { return [] }
        let kinds: [String: [String]] = ["command": ["command"], "change": ["edit", "write"], "read": ["read", "search"]]
        return log.entries.filter { filter == "all" || kinds[filter]?.contains($0.kind) == true }.reversed()
    }

    var body: some View {
        NavigationStack {
            List {
                if let log, !log.entries.isEmpty {
                    Section {
                        Picker("Show", selection: $filter) {
                            Text("All").tag("all")
                            Text("Commands").tag("command")
                            Text("Changes").tag("change")
                            Text("Reads").tag("read")
                        }
                        .pickerStyle(.segmented)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets())
                    } footer: {
                        Text(summary(log))
                    }
                    Section {
                        ForEach(shown) { e in
                            if e.isFile {
                                NavigationLink {
                                    FileView(path: e.target, line: 0)
                                } label: {
                                    ActionLogRow(entry: e)
                                }
                                .listRowBackground(Color(p.elevated))
                            } else if e.kind == "command" {
                                Button { UIPasteboard.general.string = e.target } label: {
                                    ActionLogRow(entry: e).contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .accessibilityHint("Copies the command")
                                .listRowBackground(Color(p.elevated))
                            } else {
                                ActionLogRow(entry: e)
                                    .listRowBackground(Color(p.elevated))
                            }
                        }
                    }
                } else if !loading {
                    Text(log == nil ? "harlo couldn't read this session's actions." : "Nothing yet: commands the agent runs and files it reads or changes show here.")
                        .foregroundStyle(Color(p.muted))
                        .listRowBackground(Color.clear)
                }
            }
            .screen()
            .overlay { if loading { ProgressView() } }
            .navigationTitle("Action log")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .refreshable { await load() }
            // Again after each turn.
            .task(id: session.busy) { await load() }
        }
    }

    func load() async {
        log = await model.actionLog()
        loading = false
    }

    func summary(_ log: ActionLog) -> String {
        var parts = [log.commands == 1 ? "1 command" : "\(log.commands) commands"]
        if log.failed > 0 { parts.append("\(log.failed) failed") }
        parts.append(log.changed.count == 1 ? "1 file changed" : "\(log.changed.count) files changed")
        parts.append("\(log.read.count) read")
        let latest = log.entries.count < log.total ? " The latest \(log.entries.count.formatted()) of \(log.total.formatted()) are listed." : ""
        return parts.joined(separator: " · ") + "." + latest + " Tap a file to open it, a command to copy it."
    }
}
