import Foundation

/// harlo's rich text (markdown.rs: pulldown-cmark's HTML for one top-level
/// block, and highlighted code with token classes) as blocks of styled
/// spans, for native text views. Tags it doesn't know keep their text.
public enum RichBlock: Equatable, Sendable {
    case paragraph([Span])
    case heading(level: Int, [Span])
    /// Items, each its own blocks (a nested list is a block of its item).
    case list(ordered: Bool, start: Int, items: [[RichBlock]])
    case quote([RichBlock])
    /// Lines of highlighted code.
    case code([Span])
    case rule
    case table(header: [[Span]], rows: [[[Span]]])
}

/// A run of text with one style.
public struct Span: Equatable, Sendable {
    public var text: String
    public var bold = false
    public var italic = false
    public var code = false
    public var strike = false
    public var link: String?
    /// The token class of highlighted code (`.s1` … `.s11`; 0: none):
    /// 1 comment, 2 keyword, 3 string, 4 number, 5 function, 6 variable,
    /// 7 type, 8 operator, 9 diff added, 10 removed, 11 changed.
    public var token = 0

    public init(_ text: String, bold: Bool = false, italic: Bool = false, code: Bool = false, strike: Bool = false,
                link: String? = nil, token: Int = 0)
    {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.code = code
        self.strike = strike
        self.link = link
        self.token = token
    }

    func sameStyle(_ o: Span) -> Bool {
        bold == o.bold && italic == o.italic && code == o.code && strike == o.strike && link == o.link && token == o.token
    }
}

public enum RichText {
    public static func parse(_ html: String) -> [RichBlock] {
        let root = Node.parse(html)
        return blocks(root.children)
    }

    /// Plain text of spans (copying, accessibility).
    public static func plain(_ spans: [Span]) -> String { spans.map(\.text).joined() }

    /// Lines in code spans.
    public static func lineCount(_ spans: [Span]) -> Int {
        spans.reduce(1) { $0 + $1.text.filter { $0 == "\n" }.count }
    }

    /// The first `n` lines of code spans (a long block folded).
    public static func firstLines(_ spans: [Span], _ n: Int) -> [Span] {
        var out: [Span] = []
        var left = n
        for s in spans {
            guard left > 0 else { break }
            var piece = ""
            for ch in s.text {
                if ch == "\n" {
                    left -= 1
                    if left == 0 { break }
                }
                piece.append(ch)
            }
            var t = s
            t.text = piece
            if !piece.isEmpty { out.append(t) }
        }
        return out
    }

    /// One highlighted line of a file (harlo's `highlight_lines`).
    public static func codeLine(_ html: String) -> [Span] {
        inline(Node.parse(html).children, Style(code: true), keepSpace: true)
    }

    /// Blocks as readable plain text (copying a reply): paragraphs apart,
    /// list items marked, code as it is.
    public static func plainText(_ blocks: [RichBlock]) -> String {
        blocks.map { b -> String in
            switch b {
            case .paragraph(let s), .heading(_, let s), .code(let s): return plain(s)
            case .list(let ordered, let start, let items):
                return items.enumerated().map { i, item in (ordered ? "\(start + i). " : "- ") + plainText(item) }.joined(separator: "\n")
            case .quote(let inner): return plainText(inner).split(separator: "\n", omittingEmptySubsequences: false).map { "> " + $0 }.joined(separator: "\n")
            case .rule: return "---"
            case .table(let head, let rows):
                return ([head] + rows).filter { !$0.isEmpty }.map { $0.map(plain).joined(separator: "\t") }.joined(separator: "\n")
            }
        }.joined(separator: "\n\n")
    }

    // ---- tree → blocks -------------------------------------------------------

    static let blockTags: Set<String> = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "blockquote", "pre", "hr", "table", "div", "li"]

    static func blocks(_ nodes: [Node]) -> [RichBlock] {
        var out: [RichBlock] = []
        var loose: [Node] = []
        func flush() {
            let spans = trimmed(inline(loose, Style()))
            if !spans.isEmpty { out.append(.paragraph(spans)) }
            loose.removeAll()
        }
        for n in nodes {
            guard case .element(let tag, let attrs, let children) = n, blockTags.contains(tag) else {
                loose.append(n)
                continue
            }
            flush()
            switch tag {
            case "p", "div", "li":
                let spans = trimmed(inline(children, Style()))
                if !spans.isEmpty { out.append(.paragraph(spans)) }
            case "h1", "h2", "h3", "h4", "h5", "h6":
                out.append(.heading(level: Int(String(tag.dropFirst())) ?? 1, trimmed(inline(children, Style()))))
            case "ul", "ol":
                let items = children.compactMap { c -> [RichBlock]? in
                    guard case .element("li", _, let kids) = c else { return nil }
                    return blocks(kids)
                }
                out.append(.list(ordered: tag == "ol", start: Int(attrs["start"] ?? "") ?? 1, items: items))
            case "blockquote":
                out.append(.quote(blocks(children)))
            case "pre":
                var spans = inline(children, Style(code: true), keepSpace: true)
                if let last = spans.last, last.text.hasSuffix("\n") {
                    spans[spans.count - 1].text.removeLast()
                    if spans[spans.count - 1].text.isEmpty { spans.removeLast() }
                }
                out.append(.code(spans))
            case "hr":
                out.append(.rule)
            case "table":
                out.append(table(children))
            default:
                break
            }
        }
        flush()
        return out
    }

    static func table(_ nodes: [Node]) -> RichBlock {
        var header: [[Span]] = [], rows: [[[Span]]] = []
        func walk(_ nodes: [Node], inHead: Bool) {
            for n in nodes {
                guard case .element(let tag, _, let kids) = n else { continue }
                switch tag {
                case "thead": walk(kids, inHead: true)
                case "tbody": walk(kids, inHead: false)
                case "tr":
                    let cells = kids.compactMap { c -> [Span]? in
                        guard case .element(let t, _, let k) = c, t == "td" || t == "th" else { return nil }
                        return trimmed(inline(k, Style()))
                    }
                    if inHead && header.isEmpty { header = cells } else { rows.append(cells) }
                default: break
                }
            }
        }
        walk(nodes, inHead: false)
        return .table(header: header, rows: rows)
    }

    struct Style {
        var bold = false, italic = false, code = false, strike = false
        var link: String?
        var token = 0
    }

    static func inline(_ nodes: [Node], _ style: Style, keepSpace: Bool = false) -> [Span] {
        var out: [Span] = []
        func add(_ text: String, _ s: Style) {
            guard !text.isEmpty else { return }
            let span = Span(text, bold: s.bold, italic: s.italic, code: s.code, strike: s.strike, link: s.link, token: s.token)
            if let last = out.last, last.sameStyle(span) { out[out.count - 1].text += text } else { out.append(span) }
        }
        for n in nodes {
            switch n {
            case .text(let t):
                add(keepSpace || style.code ? t : collapse(t), style)
            case .element(let tag, let attrs, let kids):
                var s = style
                switch tag {
                case "strong", "b": s.bold = true
                case "em", "i": s.italic = true
                case "code": s.code = true
                case "del", "s", "strike": s.strike = true
                case "a": s.link = attrs["href"].flatMap { $0.hasPrefix("http://") || $0.hasPrefix("https://") || $0.hasPrefix("mailto:") ? $0 : nil }
                case "span":
                    if let c = attrs["class"], c.hasPrefix("s"), let n = Int(c.dropFirst()) { s.token = n }
                case "br":
                    add("\n", style)
                    continue
                case "img":
                    add(attrs["alt"].map { "[\($0)]" } ?? "[image]", style)
                    continue
                default: break
                }
                for span in inline(kids, s, keepSpace: keepSpace) { add(span.text, Style(bold: span.bold, italic: span.italic, code: span.code, strike: span.strike, link: span.link, token: span.token)) }
            }
        }
        return out
    }

    /// HTML's whitespace: runs of spaces and newlines read as one space.
    static func collapse(_ t: String) -> String {
        var out = "", space = false
        for ch in t {
            if ch == " " || ch == "\n" || ch == "\t" || ch == "\r" {
                if !space { out.append(" ") }
                space = true
            } else {
                out.append(ch)
                space = false
            }
        }
        return out
    }

    /// Without the spaces at either end (and none left empty).
    static func trimmed(_ spans: [Span]) -> [Span] {
        var s = spans
        while let f = s.first, !f.code, f.text.hasPrefix(" ") {
            s[0].text.removeFirst()
            if s[0].text.isEmpty { s.removeFirst() }
        }
        while let l = s.last, !l.code, l.text.hasSuffix(" ") {
            s[s.count - 1].text.removeLast()
            if s[s.count - 1].text.isEmpty { s.removeLast() }
        }
        return s
    }
}

/// A forgiving HTML tree: enough for the HTML harlo writes.
indirect enum Node: Equatable {
    case text(String)
    case element(String, [String: String], [Node])

    var children: [Node] { if case .element(_, _, let c) = self { return c }; return [] }

    static let void: Set<String> = ["br", "hr", "img", "input", "meta", "link", "col", "wbr"]

    static func parse(_ html: String) -> Node {
        // A stack of open elements: (tag, attributes, children so far).
        var stack: [(String, [String: String], [Node])] = [("", [:], [])]
        func append(_ n: Node) { stack[stack.count - 1].2.append(n) }
        func close(_ tag: String) {
            guard stack.dropFirst().contains(where: { $0.0 == tag }) else { return }
            while stack.count > 1 {
                let (t, a, c) = stack.removeLast()
                append(.element(t, a, c))
                if t == tag { break }
            }
        }
        let chars = Array(html)
        var i = 0
        var text = ""
        func flushText() {
            if !text.isEmpty { append(.text(decode(text))) }
            text = ""
        }
        while i < chars.count {
            let ch = chars[i]
            guard ch == "<", i + 1 < chars.count, chars[i + 1] == "/" || chars[i + 1].isLetter || chars[i + 1] == "!" else {
                text.append(ch)
                i += 1
                continue
            }
            guard let end = chars[i...].firstIndex(of: ">") else {
                text.append(ch)
                i += 1
                continue
            }
            flushText()
            let inner = String(chars[(i + 1)..<end])
            i = end + 1
            if inner.hasPrefix("!") { continue }
            if inner.hasPrefix("/") {
                close(inner.dropFirst().trimmingCharacters(in: .whitespaces).lowercased())
                continue
            }
            let (tag, attrs) = parseTag(inner)
            if void.contains(tag) || inner.hasSuffix("/") {
                append(.element(tag, attrs, []))
            } else {
                // A new list item or paragraph ends an open one.
                if tag == "li", stack.last?.0 == "li" { close("li") }
                if tag == "p", stack.last?.0 == "p" { close("p") }
                stack.append((tag, attrs, []))
            }
        }
        flushText()
        while stack.count > 1 {
            let (t, a, c) = stack.removeLast()
            append(.element(t, a, c))
        }
        return .element("", [:], stack[0].2)
    }

    static func parseTag(_ inner: String) -> (String, [String: String]) {
        var s = Substring(inner.hasSuffix("/") ? String(inner.dropLast()) : inner)
        let name = s.prefix { !$0.isWhitespace }
        s = s.dropFirst(name.count)
        var attrs: [String: String] = [:]
        while true {
            s = s.drop { $0.isWhitespace }
            guard !s.isEmpty else { break }
            let key = s.prefix { $0 != "=" && !$0.isWhitespace }
            guard !key.isEmpty else { break }
            s = s.dropFirst(key.count).drop { $0.isWhitespace }
            var value = ""
            if s.first == "=" {
                s = s.dropFirst().drop { $0.isWhitespace }
                if let q = s.first, q == "\"" || q == "'" {
                    s = s.dropFirst()
                    value = String(s.prefix { $0 != q })
                    s = s.dropFirst(value.count + 1)
                } else {
                    value = String(s.prefix { !$0.isWhitespace })
                    s = s.dropFirst(value.count)
                }
            }
            attrs[key.lowercased()] = decode(value)
        }
        return (name.lowercased(), attrs)
    }

    static let entities: [String: String] = ["amp": "&", "lt": "<", "gt": ">", "quot": "\"", "apos": "'", "#39": "'", "nbsp": "\u{a0}"]

    static func decode(_ s: String) -> String {
        guard s.contains("&") else { return s }
        var out = ""
        var rest = Substring(s)
        while let amp = rest.firstIndex(of: "&") {
            out += rest[..<amp]
            let after = rest[rest.index(after: amp)...]
            if let semi = after.prefix(10).firstIndex(of: ";") {
                let name = String(after[..<semi])
                var decoded: String?
                if let e = entities[name] { decoded = e }
                else if name.hasPrefix("#x") || name.hasPrefix("#X"), let n = UInt32(name.dropFirst(2), radix: 16), let u = Unicode.Scalar(n) { decoded = String(Character(u)) }
                else if name.hasPrefix("#"), let n = UInt32(name.dropFirst()), let u = Unicode.Scalar(n) { decoded = String(Character(u)) }
                if let decoded {
                    out += decoded
                    rest = after[after.index(after: semi)...]
                    continue
                }
            }
            out += "&"
            rest = after
        }
        return out + rest
    }
}
