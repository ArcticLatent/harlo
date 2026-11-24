import Foundation

/// One of a pi extension's views, as the phone shows it: its lines as
/// plain text, with the Markdown and pictures among them in `blocks`. A
/// screen or an overlay comes without lines: it takes the computer's keys,
/// so the phone says it's open there.
public struct ExtView: Equatable, Sendable, Identifiable {
    public var id: String
    /// "widget", "footer", "header", "screen", "overlay", "tool" or
    /// "message".
    public var kind: String
    public var lines: [String]
    /// A tool's result lines (its call's are `lines`).
    public var result: [String]
    /// `lines` in blocks, when they hold Markdown or a picture (none: the
    /// lines as they are).
    public var blocks: [ExtBlock]
    /// `result` in blocks, likewise.
    public var resultBlocks: [ExtBlock]
    /// The tool call it draws ("" none).
    public var toolCallId: String
    /// The custom message it draws, by the message's time (0 none).
    public var timestamp: Int64
    /// The extension's own entry it draws, by the entry's id ("" none).
    public var entryId: String
    /// Widgets: "aboveEditor" or "belowEditor".
    public var placement: String
    public var hidden: Bool

    /// Whether it takes the computer's keys (shown as open there).
    public var interactive: Bool { kind == "screen" || kind == "overlay" }

    public init(_ v: JSON) {
        id = v["id"].text
        kind = v["kind"].text
        lines = (v["lines"].array ?? []).compactMap(\.string)
        result = (v["result"].array ?? []).compactMap(\.string)
        blocks = ExtBlock.list(v["blocks"], lines: lines)
        resultBlocks = ExtBlock.list(v["resultBlocks"], lines: result)
        toolCallId = v["toolCallId"].text
        timestamp = v["timestamp"].int ?? 0
        entryId = v["entryId"].text
        placement = v["placement"].text
        hidden = v["hidden"].bool ?? false
    }
}

/// What changed of a session's extension views.
public enum ExtChange: Equatable, Sendable {
    case view(ExtView)
    case close(id: String)
    /// The extensions bound again: none of the views are left.
    case clear

    init?(_ v: JSON) {
        switch v["op"].text {
        case "view": self = .view(ExtView(v["view"]))
        case "close": self = .close(id: v["id"].text)
        case "clear": self = .clear
        default: return nil
        }
    }
}

extension Array where Element == ExtView {
    /// The list with `change` made (a view keeps its place as it changes).
    mutating func apply(_ change: ExtChange) {
        switch change {
        case .view(let v):
            if let i = firstIndex(where: { $0.id == v.id }) { self[i] = v } else { append(v) }
        case .close(let id):
            removeAll { $0.id == id }
        case .clear:
            removeAll()
        }
    }
}
