import Foundation

// Sample content for screenshots of the app (its tour mode) without a
// harlo to connect to.

extension HarloModel {
    /// A paired, connected-looking model with sessions in two projects
    /// (`busier`: another approval waiting, for the screenshots).
    public static func sample(now: Date = Date(), busier: Bool = false) -> HarloModel {
        let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        m.samples = .standard
        m.host = PairedHarlo(key: Base64.encode(Data(repeating: 7, count: 32)), name: "archlinux", endpoints: [URL(string: "ws://192.168.1.20:7676/ws")!])
        m.state = .connected
        let t = Int64(now.timeIntervalSince1970)
        func s(_ key: String, _ title: String, _ project: String, _ harness: String, _ status: String, _ ago: Int64, branch: String = "", pinned: Bool = false,
               requests: [JSON] = []) -> SessionSummary {
            SessionSummary(["key": .string(key), "title": .string(title), "project": .string(project), "harness": .string(harness),
                            "status": .string(status), "modified": .int(t - ago), "branch": .string(branch), "where": "~/Projects/harlo", "pinned": .bool(pinned),
                            "requests": .array(requests)])
        }
        let approval: JSON = ["id": "r1", "method": "select", "title": "Allow tool: bash", "message": "bash: rm -rf target/debug/incremental",
                              "options": ["Approve", "Deny"], "approval": true, "asked": .int((t - 120) * 1000)]
        let command: JSON = ["id": "r2", "method": "select", "title": "Allow tool: bash", "message": "bash: cargo test -p harlo-relay",
                             "options": ["Approve", "Deny"], "approval": true, "asked": .int((t - 480) * 1000)]
        m.sessions = [
            s("live:1", "Fix the session list's scrolling", "harlo", "omp", "busy", 20, pinned: true),
            s("live:2", "Add photos to the phone composer", "harlo", "claude", "input", 300, branch: "phone-photos", requests: [approval]),
            s("live:3", "Why does the relay drop idle phones?", "harlo", "opencode", "unread", 3600),
            s("/s/4", "Release notes for 0.4", "harlo", "pi", "", 86400 * 2),
            s("/s/5", "Tidy the README", "website", "claude", "", 86400 * 5),
            s("/s/6", "Benchmark the markdown renderer", "", "omp", "", 86400 * 12),
        ]
        if busier {
            m.sessions.insert(s("live:5", "Run the relay's tests", "harlo", "claude", "input", 600, requests: [command]), at: 2)
        }
        return m
    }

    /// Opens a sample session: a finished exchange, a run of tool calls,
    /// code, todos, subagents, and (when `asking`) an approval waiting.
    public func openSample(asking: Bool = false, now: Date = Date()) {
        session = Self.sampleSession(asking: asking, now: now)
    }

    /// The sample session `openSample` opens.
    static func sampleSession(asking: Bool = false, now: Date = Date()) -> OpenSession {
        let s = OpenSession(key: "live:1", title: "Fix the session list's scrolling", harness: "omp")
        let ms = Int64(now.timeIntervalSince1970 * 1000)
        s.liveKey = "live:1"
        s.loading = false
        s.status = "busy"
        var info = SessionInfo()
        info.title = s.title
        info.model = "Claude Sonnet 4.5"
        info.modelId = "anthropic/claude-sonnet-4-5"
        info.thinking = "medium"
        info.contextTokens = 48_200
        info.contextWindow = 200_000
        s.info = info
        s.model = info.model
        s.modes = Modes(approval: "always-ask", approvals: ["always-ask", "write", "yolo"], plan: false)
        s.commands = [SlashCommand(name: "plan", description: "Plan before changing files"), SlashCommand(name: "compact", description: "Summarise the conversation so far")]
        s.rows = [
            Row(key: "u1", kind: "user", text: "The session list jumps to the top when a session finishes. Can you find out why and fix it?"),
            Row(key: "t1", kind: "thinking", text: "The list is rebuilt on every status change…", group: "g1", summary: "Thought, read 2 files, edited 1 file, ran 1 command",
                time: ms - 95_000, runMs: 18_000),
            Row(key: "c1", kind: "tool", text: "crates/harlo-app/qml/Sidebar.qml", title: "read", subtitle: "Sidebar.qml", state: "done", group: "g1", result: "214 lines"),
            Row(key: "c2", kind: "tool", text: "", title: "read", subtitle: "sidebar.rs", state: "done", group: "g1", result: "460 lines"),
            Row(key: "c2b", kind: "tool", text: "", title: "edit", subtitle: "sidebar.rs", state: "done", group: "g1", result: "+4 −2",
                diff: "     for project in input.projects {\n-        rows.clear();\n+        let fresh = project_rows(project, input);\n+        merge_by_key(&mut rows, fresh);"),
            Row(key: "c3", kind: "tool", text: "test result: ok. 112 passed", title: "bash", subtitle: "cargo test -p harlo-core sidebar", state: "done", group: "g1", result: "112 passed"),
            Row(key: "m1", kind: "markdown", html: """
                <p>Found it in <code>crates/harlo-core/src/sidebar.rs:212</code>: <code>rebuild()</code> replaces the <strong>whole model</strong> when a status changes, so the view resets its position.</p>
                <p>The fix keeps the rows and sends only what changed:</p>
                <ul><li>diff the old and new rows by key</li><li>update rows in place</li></ul>
                """, state: "done"),
            Row(key: "k1", kind: "code", html: """
                <pre><code><span class="s2">let</span> changes <span class="s8">=</span> <span class="s5">diff_by</span>(<span class="s8">&amp;</span>self.rows, <span class="s8">&amp;</span>new_rows, <span class="s8">|</span>a, b<span class="s8">|</span> a.key <span class="s8">==</span> b.key);
                <span class="s1">// Only what changed goes to the view.</span>
                self.<span class="s5">apply_changes</span>(changes);</code></pre>
                """, text: "let changes = diff_by(&self.rows, &new_rows, |a, b| a.key == b.key);", title: "rust", state: "done"),
            Row(key: "u2", kind: "user", text: "Great, go ahead and run the tests too."),
            Row(key: "c4", kind: "tool", text: "", title: "bash", subtitle: "cargo test --workspace", state: "running", group: "g2", summary: "Running 1 command", groupOpen: true,
                time: ms - 12_000),
        ]
        s.todos = TodoPhase.list([["name": "Fix", "tasks": [["content": "Find why the list resets", "status": "completed"],
                                                           ["content": "Update rows in place", "status": "completed"],
                                                           ["content": "Run the tests", "status": "in_progress"]]],
                                  ["name": "Check", "tasks": [["content": "Try it with 200 sessions", "status": "pending"]]]])
        s.followUp = ["Then update the changelog"]
        s.subagents = [Subagent(["id": "a1", "agent": "scout", "description": "Look for other full rebuilds", "status": "running"])]
        if asking {
            s.requests = [UIRequest(["id": "r1", "method": "select", "title": "Allow tool: bash", "message": "cargo test --workspace --release",
                                     "options": ["Approve", "Deny"], "approval": true])]
        }
        return s
    }
}

/// What a sample model answers instead of asking harlo.
public struct Samples: Sendable {
    public var choices: NewSessionChoices
    public var models: [ModelChoice]
    public var changes: Changes
    public var diff: FileDiff
    public var actions = ActionLog(["commands": 2, "failed": 1, "changed": ["crates/harlo-core/src/sidebar.rs"], "read": ["crates/harlo-core/src/listdiff.rs"], "actions": [
        ["ts": 1_760_000_000_000, "kind": "read", "tool": "read", "target": "crates/harlo-core/src/listdiff.rs", "result": "214 lines"],
        ["ts": 1_760_000_020_000, "kind": "edit", "tool": "edit", "target": "crates/harlo-core/src/sidebar.rs", "result": "+8 −2"],
        ["ts": 1_760_000_040_000, "kind": "command", "tool": "bash", "target": "cargo test -p harlo-core", "result": "1 failed", "error": true],
        ["ts": 1_760_000_090_000, "kind": "command", "tool": "bash", "target": "cargo test -p harlo-core", "result": "128 passed"],
    ]])
    public var markdown = "# Fix the session list's scrolling\n\n*Oh My Pi · Claude Sonnet 4.5 · `~/Projects/harlo`*\n\n---\n\n## You\n\nThe session list jumps to the top when a session's status changes. Keep the scroll position.\n\n## Oh My Pi\n\n- **read** `crates/harlo-core/src/sidebar.rs` → 214 lines\n- **edit** `crates/harlo-core/src/sidebar.rs` → +8 −2\n\nThe list is now updated in place, so it keeps its position.\n"
    public var files: [String] = ["crates/harlo-core/src/sidebar.rs", "crates/harlo-app/qml/Sidebar.qml", "crates/harlo-core/src/listdiff.rs", "README.md"]
    public var file = FileText(["path": "crates/harlo-core/src/sidebar.rs", "total": 4, "lines": [
        "<span class=\"s2\">pub</span> <span class=\"s2\">fn</span> <span class=\"s5\">build</span>(input: <span class=\"s8\">&amp;</span><span class=\"s7\">SidebarInput</span>) -&gt; <span class=\"s7\">Vec</span>&lt;<span class=\"s7\">SidebarRow</span>&gt; {",
        "    <span class=\"s2\">let</span> <span class=\"s2\">mut</span> rows <span class=\"s8\">=</span> <span class=\"s7\">Vec</span>::<span class=\"s5\">new</span>();",
        "    <span class=\"s1\">// Keep the rows; only what changed goes to the view.</span>",
        "    rows",
    ]])
    public var branches = Branches(["current": "main", "changed": 3, "local": [
        ["name": "main", "upstream": "origin/main", "ahead": 1, "date": 1_760_000_000],
        ["name": "phone-photos", "upstream": "", "ahead": 0, "date": 1_759_900_000]],
        "remote": [["name": "origin/relay-limits", "date": 1_759_800_000]]])

    public static let standard = Samples(
        choices: NewSessionChoices(["projects": [["id": 1, "name": "harlo", "where": "~/Projects/harlo", "agent": "omp"],
                                                 ["id": 2, "name": "website", "where": "~/Projects/website", "agent": "claude"]],
                                    "agents": [["id": "claude", "label": "Claude"], ["id": "opencode", "label": "OpenCode"], ["id": "pi", "label": "Pi"], ["id": "omp", "label": "Oh My Pi"]],
                                    "defaultAgent": "omp",
                                    "approvals": ["claude": "always-ask", "opencode": "always-ask", "pi": "", "omp": "write"],
                                    "models": [["harness": "omp", "model": "anthropic/claude-sonnet-4-5"], ["harness": "omp", "model": "openai/gpt-5"]],
                                    "templates": [["id": "t1", "name": "Review a PR", "harness": "claude", "approval": "always-ask", "text": "Review the open pull request: correctness first, then tests."],
                                                  ["id": "t2", "name": "Fix the failing tests", "harness": "omp", "model": "openai/gpt-5", "approval": "write", "text": "Run the tests and fix what fails."]]]),
        models: [ModelChoice(["provider": "anthropic", "id": "claude-sonnet-4-5", "name": "Claude Sonnet 4.5", "reasoning": true]),
                 ModelChoice(["provider": "anthropic", "id": "claude-haiku-4-5", "name": "Claude Haiku 4.5", "reasoning": true]),
                 ModelChoice(["provider": "openai", "id": "gpt-5", "name": "GPT-5", "reasoning": true]),
                 ModelChoice(["provider": "openai", "id": "gpt-5-mini", "name": "GPT-5 mini", "reasoning": false])],
        changes: Changes(["branch": "main", "ahead": 1, "files": [
            ["path": "crates/harlo-core/src/sidebar.rs", "status": "M"],
            ["path": "crates/harlo-app/qml/Sidebar.qml", "status": "M"],
            ["path": "crates/harlo-core/tests/sidebar_order.rs", "status": "?", "untracked": true],
            ["path": "docs/old-notes.md", "status": "D"]]]),
        diff: FileDiff(["path": "crates/harlo-core/src/sidebar.rs", "added": 4, "removed": 2, "hunks": [[
            "header": "@@ -212,9 +212,11 @@ pub fn build(input: &SidebarInput) -> Vec<SidebarRow> {",
            "lines": [["k": " ", "o": 212, "n": 212, "t": "    let mut rows = Vec::new();"],
                      ["k": " ", "o": 213, "n": 213, "t": "    for project in input.projects {"],
                      ["k": "-", "o": 214, "t": "        rows.clear();"],
                      ["k": "-", "o": 215, "t": "        rows.extend(project_rows(project, input));"],
                      ["k": "+", "n": 214, "t": "        // Keep the rows; only what changed goes to the view."],
                      ["k": "+", "n": 215, "t": "        let fresh = project_rows(project, input);"],
                      ["k": "+", "n": 216, "t": "        merge_by_key(&mut rows, fresh);"],
                      ["k": "+", "n": 217, "t": "        rows.sort_by_key(|r| r.position);"],
                      ["k": " ", "o": 216, "n": 218, "t": "    }"],
                      ["k": " ", "o": 217, "n": 219, "t": "    rows"]]]]])
    )
}
