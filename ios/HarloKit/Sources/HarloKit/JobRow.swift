import Foundation

/// Something a session has going beside its conversation, as harlo on the
/// computer lists it above the message box: a command or subagent it sent
/// to the background, or its goal. Each says what it is and offers the one
/// thing to do to it ("Stop", "Pause", "Resume", "Dismiss").
public struct JobRow: Equatable, Sendable, Identifiable {
    /// What harlo knows it by: sent back to have its action done.
    public var id: String
    /// Its SF Symbol.
    public var symbol: String
    /// What it is: "Command", "Goal", a subagent's name.
    public var title: String
    /// The command, the goal's words, the subagent's task.
    public var text: String
    /// More about where it stands ("Round 2 of 10", why a goal paused).
    public var detail: String
    /// Where it stands, in a word or two ("In the background", "Paused").
    public var note: String
    /// What can be done to it ("" nothing).
    public var action: String
    /// Still at work.
    public var working: Bool

    public init(_ v: JSON) {
        id = v["id"].text
        symbol = v["symbol"].text
        title = v["title"].text
        text = v["text"].text
        detail = v["detail"].text
        note = v["note"].text
        action = v["action"].text
        working = v["working"].bool ?? false
    }

    public init(id: String, symbol: String = "", title: String, text: String = "", detail: String = "",
                note: String = "", action: String = "", working: Bool = false)
    {
        self.id = id
        self.symbol = symbol
        self.title = title
        self.text = text
        self.detail = detail
        self.note = note
        self.action = action
        self.working = working
    }

    public static func list(_ v: JSON) -> [JobRow] {
        (v.array ?? []).map(JobRow.init).filter { !$0.id.isEmpty }
    }

    /// The whole row read aloud: "Command, npm run dev, In the background".
    public var spoken: String {
        [title, text, detail, note].filter { !$0.isEmpty }.joined(separator: ", ")
    }
}
