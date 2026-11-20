import Foundation

/// A session in harlo's list (`publish_phone_sessions` in workspace.rs).
public struct SessionSummary: Equatable, Sendable, Identifiable {
    public var key: String
    public var title: String
    /// The project (or "Other sessions") it's grouped under.
    public var project: String
    /// "harlo", "claude", "pi", "demo".
    public var harness: String
    /// "busy", "input", "live", "idle", "exited", "unread", …
    public var status: String
    /// Unix seconds.
    public var modified: Int64
    public var branch: String
    /// Its folder (`~/…`).
    public var location: String
    public var pinned: Bool
    /// Questions and approvals it waits on, oldest first.
    public var requests: [UIRequest]
    /// The machine it runs on: "" harlo's computer, else a remote host.
    public var host: String
    /// Why harlo put it on its project's Settled shelf ("Merged", …), "" when
    /// it isn't.
    public var settled: String
    /// Its model's name (as it runs, else as it last ran; "" not known),
    /// and who made it: a logo's id ("anthropic", "zai-coding-plan", …).
    public var modelName: String
    public var maker: String

    public var id: String { key }

    public init(_ v: JSON) {
        key = v["key"].text
        title = v["title"].text
        project = v["project"].text
        harness = v["harness"].text
        status = v["status"].text
        modified = v["modified"].int ?? 0
        branch = v["branch"].text
        location = v["where"].text
        pinned = v["pinned"].bool ?? false
        requests = (v["requests"].array ?? []).map(UIRequest.init)
        host = v["host"].text
        settled = v["settled"].text
        modelName = v["modelName"].text
        maker = v["maker"].text
    }

    public static func list(_ v: JSON) -> [SessionSummary] {
        (v.array ?? []).map(SessionSummary.init)
    }
}

/// One row of a transcript (harlo-core's `Row`).
public struct Row: Equatable, Sendable, Identifiable {
    public var key: String
    /// "user", "markdown", "code", "thinking", "tool", "notice", …
    public var kind: String
    /// Rich text (markdown and code rows), harlo's HTML.
    public var html: String
    /// Plain text: the prompt, a thought, raw code, a tool's output, a notice.
    public var text: String
    /// The tool's name, the code's language, the notice's level.
    public var title: String
    /// The tool call's argument summary.
    public var subtitle: String
    /// `streaming` / `done`; tools `running` / `done` / `error`.
    public var state: String
    /// The run of tool calls and thoughts it folds under (its first row's
    /// key), or "".
    public var group: String
    /// The run's summary line, on its first row.
    public var summary: String
    /// The run is still going: shown open.
    public var groupOpen: Bool
    /// When it started (unix ms; 0 unknown).
    public var time: Int64
    /// Tool rows: when the call returned (unix ms; 0 running or unknown).
    public var ended: Int64
    /// Tool rows: what the call came to ("+4 −2", "48 passed", "exit 1").
    public var result: String
    /// Tool rows: an edit's or a write's diff (`+`, `-`, ` ` lines).
    public var diff: String
    /// A finished run's first row: how long the run took (ms; 0 unknown).
    public var runMs: Int64
    /// Tool rows: what the call is for, in the agent's words; harlo's
    /// messages: their plain line.
    public var label: String
    /// Tool rows: the command a shell call ran ("" for other tools).
    public var command: String
    /// Replies: the model that wrote it ("" when it's the session's own).
    public var model: String
    /// The pictures it shows (one you sent, one a tool returned or a model
    /// made, a picture file it names), as harlo names them: ask harlo for
    /// one with `HarloModel.picture`.
    public var images: [String]
    /// Tool rows: the output as code, when it's a file's lines (nil for
    /// other output, it shows as text).
    public var code: ToolCode?
    /// Tool rows: the command highlighted as shell ("" none: it shows plain).
    public var commandSpans: [Span]
    /// Tool rows: the call's id (a pi extension may draw the call: its
    /// `ExtView` has this `toolCallId`).
    public var callId = ""

    public var id: String { key }

    public init(_ v: JSON) {
        key = v["key"].text
        kind = v["kind"].text
        html = v["html"].text
        text = v["text"].text
        title = v["title"].text
        subtitle = v["subtitle"].text
        state = v["state"].text
        group = v["group"].text
        summary = v["summary"].text
        groupOpen = v["groupOpen"].bool ?? false
        time = v["time"].int ?? 0
        ended = v["ended"].int ?? 0
        result = v["result"].text
        diff = v["diff"].text
        runMs = v["runMs"].int ?? 0
        label = v["label"].text
        command = v["command"].text
        model = v["model"].text
        images = (v["images"].array ?? []).compactMap(\.string)
        code = ToolCode(v["code"])
        let commandHtml = v["commandHtml"].text
        commandSpans = commandHtml.isEmpty ? [] : RichText.codeLine(commandHtml)
        callId = v["callId"].text
    }

    public init(key: String, kind: String, html: String = "", text: String = "", title: String = "", subtitle: String = "",
                state: String = "", group: String = "", summary: String = "", groupOpen: Bool = false,
                time: Int64 = 0, ended: Int64 = 0, result: String = "", diff: String = "", runMs: Int64 = 0,
                label: String = "", command: String = "", model: String = "", images: [String] = [],
                code: ToolCode? = nil, commandSpans: [Span] = [])
    {
        self.code = code
        self.commandSpans = commandSpans
        self.images = images
        self.label = label
        self.command = command
        self.model = model
        self.time = time
        self.ended = ended
        self.result = result
        self.diff = diff
        self.runMs = runMs
        self.key = key
        self.kind = kind
        self.html = html
        self.text = text
        self.title = title
        self.subtitle = subtitle
        self.state = state
        self.group = group
        self.summary = summary
        self.groupOpen = groupOpen
    }
}

extension Row {
    /// A message harlo sent the agent (a workflow's note, the answer to a
    /// split, …): a notice titled "harlo", the session it's about in
    /// `subtitle`.
    public var fromHarlo: Bool { kind == "notice" && title == "harlo" }

    /// A message sent to the agent, yours or harlo's: each starts a turn.
    public var isSent: Bool { kind == "user" || fromHarlo }
}

/// A change to a transcript's rows.
public enum RowChange: Equatable, Sendable {
    case update(at: Int, row: Row)
    case remove(at: Int, count: Int)
    case insert(at: Int, rows: [Row])

    init?(_ v: JSON) {
        let at = Int(v["at"].int ?? -1)
        switch v["op"].text {
        case "update": self = .update(at: at, row: Row(v["row"]))
        case "remove": self = .remove(at: at, count: Int(v["count"].int ?? 0))
        case "insert": self = .insert(at: at, rows: (v["rows"].array ?? []).map(Row.init))
        default: return nil
        }
    }
}

extension Array where Element == Row {
    /// Applies harlo's changes (out-of-range ones are clamped, so a missed
    /// update can't crash; the next reset puts things right).
    public mutating func apply(_ changes: [RowChange]) {
        for change in changes {
            switch change {
            case .update(let at, let row):
                if indices.contains(at) { self[at] = row }
            case .remove(let at, let count):
                guard at >= 0, at < self.count else { continue }
                removeSubrange(at..<Swift.min(at + count, self.count))
            case .insert(let at, let rows):
                insert(contentsOf: rows, at: Swift.max(0, Swift.min(at, self.count)))
            }
        }
    }
}

/// A question or approval the agent is waiting on.
public struct UIRequest: Equatable, Sendable, Identifiable {
    public var id: String
    /// "confirm", "select", "input", "editor", "ask", …
    public var method: String
    public var title: String
    public var message: String
    public var options: [String]
    public var placeholder: String
    public var prefill: String
    /// An approval (Allow / Deny a tool call).
    public var approval: Bool
    /// `ask`'s questions, as the agent sent them.
    public var questions: [Question]
    /// When it came (unix ms; 0 unknown): in the session list's.
    public var asked: Int64

    public struct Question: Equatable, Sendable {
        public var id: String
        public var question: String
        public var header: String
        public var options: [String]
        public var multi: Bool
    }

    public init(_ v: JSON) {
        id = v["id"].text
        method = v["method"].text
        title = v["title"].text
        message = v["message"].text
        options = (v["options"].array ?? []).compactMap { $0.string }
        placeholder = v["placeholder"].text
        prefill = v["prefill"].text
        approval = v["approval"].bool ?? false
        asked = v["asked"].int ?? 0
        questions = (v["questions"].array ?? []).map { q in
            Question(
                id: q["id"].string ?? (q["id"].int.map { String($0) } ?? ""),
                question: q["question"].text,
                header: q["header"].text,
                options: (q["options"].array ?? []).compactMap { $0.string ?? $0["label"].string },
                multi: q["multi"].bool ?? false
            )
        }
    }

    /// The answer for an `ask`: each question's picks and typed text.
    public static func askAnswer(_ questions: [Question], picked: [[String]], typed: [String]) -> JSON {
        ["answers": .array(questions.enumerated().map { i, q in
            var a: JSON = ["id": .string(q.id), "selectedOptions": .array((i < picked.count ? picked[i] : []).map(JSON.string))]
            let t = i < typed.count ? typed[i].trimmingCharacters(in: .whitespacesAndNewlines) : ""
            if !t.isEmpty { a["customInput"] = .string(t) }
            return a
        })]
    }
}

/// What harlo tells a connected phone without being asked.
public enum Event: Equatable, Sendable {
    case sessions([SessionSummary])
    /// The session's latest rows; `earlier` more are only in harlo.
    case reset(key: String, rows: [Row], earlier: Int)
    case rows(key: String, changes: [RowChange])
    case status(key: String, status: String)
    case meta(key: String, info: SessionInfo)
    case request(key: String, request: UIRequest)
    case cleared(key: String, id: String)
    case commands(key: String, commands: [SlashCommand])
    /// Messages waiting for the agent: steering, then follow-ups.
    case queue(key: String, steering: [String], followUp: [String])
    case todos(key: String, phases: [TodoPhase])
    /// Messages stopped before they were sent: their words and photos, to
    /// put back in the composer.
    case unsent(key: String, text: String, photos: [Photo])
    /// Plan limits changed (this computer's or a machine's).
    case limits(PlanLimits)
    /// A workflow run changed: ask for the open session's (and the list)
    /// again.
    case workflow(key: String)
    /// A pi extension's view changed, closed, or all of them went.
    case ext(key: String, change: ExtChange)
    /// This phone was removed in harlo.
    case revoked

    public init?(_ v: JSON) {
        let key = v["key"].text
        switch v["ev"].text {
        case "sessions": self = .sessions(SessionSummary.list(v["sessions"]))
        case "reset": self = .reset(key: key, rows: (v["rows"].array ?? []).map(Row.init), earlier: Int(v["earlier"].int ?? 0))
        case "rows": self = .rows(key: key, changes: (v["changes"].array ?? []).compactMap(RowChange.init))
        case "status": self = .status(key: key, status: v["status"].text)
        case "meta": self = .meta(key: key, info: SessionInfo(v))
        case "request": self = .request(key: key, request: UIRequest(v["request"]))
        case "cleared": self = .cleared(key: key, id: v["id"].text)
        case "commands":
            self = .commands(key: key, commands: (v["commands"].array ?? []).map {
                SlashCommand(name: $0["name"].text, description: $0["description"].text, hint: $0["hint"].text)
            })
        case "queue": self = .queue(key: key, steering: (v["steering"].array ?? []).compactMap(\.string), followUp: (v["followUp"].array ?? []).compactMap(\.string))
        case "todos": self = .todos(key: key, phases: TodoPhase.list(v["phases"]))
        case "unsent":
            self = .unsent(key: key, text: v["text"].text, photos: (v["photos"].array ?? []).compactMap { p in
                Data(base64Encoded: p["data"].text).map { Photo(data: $0, mimeType: p["mimeType"].text) }
            })
        case "limits": self = .limits(PlanLimits(v))
        case "workflow": self = .workflow(key: key)
        case "ext":
            guard let change = ExtChange(v) else { return nil }
            self = .ext(key: key, change: change)
        case "revoked": self = .revoked
        default: return nil
        }
    }
}

/// How a message is sent: a new turn, or (while it works) steering it,
/// queueing a follow-up, or stopping the run and sending it instead.
public enum SendMode: String, Sendable {
    case normal = ""
    case steer
    case followUp
    case stopAndSend
}

extension SessionSummary {
    /// Sessions by when they last changed: Today, Yesterday, Previous 7
    /// days, Previous 30 days, Older (pinned ones first, in their own group),
    /// newest first. One working now counts as today.
    public static func byDay(_ list: [SessionSummary], now: Date = Date.now, calendar: Calendar = .current) -> [(name: String, sessions: [SessionSummary])] {
        let today = calendar.startOfDay(for: now)
        func when(_ s: SessionSummary) -> Date {
            s.modified > 0 ? Date(timeIntervalSince1970: TimeInterval(s.modified)) : now
        }
        func group(_ s: SessionSummary) -> String {
            if ["busy", "starting", "input"].contains(s.status) { return "Today" }
            let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: when(s)), to: today).day ?? 0
            switch days {
            case ..<1: return "Today"
            case 1: return "Yesterday"
            case 2...7: return "Previous 7 days"
            case 8...30: return "Previous 30 days"
            default: return "Older"
            }
        }
        let order = ["Pinned", "Today", "Yesterday", "Previous 7 days", "Previous 30 days", "Older"]
        var groups: [String: [SessionSummary]] = [:]
        for s in list.sorted(by: { when($0) > when($1) }) {
            groups[s.pinned ? "Pinned" : group(s), default: []].append(s)
        }
        return order.compactMap { name in groups[name].map { (name, $0) } }
    }
}
