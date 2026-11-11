import Foundation

/// One window of a plan's limits: "Session (5 h)", "Weekly", "Weekly · Opus".
public struct LimitWindow: Equatable, Sendable, Identifiable {
    public var label: String
    /// 0…1 (more when it's over).
    public var usedFraction: Double
    /// When it resets (unix ms), 0 unknown.
    public var resetsAt: Int64
    /// "ok" or "exhausted".
    public var status: String

    public var id: String { label }

    public init(label: String, usedFraction: Double, resetsAt: Int64 = 0, status: String = "ok") {
        self.label = label
        self.usedFraction = usedFraction
        self.resetsAt = resetsAt
        self.status = status
    }

    public init(_ v: JSON) {
        label = v["label"].text
        usedFraction = v["usedFraction"].double ?? 0
        resetsAt = v["resetsAt"].int ?? 0
        status = v["status"].string ?? "ok"
    }

    public var usedUp: Bool { status == "exhausted" || usedFraction >= 1 }
    /// From 90%, harlo warns.
    public var nearly: Bool { usedFraction >= PlanLimits.nearly || usedUp }

    /// "5-hour limit", "weekly limit", "weekly Opus limit".
    public var name: String {
        switch label {
        case "Session (5 h)": return "5-hour limit"
        case "Weekly": return "weekly limit"
        case let l where l.hasPrefix("Weekly · "): return "weekly \(l.dropFirst("Weekly · ".count)) limit"
        default: return "\(label) limit"
        }
    }
}

/// An account's plan limits (a Claude sign-in, a ChatGPT plan, …).
public struct LimitAccount: Equatable, Sendable, Identifiable {
    /// "Claude", "openai-codex", "anthropic", …
    public var provider: String
    public var plan: String
    public var account: String
    public var windows: [LimitWindow]

    public var id: String { provider + "|" + account }

    public init(provider: String, plan: String = "", account: String = "", windows: [LimitWindow]) {
        self.provider = provider
        self.plan = plan
        self.account = account
        self.windows = windows
    }

    public init(_ v: JSON) {
        provider = v["provider"].text
        plan = v["plan"].text
        account = v["account"].text
        windows = (v["windows"].array ?? []).map(LimitWindow.init)
    }

    /// Who it is, as people say it: "Claude", "ChatGPT".
    public var name: String {
        switch provider {
        case "Claude", "anthropic": return "Claude"
        case "openai-codex": return "ChatGPT"
        default: return provider
        }
    }
}

/// Plan limits on this computer and on each machine harlo is connected to
/// (harlo's Settings → Plan limits), as harlo sends them (`limits`).
public struct PlanLimits: Equatable, Sendable {
    public struct Machine: Equatable, Sendable, Identifiable {
        /// "" for the computer harlo runs on.
        public var host: String
        public var accounts: [LimitAccount]
        /// When harlo last heard (unix ms), 0 unknown.
        public var updatedAt: Int64
        public var id: String { host }
    }

    /// The computer harlo runs on first, then each machine by name.
    public var machines: [Machine]

    /// From this much of a window used, harlo warns.
    public static let nearly = 0.9

    public init(machines: [Machine] = []) {
        self.machines = machines
    }

    public init(_ v: JSON) {
        func machine(_ host: String, _ m: JSON) -> Machine {
            let accounts: [LimitAccount] = (m["accounts"].array ?? []).map(LimitAccount.init)
            return Machine(host: host, accounts: accounts, updatedAt: m["updatedAt"].int ?? 0)
        }
        let hosts: [(key: String, value: JSON)] = (v["hosts"].object ?? [:]).sorted { $0.key < $1.key }
        machines = [machine("", v["here"])] + hosts.map { machine($0.key, $0.value) }.filter { !$0.accounts.isEmpty }
    }

    /// Nothing to show.
    public var isEmpty: Bool { machines.allSatisfy { $0.accounts.isEmpty } }
}

/// A session's plan limit, as the line above harlo's composer says it
/// (`limitOf`): its fullest window of 90% or more, whether one is used up,
/// and when work can go on.
public struct SessionLimit: Equatable, Sendable {
    public var provider: String
    public var window: LimitWindow
    public var out: Bool
    /// When the last used-up window resets (unix ms), 0 when none is used up.
    public var resetAt: Int64

    public init?(_ v: JSON) {
        guard case .object = v else { return nil }
        provider = v["provider"].text
        window = LimitWindow(label: v["label"].text, usedFraction: v["usedFraction"].double ?? 0, resetsAt: v["resetsAt"].int ?? 0)
        out = v["out"].bool ?? false
        resetAt = v["resetAt"].int ?? 0
    }

    public init(provider: String, window: LimitWindow, out: Bool, resetAt: Int64) {
        self.provider = provider
        self.window = window
        self.out = out
        self.resetAt = resetAt
    }
}

/// When a limit resets, as harlo says it: "at 18:30", or "Thu 09:00" further
/// off than 20 hours.
public func resetTime(_ ms: Int64, now: Date = Date(), calendar: Calendar = .current) -> String {
    let at = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    let f = DateFormatter()
    f.calendar = calendar
    f.timeZone = calendar.timeZone
    f.locale = Locale(identifier: "en_US_POSIX")
    if at.timeIntervalSince(now) > 20 * 3600 {
        f.dateFormat = "EEE HH:mm"
        return f.string(from: at)
    }
    f.dateFormat = "HH:mm"
    return "at " + f.string(from: at)
}

extension SessionInfo {
    /// The line above the composer as the context nears where the agent
    /// compacts on its own ("" until 85% of the way there).
    public var compactNote: String {
        guard compactAt > 0, contextTokens > 0, Double(contextTokens) >= Double(compactAt) * 0.85 else { return "" }
        let left = compactAt - min(compactAt, contextTokens)
        if left == 0 { return "The context is at its limit: it compacts next" }
        return "\(left >= 1000 ? "\((left + 500) / 1000)k" : "\(left)") tokens left before the context compacts"
    }

    /// How far it is to where it compacts (or, not knowing, to the
    /// window's end), 0…1: the context bar's colour.
    public var contextWay: Double {
        compactMark > 0 ? min(1, contextUsed / compactMark) : contextUsed
    }

    /// Where it compacts, as a part of the window (0 unknown).
    public var compactMark: Double {
        contextWindow > 0 && compactAt > 0 ? min(1, Double(compactAt) / Double(contextWindow)) : 0
    }
}
