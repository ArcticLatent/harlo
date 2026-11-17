import Foundation

extension Row {
    /// A workflow's message to its agent, which starts "[harlo workflow
    /// <id> · attempt N · part]": what it's for, in a line ("Sent back for
    /// attempt 2: tests failed after attempt 1"); nil for anything else.
    public var workflowNote: String? {
        guard kind == "user", text.hasPrefix("[harlo workflow ") else { return nil }
        let first = text.prefix { $0 != "\n" }
        guard first.hasSuffix("]") else { return nil }
        let parts = first.dropFirst().dropLast().components(separatedBy: " · ")
        guard parts.count >= 3, parts[1].hasPrefix("attempt "), let attempt = Int(parts[1].dropFirst("attempt ".count)) else { return nil }
        let body = text.dropFirst(first.count).drop { $0 == "\n" }
        if parts[2].hasPrefix("review") { return "Asked for a review of attempt \(attempt)" }
        if body.hasPrefix("You ended your turn without calling report_done") { return "Reminded the agent to report attempt \(attempt)" }
        if attempt == 1 { return "Sent the task" }
        // Why it came back: "Tests failed after attempt 1: …", "Review of
        // attempt 2 asked for changes (the reviewer's words, …):".
        let why = body.split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { $0.contains(" failed") || $0.contains("asked for changes") } ?? ""
        let reason = String(why.prefix { $0 != ":" }).components(separatedBy: " (")[0]
        guard let head = reason.first else { return "Sent back for attempt \(attempt)" }
        return "Sent back for attempt \(attempt): " + head.lowercased() + reason.dropFirst()
    }

    /// A workflow agent's report (a `report_done` call), read from its
    /// argument line ("outcome=…  summary=…  attempt=2"); nil for any other
    /// row.
    public var workflowReport: WorkflowReport? {
        guard kind == "tool", ToolName.harloTool(title) == "report_done" else { return nil }
        // Fields are two spaces apart; a value's own double space stays in it.
        var fields: [String: String] = [:]
        var last = ""
        for piece in subtitle.components(separatedBy: "  ") {
            if let eq = piece.firstIndex(of: "="), !piece[..<eq].isEmpty, piece[..<eq].allSatisfy({ $0.isLetter || $0 == "_" }) {
                last = String(piece[..<eq])
                fields[last] = String(piece[piece.index(after: eq)...])
            } else if !last.isEmpty {
                fields[last, default: ""] += "  " + piece
            }
        }
        return WorkflowReport(
            attempt: fields["attempt"].flatMap { Int($0.trimmingCharacters(in: .whitespaces)) },
            summary: fields["summary"]?.trimmingCharacters(in: .whitespaces) ?? "",
            superseded: state == "error" && text.contains("This one isn't used.")
        )
    }
}
