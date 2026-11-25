// Tool calls named for people: Harlo's tools, an MCP server's, and older
// sessions' tool names.

import Foundation

public enum ToolName {
    /// harlo's own tools (Claude's `mcp__harlo__x`, others' `harlo_x`,
    /// Harlo's own `x`): what they do.
    static let harlo: [String: String] = [
        "read_session": "harlo: read a session",
        "list_sessions": "harlo: list sessions",
        "request_approval": "harlo: ask you to approve",
        "report_done": "harlo: report back",
        "ask_parent": "harlo: ask the session that started it",
    ]

    /// The project wiki's tool (a wiki session's).
    static let wiki: [String: String] = [
        "wiki_check": "Check the wiki",
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
        if let said = wiki[h] ?? harlo[h] { return said }
        if let m = mcp(tool) { return m.server + " " + m.tool.replacing("_", with: " ") }
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
        case "execute": return pick("Running a script", "Ran a script")
        case "remember", "remember_user": return pick("Keeping a note", "Kept a note")
        case "goal": return pick("Setting the goal", "Set the goal")
        case "search_sessions": return pick("Searching past sessions", "Searched past sessions")
        case "lsp": return pick("Looking up the code", "Looked up the code")
        case "ast_grep": return pick("Searching the code's structure", "Searched the code's structure")
        case "ast_edit": return pick("Rewriting by structure", "Rewrote by structure")
        case "rename": return pick("Renaming a symbol", "Renamed a symbol")
        case "locate": return pick("Locating in the code", "Located in the code")
        case "debug": return pick("Debugging", "Debugged")
        case "memory": return pick("Keeping a note", "Kept a note")
        case "wait": return pick("Waiting for subagents", "Waited for subagents")
        case "current_time": return "Checked the time"
        case "ask", "askuserquestion": return pick("Asking you", "Asked you")
        case "mcp_resources": return pick("Reading a server's resources", "Read a server's resources")
        case "exitplanmode": return "Plan ready"
        case "skill": return pick("Using a skill", "Used a skill")
        default:
            // (As the desktop says one it doesn't know: "web_thing" → "Web thing".)
            guard !tool.isEmpty else { return "Tool" }
            let spaced = tool.replacing("_", with: " ")
            return spaced.prefix(1).uppercased() + spaced.dropFirst()
        }
    }

    /// Its SF Symbol.
    public static func symbol(_ tool: String) -> String {
        let h = harloTool(tool)
        if wiki[h] != nil { return "book" }
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
        case "run_code", "execute": return "chevron.left.forwardslash.chevron.right"
        case "remember", "remember_user": return "note.text"
        case "goal": return "scope"
        case "search_sessions": return "magnifyingglass"
        case "lsp": return "curlybraces"
        case "ast_grep", "locate": return "magnifyingglass"
        case "ast_edit", "rename": return "pencil.line"
        case "debug": return "ladybug"
        case "memory": return "note.text"
        case "wait": return "person.2"
        case "current_time": return "clock"
        case "ask", "askuserquestion": return "questionmark.bubble"
        case "mcp_resources": return "tray.full"
        case "exitplanmode": return "checklist"
        case "skill": return "sparkles"
        default: return "wrench.and.screwdriver"
        }
    }

    /// Whether the line under its name is words (a question, a plan, a
    /// subagent's task), not a path or a command: shown in the text font,
    /// cut at its end rather than in the middle.
    public static func saysWords(_ tool: String) -> Bool {
        switch tool.lowercased() {
        case "ask", "askuserquestion", "task", "agent", "subagent", "todo", "todowrite", "exitplanmode",
             "web_search", "websearch", "skill", "goal", "remember", "remember_user", "search_sessions":
            return true
        default:
            return false
        }
    }

    /// One of harlo's own tools (read a session, report a workflow stage, …).
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
