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
            s.modes = summary.harness == "pi" ? nil : Modes(approval: "always-ask", approvals: ["always-ask", "write", "yolo"], plan: false)
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
