import HarloKit
import SwiftUI
import UIKit

struct SessionView: View {
    @Environment(HarloModel.self) private var model

    var body: some View {
        if let s = model.session {
            // (Another session opened from "Needs you" starts afresh.)
            SessionContent(session: s)
                .id(ObjectIdentifier(s))
        } else {
            ProgressView()
        }
    }
}

/// The open session: its transcript, what it's doing (todos, subagents,
/// the queue), a question waiting, and the composer with its settings.
struct SessionContent: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    @State private var sheet: SessionSheet?
    @State private var renaming = false
    @State private var newName = ""
    /// The repository's branch (nil: not one, or not known yet).
    @State private var branch: String?
    /// Undo a turn (Settings → Undo a turn).
    @AppStorage("undoTurns") private var undoOn = true

    /// The agent and the session's project.
    var subtitle: String {
        let project = model.sessions.first { $0.key == session.liveKey || $0.key == session.key }?.project ?? ""
        return [Style.harnessLabel(session.harness), project].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The files the last turn edited (once it's done), for "View diff".
    var changedFiles: [String] {
        guard !session.busy else { return [] }
        let rows = session.rows
        let start = (rows.lastIndex { $0.kind == "user" } ?? -1) + 1
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
                LazyVStack(alignment: .leading, spacing: 14) {
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
                        VStack(alignment: .leading, spacing: 10) {
                            // The reply opens with the agent's tile, name and time.
                            if !Self.isUser(item), i == 0 || Self.isUser(items[i - 1]) {
                                AgentHead(session: session, time: Self.time(of: item))
                                    .padding(.top, 6)
                            }
                            // Replies full width under the agent's name.
                            ItemView(item: item, session: session)
                                .contextMenu { menu(for: item) }
                        }
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
                                    .font(.system(size: 17))
                                    .foregroundStyle(Color(p.muted))
                                Text(changed.count == 1 ? "1 file changed" : "\(changed.count) files changed")
                                    .font(.system(size: 16))
                                Spacer(minLength: 6)
                                Text("View diff")
                                    .font(.system(size: 16, weight: .medium))
                                    .foregroundStyle(Color(p.primary))
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 13, weight: .semibold))
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
            .onChange(of: session.rows.count) { _, _ in
                withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo("bottom", anchor: .bottom) }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                VStack(spacing: 0) {
                    ActivityLine(session: session)
                    LimitLine(session: session)
                    CompactLine(session: session)
                    ActivityBar(session: session) { sheet = $0 }
                    if let req = session.requests.first {
                        RequestCard(request: req)
                            .id(req.id)
                    }
                    Composer(session: session, openSheet: { sheet = $0 }) {
                        proxy.scrollTo("bottom", anchor: .bottom)
                    }
                }
                .background(Color(p.background).opacity(0.97).ignoresSafeArea(edges: .bottom))
            }
        }
        .screen()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbar {
            // The agent's tile, the session's name, and the agent · project · branch.
            ToolbarItem(placement: .principal) {
                HStack(spacing: 10) {
                    AgentTile(harness: session.harness, size: 34)
                        .overlay(alignment: .bottomTrailing) {
                            if session.busy || !session.requests.isEmpty {
                                StatusDot(status: session.busy ? "busy" : "input")
                                    .padding(2)
                                    .background(Circle().fill(Color(p.background)))
                                    .offset(x: 4, y: 4)
                            }
                        }
                    VStack(alignment: .leading, spacing: 1) {
                        Text(session.title.isEmpty ? "Session" : session.title)
                            .font(.system(size: 16, weight: .semibold))
                            .lineLimit(1)
                    HStack(spacing: 6) {
                        if !subtitle.isEmpty {
                            Text(subtitle)
                                .lineLimit(1)
                        }
                        // The branch: tap to switch.
                        if let branch, !branch.isEmpty {
                            Button {
                                sheet = .branches
                            } label: {
                                HStack(spacing: 2) {
                                    Image(systemName: "arrow.triangle.branch").font(.system(size: 9, weight: .semibold))
                                    Text(branch).lineLimit(1)
                                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                                }
                                .foregroundStyle(Color(p.primary))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Branch \(branch). Switch branch")
                        }
                    }
                    .font(.system(size: 12.5))
                    .foregroundStyle(Color(p.muted))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            ToolbarItem(placement: .topBarTrailing) {
                let others = model.waiting.filter { $0.session.key != session.liveKey && $0.session.key != session.key }.count
                if others > 0 {
                    Button {
                        sheet = .inbox
                    } label: {
                        HStack(spacing: 3) {
                            Image(systemName: "hand.raised.fill")
                            Text("\(others)").font(.system(size: 13.5, weight: .semibold))
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
                            Label("Plan and specs", systemImage: "list.bullet.clipboard")
                        }
                    }
                    if !session.todos.isEmpty || !session.subagents.isEmpty {
                        Button {
                            sheet = .activity
                        } label: {
                            Label("Todos and subagents", systemImage: "checklist")
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
        // Paths in replies open the file.
        .environment(\.openURL, OpenURLAction { url in
            guard let target = FileLinks.target(url) else { return .systemAction }
            sheet = .file(target.path, target.line)
            return .handled
        })
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

    static func isUser(_ item: TranscriptItem) -> Bool {
        if case .row(let r) = item { return r.kind == "user" }
        return false
    }

    /// Your last message's row key.
    static func lastUser(_ rows: [Row]) -> String {
        rows.last { $0.kind == "user" }?.key ?? ""
    }

    /// When an item started (unix ms; 0: unknown).
    static func time(of item: TranscriptItem) -> Int64 {
        switch item {
        case .row(let r): return r.time
        case .run(_, _, let rows, _, _): return rows.first?.time ?? 0
        }
    }

    /// A long press on a message: copy it; yours also edit & resend, fork,
    /// or ask again with another model.
    @ViewBuilder func menu(for item: TranscriptItem) -> some View {
        if case .row(let r) = item {
            if r.kind == "user" {
                Button {
                    UIPasteboard.general.string = r.text
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }

                if session.canBranch {
                    Button {
                        Task { await model.editInComposer(r.key) }
                    } label: {
                        Label("Edit & resend", systemImage: "pencil")
                    }
                    Button {
                        sheet = .retry(r.key)
                    } label: {
                        Label("Retry with another model", systemImage: "arrow.triangle.2.circlepath")
                    }
                }
                if session.canFork {
                    Button {
                        Task { await model.forkFrom(r.key) }
                    } label: {
                        Label("Fork from here", systemImage: "arrow.triangle.branch")
                    }
                }
            } else if r.kind == "markdown" || r.kind == "code" {
                Button {
                    UIPasteboard.general.string = r.kind == "code" && !r.text.isEmpty ? r.text : RichText.plainText(RichCache.blocks(r.html))
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                undoItem(after: r.key)
            } else if r.kind == "tool" || r.kind == "notice" {
                Button {
                    UIPasteboard.general.string = r.kind == "tool" ? "\(r.title) \(r.subtitle)\n\n\(r.text)" : r.text
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                undoItem(after: r.key)
            }
        } else if case .run(_, _, let rows, _, _) = item, let first = rows.first {
            undoItem(after: first.key)
        }
    }

    /// The agent's work in a turn (a reply, a run): undo that turn, from
    /// your message that started it.
    @ViewBuilder func undoItem(after key: String) -> some View {
        if undoOn, !session.busy, let start = Self.turnStart(session.rows, key), session.undoable[start] != nil {
            Button {
                sheet = .undo(start)
            } label: {
                Label(Self.lastUser(session.rows) == start ? "Undo this turn" : "Undo from here", systemImage: "arrow.uturn.backward")
            }
        }
    }

    /// Your message that started the turn a row is in.
    static func turnStart(_ rows: [Row], _ key: String) -> String? {
        guard let i = rows.firstIndex(where: { $0.key == key }) else { return nil }
        return rows[...i].last { $0.kind == "user" }?.key
    }
}

/// The sheets a session opens.
enum SessionSheet: Identifiable {
    case changes, activity, models, actions, plan
    /// The session as a Markdown file, to share.
    case share(URL)
    /// What waits on you in other sessions.
    case inbox
    /// Ask again with another model, from this message (its row key).
    case retry(String)
    /// A file a reply mentions, at a line (0: its start).
    case file(String, Int)
    case branches
    /// Undo from one of your messages (its row key).
    case undo(String)

    var id: String {
        switch self {
        case .file(let path, let line): return "file:\(path):\(line)"
        case .branches: return "branches"
        case .changes: return "changes"
        case .plan: return "plan"
        case .activity: return "activity"
        case .actions: return "actions"
        case .share(let url): return "share:\(url.path)"
        case .models: return "models"
        case .inbox: return "inbox"
        case .retry(let row): return "retry:" + row
        case .undo(let row): return "undo:" + row
        }
    }
}

/// What the session is doing besides talking: todo progress, subagents at
/// work, messages waiting. Tapping opens the details.
struct ActivityBar: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    var open: (SessionSheet) -> Void

    var todo: (done: Int, total: Int, now: String)? {
        let tasks = session.todos.flatMap(\.tasks)
        guard !tasks.isEmpty else { return nil }
        let now = tasks.first { $0.status == "in_progress" }?.content ?? tasks.first { $0.status != "completed" }?.content ?? "All done"
        return (tasks.filter { $0.status == "completed" }.count, tasks.count, now)
    }

    var running: Int { session.subagents.filter { $0.status == "running" }.count }

    var body: some View {
        let queued = session.steering + session.followUp
        let plan = session.plan.flatMap { $0.shows ? $0 : nil }
        if plan != nil || todo != nil || running > 0 || !queued.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                if let plan {
                    PlanLine(plan: plan) { open(.plan) }
                }
                if let t = todo {
                    Button { open(.activity) } label: {
                        HStack(spacing: 8) {
                            ProgressRing(fraction: Double(t.done) / Double(max(t.total, 1)))
                            Text("\(t.done)/\(t.total)")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(Color(p.muted))
                            Text(t.now)
                                .font(.system(size: 13))
                                .lineLimit(1)
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.up").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color(p.muted))
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if running > 0 {
                    Button { open(.activity) } label: {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.mini)
                            Text(running == 1 ? "1 subagent working" : "\(running) subagents working")
                                .font(.system(size: 13))
                            Spacer(minLength: 0)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                ForEach(queued.indices, id: \.self) { i in
                    HStack(spacing: 6) {
                        Text(i < session.steering.count ? "Steer" : "Next")
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(Color(p.primary))
                        Text(queued[i])
                            .font(.system(size: 13))
                            .foregroundStyle(Color(p.muted))
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        if session.canUnqueue {
                            // A follow-up can go now, as steering.
                            if i >= session.steering.count {
                                Button {
                                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                    Task { await model.sendNow(i) }
                                } label: {
                                    Text("Send now").font(.system(size: 12.5, weight: .medium)).foregroundStyle(Color(p.primary))
                                }
                                .buttonStyle(.plain)
                            }
                            Button {
                                Task { await model.unqueue(i) }
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .font(.system(size: 16))
                                    .foregroundStyle(Color(p.muted))
                                    .frame(width: 30, height: 26)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Take back")
                        }
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .card(p, radius: 10)
            .padding(.horizontal, 12)
            .padding(.top, 8)
        }
    }
}

struct ProgressRing: View {
    @Environment(\.palette) private var p
    let fraction: Double

    var body: some View {
        ZStack {
            Circle().stroke(Color(p.border), lineWidth: 2.5)
            Circle().trim(from: 0, to: fraction).stroke(Color(p.success), style: StrokeStyle(lineWidth: 2.5, lineCap: .round)).rotationEffect(.degrees(-90))
        }
        .frame(width: 14, height: 14)
    }
}

/// What to type (photos, slash commands), the session's settings (model,
/// thinking, approval mode, Plan), and send / stop.
struct Composer: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Bindable var session: OpenSession
    var openSheet: (SessionSheet) -> Void
    var onSend: () -> Void
    @State private var photos: [Photo] = []
    /// Photos sent and not yet with harlo (shown, dimmed, until they are).
    @State private var sendingPhotos: [Photo] = []
    @State private var levels: [String] = []
    /// Files matching the `@` being typed.
    @State private var fileMatches: [String] = []
    @FocusState private var focused: Bool

    var text: String { session.draft }

    /// What follows the `@` being typed (nil: none).
    var mention: String? { Mention.query(in: text) }

    var canSend: Bool {
        (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !photos.isEmpty) && model.state == .connected
    }

    var commands: [SlashCommand] { SlashCommand.matching(session.commands, typed: text) }

    var placeholder: String {
        guard session.busy else { return "Message \(Style.harnessLabel(session.harness))…" }
        return session.steer ? "Steer the current run…" : "Queue a follow-up…"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !commands.isEmpty && focused {
                SlashList(commands: Array(commands.prefix(6))) { c in
                    session.draft = "/" + c.name + " "
                }
            } else if focused && mention != nil && !fileMatches.isEmpty {
                FileList(files: Array(fileMatches.prefix(6))) { f in
                    session.draft = Mention.insert(f, into: session.draft)
                }
            }
            // What to say, then one line: photos, the model (while it works:
            // steer or queue), the session's options in one menu, the
            // context, dictation, and send or stop.
            VStack(alignment: .leading, spacing: 6) {
                if !sendingPhotos.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        PhotoStrip(photos: .constant(sendingPhotos), sending: true)
                        Text(sendingPhotos.count == 1 ? "Sending 1 photo…" : "Sending \(sendingPhotos.count) photos…")
                            .font(.system(size: 12.5))
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 6)
                    }
                    .accessibilityElement(children: .combine)
                }
                if !photos.isEmpty { PhotoStrip(photos: $photos) }
                TextField(placeholder, text: $session.draft, axis: .vertical)
                    .lineLimit(1...6)
                    .font(.system(size: 16.5))
                    .focused($focused)
                    .padding(.horizontal, 6)
                    .padding(.top, 4)
                HStack(spacing: 6) {
                    PhotoButton(photos: $photos, size: 34)
                    if session.busy { steerOrQueue } else { modelChip }
                    options
                    Spacer(minLength: 0)
                    context
                    MicButton(text: $session.draft, size: 34)
                    sendOrStop
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .card(p, radius: 22)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        // `@`: the session folder's files as you type.
        .task(id: mention) {
            guard let q = mention else {
                fileMatches = []
                return
            }
            try? await Task.sleep(nanoseconds: 150_000_000)
            guard !Task.isCancelled else { return }
            let found = await model.files(matching: q)
            if !Task.isCancelled { fileMatches = found }
        }
        // The thinking levels its model has (for the options menu).
        .task(id: session.liveKey + "|" + session.info.modelId) {
            guard !session.liveKey.isEmpty, !session.info.thinking.isEmpty else {
                levels = []
                return
            }
            levels = await model.thinkingLevels()
        }
    }

    /// While it works: a message steers the run, or waits for it to end.
    var steerOrQueue: some View {
        HStack(spacing: 0) {
            ForEach([true, false], id: \.self) { steer in
                Button {
                    UISelectionFeedbackGenerator().selectionChanged()
                    session.steer = steer
                } label: {
                    Text(steer ? "Steer" : "Queue")
                        .font(.system(size: 13, weight: .semibold))
                        .padding(.horizontal, 10)
                        .frame(height: 32)
                        .background(Color(session.steer == steer ? p.primary : p.subtle))
                        .foregroundStyle(Color(session.steer == steer ? p.onPrimary : p.muted))
                }
                .buttonStyle(.plain)
            }
        }
        .clipShape(Capsule())
        .overlay(Capsule().strokeBorder(Color(p.border), lineWidth: 1))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(session.steer ? "Steer the run" : "Queue for after the run")
    }

    @ViewBuilder var sendOrStop: some View {
        if session.busy && text.isEmpty && photos.isEmpty {
            Button {
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                Task { await model.stop() }
            } label: {
                Image(systemName: "stop.fill")
                    .font(.system(size: 13, weight: .bold))
                    .frame(width: 36, height: 36)
                    .background(Circle().fill(Color(p.error)))
                    .foregroundStyle(Color.white)
            }
            .accessibilityLabel("Stop")
        } else {
            Button(action: send) {
                Image(systemName: "arrow.up")
                    .font(.system(size: 16, weight: .bold))
                    .frame(width: 36, height: 36)
                    .background(Circle().fill(Color(canSend ? p.primary : p.subtle)))
                    .foregroundStyle(Color(canSend ? p.onPrimary : p.muted))
            }
            .disabled(!canSend)
            .accessibilityLabel("Send")
        }
    }

    /// The model: its agent's tile and name; tap to pick another.
    var modelChip: some View {
        Button { openSheet(.models) } label: {
            HStack(spacing: 6) {
                AgentTile(harness: session.harness, size: 20)
                Text(session.model.isEmpty ? "Model" : session.model)
                    .font(.system(size: 13.5, weight: .medium))
                    .lineLimit(1)
                    .frame(maxWidth: 120, alignment: .leading)
                    .fixedSize(horizontal: true, vertical: false)
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold)).foregroundStyle(Color(p.muted))
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .frame(height: 34)
            .background(Capsule().fill(Color(p.raised.mix(p.foreground, 0.04))))
            .overlay(Capsule().strokeBorder(Color(p.hairline), lineWidth: 1))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Model: \(session.model.isEmpty ? "default" : session.model)")
    }

    /// The thinking level, approval mode and Plan, in one menu: its icon
    /// lights up while Plan is on or nothing asks first.
    @ViewBuilder var options: some View {
        let m = session.modes
        let approvals = m?.approvals ?? []
        if !levels.isEmpty || !approvals.isEmpty {
            let plan = m?.plan == true, yolo = m?.approval == "yolo"
            Menu {
                if !levels.isEmpty {
                    Picker(selection: Binding(get: { session.info.thinking }, set: { l in Task { await model.setThinking(l) } })) {
                        ForEach(levels, id: \.self) { Text($0.capitalized).tag($0) }
                    } label: {
                        Label("Thinking: \(session.info.thinking.capitalized)", systemImage: "brain")
                    }
                    .pickerStyle(.menu)
                }
                if let m, !approvals.isEmpty {
                    Picker(selection: Binding(get: { m.approval }, set: { a in Task { await model.setApproval(a) } })) {
                        ForEach(approvals, id: \.self) { Text(Modes.label($0)).tag($0) }
                    } label: {
                        Label("Approvals: \(Modes.short(m.approval))", systemImage: yolo ? "bolt.shield" : "shield")
                    }
                    .pickerStyle(.menu)
                    Toggle(isOn: Binding(get: { m.plan }, set: { on in Task { await model.setPlan(on) } })) {
                        Label("Plan first", systemImage: "list.bullet.clipboard")
                    }
                }
            } label: {
                Image(systemName: plan ? "list.bullet.clipboard" : yolo ? "bolt.shield" : "slider.horizontal.3")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 34, height: 34)
                    .background(Circle().fill(Color(plan || yolo ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))))
                    .overlay(Circle().strokeBorder(Color(plan || yolo ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1))
                    .foregroundStyle(Color(plan || yolo ? p.primary : p.muted))
            }
            .accessibilityLabel("Options: thinking \(session.info.thinking), approvals \(Modes.label(m?.approval ?? "")), Plan \(plan ? "on" : "off")")
        }
    }

    /// How full the context is, as a ring (amber near where it compacts);
    /// tap for the numbers.
    @ViewBuilder var context: some View {
        let i = session.info
        if i.contextWindow > 0 {
            let near = !i.compactNote.isEmpty || i.contextUsed >= 0.9
            Menu {
                Text("Context: \(Style.tokens(i.contextTokens)) of \(Style.tokens(i.contextWindow)) (\(Int(i.contextUsed * 100))%)")
                if i.compactAt > 0 { Text("Compacts at \(Style.tokens(i.compactAt))") }
                if i.cost > 0 { Text(String(format: "$%.2f so far", i.cost)) }
            } label: {
                // (The number in it: a gauge, not a spinner.)
                ZStack {
                    Circle().stroke(Color(p.muted).opacity(0.3), lineWidth: 2.5)
                    Circle().trim(from: 0, to: max(0.02, i.contextUsed))
                        .stroke(Color(near ? p.warning : p.primary), style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                    Text("\(Int(i.contextUsed * 100))")
                        .font(.system(size: 9.5, weight: .semibold))
                        .monospacedDigit()
                        .foregroundStyle(Color(near ? p.warning : p.muted))
                }
                .frame(width: 28, height: 28)
                .frame(width: 32, height: 34)
                .contentShape(Rectangle())
            }
            .accessibilityLabel("Context \(Int(i.contextUsed * 100))%, \(Style.tokens(i.contextTokens)) of \(Style.tokens(i.contextWindow))"
                + (i.compactAt > 0 ? ", compacts at \(Style.tokens(i.compactAt))" : ""))
        }
    }

    func send() {
        let (t, ph) = (text, photos)
        session.draft = ""
        photos = []
        sendingPhotos = ph
        onSend()
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        Task {
            let sent = await model.send(t, photos: ph)
            sendingPhotos = []
            if sent {
                // On the lock screen while it works.
                LiveActivities.start(for: session, model: model)
            } else {
                session.draft = t
                photos = ph
            }
        }
    }
}

/// Files matching an `@` mention; tapping one puts it in the message.
struct FileList: View {
    @Environment(\.palette) private var p
    let files: [String]
    var pick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(files, id: \.self) { f in
                Button { pick(f) } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "doc.text").font(.system(size: 12)).foregroundStyle(Color(p.muted))
                        Text(f.split(separator: "/").last.map(String.init) ?? f).font(Style.mono(13.5, .semibold)).lineLimit(1)
                        if f.contains("/") {
                            Text(f.split(separator: "/").dropLast().joined(separator: "/"))
                                .font(Style.mono(11.5))
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(1)
                                .truncationMode(.head)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if f != files.last { Divider().overlay(Color(p.border)) }
            }
        }
        .card(p, radius: 12)
    }
}

/// Slash commands matching what's typed.
struct SlashList: View {
    @Environment(\.palette) private var p
    let commands: [SlashCommand]
    var pick: (SlashCommand) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(commands) { c in
                Button { pick(c) } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text("/" + c.name).font(Style.mono(13.5, .semibold))
                            if !c.hint.isEmpty { Text(c.hint).font(Style.mono(11.5)).foregroundStyle(Color(p.muted)) }
                        }
                        if !c.description.isEmpty {
                            Text(c.description).font(.system(size: 12.5)).foregroundStyle(Color(p.muted)).lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if c.id != commands.last?.id { Divider().overlay(Color(p.border)) }
            }
        }
        .card(p, radius: 12)
    }
}

/// While it works or waits for you: what it's doing, for how long.
struct ActivityLine: View {
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        if let a = session.activity {
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                HStack(spacing: 8) {
                    StatusDot(status: session.requests.isEmpty ? "busy" : "input")
                    Text(a.text)
                        .font(.system(size: 13.5))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if a.since > 0 {
                        Text(Style.elapsed(since: a.since, now: ctx.date))
                            .font(.system(size: 13.5))
                            .foregroundStyle(Color(p.muted))
                            .monospacedDigit()
                    }
                    Spacer(minLength: 0)
                }
                .foregroundStyle(Color(session.requests.isEmpty ? p.foreground : p.warning))
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }
}
