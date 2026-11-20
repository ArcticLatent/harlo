import Foundation

// The demo: harlo's app without a harlo to connect to (for App Review, and
// for a look around before installing harlo on a computer). Sample sessions
// on a pretend computer; opening, answering, starting and switching work
// on them, and a message gets a canned reply. Nothing is saved, nothing
// connects anywhere, and leaving it (Forget) goes back to pairing.

extension HarloModel {
    /// The demo's computer.
    public static let demoName = "Demo computer"

    public func startDemo(now: Date = Date.now) {
        hangUp()
        let sample = HarloModel.sample(now: now, busier: true)
        samples = .standard
        // (In memory only: a real pairing replaces it, a restart forgets it.)
        host = PairedHarlo(key: sample.host?.key ?? "", name: Self.demoName, endpoints: [])
        sessions = sample.sessions
        limits = sample.limits
        demoServers = Self.sampleServers
        session = nil
        answered = []
        state = .connected
        demo = true
    }

    public func endDemo() {
        demo = false
        samples = nil
        session = nil
        sessions = []
        limits = PlanLimits()
        answered = []
        host = nil
        state = .unpaired
    }

    /// A demo session: the sample with its tool calls for the first one,
    /// a short exchange for the others; what waits on you, still waiting.
    func demoOpen(_ summary: SessionSummary) {
        let s: OpenSession
        if summary.key == "live:1" {
            s = Self.sampleSession()
        } else {
            s = OpenSession(key: summary.key, title: summary.title, harness: summary.harness)
            s.liveKey = summary.key
            s.loading = false
            s.model = "Claude Sonnet 4.5"
            s.info.model = s.model
            s.info.thinking = "medium"
            s.modes = Modes(approval: "always-ask", approvals: ["always-ask", "write", "yolo"])
            let t = summary.modified * 1000
            s.rows = [
                Row(key: summary.key + "-u", kind: "user", text: summary.title, time: t - 60_000),
                Row(key: summary.key + "-a", kind: "markdown", html: "<p>This is a sample session in harlo's demo. With harlo running on your computer, your own sessions show here: what the agent is doing, its replies, and what it asks you.</p>",
                    state: "done", time: t - 50_000),
            ]
        }
        s.status = summary.status == "unread" ? "" : summary.status
        s.requests = summary.requests.filter { !answered.contains($0.id) }
        session = s
    }

    /// A message in the demo: it shows, the agent works a moment, and a
    /// canned reply says what would happen with harlo.
    func demoSend(_ text: String) -> Bool {
        guard let s = session else { return false }
        demoCount += 1
        let n = demoCount, ms = Int64(Date.now.timeIntervalSince1970 * 1000)
        s.rows.append(Row(key: "demo-u\(n)", kind: "user", text: text, time: ms))
        s.status = "busy"
        Task { @MainActor [weak self, weak s] in
            try? await Task.sleep(for: .milliseconds(1200))
            guard let s else { return }
            s.rows.append(Row(key: "demo-a\(n)", kind: "markdown",
                              html: "<p>In the demo, no agent runs. Paired with harlo on your computer, your message goes to the session's agent there, and its reply streams back here as it works.</p>",
                              state: "done", time: ms + 1200))
            s.status = ""
            self?.demoStatus(s.key, "")
        }
        demoStatus(s.key, "busy")
        return true
    }

    /// A new session in the demo: on the list, opened, with its first message.
    func demoStart(project: Project?, agent: String, model: String, text: String) -> Bool {
        demoCount += 1
        let key = "demo-session-\(demoCount)"
        let title = String(text.split(separator: "\n").first ?? "").prefix(80)
        let now = Int64(Date.now.timeIntervalSince1970)
        sessions.insert(SessionSummary(["key": .string(key), "title": .string(String(title)), "project": .string(project?.name ?? ""),
                                        "harness": .string(agent), "status": "busy", "modified": .int(now)]), at: 0)
        let s = OpenSession(key: key, title: String(title), harness: agent)
        s.liveKey = key
        s.loading = false
        s.model = model.isEmpty ? "Default model" : String(model.split(separator: "/").last ?? "")
        s.info.model = s.model
        session = s
        return demoSend(text)
    }

    /// An answer in the demo: the agent goes on (a note says so).
    func demoAnswered(_ req: UIRequest, _ answer: JSON, in key: String) {
        if let i = sessions.firstIndex(where: { $0.key == key }) {
            sessions[i].requests.removeAll { $0.id == req.id }
            if sessions[i].requests.isEmpty && sessions[i].status == "input" { sessions[i].status = "busy" }
        }
        guard let s = session, s.key == key else { return }
        let value = answer["value"].string ?? (answer["confirmed"].bool == false ? "Deny" : "Approve")
        let denied = UIRequest.role(of: value) == .deny
        demoCount += 1
        s.rows.append(Row(key: "demo-n\(demoCount)", kind: "notice", text: denied ? "Denied: the agent won't run it." : "Approved: the agent goes on.",
                          title: denied ? "warning" : "info", time: Int64(Date.now.timeIntervalSince1970 * 1000)))
    }

    /// A demo session's state on the list.
    func demoStatus(_ key: String, _ status: String) {
        if let i = sessions.firstIndex(where: { $0.key == key }) { sessions[i].status = status }
    }
}

extension HarloModel {
    nonisolated static let sampleWiki = Wiki(pages: [
        WikiPage(path: "overview.md", title: "Overview", summary: "Where to start: what harlo is and how its parts fit.", kind: "Overview",
                 links: ["architecture/sessions.md"]),
        WikiPage(path: "architecture/sessions.md", title: "Sessions", summary: "How a session runs an agent and streams its transcript.", kind: "Architecture",
                 links: ["overview.md"], owes: 1),
        WikiPage(path: "workflows/release.md", title: "Releasing", summary: "How a release is built, signed and published.", kind: "Workflow",
                 links: ["architecture/sessions.md"]),
    ], work: Wiki.Work(count: 1, stale: 1), written: ISO8601DateFormatter().string(from: Date.now.addingTimeInterval(-3 * 86400)))

    nonisolated static func sampleWikiPage(_ path: String) -> WikiPageText {
        let page = sampleWiki.pages.first { $0.path == path } ?? sampleWiki.pages[0]
        let written = JSON.string(ISO8601DateFormatter().string(from: Date.now.addingTimeInterval(-3 * 86400)))
        let text: [String: JSON] = [
            "overview.md": ["path": "overview.md", "title": "Overview", "type": "Overview", "description": "Where to start: what harlo is and how its parts fit.",
                            "generated": ["by": "claude/claude-opus-5-5", "at": written],
                            "blocks": [["kind": "prose", "html": "<h1>Overview</h1><p>harlo runs coding agents in <a href=\"/architecture/sessions.md\">sessions</a>, each streaming its transcript to the desktop and the phone.</p>"],
                                       ["kind": "mermaid", "code": "flowchart LR\n  phone --> desktop --> agent"]],
                            "backlinks": [["path": "architecture/sessions.md", "title": "Sessions"]]],
            "architecture/sessions.md": ["path": "architecture/sessions.md", "title": "Sessions", "type": "Architecture", "description": .string(page.summary),
                                         "generated": ["by": "claude/claude-opus-5-5", "at": written],
                                         "blocks": [["kind": "prose", "html": "<h1>Sessions</h1><p>A session starts its agent and keeps its rows by key.<a href=\"okf-fn:start\">[1]</a> Back to the <a href=\"/overview.md\">overview</a>.</p>", "anchor": "sessions"],
                                                    ["kind": "code", "html": "<pre><code>let key = registry::start(kind, cwd);</code></pre>", "code": "let key = registry::start(kind, cwd);", "lang": "rust"]],
                                         "footnotes": [["n": 1, "label": "start", "path": "crates/harlo-core/src/session.rs", "locator": "fn start", "start": 31, "end": 40, "state": "stale"]],
                                         "items": ["Footnote 1: its code changed since the page was written (crates/harlo-core/src/session.rs, fn start)"],
                                         "backlinks": [["path": "overview.md", "title": "Overview"], ["path": "workflows/release.md", "title": "Releasing"]]],
        ]
        return WikiPageText(text[page.path] ?? ["path": .string(page.path), "title": .string(page.title), "type": .string(page.kind), "description": .string(page.summary),
                                                "edited": true, "blocks": [["kind": "prose", "html": .string("<h1>\(page.title)</h1><p>\(page.summary)</p>")]]])
    }

    nonisolated static func sampleWikiHits(_ query: String) -> WikiHits {
        WikiHits(["hits": [["page": "architecture/sessions.md", "section": "sessions", "heading": "Sessions",
                            "snippet": "A session starts its agent and keeps its rows by key."]],
                  "code": [["path": "src/session.rs", "name": "Session::start", "kind": "fn", "line": 42,
                            "signature": "pub fn start(&mut self, agent: Agent) -> Result<()>", "doc": "Starts the session's agent."]]])
    }

    nonisolated static func sampleCodeCard(_ code: WikiHits.Code) -> String {
        "\(code.signature)\n\(code.doc)\ncalls: Agent::spawn (src/agent.rs:20)\ncalled by:\n  Workspace::open (src/workspace.rs:88)"
    }
}
