import Foundation

/// A page of the session's repository's wiki (Open Knowledge Format, in
/// .okf/).
public struct WikiPage: Equatable, Sendable, Identifiable {
    /// Within .okf/ ("architecture/jobs.md").
    public var path: String
    public var title: String
    public var summary: String
    /// Its type ("Overview", "Architecture", "Decision", …; "" when it
    /// doesn't say).
    public var kind: String
    /// The pages it links to.
    public var links: [String]
    /// How many things it owes the code (footnotes whose code changed, new
    /// code it covers).
    public var owes: Int
    /// Edited since harlo last checked it (or never checked).
    public var edited: Bool
    public var id: String { path }

    public init(path: String, title: String, summary: String = "", kind: String = "", links: [String] = [], owes: Int = 0, edited: Bool = false) {
        self.path = path
        self.title = title
        self.summary = summary
        self.kind = kind
        self.links = links
        self.owes = owes
        self.edited = edited
    }

    /// Its folder as a heading ("architecture/jobs.md" → "Architecture";
    /// "" at the top).
    public var group: String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        let folder = path[..<slash].split(separator: "/").last.map(String.init) ?? ""
        let words = folder.replacing("-", with: " ").replacing("_", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }

    /// Under its title in the list: what it owes, or that it's not checked.
    public var mark: String {
        if owes > 0 { return owes == 1 ? "1 thing to do" : "\(owes) things to do" }
        return edited ? "Edited, not checked yet" : ""
    }
}

/// The wiki of a session's repository: its pages (the overview first) and
/// what it owes the code.
public struct Wiki: Equatable, Sendable {
    /// Inside a harlo project (a wiki is a project's).
    public var inProject: Bool
    /// Its agents are pointed to it (the project's Wiki setting).
    public var enabled: Bool
    public var exists: Bool
    public var pages: [WikiPage]
    /// What it owes the code, by kind.
    public var work: Work
    /// When a page was last written (ISO 8601; "" never).
    public var written: String
    /// A wiki session is at work on it.
    public var running: Bool
    /// That session's key.
    public var session: String
    /// Still being read.
    public var loading: Bool
    /// harlo checking, with no model, what there is to do before a session.
    public var checking: Bool
    /// Why it couldn't be read ("": it could): its machine didn't answer, say.
    public var error: String
    /// The repository's folder (on its machine): code links' files are in it.
    public var root: String

    /// What the wiki owes the code: footnotes whose code changed, new
    /// declarations no page mentions, files no page covers, footnotes that
    /// find no code.
    public struct Work: Equatable, Sendable {
        public var count: Int
        public var stale: Int
        public var new: Int
        public var unowned: Int
        public var broken: Int
        public init(count: Int = 0, stale: Int = 0, new: Int = 0, unowned: Int = 0, broken: Int = 0) {
            self.count = count
            self.stale = stale
            self.new = new
            self.unowned = unowned
            self.broken = broken
        }
    }

    public init(_ v: JSON) {
        inProject = (v["project"].int ?? -1) >= 0
        enabled = v["enabled"].bool ?? false
        exists = v["exists"].bool ?? false
        pages = (v["pages"].array ?? []).map {
            WikiPage(path: $0["path"].text, title: $0["title"].text, summary: $0["description"].text, kind: $0["type"].text,
                     links: ($0["links"].array ?? []).map(\.text), owes: Int($0["owes"].int ?? 0), edited: $0["edited"].bool ?? false)
        }
        let w = v["work"]
        work = Work(count: Int(w["count"].int ?? 0), stale: Int(w["stale"].int ?? 0), new: Int(w["new"].int ?? 0),
                    unowned: Int(w["unowned"].int ?? 0), broken: Int(w["broken"].int ?? 0))
        written = v["last"].text
        running = v["running"].bool ?? false
        session = v["session"].text
        loading = v["loading"].bool ?? false
        checking = v["checking"].bool ?? false
        error = v["error"].text
        root = v["root"].text
    }

    public init(pages: [WikiPage] = [], inProject: Bool = true, enabled: Bool = true, exists: Bool = true, work: Work = Work(), written: String = "",
                running: Bool = false, session: String = "", loading: Bool = false, checking: Bool = false, error: String = "", root: String = "")
    {
        self.pages = pages
        self.inProject = inProject
        self.enabled = enabled
        self.exists = exists
        self.work = work
        self.written = written
        self.running = running
        self.session = session
        self.loading = loading
        self.checking = checking
        self.error = error
        self.root = root
    }

    /// Where it stands, in phrases (as the desktop's Wiki panel says it):
    /// "1 footnote's code changed", "2 new declarations no page mentions",
    /// "Up to date", "written 3 days ago".
    public func freshness(now: Date = Date.now) -> [String] {
        if running { return ["A session is writing the wiki."] }
        if checking { return ["Checking what changed…"] }
        if loading { return ["Reading the wiki…"] }
        func n(_ k: Int, _ one: String, _ many: String) -> String { "\(k) " + (k == 1 ? one : many) }
        var out: [String] = []
        if work.stale > 0 { out.append(n(work.stale, "footnote's code changed", "footnotes' code changed")) }
        if work.new > 0 { out.append(n(work.new, "new declaration no page mentions", "new declarations no page mentions")) }
        if work.unowned > 0 { out.append(n(work.unowned, "file no page covers", "files no page covers")) }
        if work.broken > 0 { out.append(n(work.broken, "footnote finds no code", "footnotes find no code")) }
        if !out.isEmpty { return out }
        return ["Up to date"] + (written.isEmpty ? [] : ["written " + Wiki.ago(written, now: now)])
    }

    /// Filler words a search hit doesn't mark ("how", "the", "to").
    static let fillerWords: Set<String> = ["the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "for", "on", "with", "as", "at", "by",
                                           "how", "what", "where", "when", "why", "does", "do", "are", "be", "from", "this", "that"]

    /// The query's words worth marking in a hit, lowercased: not filler
    /// words, unless the query is nothing else (as the desktop marks them).
    public static func markWords(_ query: String) -> [String] {
        let words = query.lowercased().split { !$0.isLetter && !$0.isNumber }.map(String.init).filter { $0.count > 1 }
        let kept = words.filter { !fillerWords.contains($0) }
        return kept.isEmpty ? words : kept
    }

    /// "just now", "5 minutes ago", "yesterday", "3 days ago", from an ISO
    /// time (as the desktop says it).
    public static func ago(_ iso: String, now: Date = Date.now) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let t = f.date(from: iso) ?? ISO8601DateFormatter().date(from: iso) else { return "" }
        let s = max(0, now.timeIntervalSince(t))
        if s < 90 { return "just now" }
        if s < 3600 { return "\(Int((s / 60).rounded())) minutes ago" }
        if s < 86400 { let h = Int((s / 3600).rounded()); return h == 1 ? "an hour ago" : "\(h) hours ago" }
        if s < 86400 * 45 { let d = Int((s / 86400).rounded()); return d == 1 ? "yesterday" : "\(d) days ago" }
        return t.formatted(date: .abbreviated, time: .omitted)
    }

    /// Update is the thing to do: the wiki owes the code something.
    public var wantsUpdate: Bool { work.count > 0 }

    /// The pages shown in the graph: all of them, or for a big wiki the
    /// overview (the first) and the pages it links to or from.
    public var graphPages: [WikiPage] {
        guard pages.count > 1 else { return [] }
        guard pages.count > 40, let first = pages.first else { return pages }
        let near = Set([first.path] + first.links + pages.filter { $0.links.contains(first.path) }.map(\.path))
        return pages.filter { near.contains($0.path) }
    }

    /// The pages and their links as a Mermaid flowchart ("" under two
    /// pages), as the desktop's Wiki panel draws it.
    public var mermaid: String {
        let shown = graphPages
        guard shown.count > 1 else { return "" }
        let at = Dictionary(shown.enumerated().map { ($1.path, $0) }, uniquingKeysWith: { a, _ in a })
        var out = "flowchart LR\n"
        for (i, p) in shown.enumerated() {
            var title = p.title.replacing("\"", with: "#quot;")
            for c in ["[", "]", "{", "}", "<", ">"] { title = title.replacing(c, with: " ") }
            out += "  p\(i)[\"\(title)\"]\n"
        }
        for (i, p) in shown.enumerated() {
            for l in p.links { if let j = at[l] { out += "  p\(i) --> p\(j)\n" } }
        }
        return out
    }

    /// The page a graph box is (its node "p3" is the fourth of `graphPages`).
    public func graphPage(_ id: String) -> WikiPage? {
        guard id.hasPrefix("p"), let i = Int(id.dropFirst()) else { return nil }
        let shown = graphPages
        return shown.indices.contains(i) ? shown[i] : nil
    }

    /// A file in the repository, on its machine ("src/a.rs" →
    /// "/home/me/shop/src/a.rs").
    public func file(_ path: String) -> String { root.isEmpty ? path : root + "/" + path }
}

/// A wiki page as read: what it is, who wrote and reviewed it, what it
/// owes the code, its text in blocks, its footnotes and the pages linking
/// to it.
public struct WikiPageText: Equatable, Sendable {
    public var path: String
    public var title: String
    public var kind: String
    public var summary: String
    /// "draft", "deprecated" or "".
    public var status: String
    /// Who wrote it last, and when (harlo stamps it when a session's check passes).
    public var written: Stamp?
    /// Who read it against its code, and when: the last of them.
    public var reviewed: Stamp?
    /// Edited since its last stamp (or never stamped).
    public var edited: Bool
    public var blocks: [Block]
    public var footnotes: [Footnote]
    public var backlinks: [WikiPage]
    /// What it owes the code, a line each.
    public var owes: [String]
    /// The person at the computer, as an actor ("human:sam").
    public var you: String
    /// Why it couldn't be read ("" when it was).
    public var error: String

    /// An actor and a time: `by` is "human:<login>" or "<agent>/<model>".
    public struct Stamp: Equatable, Sendable {
        public var by: String
        public var at: String
    }

    /// Prose (HTML, as a reply's), code (`code`, in `lang`) or a Mermaid
    /// diagram; `anchor` names a heading's section.
    public struct Block: Equatable, Sendable {
        public var kind: String
        public var html: String
        public var code: String
        public var lang: String
        public var anchor: String
    }

    /// A footnote: the code a claim comes from, at its lines now.
    public struct Footnote: Equatable, Sendable, Identifiable {
        public var n: Int
        public var label: String
        public var path: String
        /// "fn retry", "struct Ledger" ("" the whole file).
        public var locator: String
        /// Its lines (0: not found).
        public var start: Int
        public var end: Int
        /// "current", "stale" (its code changed), "missing" (names nothing
        /// there) or "gone" (the file isn't there).
        public var state: String
        /// The declaration as the code index has it ("pub fn retry(n: u32)";
        /// "" for a file, or without the index).
        public var signature: String = ""
        public var id: String { label }

        /// "src/jobs.rs, fn retry".
        public var place: String { locator.isEmpty ? path : path + ", " + locator }

        /// "lines 1–3", "line 4", "".
        public var lines: String { start == 0 ? "" : start == end ? "line \(start)" : "lines \(start)–\(end)" }

        /// What's wrong with it ("" nothing).
        public var note: String {
            switch state {
            case "stale": "its code changed since the page was written"
            case "missing": "names nothing in the file"
            case "gone": "the file isn't there"
            default: ""
            }
        }
    }

    public init(_ v: JSON) {
        path = v["path"].text
        title = v["title"].text
        kind = v["type"].text
        summary = v["description"].text
        status = v["status"].text
        let stamp = { (s: JSON) -> Stamp? in s["by"].text.isEmpty ? nil : Stamp(by: s["by"].text, at: s["at"].text) }
        written = stamp(v["generated"])
        reviewed = (v["verified"].array ?? []).last.flatMap(stamp)
        edited = v["edited"].bool ?? false
        blocks = (v["blocks"].array ?? []).map { Block(kind: $0["kind"].text, html: $0["html"].text, code: $0["code"].text, lang: $0["lang"].text, anchor: $0["anchor"].text) }
        footnotes = (v["footnotes"].array ?? []).map {
            Footnote(n: Int($0["n"].int ?? 0), label: $0["label"].text, path: $0["path"].text, locator: $0["locator"].text,
                     start: Int($0["start"].int ?? 0), end: Int($0["end"].int ?? 0), state: $0["state"].text, signature: $0["signature"].text)
        }
        backlinks = (v["backlinks"].array ?? []).map { WikiPage(path: $0["path"].text, title: $0["title"].text) }
        owes = (v["items"].array ?? []).map(\.text)
        you = v["you"].text
        error = v["error"].text
    }

    /// Who an actor is, in words: "you" (the person at the computer), "sam",
    /// "Claude Code with Opus 5.5", "Harlo with qwen3-coder".
    public func actor(_ by: String) -> String {
        if by.hasPrefix("human:") { return by == you ? "you" : String(by.dropFirst(6)) }
        let parts = by.split(separator: "/", maxSplits: 1).map(String.init)
        let agent = parts.first ?? by
        let name = agent == "claude" ? "Claude Code" : agent == "harlo" ? "Harlo" : agent
        return parts.count > 1 ? "\(name) with \(WorkflowStage.modelName(parts[1]))" : name
    }

    /// What it is and where it stands, in a line (as the desktop's reader
    /// says it): "Architecture · written by Harlo with qwen3-coder 2 days
    /// ago · reviewed by you just now".
    public func meta(now: Date = Date.now) -> String {
        var out = [kind, status == "draft" || status == "deprecated" ? status : ""]
        if let w = written { out.append("written by \(actor(w.by)) " + Wiki.ago(w.at, now: now)) }
        if let r = reviewed { out.append("reviewed by \(actor(r.by)) " + Wiki.ago(r.at, now: now)) }
        if edited { out.append(written != nil || reviewed != nil ? "edited since" : "not checked yet") }
        return out.filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// Where a link in it goes: a page of the wiki (it stays in .okf/:
    /// "../overview.md#x" from "architecture/jobs.md"), or a file of the
    /// repository ("../../README.md"); nil for the web and the like.
    public func target(of link: String) -> Target? {
        if link.hasPrefix("okf-fn:") {
            let label = String(link.dropFirst(7)).removingPercentEncoding ?? String(link.dropFirst(7))
            return footnotes.first { $0.label == label }.map { .footnote($0) }
        }
        if link.hasPrefix("#") { return .page(path: path, section: String(link.dropFirst())) }
        if link.firstIndex(of: ":").map({ link[..<$0].allSatisfy { $0.isLetter || $0.isNumber || "+.-".contains($0) } }) == true { return nil }
        let parts = link.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
        let target = parts[0].removingPercentEncoding ?? parts[0], section = parts.count > 1 ? parts[1] : ""
        if target.isEmpty { return .page(path: path, section: section) }
        var at = [".okf"] + (target.hasPrefix("/") ? [] : path.split(separator: "/").dropLast().map(String.init))
        for c in target.split(separator: "/") {
            if c == ".." { _ = at.popLast() } else if c != "." { at.append(String(c)) }
        }
        if at.first == ".okf" && target.hasSuffix(".md") { return .page(path: at.dropFirst().joined(separator: "/"), section: section) }
        return .file(path: at.joined(separator: "/"))
    }

    /// Where a link goes (see `target(of:)`).
    public enum Target: Equatable, Sendable {
        case page(path: String, section: String)
        /// In the repository ("README.md", ".okf/x.png").
        case file(path: String)
        case footnote(Footnote)
    }

    /// A page's first prose without its top heading (the `<h1>` its text
    /// starts with), which the screen shows as its title already.
    public static func withoutTitle(_ html: String) -> String {
        let text = html.drop { $0.isWhitespace }
        guard text.hasPrefix("<h1"), let end = text.range(of: "</h1>") else { return html }
        return String(text[end.upperBound...].drop { $0.isWhitespace })
    }
}

/// What a wiki search found: sections, best first, and declarations in the
/// code whose names hold a one-word query (from harlo's code index).
public struct WikiHits: Equatable, Sendable {
    public struct Hit: Equatable, Sendable, Identifiable {
        /// Within .okf/ ("architecture/jobs.md").
        public var page: String
        public var section: String
        public var heading: String
        public var snippet: String
        public var id: String { page + "#" + section }
    }
    /// A declaration in the code.
    public struct Code: Equatable, Sendable, Identifiable, Hashable {
        /// In the repository ("src/jobs.rs").
        public var path: String
        /// Its full name ("Queue::retry").
        public var name: String
        /// "fn", "type", …
        public var kind: String
        public var line: Int
        public var signature: String
        /// Its doc comment's first line.
        public var doc: String
        public var id: String { path + "#" + name }

        /// "src/jobs.rs:12".
        public var place: String { "\(path):\(line)" }
    }
    public var hits: [Hit]
    public var code: [Code]

    public init(_ v: JSON) {
        hits = (v["hits"].array ?? []).map { Hit(page: $0["page"].text, section: $0["section"].text, heading: $0["heading"].text, snippet: $0["snippet"].text) }
        code = (v["code"].array ?? []).map {
            Code(path: $0["path"].text, name: $0["name"].text, kind: $0["kind"].text, line: Int($0["line"].int ?? 0), signature: $0["signature"].text, doc: $0["doc"].text)
        }
    }
}
