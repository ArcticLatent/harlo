import HarloKit
import SwiftUI
import UIKit

/// The open session: its transcript, what it's doing (todos, subagents,
/// the queue), a question waiting, and the composer with its settings.
struct SessionContent: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    @State private var sheet: SessionSheet?
    /// The picture the viewer is open at.
    @State private var picture: PictureOpening?
    @State private var renaming = false
    @State private var newName = ""
    /// The repository's branch (nil: not one, or not known yet).
    @State private var branch: String?
    /// Undo a turn (Settings → Undo a turn).
    @AppStorage("undoTurns") private var undoOn = true
    /// The transcript is scrolled to its end (new rows follow then; read
    /// further up and they wait).
    @State private var atBottom = true
    /// The side question's answer folded to its question (once you send
    /// the next message; tap it to open it again).
    @State private var asideFolded = false
    /// The phone's text size (the navigation bar keeps its own at the
    /// default; the header takes this, as far as the bar has room).
    @Environment(\.dynamicTypeSize) private var typeSize

    /// The agent and the session's project.
    var subtitle: String {
        let project = model.sessions.first { $0.key == session.liveKey || $0.key == session.key }?.project ?? ""
        return [Style.harnessLabel(session.harness), project].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The files the last turn edited (once it's done), for "View diff".
    var changedFiles: [String] {
        guard !session.busy else { return [] }
        let rows = session.rows
        let start = (rows.lastIndex { $0.isSent } ?? -1) + 1
        guard start < rows.count else { return [] }
        var out: [String] = []
        for r in rows[start...] where r.kind == "tool" && !r.diff.isEmpty && !out.contains(r.subtitle) {
            out.append(r.subtitle)
        }
        return out
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                // Drawn all at once, not lazily (the phone has at most the
                // last 300 rows, tool runs folded): a lazy stack guesses the
                // heights of rows it hasn't drawn, so following the end, or
                // rows changing height as a reply streams in, could land
                // where nothing is drawn: a blank screen until you scrolled.
                VStack(alignment: .leading, spacing: 14) {
                    if model.state != .connected {
                        ConnectionBanner()
                            .padding(12)
                            .card(p)
                    }
                    if let error = session.error {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(Color(p.warning))
                    }
                    if session.earlier > 0 {
                        Label(session.earlier == 1 ? "1 earlier message is in harlo on your computer" : "\(session.earlier) earlier messages are in harlo on your computer",
                              systemImage: "clock.arrow.circlepath")
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted))
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 6)
                    }
                    let items = session.items
                    ForEach(Array(items.enumerated()), id: \.element.id) { i, item in
                        TranscriptRow(item: item, opensReply: !Self.isSent(item) && (i == 0 || Self.isSent(items[i - 1])), session: session) { sheet = $0 }
                    }
                    // What the last turn changed: tap for the diff, or undo it
                    // (in a git repository: elsewhere there's no diff to show).
                    let changed = changedFiles
                    if !changed.isEmpty && session.git {
                        HStack(spacing: 8) {
                        Button {
                            sheet = .changes
                        } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "doc.text")
                                    .scaledFont(17)
                                    .foregroundStyle(Color(p.muted))
                                // (At the largest sizes "View diff" goes under
                                // it, so neither breaks inside a word.)
                                let words = typeSize.isAccessibilitySize
                                    ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
                                    : AnyLayout(HStackLayout(spacing: 6))
                                words {
                                    Text(changed.count == 1 ? "1 file changed" : "\(changed.count) files changed")
                                        .scaledFont(16)
                                    if !typeSize.isAccessibilitySize { Spacer(minLength: 0) }
                                    Text("View diff")
                                        .scaledFont(16, .medium)
                                        .foregroundStyle(Color(p.primary))
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                Image(systemName: "chevron.right")
                                    .scaledFont(13, .semibold)
                                    .foregroundStyle(Color(p.muted))
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 13)
                            .card(p, radius: 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        let last = Self.lastUser(session.rows)
                        if undoOn && session.undoable[last] != nil {
                            Button {
                                sheet = .undo(last)
                            } label: {
                                Image(systemName: "arrow.uturn.backward")
                                    .font(.system(size: 16, weight: .semibold))
                                    .frame(width: 48, height: 48)
                                    .card(p, radius: 14)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Undo these changes")
                        }
                        }
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .overlay {
                if session.loading && session.rows.isEmpty && session.error == nil { ProgressView() }
            }
            // New rows: follow them while at the end (not animated; after
            // they're laid out, or it stops at the old end).
            .onChange(of: session.rows.count) { _, _ in
                if atBottom { Task { proxy.scrollTo("bottom", anchor: .bottom) } }
            }
            // Opened (again): at the end once its rows are in, wherever the
            // rows loading on the way had left it.
            .onChange(of: session.loading) { _, loading in
                guard !loading else { return }
                atBottom = true
                Task { proxy.scrollTo("bottom", anchor: .bottom) }
            }
            .onChange(of: session.aside?.question) { _, _ in asideFolded = false }
            .modifier(FollowsTheEnd(atBottom: $atBottom) { proxy.scrollTo("bottom", anchor: .bottom) })
            .safeAreaInset(edge: .bottom, spacing: 0) {
                VStack(spacing: 0) {
                    ActivityLine(session: session)
                    LimitLine(session: session)
                    ActivityBar(session: session) { sheet = $0 }
                    // (Under the plan's card: beside the context bar it's about.)
                    CompactLine(session: session)
                    if let aside = session.aside {
                        AsideCard(aside: aside, folded: $asideFolded) { session.aside = nil }
                    }
                    if let req = session.requests.first {
                        RequestCard(request: req)
                            .id(req.id)
                    }
                    Composer(session: session, openSheet: { sheet = $0 }) {
                        if session.aside?.done == true { asideFolded = true }
                        atBottom = true
                        proxy.scrollTo("bottom", anchor: .bottom)
                    }
                }
                // (Solid, with a hairline: nothing of the conversation shows through.)
                .background(Color(p.background).ignoresSafeArea(edges: .bottom))
                .overlay(alignment: .top) { Rectangle().fill(Color(p.hairline)).frame(height: 1) }
                // (For the UI tests: where the conversation's visible part ends.)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("sessionBottom")
                // Taller or shorter (a side question's answer coming in, a
                // card going): the end kept in sight, after this layout.
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { _ in
                    if atBottom { Task { @MainActor in proxy.scrollTo("bottom", anchor: .bottom) } }
                }
            }
        }
        .screen()
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.visible, for: .navigationBar)
        // (The conversation gets the room; Back returns to the tabs.)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            // The session's tile (its model's maker, the agent as a tag), its
            // name, and the agent · project · branch.
            ToolbarItem(placement: .principal) {
                HStack(spacing: 10) {
                    AgentTile(harness: session.harness, maker: session.info.maker, size: 34)
                        .overlay(alignment: .bottomTrailing) {
                            if session.busy || !session.requests.isEmpty {
                                StatusDot(status: session.busy ? "busy" : "input")
                                    .padding(2)
                                    .background { Circle().fill(Color(p.background)) }
                                    .offset(x: 4, y: 4)
                                    // (The line above the composer says it.)
                                    .accessibilityHidden(true)
                            }
                        }
                    VStack(alignment: .leading, spacing: 1) {
                        Text(session.title.isEmpty ? "Session" : session.title)
                            .scaledFont(16, .semibold)
                            .lineLimit(1)
                            // (Larger than the bar has room for: press and
                            // hold to read it big.)
                            .accessibilityShowsLargeContentViewer()
                    HStack(spacing: 6) {
                        if !subtitle.isEmpty {
                            Text(subtitle)
                                .lineLimit(1)
                                .accessibilityShowsLargeContentViewer()
                        }
                        // The branch: tap to switch.
                        if let branch, !branch.isEmpty {
                            Button {
                                sheet = .branches
                            } label: {
                                HStack(spacing: 2) {
                                    Image(systemName: "arrow.triangle.branch").scaledFont(9, .semibold)
                                    Text(branch).lineLimit(1)
                                    Image(systemName: "chevron.down").scaledFont(7, .bold)
                                }
                                .foregroundStyle(Color(p.primary))
                                // (A full-size target, though its line is small.)
                                .padding(.vertical, 14)
                                .padding(.horizontal, 6)
                                .contentShape(Rectangle())
                                .padding(.vertical, -14)
                                .padding(.horizontal, -6)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Branch \(branch). Switch branch")
                            .accessibilityShowsLargeContentViewer {
                                Label(branch, systemImage: "arrow.triangle.branch")
                            }
                        }
                    }
                    .scaledFont(12.5)
                    .foregroundStyle(Color(p.muted))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                // The phone's text size, up to what fits the bar's height.
                .environment(\.dynamicTypeSize, min(typeSize, .xLarge))
            }
            ToolbarItem(placement: .topBarTrailing) {
                let others = model.waiting.count(where: { $0.session.key != session.liveKey && $0.session.key != session.key })
                if others > 0 {
                    Button {
                        sheet = .inbox
                    } label: {
                        HStack(spacing: 3) {
                            Image(systemName: "hand.raised.fill")
                            Text("\(others)").scaledFont(13.5, .semibold)
                        }
                        .foregroundStyle(Color(p.warning))
                    }
                    .accessibilityLabel(others == 1 ? "1 other session needs you" : "\(others) other sessions need you")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button {
                        sheet = .changes
                    } label: {
                        Label("Changed files", systemImage: "plusminus")
                    }
                    // (Also the branch under the title, a small target.)
                    if let branch, !branch.isEmpty {
                        Button {
                            sheet = .branches
                        } label: {
                            Label("Switch branch (\(branch))", systemImage: "arrow.triangle.branch")
                        }
                    }
                    Button {
                        sheet = .actions
                    } label: {
                        Label("Action log", systemImage: "list.bullet.rectangle")
                    }
                    Button {
                        Task { await shareMarkdown() }
                    } label: {
                        Label("Share as Markdown", systemImage: "square.and.arrow.up")
                    }
                    if model.hasPlans {
                        Button {
                            sheet = .plan
                        } label: {
                            Label("Plan", systemImage: "list.bullet.clipboard")
                        }
                    }
                    if model.hasWiki && session.wiki?.inProject != false {
                        Button {
                            sheet = .wiki
                        } label: {
                            Label("Wiki", systemImage: "book")
                        }
                    }
                    if let last = session.rows.last(where: { !$0.images.isEmpty })?.images.last {
                        Button {
                            picture = PictureOpening(path: last, grid: true)
                        } label: {
                            Label("Pictures", systemImage: "photo.on.rectangle")
                        }
                    }
                    if !session.todos.isEmpty || !session.subagents.isEmpty || !session.helpers.isEmpty {
                        Button {
                            sheet = .activity
                        } label: {
                            Label(session.helpers.isEmpty ? "Todos and subagents" : "Todos and helpers", systemImage: "checklist")
                        }
                    }
                    Divider()
                    Button {
                        newName = session.title
                        renaming = true
                    } label: {
                        Label("Rename", systemImage: "pencil")
                    }
                    if let row = model.sessions.first(where: { $0.key == session.liveKey || $0.key == session.key }) {
                        Button {
                            Task {
                                await model.archive(row)
                                model.close()
                            }
                        } label: {
                            Label("Archive", systemImage: "archivebox")
                        }
                    }
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 17, weight: .semibold))
                        .frame(width: 34, height: 34)
                }
                .accessibilityLabel("Session menu")
            }
        }
        .alert("Rename session", isPresented: $renaming) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") {
                if let row = model.sessions.first(where: { $0.key == session.liveKey || $0.key == session.key }) {
                    Task { await model.rename(row, to: newName) }
                }
            }
        }
        // Paths in replies open the file (a picture in the viewer).
        .environment(\.openURL, OpenURLAction { url in
            guard let target = FileLinks.target(url) else { return .systemAction }
            // (A picture opens the viewer, which says when this harlo is
            // too old to send it.)
            if PictureNames.isPicture(target.path) {
                picture = PictureOpening(path: PictureNames.match(target.path, in: session.pictures) ?? target.path)
            } else {
                sheet = .file(target.path, target.line)
            }
            return .handled
        })
        .environment(\.openPicture, { (path: String) in picture = PictureOpening(path: path) })
        .fullScreenCover(item: $picture) { opening in
            let all = session.pictures
            PictureViewer(paths: all.contains(opening.path) ? all : [opening.path], current: opening.path, grid: opening.grid)
                .presentationBackground(.clear)
        }
        // The branch: when it opens, and after each run (the agent may switch).
        .task(id: session.liveKey) {
            guard !session.liveKey.isEmpty else { return }
            branch = await model.branches()?.current
        }
        .onChange(of: session.busy) { _, busy in
            if !busy { Task { branch = await model.branches()?.current } }
        }
        // Which of your messages a turn can be undone to: as it opens, and
        // after each turn.
        .task(id: "\(session.liveKey)-\(session.busy)") {
            if !session.busy, !session.liveKey.isEmpty { await model.loadUndo() }
        }
        .sheet(item: $sheet) { which in
            Group {
                switch which {
                case .file(let path, let line): FileView(path: path, line: line)
                case .branches: BranchesView(busy: session.busy) { branch = $0 }
                case .changes: ChangesView()
                case .plan: PlanSheet(session: session)
                case .wiki: WikiSheet(session: session)
                case .workflow(let id): WorkflowSheet(id: id)
                case .activity: ActivityView(session: session)
                case .actions: ActionLogView(session: session)
                case .share(let url): ShareSheet(items: [url])
                case .models: ModelPicker(session: session)
                case .inbox: InboxView(except: [session.key, session.liveKey])
                case .undo(let row): UndoSheet(row: row)
                case .retry(let row):
                    ModelPicker(session: session, title: "Retry with") { m in
                        Task { await model.retry(row, with: m) }
                    }
                }
            }
            .modifier(Themed())
        }
    }
}

extension SessionContent {
    /// The session as a Markdown file (named after it), for the share sheet.
    func shareMarkdown() async {
        guard let md = await model.markdown() else { return }
        let name = session.title.components(separatedBy: CharacterSet(charactersIn: "/\\:*?\"<>|\n")).joined(separator: " ")
            .trimmingCharacters(in: .whitespaces).prefix(80)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent((name.isEmpty ? "Session" : String(name)) + ".md")
        do {
            try md.write(to: url, atomically: true, encoding: .utf8)
            sheet = .share(url)
        } catch {
            model.alert = "Couldn't make the file: \(error.localizedDescription)"
        }
    }

    /// A message sent to the agent: yours, or harlo's (a helper's report).
    static func isSent(_ item: TranscriptItem) -> Bool {
        if case .row(let r) = item { return r.isSent }
        return false
    }

    /// The last turn's start: your last message's row key (or harlo's).
    static func lastUser(_ rows: [Row]) -> String {
        rows.last { $0.isSent }?.key ?? ""
    }

    /// When an item started (unix ms; 0: unknown).
    static func time(of item: TranscriptItem) -> Int64 {
        switch item {
        case .row(let r): return r.time
        case .run(_, _, let rows, _, _): return rows.first?.time ?? 0
        }
    }

    /// The model that wrote it ("" unknown).
    static func model(of item: TranscriptItem) -> String {
        switch item {
        case .row(let r): return r.model
        case .run(_, _, let rows, _, _): return rows.first { !$0.model.isEmpty }?.model ?? ""
        }
    }

    /// The message that started the turn a row is in (yours, or harlo's).
    static func turnStart(_ rows: [Row], _ key: String) -> String? {
        guard let i = rows.firstIndex(where: { $0.key == key }) else { return nil }
        return rows[...i].last { $0.isSent }?.key
    }
}
