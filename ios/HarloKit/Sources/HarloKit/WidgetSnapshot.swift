import Foundation

/// The home screen widget's sessions: the app writes them as its list
/// changes, the notification extension as harlo's notifications arrive
/// (needs you, done) while the app is closed, and the widget shows them. It
/// keeps them in the Keychain group the three share.
public struct WidgetSnapshot: Codable, Equatable, Sendable {
    public struct Item: Codable, Equatable, Sendable, Identifiable {
        public var key: String
        public var title: String
        public var harness: String
        /// "busy", "input" (needs you), "unread" (done), "failed", or "".
        public var status: String
        /// Last active: unix seconds.
        public var modified: Int64
        public var id: String { key }

        public init(key: String, title: String, harness: String, status: String, modified: Int64) {
            self.key = key
            self.title = title
            self.harness = harness
            self.status = status
            self.modified = modified
        }
    }

    /// The computer's name ("" when not paired).
    public var host: String
    /// The sessions to show, in order (at most `shown`).
    public var items: [Item]
    /// How many work, and wait for you, of all.
    public var working: Int
    public var waiting: Int
    public var total: Int
    /// When it was written.
    public var updated: Date

    public static let shown = 8
    static let storeName = "widget"

    public init(host: String, items: [Item], working: Int, waiting: Int, total: Int, updated: Date = Date()) {
        self.host = host
        self.items = items
        self.working = working
        self.waiting = waiting
        self.total = total
        self.updated = updated
    }

    /// Not paired yet.
    public static let unpaired = WidgetSnapshot(host: "", items: [], working: 0, waiting: 0, total: 0)

    /// From the session list: what waits on you first, then what works,
    /// what finished unseen, then the most recent.
    public static func of(host: String, sessions: [SessionSummary], unread: (SessionSummary) -> Bool, now: Date = Date()) -> WidgetSnapshot {
        let items = sessions.map { s -> Item in
            let status = ["busy", "starting"].contains(s.status) ? "busy" : s.status == "input" || !s.requests.isEmpty ? "input" : unread(s) ? "unread" : ""
            return Item(key: s.key, title: s.title.isEmpty ? "New session" : s.title, harness: s.harness, status: status, modified: s.modified)
        }
        let rank = ["input": 0, "busy": 1, "unread": 2, "failed": 2]
        let ordered = items.enumerated().sorted { a, b in
            let (ra, rb) = (rank[a.element.status] ?? 3, rank[b.element.status] ?? 3)
            if ra != rb { return ra < rb }
            if a.element.modified != b.element.modified { return a.element.modified > b.element.modified }
            return a.offset < b.offset
        }.map(\.element)
        return WidgetSnapshot(host: host, items: Array(ordered.prefix(shown)), working: items.filter { $0.status == "busy" }.count,
                              waiting: items.filter { $0.status == "input" }.count, total: items.count, updated: now)
    }

    /// A notification while the app is closed: its session needs you, or
    /// finished (or failed); it moves to the front.
    public mutating func apply(_ push: PushContent, now: Date = Date()) {
        let status: String
        switch push.kind {
        case "ask": status = "input"
        case "done": status = "unread"
        case "failed": status = "failed"
        default: return
        }
        // (A pull request's checks: no session.)
        guard !push.session.isEmpty else { return }
        let t = Int64(now.timeIntervalSince1970)
        if let i = items.firstIndex(where: { $0.key == push.session }) {
            var it = items.remove(at: i)
            it.status = status
            it.modified = t
            if !push.title.isEmpty { it.title = push.title }
            items.insert(it, at: 0)
        } else {
            items.insert(Item(key: push.session, title: push.title.isEmpty ? "Session" : push.title, harness: "", status: status, modified: t), at: 0)
            total += 1
            items = Array(items.prefix(Self.shown))
        }
        working = items.filter { $0.status == "busy" }.count
        waiting = items.filter { $0.status == "input" }.count
        updated = now
    }

    public static func load(_ store: SecretStore) -> WidgetSnapshot? {
        store.load(storeName).flatMap { try? JSONDecoder().decode(WidgetSnapshot.self, from: $0) }
    }

    public func save(_ store: SecretStore) {
        store.save(Self.storeName, try? JSONEncoder().encode(self))
    }

    /// The same sessions as shown (`updated` aside): no write needed.
    public func sameAs(_ other: WidgetSnapshot?) -> Bool {
        guard let other else { return false }
        return host == other.host && items == other.items && working == other.working && waiting == other.waiting && total == other.total
    }

    /// Sample sessions (the widget gallery, previews, the screenshots).
    public static func sample(now: Date = Date()) -> WidgetSnapshot {
        let t = Int64(now.timeIntervalSince1970)
        return WidgetSnapshot(host: "archlinux", items: [
            Item(key: "s1", title: "Fix the session list", harness: "claude", status: "busy", modified: t - 30),
            Item(key: "s2", title: "Relay load test", harness: "omp", status: "busy", modified: t - 90),
            Item(key: "s3", title: "Phone photos", harness: "pi", status: "input", modified: t - 300),
            Item(key: "s4", title: "Release notes", harness: "opencode", status: "unread", modified: t - 1200),
            Item(key: "s5", title: "Tidy the README", harness: "claude", status: "", modified: t - 7200),
            Item(key: "s6", title: "Benchmark markdown", harness: "omp", status: "", modified: t - 86400),
        ], working: 2, waiting: 1, total: 6, updated: now)
    }
}

extension HarloModel {
    /// Writes the widget's sessions when what it shows changed (not for
    /// the demo or the screenshots' samples).
    func writeWidget() {
        guard !demo, samples == nil else { return }
        let snap = host.map { h in WidgetSnapshot.of(host: h.name, sessions: sessions, unread: { self.isUnread($0) }) } ?? .unpaired
        if snap.sameAs(lastWidget) { return }
        lastWidget = snap
        snap.save(store)
        widgetChanged?()
    }
}
