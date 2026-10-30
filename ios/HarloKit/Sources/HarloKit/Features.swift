import Foundation

// What the phone can do beyond reading and answering: start sessions,
// switch their model and modes, send photos, see changed files, and the
// desktop's own colours.

/// A project in harlo (Settings → Projects).
public struct Project: Equatable, Sendable, Identifiable {
    public var id: Int64
    public var name: String
    /// Its folder (`~/…`).
    public var location: String
    /// The agent its new sessions use.
    public var agent: String
    public var icon: String
    /// A theme colour's key (`keyword`, `string`…), or "".
    public var color: String
    /// The shield's mode each agent starts in here ("" none).
    public var approvals: [String: String]
    /// The machine it's on: "" this computer, else a remote host's name.
    public var host: String
    /// The agents a session here can use (that machine's; empty: unknown).
    public var agents: [String]

    public init(_ v: JSON) {
        id = v["id"].int ?? -1
        name = v["name"].text
        location = v["where"].text
        agent = v["agent"].text
        icon = v["icon"].text
        color = v["color"].text
        approvals = Self.modes(v["approvals"])
        host = v["host"].text
        agents = (v["agents"].array ?? []).compactMap(\.string)
    }

    static func modes(_ v: JSON) -> [String: String] {
        guard case .object(let o) = v else { return [:] }
        return o.compactMapValues(\.string)
    }
}

/// An agent harlo can start sessions with.
public struct Agent: Equatable, Sendable, Identifiable {
    public var id: String
    public var label: String
}

/// What a new session can be: where, with which agent and model.
/// What a session ran and touched (harlo's `actions`): every command, every
/// file it read, changed or wrote, other tools too, in order.
public struct ActionLog: Equatable, Sendable {
    public struct Entry: Equatable, Sendable, Identifiable {
        public var id: Int
        /// Unix ms (nil: unknown).
        public var time: Int64?
        /// command, edit, write, read, search, web, agent, other.
        public var kind: String
        public var tool: String
        public var target: String
        /// How it went, in a word or two ("12 passed", "+2 −1").
        public var result: String
        public var failed: Bool
        /// No result yet: still running.
        public var running: Bool

        /// A file it read or changed (opens).
        public var isFile: Bool { ["edit", "write", "read"].contains(kind) && !target.isEmpty && !target.contains(", ") }
    }

    public var entries: [Entry]
    public var commands: Int
    public var failed: Int
    public var changed: [String]
    public var read: [String]

    public init(_ v: JSON) {
        entries = (v["actions"].array ?? []).enumerated().map { i, a in
            Entry(id: i, time: a["ts"].int, kind: a["kind"].text, tool: a["tool"].text, target: a["target"].text, result: a["result"].text,
                  failed: a["error"].bool ?? false, running: a["pending"].bool ?? false)
        }
        commands = Int(v["commands"].int ?? 0)
        failed = Int(v["failed"].int ?? 0)
        changed = (v["changed"].array ?? []).compactMap(\.string)
        read = (v["read"].array ?? []).compactMap(\.string)
    }
}

/// A session template saved in harlo (its new-session page): an agent, a
/// model, what runs without asking, and a first message, under a name.
public struct SessionTemplate: Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    /// "" = the project's agent.
    public var agent: String
    /// `provider/model`; "" = the agent's default.
    public var model: String
    /// The shield's mode ("" = the one it starts in).
    public var approval: String
    public var text: String

    public init(_ v: JSON) {
        id = v["id"].text
        name = v["name"].text
        agent = v["harness"].text
        model = v["model"].text
        approval = v["approval"].text
        text = v["text"].text
    }
}

public struct NewSessionChoices: Equatable, Sendable {
    /// Templates saved in harlo.
    public var templates: [SessionTemplate]
    public var projects: [Project]
    public var agents: [Agent]
    public var defaultAgent: String
    /// Models used before, most recent first: (agent, `provider/model`).
    public var models: [(agent: String, model: String)]
    /// The shield's mode each agent starts in, outside a project.
    public var approvals: [String: String]
    /// Every model each agent offers (as harlo's Settings → Providers reads
    /// them): `provider/model` and its name.
    public var agentModels: [String: [(id: String, name: String)]]

    public init(_ v: JSON) {
        projects = (v["projects"].array ?? []).map(Project.init)
        templates = (v["templates"].array ?? []).map(SessionTemplate.init).filter { !$0.id.isEmpty && !$0.name.isEmpty }
        agents = (v["agents"].array ?? []).map { Agent(id: $0["id"].text, label: $0["label"].text) }
        defaultAgent = v["defaultAgent"].text
        models = (v["models"].array ?? []).map { ($0["harness"].text, $0["model"].text) }
        approvals = Project.modes(v["approvals"])
        agentModels = (v["agentModels"].object ?? [:]).mapValues { list in (list.array ?? []).map { (id: $0["id"].text, name: $0["name"].text) } }
    }

    /// The shield's mode `agent` starts in, in `project` ("" none: pi).
    public func approval(for agent: String, in project: Project?) -> String {
        (project?.approvals ?? approvals)[agent] ?? ""
    }

    /// The agents a new session in `project` can use (its machine's).
    public func agents(for project: Project?) -> [Agent] {
        guard let p = project, !p.agents.isEmpty else { return agents }
        return agents.filter { p.agents.contains($0.id) }
    }

    /// The agent a new session in `project` starts with (nil: no project).
    public func agent(for project: Project?) -> String {
        let offered = agents(for: project)
        let wanted = project?.agent ?? defaultAgent
        return offered.contains { $0.id == wanted } ? wanted : (offered.first?.id ?? wanted)
    }

    /// The machines with projects, this computer first: their names ("" this
    /// computer), each with its projects.
    public var machines: [(host: String, projects: [Project])] {
        var order: [String] = []
        for p in projects where !order.contains(p.host) { order.append(p.host) }
        order.sort { a, b in a.isEmpty != b.isEmpty ? a.isEmpty : a < b }
        return order.map { h in (h, projects.filter { $0.host == h }) }
    }

    /// Models used before with `agent`.
    public func models(for agent: String) -> [String] {
        var seen = Set<String>()
        return models.filter { $0.agent == agent && !$0.model.isEmpty && seen.insert($0.model).inserted }.map(\.model)
    }

    /// What a new session with `agent` can start with: models used before
    /// first, then every other one it offers.
    public func modelChoices(for agent: String) -> [(id: String, name: String)] {
        let all = agentModels[agent] ?? []
        let recent = models(for: agent).map { m in (id: m, name: all.first { $0.id == m }?.name ?? m) }
        let used = Set(recent.map(\.id))
        return recent + all.filter { !used.contains($0.id) && !$0.id.isEmpty }
    }

    public static func == (a: Self, b: Self) -> Bool {
        a.projects == b.projects && a.templates == b.templates && a.agents == b.agents && a.defaultAgent == b.defaultAgent
            && a.models.map { "\($0.agent)\t\($0.model)" } == b.models.map { "\($0.agent)\t\($0.model)" }
            && a.agentModels.mapValues { $0.map(\.id) } == b.agentModels.mapValues { $0.map(\.id) }
    }
}

/// A model a running session can switch to.
public struct ModelChoice: Equatable, Sendable, Identifiable {
    public var provider: String
    public var modelId: String
    public var name: String
    /// It thinks (thinking levels apply).
    public var reasoning: Bool

    public var id: String { provider + "/" + modelId }

    public init(_ v: JSON) {
        provider = v["provider"].text
        modelId = v["id"].text
        name = v["name"].string.flatMap { $0.isEmpty ? nil : $0 } ?? v["id"].text
        reasoning = v["reasoning"].bool ?? false
    }
}

/// A session's approval mode and Plan.
public struct Modes: Equatable, Sendable {
    /// `always-ask`, `write`, `yolo`; "" when the agent has none (pi).
    public var approval: String
    public var approvals: [String]
    public var plan: Bool

    public init(_ v: JSON) {
        approval = v["approval"].text
        approvals = (v["approvals"].array ?? []).compactMap(\.string)
        plan = v["plan"].bool ?? false
    }

    public init(approval: String = "", approvals: [String] = [], plan: Bool = false) {
        self.approval = approval
        self.approvals = approvals
        self.plan = plan
    }

    /// An approval mode for people.
    public static func label(_ mode: String) -> String {
        ["always-ask": "Ask before tools", "write": "Allow edits", "yolo": "Allow everything"][mode] ?? mode
    }

    /// The same, in a word (the composer's chip).
    public static func short(_ mode: String) -> String {
        ["always-ask": "Ask", "write": "Edits", "yolo": "Auto"][mode] ?? mode
    }
}

/// What the session header shows: its name, model, thinking and usage.
public struct SessionInfo: Equatable, Sendable {
    public var title = ""
    public var model = ""
    /// `provider/id`.
    public var modelId = ""
    public var thinking = ""
    public var contextTokens: Int64 = 0
    public var contextWindow: Int64 = 0
    /// USD so far.
    public var cost: Double = 0

    public init() {}

    public init(_ v: JSON) {
        title = v["title"].text
        model = v["model"].text
        modelId = v["modelId"].text
        thinking = v["thinking"].text
        contextTokens = v["contextTokens"].int ?? 0
        contextWindow = v["contextWindow"].int ?? 0
        cost = v["cost"].double ?? 0
    }

    /// How full the context is, 0…1 (0 when unknown).
    public var contextUsed: Double {
        contextWindow > 0 ? min(1, Double(contextTokens) / Double(contextWindow)) : 0
    }
}

/// A slash command the agent takes in a message.
public struct SlashCommand: Equatable, Sendable, Identifiable {
    public var name: String
    public var description: String
    /// What it takes, e.g. `<plan|scan>`.
    public var hint: String
    public var id: String { name }

    public init(name: String, description: String = "", hint: String = "") {
        self.name = name
        self.description = description
        self.hint = hint
    }

    /// Commands for what's typed: `/` and a start of the name (or a part of
    /// it), name starts first. Nothing once a space follows the name.
    public static func matching(_ commands: [SlashCommand], typed: String) -> [SlashCommand] {
        guard typed.hasPrefix("/"), !typed.contains(" "), !typed.contains("\n") else { return [] }
        let q = typed.dropFirst().lowercased()
        let starts = commands.filter { $0.name.lowercased().hasPrefix(q) }
        let inside = commands.filter { !$0.name.lowercased().hasPrefix(q) && $0.name.lowercased().contains(q) }
        return starts + inside
    }
}

/// omp's todo list: phases of tasks.
public struct TodoPhase: Equatable, Sendable, Identifiable {
    public struct Task: Equatable, Sendable, Identifiable {
        public var content: String
        /// pending | in_progress | completed | blocked …
        public var status: String
        public var id: String { content }
    }

    public var name: String
    public var tasks: [Task]
    public var id: String { name }

    public static func list(_ v: JSON) -> [TodoPhase] {
        (v.array ?? []).map { p in
            TodoPhase(name: p["name"].text, tasks: (p["tasks"].array ?? []).map { Task(content: $0["content"].text, status: $0["status"].text) })
        }
    }

    public var done: Int { tasks.filter { $0.status == "completed" }.count }
}

/// A subagent the session started.
public struct Subagent: Equatable, Sendable, Identifiable {
    public var id: String
    public var agent: String
    public var description: String
    /// running | completed | failed | aborted
    public var status: String

    public init(_ v: JSON) {
        id = v["id"].text
        agent = v["agent"].text
        description = v["description"].text
        status = v["status"].text
    }
}

/// The session's repository: its branch and changed files.
public struct Changes: Equatable, Sendable {
    public struct File: Equatable, Sendable, Identifiable {
        public var path: String
        /// The old path of a rename.
        public var from: String
        /// `M`, `A`, `D`, `R`, `?` (new, untracked), …
        public var status: String
        public var staged: Bool
        public var untracked: Bool
        public var conflicted: Bool
        public var id: String { path }

        /// The file's name, and the folder it's in.
        public var name: String { path.split(separator: "/").last.map(String.init) ?? path }
        public var folder: String {
            guard let slash = path.lastIndex(of: "/") else { return "" }
            return String(path[..<slash])
        }
    }

    /// A repository in a folder of them (the session's folder isn't one).
    public struct Repo: Equatable, Sendable, Identifiable {
        /// Its place in the folder ("group/tools").
        public var name: String
        public var branch: String
        public var ahead: Int
        public var behind: Int
        public var changes: Int
        public var id: String { name }
    }

    public var branch: String
    public var ahead: Int
    public var behind: Int
    public var files: [File]
    /// A folder of repositories: each (their files' paths start with its
    /// name); empty for one repository.
    public var repos: [Repo]

    /// A repository's files (a folder of them).
    public func files(in repo: Repo) -> [File] {
        files.filter { $0.path.hasPrefix(repo.name + "/") }
    }

    public init(_ v: JSON) {
        repos = (v["repos"].array ?? []).map {
            Repo(name: $0["name"].text, branch: $0["branch"].text, ahead: Int($0["ahead"].int ?? 0), behind: Int($0["behind"].int ?? 0), changes: Int($0["changes"].int ?? 0))
        }
        branch = v["branch"].text
        ahead = Int(v["ahead"].int ?? 0)
        behind = Int(v["behind"].int ?? 0)
        files = (v["files"].array ?? []).map {
            File(path: $0["path"].text, from: $0["from"].text, status: $0["status"].text,
                 staged: $0["staged"].bool ?? false, untracked: $0["untracked"].bool ?? false, conflicted: $0["conflicted"].bool ?? false)
        }
    }
}

/// One file's diff, read-only.
public struct FileDiff: Equatable, Sendable {
    public struct Line: Equatable, Sendable {
        public enum Kind: String, Sendable { case added = "+", removed = "-", context = " ", noNewline = "\\" }
        public var kind: Kind
        public var old: Int?
        public var new: Int?
        public var text: String
    }

    public struct Hunk: Equatable, Sendable {
        public var header: String
        public var lines: [Line]
    }

    public var path: String
    public var binary: Bool
    public var added: Int
    public var removed: Int
    public var hunks: [Hunk]
    /// Too long: the rest wasn't sent.
    public var cut: Bool

    public init(_ v: JSON) {
        path = v["path"].text
        binary = v["binary"].bool ?? false
        added = Int(v["added"].int ?? 0)
        removed = Int(v["removed"].int ?? 0)
        cut = v["cut"].bool ?? false
        hunks = (v["hunks"].array ?? []).map { h in
            Hunk(header: h["header"].text, lines: (h["lines"].array ?? []).map { l in
                Line(kind: Line.Kind(rawValue: l["k"].text) ?? .context, old: l["o"].int.map(Int.init), new: l["n"].int.map(Int.init), text: l["t"].text)
            })
        }
    }
}

/// A photo for a message (JPEG or PNG, already made small enough).
public struct Photo: Equatable, Sendable, Identifiable {
    public var id = UUID()
    public var data: Data
    public var mimeType: String

    public init(data: Data, mimeType: String = "image/jpeg") {
        self.data = data
        self.mimeType = mimeType
    }

    var json: JSON { ["data": .string(data.base64EncodedString()), "mimeType": .string(mimeType)] }
}

// ---- the desktop's colours -------------------------------------------------

/// A colour, 0…1 per channel.
public struct RGBA: Codable, Equatable, Sendable {
    public var r: Double
    public var g: Double
    public var b: Double
    public var a: Double

    public init(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) {
        (self.r, self.g, self.b, self.a) = (r, g, b, a)
    }

    /// `#RRGGBB`, or `#AARRGGBB` (Qt's order, as harlo sends them).
    public init?(hex: String) {
        guard hex.hasPrefix("#"), let n = UInt64(hex.dropFirst(), radix: 16) else { return nil }
        let c = { (shift: UInt64) in Double((n >> shift) & 0xff) / 255 }
        switch hex.count {
        case 7: self.init(c(16), c(8), c(0))
        case 9: self.init(c(16), c(8), c(0), c(24))
        default: return nil
        }
    }

    /// This colour over `base` (opaque).
    public func over(_ base: RGBA) -> RGBA {
        RGBA(base.r + (r - base.r) * a, base.g + (g - base.g) * a, base.b + (b - base.b) * a)
    }

    /// Between `self` (0) and `other` (1).
    public func mix(_ other: RGBA, _ t: Double) -> RGBA {
        RGBA(r + (other.r - r) * t, g + (other.g - g) * t, b + (other.b - b) * t, 1)
    }

    public var luminance: Double { 0.2126 * r + 0.7152 * g + 0.0722 * b }
}

/// One variant (dark or light) of a theme, as the desktop's Theme.qml
/// reads it.
public struct Palette: Codable, Equatable, Sendable {
    public var background: RGBA
    /// The sidebar's (lists, the composer).
    public var sidebar: RGBA
    public var foreground: RGBA
    public var muted: RGBA
    public var elevated: RGBA
    public var subtle: RGBA
    public var border: RGBA
    public var userBubble: RGBA
    public var primary: RGBA
    public var codeBackground: RGBA
    public var error: RGBA
    public var warning: RGBA
    public var success: RGBA
    public var info: RGBA
    public var link: RGBA
    public var inlineCode: RGBA
    /// Highlighted code's token colours (`.s1` … `.s11`), by number.
    public var tokens: [String: RGBA]
    public var isLight: Bool

    /// From a theme's `colors` (nil when its basics are missing).
    public init?(_ c: JSON, light: Bool) {
        let col = { (v: JSON) in v.string.flatMap { RGBA(hex: $0) } }
        guard let bg = col(c["surface"]["background"]), let fg = col(c["surface"]["foreground"]), let primary = col(c["primary"]["base"]) else { return nil }
        background = bg
        foreground = fg
        self.primary = primary
        isLight = light
        sidebar = col(c["surface"]["muted"]) ?? bg
        muted = col(c["surface"]["mutedForeground"]) ?? fg.mix(bg, 0.4)
        elevated = col(c["surface"]["elevated"]) ?? bg
        subtle = col(c["surface"]["subtle"]) ?? elevated
        border = (col(c["chat"]["divider"]) ?? fg.mix(bg, 0.85)).over(bg)
        userBubble = (col(c["chat"]["userMessageBackground"]) ?? elevated).over(bg)
        codeBackground = (col(c["syntax"]["base"]["background"]) ?? col(c["markdown"]["inlineCodeBackground"]) ?? elevated).over(bg)
        error = col(c["status"]["error"]) ?? RGBA(0.85, 0.25, 0.25)
        warning = col(c["status"]["warning"]) ?? RGBA(0.9, 0.6, 0.2)
        success = col(c["status"]["success"]) ?? RGBA(0.3, 0.7, 0.4)
        info = col(c["status"]["info"]) ?? primary
        link = col(c["markdown"]["link"]) ?? primary
        inlineCode = col(c["markdown"]["inlineCode"]) ?? primary
        let s = c["syntax"]["base"], h = c["syntax"]["highlights"]
        let named: [(Int, JSON)] = [(1, s["comment"]), (2, s["keyword"]), (3, s["string"]), (4, s["number"]), (5, s["function"]),
                                    (6, s["variable"]), (7, s["type"]), (8, s["operator"]), (9, h["diffAdded"]), (10, h["diffRemoved"]), (11, h["diffModified"])]
        tokens = [:]
        for (n, v) in named { if let c = col(v) { tokens[String(n)] = c } }
    }

    public func token(_ n: Int) -> RGBA? { tokens[String(n)] }

    // ---- roles, as the desktop's Theme.qml: what screens are built from ----

    /// Cards on the background: a run, code, the composer.
    public var raised: RGBA { background.mix(foreground, 0.035) }
    /// Lines between messages and sections: barely there.
    public var hairline: RGBA { background.mix(foreground, isLight ? 0.08 : 0.075) }
    /// A list's rows: the open one, and a field on the sidebar.
    public var rowSelected: RGBA { sidebar.mix(foreground, isLight ? 0.075 : 0.085) }
    public var well: RGBA { sidebar.mix(foreground, isLight ? 0.05 : 0.045) }
    /// Text: times and hints; section labels (small, uppercase).
    public var faint: RGBA { muted.mix(background, 0.32) }
    public var label: RGBA { muted.mix(background, 0.12) }
    /// The accent as a quiet fill (badges, soft highlights).
    public var accentSoft: RGBA { RGBA(primary.r, primary.g, primary.b, isLight ? 0.12 : 0.17) }

    /// An agent's avatar: a soft fill of its colour and its letters.
    public func agent(_ id: String) -> (fill: RGBA, ink: RGBA) {
        let base: RGBA
        switch id {
        case "claude": base = RGBA(0.898, 0.537, 0.353)
        case "omp": base = RGBA(0.608, 0.541, 0.984)
        case "pi": base = RGBA(0.31, 0.757, 0.682)
        case "opencode": base = RGBA(0.357, 0.608, 0.941)
        case "hermes": base = RGBA(0.882, 0.722, 0.29)
        default: base = RGBA(0.655, 0.667, 0.698)
        }
        return (RGBA(base.r, base.g, base.b, isLight ? 0.16 : 0.2),
                isLight ? base.mix(RGBA(0, 0, 0), 0.25) : base.mix(RGBA(1, 1, 1), 0.15))
    }

    /// An agent's tile (the dashboard, a reply's head): its colour, solid,
    /// and a dark mark on it.
    public func agentTile(_ id: String) -> (fill: RGBA, ink: RGBA) {
        let base: RGBA
        switch id {
        case "claude": base = RGBA(0.969, 0.639, 0.471)
        case "omp": base = RGBA(0.706, 0.643, 0.988)
        case "pi": base = RGBA(0.502, 0.82, 0.769)
        case "opencode": base = RGBA(0.502, 0.71, 0.984)
        case "hermes": base = RGBA(0.941, 0.792, 0.42)
        default: base = RGBA(0.733, 0.745, 0.776)
        }
        return (isLight ? base.mix(RGBA(1, 1, 1), 0.2) : base, RGBA(0.09, 0.075, 0.11))
    }

    /// Text on a primary-coloured fill.
    public var onPrimary: RGBA {
        primary.luminance > 0.55 ? RGBA(0.1, 0.1, 0.1) : (isLight ? RGBA(1, 1, 1) : RGBA(0.1, 0.075, 0.063))
    }

    /// A status dot's colour (as the desktop's sidebar).
    public func status(_ status: String) -> RGBA {
        switch status {
        case "busy", "starting": return primary
        case "input": return warning
        case "unread": return info
        case "live", "idle": return success
        case "exited": return error
        default: return muted
        }
    }

    /// A project colour by key (Settings → Projects), nil for none.
    public func projectColor(_ key: String) -> RGBA? {
        switch key {
        case "keyword": return token(2)
        case "string": return token(3)
        case "number": return token(4)
        case "type": return token(7)
        case "function": return token(5)
        case "comment": return token(1)
        case "error": return error
        case "primary": return primary
        case "success": return success
        default: return nil
        }
    }
}

/// The desktop's theme: its family and both variants, and its mode
/// (`system`, `dark`, `light`).
public struct Theme: Codable, Equatable, Sendable {
    public var family: String
    public var mode: String
    public var dark: Palette
    public var light: Palette
    /// Your avatar, as picked in harlo on the computer (one of `avatars`;
    /// nil: the default). Optional, so a theme saved before it still loads.
    public var avatar: String?

    /// The penguins harlo has (the desktop's left rail picks one).
    public static let avatars = ["classic", "beanie", "glasses", "headphones", "jacket", "scarf"]

    public init?(_ v: JSON) {
        guard let dark = Palette(v["dark"], light: false), let light = Palette(v["light"], light: true) else { return nil }
        family = v["family"].text
        mode = v["mode"].text
        self.dark = dark
        self.light = light
        avatar = Theme.avatars.contains(v["avatar"].text) ? v["avatar"].text : nil
    }

    /// The avatar to show: the one picked, else the classic penguin.
    public var penguin: String { avatar ?? "classic" }

    public func palette(dark isDark: Bool) -> Palette { isDark ? dark : light }
}

/// Undoing a turn (harlo's `undoPlan` / `undo`): files that come back as
/// they were before the message, files that go (new since), whether commits
/// were made since (they stay), how many of your messages follow it.
public struct UndoPlan: Equatable, Sendable {
    public var restore: [String]
    public var remove: [String]
    public var headMoved: Bool
    public var later: Int
    /// Done (`undo`), or only looked at (`undoPlan`).
    public var done: Bool

    public init(_ v: JSON) {
        restore = (v["restore"].array ?? []).compactMap(\.string)
        remove = (v["remove"].array ?? []).compactMap(\.string)
        headMoved = v["headMoved"].bool ?? false
        later = Int(v["later"].int ?? 0)
        done = v["done"].bool ?? false
    }

    public var changes: Int { restore.count + remove.count }
}

/// A review comment on a line of a diff, for the agent (the phone's diff
/// view; the desktop's makes the same message).
public struct ReviewComment: Equatable, Sendable, Identifiable {
    public var path: String
    /// "new" (the file as it is), or "old" (a line that was removed).
    public var side: String
    public var line: Int
    public var code: String
    public var text: String
    public var id: String { "\(path):\(side):\(line)" }

    public init(path: String, side: String, line: Int, code: String, text: String) {
        self.path = path
        self.side = side
        self.line = line
        self.code = code
        self.text = text
    }

    /// The comments as one message to the agent: each one's file and line,
    /// its code, and what to do.
    public static func message(_ comments: [ReviewComment]) -> String {
        var lines = ["Review comments on your changes. Please address each one:", ""]
        for c in comments.sorted(by: { ($0.path, $0.line) < ($1.path, $1.line) }) {
            let code = c.code.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "`", with: "'")
            lines.append("- `\(c.path):\(c.line)`" + (c.side == "old" ? " (a line you removed)" : "") + (code.isEmpty ? "" : ": `\(code)`"))
            for t in c.text.split(separator: "\n", omittingEmptySubsequences: false) { lines.append("  " + t) }
        }
        return lines.joined(separator: "\n")
    }
}
