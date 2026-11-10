// Tool calls named as the desktop names them (crates/harlo-app/qml/Theme.qml
// `toolLabel`, TranscriptView's `toolName`), for every agent: Harlo's and
// Claude Code's tools, an MCP server's, harlo's own and the shared plan's.

import Foundation

public enum ToolName {
    /// harlo's own tools (Claude's `mcp__harlo__x`, others' `harlo_x`,
    /// Harlo's own `x`): what they do.
    static let harlo: [String: String] = [
        "propose_helpers": "harlo: offer a split",
        "start_session": "harlo: start a session",
        "send_message": "harlo: message a session",
        "read_session": "harlo: read a session",
        "list_sessions": "harlo: list sessions",
        "list_agents": "harlo: list agents",
        "stop_session": "harlo: stop a session",
        "request_approval": "harlo: ask you to approve",
        "report_done": "harlo: report back",
        "ask_parent": "harlo: ask the session that started it",
        "bring_over": "harlo: bring over a helper's changes",
    ]

    /// The shared plan's tools.
    static let plan: [String: String] = [
        "plan_set": "Plan", "plan_read": "Read the plan", "plan_start": "Start a step", "plan_done": "Finish a step",
    ]

    /// The project's specs' tools.
    static let specs: [String: String] = [
        "spec_graph": "Look at the specs", "spec_list": "List the specs", "spec_search": "Search the specs", "spec_read": "Read a spec",
        "spec_validate": "Check the specs",
    ]

    /// A tool's name without harlo's prefix (`mcp__harlo__x`, `harlo_x`;
    /// Harlo calls them by their own names).
    static func harloTool(_ name: String) -> String {
        for prefix in ["mcp__harlo__", "harlo_"] where name.hasPrefix(prefix) {
            return String(name.dropFirst(prefix.count))
        }
        return name
    }

    /// An MCP server's tool (`mcp__<server>__<tool>`): the server and the
    /// tool.
    public static func mcp(_ name: String) -> (server: String, tool: String)? {
        guard name.hasPrefix("mcp__") else { return nil }
        let rest = name.dropFirst(5)
        guard let sep = rest.range(of: "__"), sep.lowerBound > rest.startIndex else { return nil }
        let tool = rest[sep.upperBound...]
        return tool.isEmpty ? nil : (String(rest[..<sep.lowerBound]), String(tool))
    }

    /// The call's name as words: "Read file", "Ran command", "Asked you",
    /// "context-mode ctx execute", "harlo: start a session".
    public static func words(_ tool: String, running: Bool) -> String {
        let h = harloTool(tool)
        if let said = plan[h] ?? specs[h] ?? harlo[h] { return said }
        if let m = mcp(tool) { return m.server + " " + m.tool.replacingOccurrences(of: "_", with: " ") }
        let pick = { (now: String, done: String) in running ? now : done }
        switch tool.lowercased() {
        case "read", "view", "cat": return pick("Reading file", "Read file")
        case "edit", "multiedit", "str_replace", "patch", "hashline_edit", "notebookedit": return pick("Editing file", "Edited file")
        case "apply_patch": return pick("Editing files", "Edited files")
        case "write": return pick("Writing file", "Wrote file")
        case "bash", "shell", "exec", "run", "command": return pick("Running command", "Ran command")
        case "bashoutput": return pick("Reading a command's output", "Read a command's output")
        case "killshell", "killbash": return pick("Stopping a command", "Stopped a command")
        case "jobs": return pick("Checking background commands", "Checked background commands")
        case "grep", "search", "rg": return pick("Searching", "Searched")
        case "glob", "find", "ls": return pick("Listing files", "Listed files")
        case "fetch", "webfetch": return pick("Fetching", "Fetched")
        case "web_search", "websearch": return pick("Searching the web", "Searched the web")
        case "task", "agent", "subagent": return "Subagent"
        case "todo", "todowrite", "todoread": return pick("Updating the todos", "Updated the todos")
        case "run_code": return pick("Running a program", "Ran a program")
        case "lsp": return pick("Looking up the code", "Looked up the code")
        case "current_time": return "Checked the time"
        case "ask", "askuserquestion": return pick("Asking you", "Asked you")
        case "mcp_resources": return pick("Reading a server's resources", "Read a server's resources")
        case "exitplanmode": return "Plan ready"
        case "skill": return pick("Using a skill", "Used a skill")
        default:
            // (As the desktop says one it doesn't know: "web_thing" → "Web thing".)
            guard !tool.isEmpty else { return "Tool" }
            let spaced = tool.replacingOccurrences(of: "_", with: " ")
            return spaced.prefix(1).uppercased() + spaced.dropFirst()
        }
    }

    /// Its SF Symbol.
    public static func symbol(_ tool: String) -> String {
        let h = harloTool(tool)
        if plan[h] != nil { return "checklist" }
        if specs[h] != nil { return "doc.text.magnifyingglass" }
        if harlo[h] != nil { return "person.2" }
        if mcp(tool) != nil { return "puzzlepiece.extension" }
        switch tool.lowercased() {
        case "read", "view", "cat", "glob", "ls", "find": return "doc.text"
        case "edit", "multiedit", "write", "patch", "apply_patch", "str_replace", "hashline_edit", "notebookedit": return "pencil.line"
        case "bash", "shell", "exec", "run", "command", "bashoutput", "killshell", "killbash", "jobs": return "terminal"
        case "grep", "search", "rg": return "magnifyingglass"
        case "fetch", "webfetch", "web_search", "websearch": return "globe"
        case "task", "agent", "subagent": return "person.2"
        case "todo", "todowrite", "todoread": return "checklist"
        case "run_code": return "chevron.left.forwardslash.chevron.right"
        case "lsp": return "curlybraces"
        case "current_time": return "clock"
        case "ask", "askuserquestion": return "questionmark.bubble"
        case "mcp_resources": return "tray.full"
        case "exitplanmode": return "checklist"
        case "skill": return "sparkles"
        default: return "wrench.and.screwdriver"
        }
    }

    /// Whether the line under its name is words (a question, a plan's
    /// steps, a subagent's task), not a path or a command: shown in the
    /// text font, cut at its end rather than in the middle.
    public static func saysWords(_ tool: String) -> Bool {
        if plan[harloTool(tool)] != nil { return true }
        switch tool.lowercased() {
        case "ask", "askuserquestion", "task", "agent", "subagent", "todo", "todowrite", "exitplanmode",
             "web_search", "websearch", "skill":
            return true
        default:
            return false
        }
    }

    /// One of harlo's own tools (start a session, bring over a helper, …).
    public static func isHarlo(_ tool: String) -> Bool { harlo[harloTool(tool)] != nil }

    /// The line under a tool's name, and whether it's words: harlo's own
    /// tools say the session's title or what it's asked rather than their
    /// input ("agent=harlo  title=Update the changelog  worktree=…").
    public static func line(_ tool: String, _ subtitle: String) -> (text: String, words: Bool) {
        guard harlo[harloTool(tool)] != nil else { return (subtitle, saysWords(tool)) }
        var args: [String: String] = [:]
        for part in subtitle.components(separatedBy: "  ") {
            guard let eq = part.firstIndex(of: "=") else { continue }
            args[String(part[..<eq]).trimmingCharacters(in: .whitespaces)] = String(part[part.index(after: eq)...]).trimmingCharacters(in: .whitespaces)
        }
        for key in ["title", "message", "prompt", "task"] {
            if let v = args[key], !v.isEmpty { return (v, true) }
        }
        return (subtitle, false)
    }

    /// Whether what matters in its output is the end (a command's result),
    /// not the start (a file's first lines).
    public static func isCommand(_ tool: String) -> Bool {
        symbol(tool) == "terminal"
    }
}

extension Row {
    /// Its output as a card shows it, at most `limit` characters: a
    /// command's end (where its result is), anything else's start.
    public func shownOutput(limit: Int = 6000) -> String {
        guard text.count > limit else { return text }
        if !command.isEmpty || ToolName.isCommand(title) {
            var tail = text.suffix(limit)
            // (From a whole line.)
            if let nl = tail.firstIndex(of: "\n") { tail = tail[tail.index(after: nl)...] }
            return "…\n" + tail
        }
        return String(text.prefix(limit)) + "\n…"
    }
}
