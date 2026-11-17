import HarloKit
import SwiftUI

/// Your sessions on cards: the recent ones (or all of them, grouped by
/// project or by day, with a search), each with its agent, where it runs
/// and its state. Swipe or long-press a card to rename, pin, mark unread or
/// archive it.
struct DashboardView: View {
    /// The plan limits card, from the header's gauge (off at first).
    @AppStorage(DashboardView.showLimitsKey) private var showLimits = false
    static let showLimitsKey = "dashboard.showLimits"
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Binding var tab: AppTab
    @State private var search = ""
    @State private var renaming: SessionSummary?
    @State private var newName = ""
    /// All sessions (grouped), or only the recent ones.
    @State private var all = false
    /// "project" (as harlo's sidebar) or "day".
    @AppStorage("groupBy") private var groupBy = "project"

    /// The recent ones: what works or waits first, then the newest.
    static let recentCount = 8

    /// The projects whose Settled shelf is open.
    @State private var openShelves: Set<String> = []
    /// The sessions whose helpers show (folded under them otherwise).
    @State private var openHelpers: Set<String> = []
    /// The workflow run shown in full.
    @State private var openRun: WorkflowRun?
    @State private var showRuns = false

    /// Runs under way or waiting for you (paused, or done with work not
    /// merged), at most three.
    var currentRuns: [WorkflowRun] {
        Array(model.workflowRuns.filter { $0.running || $0.paused || ($0.finishable && !$0.merged) }.prefix(3))
    }

    /// Helpers fold under the sessions that started them (not while
    /// searching: then every match lists).
    var folding: Bool { search.trimmingCharacters(in: .whitespaces).isEmpty }
    var helpers: SessionSummary.Helpers { SessionSummary.helpers(model.sessions) }

    struct SessionGroup: Identifiable {
        let id: String
        var name: String
        var sessions: [SessionSummary]
        /// On its Settled shelf (by project only).
        var settled: [SessionSummary] = []
    }

    var groups: [SessionGroup] {
        let working = ["busy", "starting", "input"]
        let h = helpers
        // (A session works while one of its helpers does.)
        let works = { (s: SessionSummary) in working.contains(h.status(s)) }
        if !all {
            // (Settled ones are done with, unless they work again; helpers
            // show under their sessions.)
            let shown = model.sessions.filter { ($0.settled.isEmpty || works($0)) && !h.folded($0) }
            let busy = shown.filter(works)
            let rest = shown.filter { !works($0) }.sorted { $0.modified > $1.modified }
            return [SessionGroup(id: "", name: "", sessions: Array((busy + rest).prefix(max(Self.recentCount, busy.count))))]
        }
        let q = search.trimmingCharacters(in: .whitespaces)
        let found = model.sessions.filter { s in
            q.isEmpty ? !h.folded(s) : "\(s.title) \(s.project) \(s.branch) \(s.location) \(s.host)".localizedStandardContains(q)
        }
        if groupBy == "day" {
            return SessionSummary.byDay(found).map { SessionGroup(id: $0.name, name: $0.name, sessions: $0.sessions) }
        }
        // By project, as harlo's sidebar: each on its machine, settled ones
        // on a shelf at the end (open while searching).
        var out: [SessionGroup] = []
        for s in found {
            let id = s.host + "/" + s.project
            if out.firstIndex(where: { $0.id == id }) == nil {
                let name = s.project.isEmpty ? "Other sessions" : s.project
                out.append(SessionGroup(id: id, name: s.host.isEmpty ? name : "\(name) · \(s.host)", sessions: []))
            }
            let i = out.firstIndex { $0.id == id }!
            if s.settled.isEmpty || works(s) { out[i].sessions.append(s) } else { out[i].settled.append(s) }
        }
        return out
    }

    func shelfOpen(_ g: SessionGroup) -> Bool {
        openShelves.contains(g.id) || !search.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// A project's Settled shelf: its sessions when it's open.
    @ViewBuilder
    func shelf(_ g: SessionGroup) -> some View {
        let open = shelfOpen(g)
        FoldLine(icon: "checkmark.seal", title: "Settled · \(g.settled.count)", open: open,
                 accessibilityTitle: "Settled sessions, \(g.settled.count)") {
            if openShelves.contains(g.id) { openShelves.remove(g.id) } else { openShelves.insert(g.id) }
        }
        if open {
            ForEach(g.settled) { s in entry(s) }
        }
    }

    /// A session's card, then its helpers (folded, behind a line to open).
    @ViewBuilder
    func entry(_ s: SessionSummary) -> some View {
        card(s)
        if folding, let kids = helpers.helpers[s.key] {
            let open = openHelpers.contains(s.key)
            FoldLine(icon: "arrow.triangle.branch", title: "Helpers · \(kids.count)", open: open, nested: true,
                     waiting: kids.contains { $0.session.status == "input" },
                     working: kids.contains { ["busy", "starting"].contains($0.session.status) },
                     accessibilityTitle: "Helpers of \(s.title), \(kids.count)") {
                if open { openHelpers.remove(s.key) } else { openHelpers.insert(s.key) }
            }
            if open {
                ForEach(kids) { k in card(k.session, depth: k.depth) }
            }
        }
    }

    func card(_ s: SessionSummary, depth: Int = 0) -> DashboardRow {
        DashboardRow(session: s, depth: depth, showProject: !all || groupBy == "day") { s in
            newName = s.title
            renaming = s
        }
    }

    var body: some View {
        List {
            VStack(alignment: .leading, spacing: 22) {
                AppHeader(bell: { tab = .approvals }, limits: $showLimits)
                BigTitle(text: "Dashboard")
            }
            .plainRow(top: 8, bottom: 6)
            if model.demo {
                DemoBanner()
                    .plainRow()
            } else if model.state != .connected {
                ConnectionBanner()
                    .padding(14)
                    .card(p, radius: 16)
                    .plainRow()
            }
            // Plan limits (the header's gauge shows them): before the sessions.
            if showLimits && !all && !model.limits.isEmpty {
                Text("Plan limits")
                    .scaledFont(21, .semibold)
                    .plainRow(top: 14, bottom: 4)
                PlanLimitsCard(limits: model.limits)
                    .plainRow()
            }
            if model.hasWorkflows && !all && !model.workflowRuns.isEmpty {
                HStack {
                    Text("Workflow runs")
                        .scaledFont(21, .semibold)
                    Spacer()
                    // (A button, not a NavigationLink: in a List row a link
                    // takes the whole row and adds its own chevron.)
                    Button {
                        showRuns = true
                    } label: {
                        Text("View All")
                            .scaledFont(15, .medium)
                            .padding(.horizontal, 14)
                            .frame(minHeight: 38)
                            .card(p, radius: 12)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("All workflow runs")
                }
                .plainRow(top: 14, bottom: 4)
                if currentRuns.isEmpty {
                    Text("None under way.")
                        .scaledFont(14)
                        .foregroundStyle(Color(p.muted))
                        .plainRow()
                }
                ForEach(currentRuns) { run in
                    Button { openRun = run } label: { WorkflowRunRow(run: run) }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("workflowRun")
                        .plainRow()
                }
            }
            HStack {
                Text(all ? "All sessions" : "Recent sessions")
                    .scaledFont(21, .semibold)
                Spacer()
                if all {
                    Menu {
                        Picker("Group by", selection: $groupBy) {
                            Label("Project", systemImage: "folder").tag("project")
                            Label("Day", systemImage: "calendar").tag("day")
                        }
                    } label: {
                        Image(systemName: "line.3.horizontal.decrease")
                            .font(.system(size: 15, weight: .medium))
                            .frame(width: 38, height: 38)
                            .card(p, radius: 12)
                    }
                    .accessibilityLabel("Group sessions")
                }
                Button {
                    withAnimation(.easeOut(duration: 0.2)) {
                        all.toggle()
                        if !all { search = "" }
                    }
                } label: {
                    Text(all ? "Recent" : "View All")
                        .scaledFont(15, .medium)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 38)
                        .card(p, radius: 12)
                }
                .buttonStyle(.plain)
            }
            .plainRow(top: 14, bottom: 4)
            if all {
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass").foregroundStyle(Color(p.muted))
                    TextField("Search sessions", text: $search)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    if !search.isEmpty {
                        Button { search = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Color(p.muted)) }
                            .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 12)
                .frame(height: 42)
                .card(p, radius: 12)
                .plainRow(bottom: 4)
            }
            ForEach(groups) { g in
                if !g.name.isEmpty {
                    SectionLabel(text: g.name)
                        .plainRow(top: 12, bottom: 2)
                }
                ForEach(g.sessions) { s in
                    entry(s)
                }
                if !g.settled.isEmpty { shelf(g) }
            }
            if all && !search.isEmpty && groups.isEmpty {
                ContentUnavailableView.search(text: search)
                    .plainRow()
            }
        }
        .listStyle(.plain)
        .screen()
        .toolbarVisibility(.hidden, for: .navigationBar)
        .overlay {
            if model.state == .connected && model.sessions.isEmpty {
                ContentUnavailableView {
                    Label("No sessions yet", systemImage: "bubble.left.and.bubble.right")
                } description: {
                    Text("Start one here, or in harlo on your computer.")
                } actions: {
                    Button("New session") { tab = .newSession }
                        .buttonStyle(.borderedProminent)
                        .foregroundStyle(Color(p.onPrimary))
                }
            }
        }
        .refreshable { await model.reconnect() }
        // (Its runs, kept up while it's on screen.)
        .task(id: model.state == .connected && model.hasWorkflows) { await model.loadWorkflows() }
        .onAppear { model.watchRuns(true) }
        .onDisappear { model.watchRuns(false) }
        .sheet(item: $openRun) { run in WorkflowSheet(id: run.id) }
        .navigationDestination(isPresented: $showRuns) { WorkflowRunsView() }
        .alert("Rename session", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") {
                if let s = renaming { Task { await model.rename(s, to: newName) } }
            }
        }
    }
}
