import Foundation

/// A stage of a workflow run as its card shows it.
public struct WorkflowStage: Equatable, Sendable {
    /// Its name ("Implement", "Tests", "Review").
    public var label: String
    /// "done", "active", "asking" (under way, its session waiting on you),
    /// "paused" or "todo".
    public var status: String
    /// "implement", "check" or "review".
    public var kind: String
    /// Who or what does it: an agent's name, or a check's command in backticks.
    public var what: String
    /// An agent's stage: the model it runs on ("": the agent's default).
    public var model: String

    public init(_ v: JSON) {
        label = v["label"].text
        status = v["status"].string ?? "todo"
        kind = v["kind"].text
        what = v["what"].text
        model = v["model"].text
    }

    /// Who does it, with the model when one was picked: "pi · Opus 5.5".
    public var doer: String {
        model.isEmpty ? what : "\(what) · \(Self.modelName(model))"
    }

    /// A model's id as people say it: the server's name off the front, and a
    /// Claude id as its picker names it ("claude-opus-5-5" → "Opus 5.5").
    public static func modelName(_ id: String) -> String {
        let last = id.split(separator: "/").last.map(String.init) ?? id
        let parts = last.split(separator: "-").map(String.init)
        guard parts.count >= 3, parts[0] == "claude", let major = Int(parts[2]) else { return last }
        let family = parts[1].prefix(1).uppercased() + parts[1].dropFirst()
        let minor = parts.count > 3 ? Int(parts[3]) : nil
        return minor.map { "\(family) \(major).\($0)" } ?? "\(family) \(major)"
    }
}

/// A finished check of a run: its verdict and the end of its output.
public struct WorkflowCheck: Equatable, Sendable {
    public var name: String
    public var passed: Bool
    /// Its exit code (nil: it didn't run, or was stopped by a signal).
    public var exit: Int64?
    public var seconds: Int64
    public var tail: String
    /// The attempt it checked (0: a run from before harlo kept it).
    public var attempt: Int64

    public init(_ v: JSON) {
        name = v["name"].string ?? "Tests"
        passed = v["passed"].bool ?? false
        exit = v["exit"].int
        seconds = v["seconds"].int ?? 0
        tail = v["tail"].text
        attempt = v["attempt"].int ?? 0
    }

    /// How it ended, naming its attempt once the run has moved past it:
    /// "Attempt 2 · Tests passed in 3 s".
    public func title(runAttempt: Int64) -> String {
        let how: String
        if passed {
            how = "\(name) passed in \(seconds) s"
        } else if let exit {
            how = "\(name) failed (exit \(exit)) in \(seconds) s"
        } else {
            how = "\(name) couldn't finish"
        }
        return attempt > 0 && attempt != runAttempt ? "Attempt \(attempt) · \(how)" : how
    }
}

/// A check that's running: its output so far.
public struct WorkflowLiveCheck: Equatable, Sendable {
    public var name: String
    public var output: String
    public var seconds: Int64

    public init(_ v: JSON) {
        name = v["name"].text
        output = v["output"].text
        seconds = v["seconds"].int ?? 0
    }
}

/// A line of a run's log.
public struct WorkflowLogEntry: Equatable, Sendable {
    /// Unix seconds.
    public var at: Int64
    public var attempt: Int64
    /// "info", "good", "warn" or "bad".
    public var tone: String
    public var text: String

    public init(_ v: JSON) {
        at = v["at"].int ?? 0
        attempt = v["attempt"].int ?? 0
        tone = v["tone"].string ?? "info"
        text = v["text"].text
    }
}

/// A workflow run (Implement → checks → reviews) as harlo's card shows it.
public struct WorkflowRun: Equatable, Sendable, Identifiable {
    public var id: String
    /// The task's first line.
    public var title: String
    /// The workflow's name ("Implement and verify").
    public var workflow: String
    /// What a finished run says ("Tests passed · Review approved").
    public var doneText: String
    /// "running", "paused", "done" or "stopped".
    public var state: String
    /// Why it's paused ("" it isn't).
    public var why: String
    public var attempt: Int64
    public var stages: [WorkflowStage]
    /// What it waits for while it runs ("Running `cargo test`").
    public var waiting: String
    /// The open session's part in it: "implementation" or "review".
    public var role: String
    public var branch: String
    /// The branch it merges into.
    public var base: String
    public var implementer: String
    public var reviewer: String
    /// Its project's folder name, and its machine ("" this computer).
    public var project: String
    public var host: String
    /// When it started (unix seconds).
    public var createdAt: Int64
    public var lastCheck: WorkflowCheck?
    public var liveCheck: WorkflowLiveCheck?
    public var log: [WorkflowLogEntry]
    public var merged: Bool
    public var published: Bool
    public var removed: Bool
    /// One of its sessions waits on you (a question or an approval).
    public var asking: Bool
    /// That session's key ("" none).
    public var askingKey: String

    public init(_ v: JSON) {
        id = v["id"].text
        title = v["title"].text
        workflow = v["workflow"].text
        doneText = v["doneText"].text
        state = v["state"].string ?? "running"
        why = v["why"].text
        attempt = v["attempt"].int ?? 1
        stages = (v["stages"].array ?? []).map(WorkflowStage.init)
        waiting = v["waiting"].text
        role = v["role"].text
        branch = v["branch"].text
        base = v["base"].text
        implementer = v["implementer"].text
        reviewer = v["reviewer"].text
        project = v["project"].text
        host = v["host"].text
        createdAt = v["createdAt"].int ?? 0
        lastCheck = v["lastCheck"].object == nil ? nil : WorkflowCheck(v["lastCheck"])
        liveCheck = v["liveCheck"].object == nil ? nil : WorkflowLiveCheck(v["liveCheck"])
        log = (v["log"].array ?? []).map(WorkflowLogEntry.init)
        merged = v["merged"].bool ?? false
        published = v["published"].bool ?? false
        removed = v["removed"].bool ?? false
        asking = v["asking"].bool ?? false
        askingKey = v["askingKey"].text
        // (The stage under way waits on you too, while its session asks.)
        if asking { stages = stages.map { var s = $0; if s.status == "active" { s.status = "asking" }; return s } }
    }

    /// The card's title: the workflow and the attempt.
    public var heading: String {
        attempt > 1 ? "\(workflow) · attempt \(attempt)" : workflow
    }

    /// The line under its stages: why it waits, what it's doing, or how it
    /// ended.
    public var statusLine: String {
        switch state {
        case "paused": why
        case "done": "Done: " + doneText
        case "stopped": "Stopped"
        default: waiting
        }
    }

    /// It's finished and its work can still be merged, published or removed.
    public var finishable: Bool { state == "done" && !removed }

    /// It can be paused or stopped (it's running).
    public var running: Bool { state == "running" }
    public var paused: Bool { state == "paused" }

    /// It needs you: paused, or a session of it asking.
    public var needsYou: Bool { paused || asking }
}

/// What can be done to a run from the phone, as harlo names it.
public enum WorkflowAction: String, Sendable, CaseIterable {
    case pause, resume, stop, merge, publish, remove
}
