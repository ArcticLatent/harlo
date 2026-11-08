import Foundation

/// The plan limits widget's accounts: the app writes them as harlo's plan
/// limits change, and the widget shows them (what harlo last said; a window
/// whose reset time has passed shows as reset). It keeps them in the
/// Keychain group the app and its widgets share.
public struct LimitsSnapshot: Codable, Equatable, Sendable {
    public struct Account: Codable, Equatable, Sendable, Identifiable {
        /// "Claude", "ChatGPT", …
        public var name: String
        /// "max", "plus", … ("" not known).
        public var plan: String
        /// The machine it's signed in on ("" harlo's computer).
        public var host: String
        public var windows: [Window]
        public var id: String { host + "|" + name + "|" + plan }

        public init(name: String, plan: String = "", host: String = "", windows: [Window]) {
            self.name = name
            self.plan = plan
            self.host = host
            self.windows = windows
        }

        /// Its fullest window (the one the small widget shows).
        public var fullest: Window? { windows.max { $0.usedFraction < $1.usedFraction } }
    }

    public struct Window: Codable, Equatable, Sendable, Identifiable {
        /// "5-hour", "Weekly", "Weekly Opus".
        public var label: String
        /// 0…1 (more when it's over).
        public var usedFraction: Double
        /// When it resets (unix ms), 0 unknown.
        public var resetsAt: Int64
        public var usedUp: Bool
        public var id: String { label }

        public init(label: String, usedFraction: Double, resetsAt: Int64 = 0, usedUp: Bool = false) {
            self.label = label
            self.usedFraction = usedFraction
            self.resetsAt = resetsAt
            self.usedUp = usedUp
        }

        /// From 90%, harlo warns.
        public var nearly: Bool { usedUp || usedFraction >= PlanLimits.nearly }
        /// "73%", "100%".
        public var percent: String { "\(Int((min(usedFraction, 9.99) * 100).rounded()))%" }
    }

    /// Paired with harlo.
    public var paired: Bool
    public var accounts: [Account]
    /// When harlo's numbers were taken.
    public var updated: Date

    static let storeName = "widget-limits"

    public init(paired: Bool, accounts: [Account], updated: Date = Date()) {
        self.paired = paired
        self.accounts = accounts
        self.updated = updated
    }

    /// Not paired yet.
    public static let unpaired = LimitsSnapshot(paired: false, accounts: [])

    /// From harlo's plan limits: harlo's computer first, then each machine
    /// (an account signed in on several shows once, as the first has it).
    public static func of(_ limits: PlanLimits, now: Date = Date()) -> LimitsSnapshot {
        var seen = Set<String>()
        var accounts: [Account] = []
        for m in limits.machines {
            for a in m.accounts where !a.windows.isEmpty {
                let same = a.provider + "|" + a.account
                if !a.account.isEmpty && !seen.insert(same).inserted { continue }
                accounts.append(Account(name: a.name, plan: a.plan, host: m.host, windows: a.windows.map {
                    Window(label: shortLabel($0.label), usedFraction: $0.usedFraction, resetsAt: $0.resetsAt, usedUp: $0.usedUp)
                }))
            }
        }
        return LimitsSnapshot(paired: true, accounts: accounts, updated: now)
    }

    /// "Session (5 h)" → "5-hour", "Weekly · Opus" → "Weekly Opus".
    static func shortLabel(_ label: String) -> String {
        switch label {
        case "Session (5 h)": return "5-hour"
        case let l where l.hasPrefix("Weekly · "): return "Weekly " + l.dropFirst("Weekly · ".count)
        default: return label
        }
    }

    /// As it is at `date`: windows whose reset time has passed are reset
    /// (nothing used; when they reset next isn't known).
    public func at(_ date: Date) -> LimitsSnapshot {
        let ms = Int64(date.timeIntervalSince1970 * 1000)
        var s = self
        for a in s.accounts.indices {
            for w in s.accounts[a].windows.indices where s.accounts[a].windows[w].resetsAt > 0 && s.accounts[a].windows[w].resetsAt <= ms {
                s.accounts[a].windows[w].usedFraction = 0
                s.accounts[a].windows[w].usedUp = false
                s.accounts[a].windows[w].resetsAt = 0
            }
        }
        return s
    }

    /// The times a window resets after `date`, soonest first (the widget's
    /// timeline shows each).
    public func resets(after date: Date, within: TimeInterval = 24 * 3600) -> [Date] {
        let times = accounts.flatMap(\.windows).map(\.resetsAt).filter { $0 > 0 }
            .map { Date(timeIntervalSince1970: TimeInterval($0) / 1000) }
            .filter { $0 > date && $0.timeIntervalSince(date) <= within }
        return Array(Set(times)).sorted()
    }

    public static func load(_ store: SecretStore) -> LimitsSnapshot? {
        store.load(storeName).flatMap { try? JSONDecoder().decode(LimitsSnapshot.self, from: $0) }
    }

    public func save(_ store: SecretStore) {
        store.save(Self.storeName, try? JSONEncoder().encode(self))
    }

    /// The same accounts and numbers (`updated` aside): no write needed.
    public func sameAs(_ other: LimitsSnapshot?) -> Bool {
        guard let other else { return false }
        return paired == other.paired && accounts == other.accounts
    }

    /// Sample accounts (the widget gallery, previews).
    public static func sample(now: Date = Date()) -> LimitsSnapshot {
        of(HarloModel.sampleLimits(now: now), now: now)
    }
}

extension HarloModel {
    /// Writes the plan limits widget's accounts when they changed (not for
    /// the demo or the screenshots' samples).
    func writeLimitsWidget() {
        guard !demo, samples == nil else { return }
        let snap = host == nil ? .unpaired : LimitsSnapshot.of(limits)
        if snap.sameAs(lastLimitsWidget) { return }
        lastLimitsWidget = snap
        snap.save(store)
        limitsWidgetChanged?()
    }
}
