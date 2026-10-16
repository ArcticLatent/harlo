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
                                Text(repo.branch).foregroundStyle(Color(p.faint))
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

/// A changed file: its status letter, name and folder.
struct FileRow: View {
    @Environment(\.palette) private var p
    let file: Changes.File

    var color: RGBA {
        switch file.status {
        case "A", "?": return p.token(9) ?? p.success
        case "D": return p.token(10) ?? p.error
        default: return p.token(11) ?? p.warning
        }
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(file.status == "?" ? "U" : file.status)
                .font(Style.mono(12, .bold))
                .foregroundStyle(Color(color))
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(file.name).font(Style.mono(13.5, .medium)).lineLimit(1)
                if !file.folder.isEmpty {
                    Text(file.folder).font(Style.mono(11)).foregroundStyle(Color(p.muted)).lineLimit(1).truncationMode(.head)
                }
            }
        }
    }
}

/// One file's diff: hunks of lines with their numbers, added and removed
/// lines tinted as on the desktop.
struct DiffView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let file: Changes.File
    /// After the comments are sent: back to the session.
    var close: () -> Void = {}
    @State private var diff: FileDiff?
    @State private var loading = true
    /// Comments on lines (Settings → Comments on changes).
    @AppStorage("reviewComments") private var commentsOn = true
    @State private var commenting: FileDiff.Line?
    @State private var commentText = ""

    var review: [ReviewComment] { model.session?.review ?? [] }

    func comment(on l: FileDiff.Line) -> ReviewComment? {
        let (side, line) = l.kind == .removed ? ("old", l.old ?? 0) : ("new", l.new ?? 0)
        return review.first { $0.path == file.path && $0.side == side && $0.line == line }
    }

    var body: some View {
        // (Up and down, then across: a two-way scroll view centres a short diff.)
        ScrollView(.vertical) {
            ScrollView(.horizontal, showsIndicators: false) {
                if let d = diff {
                    VStack(alignment: .leading, spacing: 0) {
                        if d.binary {
                            Text("A binary file.").foregroundStyle(Color(p.muted)).padding()
                        }
                        ForEach(d.hunks.indices, id: \.self) { h in
                            Text(d.hunks[h].header)
                                .font(Style.mono(11.5))
                                .foregroundStyle(Color(p.muted))
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color(p.subtle))
                            ForEach(d.hunks[h].lines.indices, id: \.self) { i in
                                line(d.hunks[h].lines[i])
                            }
                        }
                        if d.cut {
                            Text("The rest of this diff is too long to show here.")
                                .font(.footnote)
                                .foregroundStyle(Color(p.muted))
                                .padding()
                        }
                    }
                    .padding(.bottom, 20)
                }
            }
        }
        .overlay { if loading { ProgressView() } }
        // Comments: how many, sending them to the agent.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if commentsOn && !review.isEmpty {
                HStack(spacing: 10) {
                    Image(systemName: "text.bubble.fill").foregroundStyle(Color(p.primary))
                    Text(review.count == 1 ? "1 comment" : "\(review.count) comments")
                        .font(.system(size: 15, weight: .medium))
                    Spacer(minLength: 6)
                    Button("Clear") { model.session?.review = [] }
                        .font(.system(size: 15))
                        .foregroundStyle(Color(p.muted))
                    Button {
                        let message = ReviewComment.message(review)
                        model.session?.review = []
                        Task { await model.send(message) }
                        close()
                    } label: {
                        Text("Send to the agent")
                            .font(.system(size: 15, weight: .semibold))
                            .padding(.horizontal, 14)
                            .frame(height: 36)
                            .background(Capsule().fill(Color(p.primary)))
                            .foregroundStyle(Color(p.onPrimary))
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color(p.raised).ignoresSafeArea(edges: .bottom))
                .overlay(alignment: .top) { Rectangle().fill(Color(p.hairline)).frame(height: 1) }
            }
        }
        .alert(commenting.map { l in "Line \(l.kind == .removed ? l.old ?? 0 : l.new ?? 0)" + (l.kind == .removed ? " (removed)" : "") } ?? "",
               isPresented: Binding(get: { commenting != nil }, set: { if !$0 { commenting = nil } })) {
            TextField("What should the agent change here?", text: $commentText)
            Button("Cancel", role: .cancel) {}
            if let l = commenting, comment(on: l) != nil {
                Button("Delete", role: .destructive) { save(nil) }
            }
            Button("Comment") { save(commentText) }
        } message: {
            Text(commenting?.text.trimmingCharacters(in: .whitespaces) ?? "")
        }
        .screen()
        .navigationTitle(file.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                if let d = diff {
                    HStack(spacing: 6) {
                        Text("+\(d.added)").foregroundStyle(Color(p.token(9) ?? p.success))
                        Text("−\(d.removed)").foregroundStyle(Color(p.token(10) ?? p.error))
                    }
                    .font(Style.mono(12, .semibold))
                }
            }
        }
        .task {
            diff = await model.diff(file.path)
            loading = false
        }
    }

    /// Keeps (or with nil, removes) the comment on the line being commented.
    func save(_ text: String?) {
        guard let l = commenting, let s = model.session else { return }
        let (side, line) = l.kind == .removed ? ("old", l.old ?? 0) : ("new", l.new ?? 0)
        s.review.removeAll { $0.path == file.path && $0.side == side && $0.line == line }
        if let t = text?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty {
            s.review.append(ReviewComment(path: file.path, side: side, line: line, code: l.text, text: t))
        }
        commenting = nil
    }

    @ViewBuilder func line(_ l: FileDiff.Line) -> some View {
        let tint: RGBA? = l.kind == .added ? (p.token(9) ?? p.success) : l.kind == .removed ? (p.token(10) ?? p.error) : nil
        let commented = commentsOn && comment(on: l) != nil
        HStack(alignment: .top, spacing: 0) {
            // A commented line's mark.
            Image(systemName: "text.bubble.fill")
                .font(.system(size: 10))
                .foregroundStyle(Color(p.primary))
                .frame(width: 16)
                .opacity(commented ? 1 : 0)
            Text(l.old.map { String($0) } ?? "")
                .frame(width: 34, alignment: .trailing)
                .foregroundStyle(Color(p.muted).opacity(0.7))
            Text(l.new.map { String($0) } ?? "")
                .frame(width: 34, alignment: .trailing)
                .foregroundStyle(Color(p.muted).opacity(0.7))
            Text(l.kind == .added ? "+" : l.kind == .removed ? "−" : " ")
                .frame(width: 18)
                .foregroundStyle(tint.map { Color($0) } ?? Color(p.muted))
            Text(l.text.isEmpty ? " " : l.text)
                .fixedSize(horizontal: true, vertical: false)
                .padding(.trailing, 12)
        }
        .font(Style.mono(12))
        .padding(.vertical, 1)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(tint.map { Color($0).opacity(0.14) } ?? Color.clear)
        .contentShape(Rectangle())
        // A tap: a comment on this line (to the agent, with the others).
        .onTapGesture {
            guard commentsOn, model.session != nil, (l.kind == .removed ? l.old : l.new) != nil else { return }
            commentText = comment(on: l)?.text ?? ""
            commenting = l
        }
    }
}
