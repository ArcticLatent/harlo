import Foundation

// The demo: harlo's app without a harlo to connect to (for App Review, and
// for a look around before installing harlo on a computer). Sample sessions
// on a pretend computer; opening, answering, starting and switching work
// on them, and a message gets a canned reply. Nothing is saved, nothing
// connects anywhere, and leaving it (Forget) goes back to pairing.

extension HarloModel {
    /// The demo's computer.
    public static let demoName = "Demo computer"

    public func startDemo(now: Date = Date()) {
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
            s.modes = Modes(approval: "always-ask", approvals: ["always-ask", "write", "yolo"], plan: false)
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
        // A side question: answered beside the conversation, not in it.
        if text.hasPrefix("/btw ") {
            let question = String(text.dropFirst(5)).trimmingCharacters(in: .whitespaces)
            s.aside = Aside(["question": .string(question), "text": "", "done": false])
            Task { @MainActor [weak s] in
                try? await Task.sleep(nanoseconds: 900_000_000)
                s?.aside = Aside(["question": .string(question), "done": true,
                                  "text": "In the demo, no agent answers. Paired with harlo, Harlo answers a side question from the conversation so far, without adding it to the conversation."])
            }
            return true
        }
        demoCount += 1
        let n = demoCount, ms = Int64(Date().timeIntervalSince1970 * 1000)
        s.rows.append(Row(key: "demo-u\(n)", kind: "user", text: text, time: ms))
        s.status = "busy"
        Task { @MainActor [weak self, weak s] in
            try? await Task.sleep(nanoseconds: 1_200_000_000)
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
        let now = Int64(Date().timeIntervalSince1970)
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
                          title: denied ? "warning" : "info", time: Int64(Date().timeIntervalSince1970 * 1000)))
    }

    /// A demo session's state on the list.
    func demoStatus(_ key: String, _ status: String) {
        if let i = sessions.firstIndex(where: { $0.key == key }) { sessions[i].status = status }
    }
}

extension HarloModel {
    /// A plan edit in the demo (and the screenshots), done here.
    nonisolated static func demoEdit(_ plan: Plan, _ edit: PlanEdit) -> Plan {
        var p = plan
        func at(_ id: Int64) -> Int? { p.steps.firstIndex { $0.id == id } }
        switch edit {
        case .add(let t, let d, let after):
            var step = PlanStep(["id": .int((p.steps.map(\.id).max() ?? 0) + 1), "title": .string(t), "detail": .string(d), "editedByUser": true])
            step.status = "pending"
            let i = after.flatMap(at).map { $0 + 1 } ?? p.steps.count
            p.steps.insert(step, at: i)
        case .edit(let id, let t, let d):
            if let i = at(id) { p.steps[i].title = t; p.steps[i].detail = d; p.steps[i].editedByUser = true }
        case .remove(let id):
            p.steps.removeAll { $0.id == id }
        case .shift(let id, let by):
            if let i = at(id) {
                let j = max(0, min(p.steps.count - 1, i + by))
                p.steps.swapAt(i, j)
            }
        case .skip(let id, let skipped):
            if let i = at(id) { p.steps[i].status = skipped ? "skipped" : "pending" }
        case .alongside(let id, let on):
            if let i = at(id), i > 0 { p.steps[i].alongside = on }
        }
        return p
    }

    nonisolated static let sampleWiki = Wiki(pages: [
        WikiPage(path: "quickstart.md", title: "Quickstart", summary: "Where to start: what harlo is and how its parts fit.", kind: "overview",
                 links: ["architecture/sessions.md"]),
        WikiPage(path: "architecture/sessions.md", title: "Sessions", summary: "How a session runs an agent and streams its transcript.", kind: "architecture",
                 links: ["quickstart.md"]),
        WikiPage(path: "workflows/release.md", title: "Releasing", summary: "How a release is built, signed and published.", kind: "workflow",
                 links: ["architecture/sessions.md"]),
    ], updatedAt: ISO8601DateFormatter().string(from: Date().addingTimeInterval(-3 * 86400)), behind: 4)

    nonisolated static func sampleWikiPage(_ path: String) -> WikiPageText {
        let page = sampleWiki.pages.first { $0.path == path } ?? sampleWiki.pages[0]
        let text: [String: JSON] = [
            "quickstart.md": ["path": "quickstart.md", "title": "Quickstart", "type": "overview", "description": "Where to start: what harlo is and how its parts fit.",
                              "blocks": [["kind": "prose", "html": "<h1>Quickstart</h1><p>harlo runs coding agents in <a href=\"architecture/sessions.md\">sessions</a>, each streaming its transcript to the desktop and the phone.</p>"],
                                         ["kind": "mermaid", "code": "flowchart LR\n  phone --> desktop --> agent"]]],
            "architecture/sessions.md": ["path": "architecture/sessions.md", "title": "Sessions", "type": "architecture", "description": .string(page.summary),
                                         "verified": .string(ISO8601DateFormatter().string(from: Date().addingTimeInterval(-86400))),
                                         "blocks": [["kind": "prose", "html": "<h1>Sessions</h1><p>A session starts its agent in <a href=\"repo://crates/harlo-core/src/session.rs#L31-L40\">session.rs</a> and keeps its rows by key. Back to the <a href=\"../quickstart.md\">quickstart</a>.</p>", "anchor": "sessions"],
                                                    ["kind": "code", "html": "<pre><code>let key = registry::start(kind, cwd);</code></pre>", "code": "let key = registry::start(kind, cwd);", "lang": "rust"]],
                                         "sources": [["path": "crates/harlo-core/src/session.rs", "ranges": [[31, 40], [88, 102]], "claims": 3],
                                                     ["path": "crates/harlo-app/src/registry.rs", "ranges": [], "claims": 0]]],
        ]
        return WikiPageText(text[page.path] ?? ["path": .string(page.path), "title": .string(page.title), "type": .string(page.kind), "description": .string(page.summary),
                                                "blocks": [["kind": "prose", "html": .string("<h1>\(page.title)</h1><p>\(page.summary)</p>")]]])
    }

    nonisolated static func sampleWikiHits(_ query: String) -> WikiHits {
        WikiHits(["hits": [["page": "openwiki/architecture/sessions.md", "section": "sessions", "heading": "Sessions",
                            "snippet": "A session starts its agent and keeps its rows by key."]]])
    }

    nonisolated static func samplePlan() -> Plan {
        var plan = Plan(steps: [
            PlanStep(["id": 1, "title": "Keep the list's rows on a status change", "status": "done", "summary": "Rows are merged by key instead of rebuilt.", "commit": "3f9a2c41d0"]),
            PlanStep(["id": 2, "title": "Keep the scroll position", "status": "active", "detail": "Anchor to the first visible row."]),
            PlanStep(["id": 3, "title": "A test for reordering", "status": "pending", "alongside": true]),
        ])
        plan.commits = true
        plan.reviewers = [Agent(id: "claude", label: "Claude"), Agent(id: "harlo", label: "Harlo")]
        plan.team = Plan.Team(agent: "harlo", agentName: "Harlo", model: "codex/gpt-6.1-sol", custom: true, reviewerName: "Claude", reviewerModel: "claude-opus-5-5")
        return plan
    }
}
