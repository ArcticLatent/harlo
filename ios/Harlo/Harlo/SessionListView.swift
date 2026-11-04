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
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        let found = model.sessions.filter { s in
            q.isEmpty ? !h.folded(s) : "\(s.title) \(s.project) \(s.branch) \(s.location) \(s.host)".lowercased().contains(q)
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
        Button {
            withAnimation(.easeOut(duration: 0.2)) {
                if openShelves.contains(g.id) { openShelves.remove(g.id) } else { openShelves.insert(g.id) }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "checkmark.seal")
                    .font(.system(size: 13))
                Text("Settled · \(g.settled.count)")
                    .font(.system(size: 14.5, weight: .medium))
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
                    .rotationEffect(.degrees(open ? 90 : 0))
                Spacer()
            }
            .foregroundStyle(Color(p.muted))
            .padding(.horizontal, 6)
            .frame(height: 34)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Settled sessions, \(g.settled.count)")
        .accessibilityValue(open ? "Shown" : "Hidden")
        .plainRow(top: 2, bottom: 2)
        if open {
            ForEach(g.settled) { s in entry(s) }
        }
    }

    /// A session's card, then its helpers (folded, behind a line to open).
    @ViewBuilder
    func entry(_ s: SessionSummary) -> some View {
        card(s)
        if folding, let kids = helpers.helpers[s.key] {
            helperLine(s, kids.map(\.session))
            if openHelpers.contains(s.key) {
                ForEach(kids) { k in card(k.session, depth: k.depth) }
            }
        }
    }

    /// "Helpers · 3", and whether one works or waits for you: opens them.
    func helperLine(_ s: SessionSummary, _ kids: [SessionSummary]) -> some View {
        let open = openHelpers.contains(s.key)
        let waiting = kids.contains { $0.status == "input" }
        let working = kids.contains { ["busy", "starting"].contains($0.status) }
        return Button {
            withAnimation(.easeOut(duration: 0.2)) {
                if open { openHelpers.remove(s.key) } else { openHelpers.insert(s.key) }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.system(size: 12))
                Text("Helpers · \(kids.count)")
                    .font(.system(size: 14.5, weight: .medium))
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
                    .rotationEffect(.degrees(open ? 90 : 0))
                Spacer()
                if waiting {
                    Text("Waiting for you")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Color(p.warning))
                } else if working {
                    Text("Working")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Color(p.primary))
                }
            }
            .foregroundStyle(Color(p.muted))
            .padding(.leading, 26)
            .padding(.trailing, 6)
            .frame(height: 30)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Helpers of \(s.title), \(kids.count)")
        .accessibilityValue(open ? "Shown" : "Hidden")
        .plainRow(top: 0, bottom: 2)
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
                    .font(.system(size: 21, weight: .semibold))
                    .plainRow(top: 14, bottom: 4)
                PlanLimitsCard(limits: model.limits)
                    .plainRow()
            }
            HStack {
                Text(all ? "All sessions" : "Recent sessions")
                    .font(.system(size: 21, weight: .semibold))
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
                        .font(.system(size: 15, weight: .medium))
                        .padding(.horizontal, 14)
                        .frame(height: 38)
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
        .toolbar(.hidden, for: .navigationBar)
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
        .alert("Rename session", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") {
                if let s = renaming { Task { await model.rename(s, to: newName) } }
            }
        }
    }

    /// A session's card; a helper's (`depth` > 0) under the session that
    /// started it, set in.
    func card(_ s: SessionSummary, depth: Int = 0) -> some View {
        Button {
            model.open(s)
        } label: {
            SessionCard(session: s, unread: model.isUnread(s), showProject: !all || groupBy == "day", nested: depth > 0)
        }
        .buttonStyle(.plain)
        .padding(.leading, CGFloat(min(depth, 3)) * 20)
        .plainRow(top: 5, bottom: 5)
        .swipeActions(edge: .trailing) {
            Button {
                Task { await model.archive(s) }
            } label: {
                Label("Archive", systemImage: "archivebox")
            }
            .tint(Color(p.muted))
        }
        .swipeActions(edge: .leading) {
            Button {
                Task { await model.pin(s, !s.pinned) }
            } label: {
                Label(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin")
            }
            .tint(Color(p.primary))
            Button {
                model.markUnread(s, !model.isUnread(s))
            } label: {
                Label(model.isUnread(s) ? "Read" : "Unread", systemImage: model.isUnread(s) ? "envelope.open" : "envelope.badge")
            }
            .tint(Color(p.info))
        }
        .contextMenu {
            Button {
                newName = s.title
                renaming = s
            } label: {
                Label("Rename", systemImage: "pencil")
            }
            Button {
                Task { await model.pin(s, !s.pinned) }
            } label: {
                Label(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin")
            }
            Button {
                model.markUnread(s, !model.isUnread(s))
            } label: {
                Label(model.isUnread(s) ? "Mark as read" : "Mark as unread", systemImage: model.isUnread(s) ? "envelope.open" : "envelope.badge")
            }
            if !s.settled.isEmpty && model.canPutBack {
                Button {
                    Task { await model.putBack(s) }
                } label: {
                    Label("Put back", systemImage: "arrow.uturn.backward")
                }
            }
            Button {
                Task { await model.archive(s) }
            } label: {
                Label("Archive", systemImage: "archivebox")
            }
        }
    }
}

extension View {
    /// A list row with no background, separator or default insets (the
    /// dashboard's and Approvals' cards).
    func plainRow(top: CGFloat = 4, bottom: CGFloat = 4) -> some View {
        listRowInsets(EdgeInsets(top: top, leading: 16, bottom: bottom, trailing: 16))
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
    }
}

/// A session on a card: its agent's tile, its name, where it runs, and its
/// state (a pill), or when it was last used.
struct SessionCard: View {
    @Environment(\.palette) private var p
    let session: SessionSummary
    /// Finished since you looked, or marked unread.
    var unread = false
    /// Its project shows (the recent ones, or grouped by day).
    var showProject = false
    /// A helper shown under the session that started it.
    var nested = false
    @State private var now = Date()

    var detail: String {
        var parts = [Style.harnessLabel(session.harness)]
        if !nested && !session.helperOf.isEmpty { parts.append("Helper of “\(session.helperOf)”") }
        if showProject && !session.project.isEmpty { parts.append(session.project) }
        // (Grouped by project, the group says its machine.)
        if showProject && !session.host.isEmpty { parts.append(session.host) }
        if !session.branch.isEmpty { parts.append(session.branch) }
        if !session.settled.isEmpty { parts.append(session.settled) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 14) {
            AgentTile(harness: session.harness, size: 46)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    if nested {
                        Image(systemName: "arrow.turn.down.right")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(Color(p.muted))
                            .accessibilityLabel("Helper of \(session.helperOf)")
                    }
                    if session.pinned {
                        Image(systemName: "pin.fill")
                            .font(.system(size: 10))
                            .foregroundStyle(Color(p.muted))
                    }
                    Text(session.title.isEmpty ? "New session" : session.title)
                        .font(.system(size: 17, weight: unread ? .semibold : .medium))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                }
                Text(detail)
                    .font(.system(size: 14.5))
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
            }
            Spacer(minLength: 6)
            if StatusPill.shows(session.status, unread: unread) {
                StatusPill(status: session.status, unread: unread)
            } else {
                Text(Style.ago(session.modified, now: now))
                    .font(.system(size: 13.5))
                    .foregroundStyle(Color(p.faint))
            }
            Image(systemName: "chevron.right")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Color(p.muted))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 14)
        .card(p, radius: 18)
        .contentShape(RoundedRectangle(cornerRadius: 18))
        .accessibilityElement(children: .combine)
    }
}

/// In the demo: what it is, and the way out to pairing.
struct DemoBanner: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "sparkles")
                .font(.system(size: 18))
                .foregroundStyle(Color(p.primary))
            VStack(alignment: .leading, spacing: 4) {
                Text("This is the demo")
                    .font(.system(size: 16, weight: .semibold))
                Text("Sample sessions, nothing connected. Install harlo on your computer and pair this phone to work with your own agents.")
                    .font(.system(size: 14))
                    .foregroundStyle(Color(p.muted))
                    .fixedSize(horizontal: false, vertical: true)
                Button("Pair with harlo") { model.endDemo() }
                    .font(.system(size: 15, weight: .medium))
                    .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .card(p, radius: 16, fill: p.accentSoft.over(p.raised))
    }
}

/// Why harlo isn't connected, with a retry.
struct ConnectionBanner: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p

    var body: some View {
        HStack(spacing: 10) {
            switch model.state {
            case .connecting, .connected:
                ProgressView()
                Text("Connecting to \(model.host?.name ?? "harlo")…")
                    .foregroundStyle(Color(p.muted))
            case .waiting(let why):
                Image(systemName: "wifi.exclamationmark")
                    .foregroundStyle(Color(p.warning))
                VStack(alignment: .leading, spacing: 2) {
                    Text("Can't reach \(model.host?.name ?? "harlo")")
                    Text(why.isEmpty ? "Trying again…" : why + " Trying again…")
                        .font(.caption)
                        .foregroundStyle(Color(p.muted))
                }
                Spacer()
                Button("Retry") { Task { await model.reconnect() } }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            case .unpaired:
                EmptyView()
            }
        }
        .font(.callout)
    }
}
