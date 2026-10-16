import Foundation

// Files in replies and in the message box: paths a reply mentions open in a
// viewer (`src/app.rs:42`), `@` mentions a file, and the session's branch.

/// A file of the session's folder, as harlo sends it to read: its lines,
/// highlighted (`.sN` token spans, as code blocks).
public struct FileText: Equatable, Sendable {
    /// Relative to the repository (or as asked).
    public var path: String
    /// Each line's HTML.
    public var lines: [String]
    /// Lines in the whole file.
    public var total: Int
    /// Longer than harlo sends: only the first lines are here.
    public var cut: Bool

    public init(_ v: JSON) {
        path = v["path"].text
        lines = (v["lines"].array ?? []).map(\.text)
        total = Int(v["total"].int ?? Int64(lines.count))
        cut = v["cut"].bool ?? false
    }

    /// Line `i` (from 0) as styled spans.
    public func spans(_ i: Int) -> [Span] {
        guard lines.indices.contains(i) else { return [] }
        return RichText.codeLine(lines[i])
    }
}

/// The session repository's branches (harlo-core's `git::branches`).
public struct Branches: Equatable, Sendable {
    public struct Branch: Equatable, Sendable, Identifiable {
        public var name: String
        public var upstream: String
        /// Commits not pushed.
        public var ahead: Int
        /// Unix time of its last commit.
        public var date: Int64
        public var id: String { name }
    }

    /// "" when detached.
    public var current: String
    /// Most recently committed first.
    public var local: [Branch]
    /// Remote branches without a local one of the same name (`origin/x`).
    public var remote: [Branch]
    /// Files with uncommitted changes.
    public var changed: Int

    public init(_ v: JSON) {
        current = v["current"].text
        func list(_ a: JSON) -> [Branch] {
            (a.array ?? []).map { Branch(name: $0["name"].text, upstream: $0["upstream"].text, ahead: Int($0["ahead"].int ?? 0), date: $0["date"].int ?? 0) }
        }
        local = list(v["local"])
        remote = list(v["remote"])
        changed = Int(v["changed"].int ?? 0)
    }
}

/// Paths in a reply, as links to open them (`harlo-file:?p=src/app.rs&l=42`).
public enum FileLinks {
    public static let scheme = "harlo-file"

    /// Extensions a path without a folder must have (so `self.rows` or
    /// `v1.2` don't read as files).
    static let known: Set<String> = [
        "rs", "swift", "ts", "tsx", "js", "jsx", "mjs", "py", "md", "json", "toml", "yml", "yaml", "qml", "sh", "zsh", "bash", "go", "c", "h", "cc",
        "cpp", "hpp", "m", "mm", "java", "kt", "kts", "gradle", "html", "css", "scss", "txt", "lock", "cfg", "ini", "conf", "xml", "plist", "sql",
        "rb", "php", "lua", "vue", "svelte", "dart", "cs", "fs", "ex", "exs", "erl", "hs", "ml", "nix", "zig", "env", "csv", "svg", "proto",
        "graphql", "tf", "dockerfile", "mdx", "rst", "tex", "pro", "pri", "cmake", "mk", "entitlements", "pbxproj", "storyboard", "xib",
    ]

    /// A path (and line) if `text` is one: `src/app.rs`, `app.rs:42`,
    /// `./x/y.ts#L7`, `Cargo.toml`.
    public static func path(_ text: String) -> (path: String, line: Int)? {
        var t = text.trimmingCharacters(in: .whitespaces)
        guard !t.isEmpty, t.count <= 240, !t.contains("://"), !t.contains(where: { $0.isWhitespace || "()[]{}<>\"'`,;|*?=$".contains($0) }) else { return nil }
        var line = 0
        if let hash = t.range(of: "#L"), let n = Int(t[hash.upperBound...].prefix { $0.isNumber }) {
            line = n
            t = String(t[..<hash.lowerBound])
        }
        // `:42` or `:42:7` at the end.
        let parts = t.split(separator: ":", omittingEmptySubsequences: false)
        if parts.count >= 2, let n = Int(parts[1]) {
            line = n
            t = String(parts[0])
            guard parts.count == 2 || (parts.count == 3 && Int(parts[2]) != nil) else { return nil }
        } else if parts.count > 1 {
            return nil
        }
        guard !t.hasPrefix("-"), !t.hasSuffix("/"), !t.hasPrefix("@"), t.contains(where: \.isLetter) else { return nil }
        let name = t.split(separator: "/").last.map(String.init) ?? t
        let ext = name.split(separator: ".").count > 1 ? name.split(separator: ".").last.map { $0.lowercased() } ?? "" : ""
        if t.contains("/") {
            // A folder's path is fine too only with a file at its end.
            guard !name.isEmpty, name != ".", name != "..", !ext.isEmpty || ["Makefile", "Dockerfile", "LICENSE", "README"].contains(name) else { return nil }
            guard ext.contains(where: \.isLetter) || ext.isEmpty else { return nil }
        } else {
            guard known.contains(ext) || ["Makefile", "Dockerfile", "Justfile"].contains(name) else { return nil }
            guard name.split(separator: ".").first.map({ !$0.isEmpty }) ?? false else { return nil }
        }
        return (t, line)
    }

    public static func url(_ path: String, line: Int) -> URL? {
        var c = URLComponents()
        c.scheme = scheme
        c.queryItems = [URLQueryItem(name: "p", value: path)] + (line > 0 ? [URLQueryItem(name: "l", value: String(line))] : [])
        return c.url
    }

    /// The path and line a link opens (nil: not a file's).
    public static func target(_ url: URL) -> (path: String, line: Int)? {
        guard url.scheme == scheme, let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let p = items.first(where: { $0.name == "p" })?.value, !p.isEmpty else { return nil }
        return (p, Int(items.first { $0.name == "l" }?.value ?? "") ?? 0)
    }

    /// Spans with the paths in them linked: a code span that's a path, and
    /// in plain text a word that's one with a folder (`crates/x/src/y.rs:12`).
    public static func link(_ spans: [Span]) -> [Span] {
        var out: [Span] = []
        for s in spans {
            if s.link != nil || s.token != 0 {
                out.append(s)
            } else if s.code {
                var l = s
                if let p = path(s.text) { l.link = url(p.path, line: p.line)?.absoluteString }
                out.append(l)
            } else {
                out += linkWords(s)
            }
        }
        return out
    }

    /// Plain text: words that are paths with a folder become links.
    static func linkWords(_ s: Span) -> [Span] {
        guard s.text.contains("/") else { return [s] }
        var out: [Span] = []
        var plain = ""
        func flush() {
            if !plain.isEmpty { var p = s; p.text = plain; out.append(p) }
            plain = ""
        }
        var word = ""
        func take() {
            // Trailing punctuation stays text ("see src/a.rs.").
            var w = Substring(word), tail = ""
            while let last = w.last, ".,;:!?)".contains(last) {
                tail = String(last) + tail
                w = w.dropLast()
            }
            if w.contains("/"), let p = path(String(w)), let u = url(p.path, line: p.line) {
                flush()
                var l = s
                l.text = String(w)
                l.link = u.absoluteString
                out.append(l)
                plain += tail
            } else {
                plain += word
            }
            word = ""
        }
        for ch in s.text {
            if ch.isWhitespace || "()[]\"'".contains(ch) {
                take()
                plain.append(ch)
            } else {
                word.append(ch)
            }
        }
        take()
        flush()
        return out
    }

    /// Blocks with their paths linked (code blocks stay as they are).
    public static func link(_ blocks: [RichBlock]) -> [RichBlock] {
        blocks.map { b in
            switch b {
            case .paragraph(let s): return .paragraph(link(s))
            case .heading(let n, let s): return .heading(level: n, link(s))
            case .list(let o, let start, let items): return .list(ordered: o, start: start, items: items.map(link))
            case .quote(let inner): return .quote(link(inner))
            case .table(let head, let rows): return .table(header: head.map(link), rows: rows.map { $0.map(link) })
            case .code, .rule: return b
            }
        }
    }
}

/// `@` in the message box: the file being mentioned, as typed so far.
public enum Mention {
    /// What follows the `@` the text ends with (nil: not mentioning).
    public static func query(in text: String) -> String? {
        guard let at = text.lastIndex(of: "@") else { return nil }
        let after = text[text.index(after: at)...]
        guard !after.contains(where: \.isWhitespace) else { return nil }
        // At the start, or after a space (not an e-mail address).
        if at > text.startIndex, !text[text.index(before: at)].isWhitespace { return nil }
        return String(after)
    }

    /// The text with the mention typed so far replaced by `path`.
    public static func insert(_ path: String, into text: String) -> String {
        guard query(in: text) != nil, let at = text.lastIndex(of: "@") else { return text + "@" + path + " " }
        return String(text[..<at]) + "@" + path + " "
    }
}
