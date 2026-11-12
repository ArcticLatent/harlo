import Foundation

/// A step of a session's shared plan.
public struct PlanStep: Equatable, Sendable, Identifiable {
    public var id: Int64
    public var title: String
    public var detail: String
    /// "pending", "active", "done" or "skipped".
    public var status: String
    /// What the agent said it did.
    public var summary: String
    /// Its commit (a hash), "" none.
    public var commit: String
    /// Why there's no commit ("Not committed: …"), "" none.
    public var commitNote: String
    /// You changed or added it.
    public var editedByUser: Bool
    /// It runs at the same time as the step before it (a run with helpers).
    public var alongside: Bool
    /// The helper session doing it, or that did (a run with helpers), "" none.
    public var helper: String
    /// The worktree branch it's done on (a step run alongside), "" none.
    public var branch: String
    /// The agent (and model) that does it in a run with helpers, "" the
    /// run's.
    public var agent: String
    public var model: String

    public init(_ v: JSON) {
        id = v["id"].int ?? -1
        title = v["title"].text
        detail = v["detail"].text
        status = v["status"].string ?? "pending"
        summary = v["summary"].text
        commit = v["commit"].text
        commitNote = v["commitNote"].text
        editedByUser = v["editedByUser"].bool ?? false
        alongside = v["alongside"].bool ?? false
        helper = v["helper"].text
        branch = v["branch"].text
        agent = v["agent"].text
        model = v["model"].text
    }

    /// It's still to do (it can be changed, or a step added after it).
    public var open: Bool { status == "pending" || status == "active" }
}

/// A plan running with helpers: harlo gives each step to a helper session,
/// stage by stage.
public struct PlanRun: Equatable, Sendable {
    /// The helpers' agent ("claude", …), and its name.
    public var agent: String
    public var agentName: String
    /// Why it stopped and waits for you ("" it's running).
    public var paused: String
    /// Steps being reviewed: the session reviewing each.
    public var reviewing: [Int64: String] = [:]

    public init(_ v: JSON) {
        agent = v["agent"].text
        agentName = v["agentName"].text
        paused = v["paused"].text
        for (id, key) in v["reviewing"].object ?? [:] {
            if let id = Int64(id) { reviewing[id] = key.text }
        }
    }

    public init(agent: String, agentName: String, paused: String = "") {
        self.agent = agent
        self.agentName = agentName
        self.paused = paused
    }
}

/// A session's shared plan (a project with the plan on), as the dock above
/// harlo's composer shows it.
public struct Plan: Equatable, Sendable {
    /// The plan is on for its project.
    public var enabled: Bool
    /// Each finished step is committed.
    public var commits: Bool
    public var steps: [PlanStep]
    /// The agents that can review a step's commit (and run it with helpers).
    public var reviewers: [Agent]
    /// Its run with helpers (nil: none).
    public var run: PlanRun?
    /// Who "the team's helpers" are: its project's Team, else the
    /// session's agent (nil: harlo doesn't say).
    public var team: Team?

    /// A project's helpers (harlo's Settings → Projects → Team).
    public struct Team: Equatable, Sendable {
        public var agent: String
        public var agentName: String
        /// Its model's id ("" the agent's default).
        public var model: String
        /// Set in the project's Team (not just the session's agent).
        public var custom: Bool
        /// Who reviews each step ("" the helpers' agent), and its model.
        public var reviewerName: String
        public var reviewerModel: String

        public init(agent: String, agentName: String, model: String = "", custom: Bool = false, reviewerName: String = "", reviewerModel: String = "") {
            self.agent = agent
            self.agentName = agentName
            self.model = model
            self.custom = custom
            self.reviewerName = reviewerName
            self.reviewerModel = reviewerModel
        }

        /// "Claude Code · sonnet-5.5, reviewed by Harlo · gpt-6.1-sol"
        public var label: String {
            func short(_ m: String) -> String { m.split(separator: "/").last.map { " · " + $0 } ?? "" }
            let who = agentName + short(model)
            return reviewerName.isEmpty ? who : "\(who), reviewed by \(reviewerName)\(short(reviewerModel))"
        }
    }

    public init(_ v: JSON) {
        enabled = v["enabled"].bool ?? false
        commits = v["commits"].bool ?? false
        steps = (v["steps"].array ?? []).map(PlanStep.init)
        reviewers = (v["reviewers"].array ?? []).map { Agent(id: $0["id"].text, label: $0["label"].text) }
        run = v["run"].object == nil ? nil : PlanRun(v["run"])
        let t = v["team"]
        team = t["agent"].text.isEmpty ? nil : Team(agent: t["agent"].text, agentName: t["agentName"].text, model: t["model"].text,
                                                    custom: t["team"].bool ?? false, reviewerName: t["reviewer"]["agentName"].text,
                                                    reviewerModel: t["reviewer"]["model"].text)
    }

    public init(steps: [PlanStep] = [], enabled: Bool = true) {
        self.enabled = enabled
        commits = false
        self.steps = steps
        reviewers = []
    }

    /// Shown: on, with steps.
    public var shows: Bool { enabled && !steps.isEmpty }
    public var finished: Int { steps.filter { $0.status == "done" || $0.status == "skipped" }.count }
    public var current: PlanStep? { steps.first { $0.status == "active" } }

    /// It can run with helpers (again, when paused): steps are left, and it
    /// isn't running.
    public var canRun: Bool { finished < steps.count && (run == nil || run?.paused.isEmpty == false) }

    /// The agents for its helpers, the session's own first.
    public func helperAgents(for harness: String) -> [Agent] {
        reviewers.filter { $0.id == harness } + reviewers.filter { $0.id != harness }
    }

    /// Reviewers, the session's own agent last (another one first).
    public func reviewers(for harness: String) -> [Agent] {
        reviewers.filter { $0.id != harness } + reviewers.filter { $0.id == harness }
    }
}

/// A change to a plan, as harlo's `planEdit` takes it.
public enum PlanEdit: Sendable {
    case add(title: String, detail: String, after: Int64?)
    case edit(id: Int64, title: String, detail: String)
    case remove(id: Int64)
    case shift(id: Int64, by: Int)
    case skip(id: Int64, skipped: Bool)
    /// Runs it at the same time as the step before (with helpers), or after.
    case alongside(id: Int64, alongside: Bool)

    public var json: JSON {
        switch self {
        case .add(let t, let d, let after):
            var v: JSON = ["op": "add", "title": .string(t), "detail": .string(d)]
            if let after { v["after"] = .int(after) }
            return v
        case .edit(let id, let t, let d): return ["op": "edit", "id": .int(id), "title": .string(t), "detail": .string(d)]
        case .remove(let id): return ["op": "remove", "id": .int(id)]
        case .shift(let id, let by): return ["op": "shift", "id": .int(id), "by": .int(Int64(by))]
        case .skip(let id, let skipped): return ["op": "skip", "id": .int(id), "skipped": .bool(skipped)]
        case .alongside(let id, let on): return ["op": "alongside", "id": .int(id), "alongside": .bool(on)]
        }
    }
}

/// A page of the session's repository's wiki (OpenWiki's, in openwiki/).
public struct WikiPage: Equatable, Sendable, Identifiable {
    /// Within openwiki/ ("architecture/jobs.md").
    public var path: String
    public var title: String
    public var summary: String
    /// Its kind ("overview", "architecture", "workflow", …; "" when it
    /// doesn't say).
    public var kind: String
    /// The pages it links to.
    public var links: [String]
    public var id: String { path }

    public init(path: String, title: String, summary: String = "", kind: String = "", links: [String] = []) {
        self.path = path
        self.title = title
        self.summary = summary
        self.kind = kind
        self.links = links
    }

    /// Its folder as a heading ("architecture/jobs.md" → "Architecture";
    /// "" at the top).
    public var group: String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        let folder = path[..<slash].split(separator: "/").last.map(String.init) ?? ""
        let words = folder.replacingOccurrences(of: "-", with: " ").replacingOccurrences(of: "_", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }
}

/// The wiki of a session's repository: its pages (the quickstart first),
/// how fresh it is, and a run writing it.
public struct Wiki: Equatable, Sendable {
    /// Inside a harlo project (a wiki is a project's).
    public var inProject: Bool
    /// Its agents use it (the project's Wiki setting).
    public var enabled: Bool
    public var exists: Bool
    public var pages: [WikiPage]
    /// A run that's writing it, or stopped part way: pages done of all.
    public var run: Run?
    /// When it was last brought up to date (ISO 8601; "" never).
    public var updatedAt: String
    /// Commits to the code since (nil: not known).
    public var behind: Int?
    /// A wiki session is at work on it.
    public var running: Bool
    /// That session's key.
    public var session: String
    /// This harlo has OpenWiki to write it.
    public var canWrite: Bool
    /// Still being read on another machine.
    public var loading: Bool
    /// The repository's folder (on its machine): code links' files are in it.
    public var root: String

    public struct Run: Equatable, Sendable {
        public var phase: String
        public var done: Int
        public var total: Int
        public init(phase: String = "", done: Int = 0, total: Int = 0) {
            self.phase = phase
            self.done = done
            self.total = total
        }
    }

    public init(_ v: JSON) {
        inProject = (v["project"].int ?? -1) >= 0
        enabled = v["enabled"].bool ?? false
        exists = v["exists"].bool ?? false
        pages = (v["pages"].array ?? []).map {
            WikiPage(path: $0["path"].text, title: $0["title"].text, summary: $0["description"].text, kind: $0["type"].text, links: ($0["links"].array ?? []).map(\.text))
        }
        run = v["run"].object == nil ? nil : Run(phase: v["run"]["phase"].text, done: Int(v["run"]["done"].int ?? 0), total: Int(v["run"]["total"].int ?? 0))
        updatedAt = v["last"]["at"].text
        behind = v["behind"].int.map(Int.init)
        running = v["running"].bool ?? false
        session = v["session"].text
        canWrite = v["runtime"].bool ?? false
        loading = v["loading"].bool ?? false
        root = v["root"].text
    }

    public init(pages: [WikiPage] = [], inProject: Bool = true, enabled: Bool = true, exists: Bool = true, run: Run? = nil, updatedAt: String = "",
                behind: Int? = nil, running: Bool = false, session: String = "", canWrite: Bool = true, loading: Bool = false, root: String = "")
    {
        self.root = root
        self.pages = pages
        self.inProject = inProject
        self.enabled = enabled
        self.exists = exists
        self.run = run
        self.updatedAt = updatedAt
        self.behind = behind
        self.running = running
        self.session = session
        self.canWrite = canWrite
        self.loading = loading
    }

    /// Its freshness in words: "Writing pages: 2 of 5", "Updated 3 days ago
    /// · 4 commits since", "A run stopped part way (1 of 3 pages).".
    public func freshness(now: Date = Date()) -> String {
        if let r = run, running {
            return r.phase == "planning" || r.total == 0 ? "Planning the wiki…" : "Writing pages: \(r.done) of \(r.total)"
        }
        if let r = run { return "A run stopped part way" + (r.total > 0 ? " (\(r.done) of \(r.total) pages)." : ".") }
        if running { return "Starting a run…" }
        guard !updatedAt.isEmpty else { return "Not written by OpenWiki yet." }
        let when = "Updated " + Wiki.ago(updatedAt, now: now)
        guard let b = behind else { return when }
        return b == 0 ? when + " · up to date" : when + " · \(b) " + (b == 1 ? "commit" : "commits") + " since"
    }

    /// "just now", "5 minutes ago", "yesterday", "3 days ago", from an ISO
    /// time (as the desktop says it).
    public static func ago(_ iso: String, now: Date = Date()) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let t = f.date(from: iso) ?? ISO8601DateFormatter().date(from: iso) else { return "" }
        let s = max(0, now.timeIntervalSince(t))
        if s < 90 { return "just now" }
        if s < 3600 { return "\(Int((s / 60).rounded())) minutes ago" }
        if s < 86400 { let h = Int((s / 3600).rounded()); return h == 1 ? "an hour ago" : "\(h) hours ago" }
        if s < 86400 * 45 { let d = Int((s / 86400).rounded()); return d == 1 ? "yesterday" : "\(d) days ago" }
        return t.formatted(date: .abbreviated, time: .omitted)
    }

    /// Update is the thing to do: it's behind the code, or a run stopped.
    public var wantsUpdate: Bool { run != nil || (behind ?? 0) > 0 }

    /// The pages shown in the graph: all of them, or for a big wiki the
    /// quickstart (the first) and the pages it links to or from.
    public var graphPages: [WikiPage] {
        guard pages.count > 1 else { return [] }
        guard pages.count > 40, let first = pages.first else { return pages }
        let near = Set([first.path] + first.links + pages.filter { $0.links.contains(first.path) }.map(\.path))
        return pages.filter { near.contains($0.path) }
    }

    /// The pages and their links as a Mermaid flowchart ("" under two
    /// pages), as the desktop's Wiki panel draws it.
    public var mermaid: String {
        let shown = graphPages
        guard shown.count > 1 else { return "" }
        let at = Dictionary(shown.enumerated().map { ($1.path, $0) }, uniquingKeysWith: { a, _ in a })
        var out = "flowchart LR\n"
        for (i, p) in shown.enumerated() {
            var title = p.title.replacingOccurrences(of: "\"", with: "#quot;")
            for c in ["[", "]", "{", "}", "<", ">"] { title = title.replacingOccurrences(of: c, with: " ") }
            out += "  p\(i)[\"\(title)\"]\n"
        }
        for (i, p) in shown.enumerated() {
            for l in p.links { if let j = at[l] { out += "  p\(i) --> p\(j)\n" } }
        }
        return out
    }

    /// A file a code link names, in the repository ("src/a.rs" →
    /// "/home/me/shop/src/a.rs").
    public func file(_ path: String) -> String { root.isEmpty ? path : root + "/" + path }
}

/// A wiki page as read: what it is, then its text in blocks.
public struct WikiPageText: Equatable, Sendable {
    public var path: String
    public var title: String
    public var kind: String
    public var summary: String
    /// When it was last checked against the code (ISO 8601; "" never).
    public var verified: String
    public var blocks: [Block]
    /// Where in the code it comes from: its sources and the lines its
    /// claims cite, by file.
    public var sources: [Source]
    /// Why it couldn't be read ("" when it was).
    public var error: String

    /// A file the page is drawn from: the line ranges cited (none: the
    /// whole file) and how many of its claims cite it.
    public struct Source: Equatable, Sendable {
        public var path: String
        public var ranges: [ClosedRange<Int>]
        public var claims: Int

        /// A link to it ("repo://path#L10-L15"), at a range or the file.
        public func link(_ range: ClosedRange<Int>? = nil) -> String {
            let p = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
            guard let r = range ?? ranges.first else { return "repo://\(p)" }
            return "repo://\(p)#L\(r.lowerBound)-L\(r.upperBound)"
        }
    }

    /// Prose (HTML, as a reply's), code (`code`, in `lang`) or a Mermaid
    /// diagram; `anchor` names a heading's section.
    public struct Block: Equatable, Sendable {
        public var kind: String
        public var html: String
        public var code: String
        public var lang: String
        public var anchor: String
    }

    public init(_ v: JSON) {
        path = v["path"].text
        title = v["title"].text
        kind = v["type"].text
        summary = v["description"].text
        verified = v["verified"].text
        error = v["error"].text
        blocks = (v["blocks"].array ?? []).map { Block(kind: $0["kind"].text, html: $0["html"].text, code: $0["code"].text, lang: $0["lang"].text, anchor: $0["anchor"].text) }
        sources = (v["sources"].array ?? []).compactMap { s in
            let path = s["path"].text
            guard !path.isEmpty else { return nil }
            let ranges: [ClosedRange<Int>] = (s["ranges"].array ?? []).compactMap { r in
                guard let a = r.array, a.count == 2, let from = a[0].int.map(Int.init), let to = a[1].int.map(Int.init), from >= 1, to >= from else { return nil }
                return from...to
            }
            return Source(path: path, ranges: ranges, claims: Int(s["claims"].int ?? 0))
        }
    }

    /// The page a link in it opens ("../quickstart.md#x" from
    /// "architecture/jobs.md" → ("quickstart.md", "x")); nil for a link
    /// elsewhere (code, the web).
    public func page(of link: String) -> (path: String, section: String)? {
        guard !link.contains("://"), !link.hasPrefix("mailto:") else { return nil }
        let parts = link.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
        let target = parts[0], section = parts.count > 1 ? parts[1] : ""
        if target.isEmpty { return (path, section) }
        guard target.hasSuffix(".md") else { return nil }
        var dirs = target.hasPrefix("/") ? [] : path.split(separator: "/").dropLast().map(String.init)
        for c in target.split(separator: "/") {
            if c == ".." { _ = dirs.popLast() } else if c != "." { dirs.append(String(c)) }
        }
        let joined = dirs.joined(separator: "/")
        return (joined.hasPrefix("openwiki/") ? String(joined.dropFirst(9)) : joined, section)
    }

    /// A link to code (`repo://src/a.rs#L10-L20`): the file and its line.
    public static func code(of link: String) -> (path: String, line: Int)? {
        guard link.hasPrefix("repo://") else { return nil }
        let rest = link.dropFirst(7)
        let parts = rest.split(separator: "#", maxSplits: 1).map(String.init)
        guard let path = parts.first?.removingPercentEncoding, !path.isEmpty else { return nil }
        var line = 0
        if parts.count > 1, parts[1].hasPrefix("L") {
            line = Int(parts[1].dropFirst().prefix { $0.isNumber }) ?? 0
        }
        return (path, line)
    }
}

/// What a wiki search found: sections, best first, or (a wiki linked to
/// others) the workspaces to pick from.
public struct WikiHits: Equatable, Sendable {
    public struct Hit: Equatable, Sendable, Identifiable {
        /// "openwiki/architecture/jobs.md".
        public var page: String
        public var section: String
        public var heading: String
        public var snippet: String
        public var id: String { page + "#" + section }
    }
    public var hits: [Hit]
    public var workspaces: [(id: String, name: String)]
    /// OpenWiki couldn't run: pages that have the words.
    public var plain: Bool

    public init(_ v: JSON) {
        hits = (v["hits"].array ?? []).map { Hit(page: $0["page"].text, section: $0["section"].text, heading: $0["heading"].text, snippet: $0["snippet"].text) }
        workspaces = (v["workspaces"].array ?? []).map { ($0["id"].text, $0["name"].text) }
        plain = v["plain"].bool ?? false
    }

    public static func == (a: WikiHits, b: WikiHits) -> Bool {
        a.hits == b.hits && a.plain == b.plain && a.workspaces.map(\.id) == b.workspaces.map(\.id)
    }
}

/// A Mermaid diagram as harlo drew it for the phone.
public struct DiagramPicture: Equatable, Sendable {
    public var png: Data
    /// Its size in points (the picture has twice the pixels).
    public var width: Double
    public var height: Double

    public init(png: Data, width: Double, height: Double) {
        self.png = png
        self.width = width
        self.height = height
    }
}
