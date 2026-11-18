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
    public var finished: Int { steps.count(where: { $0.status == "done" || $0.status == "skipped" }) }
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

/// A Mermaid diagram as harlo drew it for the phone.
public struct DiagramPicture: Equatable, Sendable {
    public var png: Data
    /// Its size in points (the picture has twice the pixels).
    public var width: Double
    public var height: Double
    /// A flowchart's boxes, to tap.
    public var boxes: [Box]

    /// A box: its node's id and where it is, as fractions of the
    /// picture's width and height.
    public struct Box: Equatable, Sendable {
        public var id: String
        public var x, y, w, h: Double

        public init(id: String, x: Double, y: Double, w: Double, h: Double) {
            self.id = id
            self.x = x
            self.y = y
            self.w = w
            self.h = h
        }
    }

    public init(png: Data, width: Double, height: Double, boxes: [Box] = []) {
        self.png = png
        self.width = width
        self.height = height
        self.boxes = boxes
    }

    /// The boxes as harlo sends them (`[{ id, x, y, w, h }]`), any without
    /// an id or a size left out.
    public static func boxes(_ v: JSON) -> [Box] {
        (v.array ?? []).compactMap { b in
            guard !b["id"].text.isEmpty, let x = b["x"].double, let y = b["y"].double, let w = b["w"].double, let h = b["h"].double, w > 0, h > 0 else { return nil }
            return Box(id: b["id"].text, x: x, y: y, w: w, h: h)
        }
    }
}

extension DiagramPicture {
    /// A flowchart drawn across (LR or RL) turned to run down (TB), past
    /// any `%%` lines before it; nil when it isn't one drawn across.
    public static func down(_ source: String) -> String? {
        let pattern = #"^(\s*(?:%%[^\n]*\n\s*)*)(flowchart|graph)([ \t]+)(LR|RL)\b"#
        guard let re = try? NSRegularExpression(pattern: pattern) else { return nil }
        let all = NSRange(source.startIndex..., in: source)
        guard re.firstMatch(in: source, range: all) != nil else { return nil }
        return re.stringByReplacingMatches(in: source, range: all, withTemplate: "$1$2$3TB")
    }

    /// Whether `down` shows larger than `across` in `width` points (each
    /// fitted to it, never enlarged), by more than 15%: then it's the one
    /// to show.
    public static func showsDown(across: DiagramPicture?, down: DiagramPicture?, width: Double) -> Bool {
        guard let across, let down, width > 0 else { return false }
        let fit = { (d: DiagramPicture) in min(1, width / max(1, d.width)) }
        return fit(down) > 1.15 * fit(across)
    }
}
