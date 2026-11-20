import Foundation

/// A piece of a pi extension's view, top to bottom (`ExtView.blocks`):
/// some of its lines, the Markdown among them, or a picture.
public enum ExtBlock: Equatable, Sendable {
    /// Lines, as plain text.
    case lines([String])
    /// Markdown, in parts drawn as a reply's are.
    case markdown([ExtMarkdownPart])
    /// A picture's bytes and type.
    case picture(data: Data, mime: String)
    /// A picture too big to send to a phone.
    case pictureTooBig

    /// The blocks of a view's `lines` as harlo sends them (`{kind, row,
    /// rows}` for lines, `{kind, parts}`, `{kind, data, mime}` or `{kind,
    /// tooBig}`): none when it sends none.
    static func list(_ v: JSON, lines: [String]) -> [ExtBlock] {
        (v.array ?? []).compactMap { b in
            switch b["kind"].text {
            case "lines":
                let row = max(0, b["row"].int ?? 0), rows = max(0, b["rows"].int ?? 0)
                let from = min(lines.count, Int(row)), to = min(lines.count, Int(row + rows))
                return .lines(Array(lines[from..<to]))
            case "markdown":
                return .markdown((b["parts"].array ?? []).map(ExtMarkdownPart.init))
            case "image":
                if b["tooBig"].bool == true { return .pictureTooBig }
                guard let data = Data(base64Encoded: b["data"].text) else { return nil }
                return .picture(data: data, mime: b["mime"].text)
            default:
                return nil
            }
        }
    }
}
