import HarloKit
import SwiftUI
import UIKit

/// A file a reply mentions (`src/app.rs:42`), read-only and highlighted,
/// at that line.
struct FileView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let path: String
    let line: Int
    @State private var file: FileText?
    @State private var problem: String?

    var name: String { path.split(separator: "/").last.map(String.init) ?? path }

    var body: some View {
        NavigationStack {
            Group {
                if let f = file {
                    lines(f)
                } else if let problem {
                    ContentUnavailableView(name, systemImage: "doc.questionmark", description: Text(problem))
                } else {
                    ProgressView()
                }
            }
            .screen()
            .navigationTitle(name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
                if let f = file {
                    ToolbarItem(placement: .topBarLeading) {
                        Button {
                            UIPasteboard.general.string = (0..<f.lines.count).map { RichText.plain(f.spans($0)) }.joined(separator: "\n")
                        } label: {
                            Image(systemName: "doc.on.doc")
                        }
                        .accessibilityLabel("Copy the file")
                    }
                }
            }
        }
        .task {
            let before = model.alert
            file = await model.file(path)
            if file == nil {
                // Said here, not as an alert over the sheet.
                problem = model.alert ?? "Couldn't open it."
                model.alert = before
            }
        }
    }

    func lines(_ f: FileText) -> some View {
        let width = String(f.lines.count).count
        return ScrollViewReader { proxy in
            ScrollView([.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    Text(f.path)
                        .font(Style.mono(11))
                        .foregroundStyle(Color(p.muted))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                    ForEach(0..<f.lines.count, id: \.self) { i in
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text(String(i + 1).leftPadded(to: width))
                                .font(Style.mono(12))
                                .foregroundStyle(Color(p.muted))
                            Text(Style.attributed(f.spans(i), p, size: 14, inCode: true))
                                .fixedSize(horizontal: true, vertical: false)
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(i + 1 == line ? Color(p.primary).opacity(0.16) : Color.clear)
                        .id(i + 1)
                    }
                    if f.cut {
                        Text("The first \(f.lines.count) of \(f.total) lines; the rest is in harlo on your computer.")
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted))
                            .padding(12)
                    }
                }
                .padding(.bottom, 20)
                .textSelection(.enabled)
            }
            .background(Color(p.codeBackground))
            .onAppear {
                if line > 1 { proxy.scrollTo(line, anchor: .center) }
            }
        }
    }
}

extension String {
    func leftPadded(to n: Int) -> String {
        count >= n ? self : String(repeating: " ", count: n - count) + self
    }
}

/// The session's branch, and switching it (as the desktop's branch picker:
/// with uncommitted changes, stash them or bring them along).
struct BranchesView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let busy: Bool
    /// Switched: the new branch.
    var switched: (String) -> Void
    @State private var branches: Branches?
    @State private var loaded = false
    @State private var asking: Branches.Branch?
    @State private var working = false
    /// Why the last switch didn't happen (git's words).
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            List {
                if let problem {
                    Label(problem, systemImage: "exclamationmark.triangle")
                        .font(.footnote)
                        .foregroundStyle(Color(p.warning))
                        .listRowBackground(Color(p.elevated))
                }
                if let b = branches {
                    Section {
                        ForEach(b.local) { br in row(br, remote: false, current: br.name == b.current) }
                    } header: {
                        Text("Branches")
                    } footer: {
                        if b.changed > 0 {
                            Text(b.changed == 1 ? "1 file has uncommitted changes." : "\(b.changed) files have uncommitted changes.")
                        }
                    }
                    .listRowBackground(Color(p.elevated))
                    if !b.remote.isEmpty {
                        Section("Remote") {
                            ForEach(b.remote) { br in row(br, remote: true, current: false) }
                        }
                        .listRowBackground(Color(p.elevated))
                    }
                }
            }
            .overlay {
                if !loaded {
                    ProgressView()
                } else if branches == nil {
                    ContentUnavailableView("No branches", systemImage: "arrow.triangle.branch",
                                           description: Text("This session's folder isn't a git repository."))
                }
            }
            .disabled(working)
            .screen()
            .navigationTitle("Switch branch")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
            .confirmationDialog(asking.map { "Switch to \(target($0.name, remote: isRemote($0)))?" } ?? "", isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
                                titleVisibility: .visible, presenting: asking) { br in
                if (branches?.changed ?? 0) > 0 {
                    Button("Stash and switch") { go(br, stash: true) }
                    Button("Bring changes along") { go(br, stash: false) }
                } else {
                    Button("Switch") { go(br, stash: false) }
                }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text(message)
            }
        }
        .task {
            branches = await model.branches()
            loaded = true
        }
    }

    var message: String {
        var parts: [String] = []
        if let n = branches?.changed, n > 0 {
            parts.append((n == 1 ? "1 file has" : "\(n) files have") + " uncommitted changes. Stash them (bring them back later from harlo's Stashes), or take them along; git refuses that if they clash.")
        }
        if busy { parts.append("The agent is working in this folder. Switching changes the files under it.") }
        return parts.joined(separator: "\n\n")
    }

    func isRemote(_ br: Branches.Branch) -> Bool { branches?.remote.contains { $0.name == br.name } ?? false }

    func target(_ name: String, remote: Bool) -> String {
        remote ? String(name.split(separator: "/", maxSplits: 1).last ?? Substring(name)) : name
    }

    func row(_ br: Branches.Branch, remote: Bool, current: Bool) -> some View {
        Button {
            guard !current else { return }
            if (branches?.changed ?? 0) > 0 || busy {
                asking = br
            } else {
                go(br, stash: false)
            }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: remote ? "cloud" : "arrow.triangle.branch")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(p.muted))
                    .frame(width: 18)
                VStack(alignment: .leading, spacing: 2) {
                    Text(br.name).font(Style.mono(14, current ? .semibold : .regular))
                    HStack(spacing: 6) {
                        if br.date > 0 { Text(Style.ago(br.date, now: Date()) + " ago") }
                        if br.ahead > 0 { Text("↑\(br.ahead) not pushed") }
                    }
                    .font(Style.mono(11))
                    .foregroundStyle(Color(p.muted))
                }
                Spacer()
                if current {
                    Image(systemName: "checkmark").foregroundStyle(Color(p.primary))
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    func go(_ br: Branches.Branch, stash: Bool) {
        let remote = isRemote(br)
        working = true
        problem = nil
        Task {
            let before = model.alert
            let said = await model.switchBranch(to: br.name, remote: remote, stash: stash)
            working = false
            if said == nil {
                // Said here, not as an alert behind the sheet.
                problem = model.alert ?? "Couldn't switch."
                model.alert = before
            } else {
                UINotificationFeedbackGenerator().notificationOccurred(.success)
                switched(target(br.name, remote: remote))
                dismiss()
            }
        }
    }
}
