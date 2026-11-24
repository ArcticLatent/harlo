/// A tool call's output as code: a file's lines (a read, a printed file,
/// grep's numbered matches), highlighted by harlo as the desktop shows them.
public struct ToolCode: Equatable, Sendable {
    /// One line: its number ("" unknown), whether it's grep's context
    /// around a match, its code, or a file's name before its lines.
    public struct Line: Equatable, Sendable {
        public var number: String
        public var context: Bool
        public var spans: [Span]
        /// A file's name, on a line before that file's lines ("" on a code line).
        public var file: String

        public init(_ v: JSON) {
            number = v["n"].text
            context = v["c"].bool ?? false
            spans = RichText.codeLine(v["h"].text)
            file = v["f"].text
        }
    }

    /// The file it's about ("" when it lists several).
    public var path: String
    /// Its lines, at most the first ones harlo sent.
    public var lines: [Line]
    /// How many lines the output has in all.
    public var total: Int
    /// More lines than these: the rest are in harlo on the computer.
    public var more: Bool
    /// Kept to its end (a log): lines before these were left out.
    public var earlier: Bool

    /// None when the output isn't code.
    public init?(_ v: JSON) {
        guard let lines = v["lines"].array, !lines.isEmpty else { return nil }
        path = v["path"].text
        self.lines = lines.map(Line.init)
        total = Int(v["total"].int ?? Int64(lines.count))
        more = v["more"].bool ?? false
        earlier = v["earlier"].bool ?? false
    }

    /// Whether any line has grep's context: the others are then its matches.
    public var hasContext: Bool { lines.contains { $0.context } }
}
