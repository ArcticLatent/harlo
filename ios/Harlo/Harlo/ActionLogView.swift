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
                                    EntryRow(entry: e)
                                }
                                .listRowBackground(Color(p.elevated))
                            } else {
                                EntryRow(entry: e)
                                    .contentShape(Rectangle())
                                    .onTapGesture { if e.kind == "command" { UIPasteboard.general.string = e.target } }
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
        return parts.joined(separator: " · ") + ". Tap a file to open it, a command to copy it."
    }
}

private struct EntryRow: View {
    @Environment(\.palette) private var p
    let entry: ActionLog.Entry

    var symbol: String {
        switch entry.kind {
        case "command": return "terminal"
        case "edit": return "pencil"
        case "write": return "doc.badge.plus"
        case "read": return "eye"
        case "search": return "magnifyingglass"
        case "web": return "globe"
        case "agent": return "person.2"
        default: return "sparkles"
        }
    }

    var detail: String {
        let how = entry.running ? "running…" : entry.failed ? (entry.result.isEmpty || entry.result == "failed" ? "failed" : "failed · \(entry.result)") : entry.result
        return [entry.tool, how].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color(entry.failed ? p.error : p.muted))
                .frame(width: 20)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 3) {
                Text(entry.target.isEmpty ? entry.tool : entry.target)
                    .font(Style.mono(13))
                    .lineLimit(entry.isFile ? 1 : 3)
                    .truncationMode(entry.isFile ? .head : .tail)
                Text(detail)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(entry.failed ? p.error : p.faint))
            }
            Spacer(minLength: 4)
            if let t = entry.time {
                Text(Date(timeIntervalSince1970: Double(t) / 1000).formatted(date: .omitted, time: .shortened))
                    .font(.system(size: 11.5))
                    .foregroundStyle(Color(p.faint))
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}
