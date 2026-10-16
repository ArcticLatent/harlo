import HarloKit
import SwiftUI

/// Your sessions on cards: the recent ones (or all of them, grouped by
/// project or by day, with a search), each with its agent, where it runs
/// and its state. Swipe or long-press a card to rename, pin, mark unread or
/// archive it.
struct DashboardView: View {
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

    struct SessionGroup: Identifiable {
        let id: String
        var sessions: [SessionSummary]
    }

    var groups: [SessionGroup] {
        if !all {
            let busy = model.sessions.filter { ["busy", "starting", "input"].contains($0.status) }
            let rest = model.sessions.filter { !["busy", "starting", "input"].contains($0.status) }.sorted { $0.modified > $1.modified }
            return [SessionGroup(id: "", sessions: Array((busy + rest).prefix(max(Self.recentCount, busy.count))))]
        }
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        let found = model.sessions.filter { s in q.isEmpty || "\(s.title) \(s.project) \(s.branch) \(s.location)".lowercased().contains(q) }
        if groupBy == "day" {
            return SessionSummary.byDay(found).map { SessionGroup(id: $0.name, sessions: $0.sessions) }
        }
        var out: [SessionGroup] = []
        for s in found {
            let name = s.project.isEmpty ? "Other sessions" : s.project
            if let i = out.firstIndex(where: { $0.id == name }) { out[i].sessions.append(s) } else { out.append(SessionGroup(id: name, sessions: [s])) }
        }
        return out
    }

    var body: some View {
        List {
            VStack(alignment: .leading, spacing: 22) {
                AppHeader { tab = .approvals }
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
                if !g.id.isEmpty {
                    SectionLabel(text: g.id)
                        .plainRow(top: 12, bottom: 2)
                }
                ForEach(g.sessions) { s in
                    card(s)
                }
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

    func card(_ s: SessionSummary) -> some View {
        Button {
            model.open(s)
        } label: {
            SessionCard(session: s, unread: model.isUnread(s), showProject: !all || groupBy == "day")
        }
        .buttonStyle(.plain)
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
    @State private var now = Date()

    var detail: String {
        var parts = [Style.harnessLabel(session.harness)]
        if showProject && !session.project.isEmpty { parts.append(session.project) }
        if !session.branch.isEmpty { parts.append(session.branch) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 14) {
            AgentTile(harness: session.harness, size: 46)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
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
