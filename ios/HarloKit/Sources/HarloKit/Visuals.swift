// What an agent can show in a reply besides diagrams (harlo-core's
// visuals.rs reads the same blocks on the computer): an ```options block,
// a comparison of choices drawn as cards, and a ```mockup block, an HTML
// file the agent wrote that harlo shows as a picture (with the one before
// it).

import Foundation

/// One of the choices an ```options block compares.
public struct Choice: Equatable, Sendable, Identifiable {
    public var name: String
    /// The one the agent would pick.
    public var recommended: Bool
    public var about: String
    public var pros: [String]
    public var cons: [String]
    public var id: String { name }

    public init(name: String, recommended: Bool = false, about: String = "", pros: [String] = [], cons: [String] = []) {
        self.name = name
        self.recommended = recommended
        self.about = about
        self.pros = pros
        self.cons = cons
    }
}

public enum Visuals {
    /// One line of a card (its name, a pro or con) as styled runs, its
    /// Markdown drawn: `code`, **bold**, *italic*, ~~struck~~, web links
    /// (the computer draws them alike). Marks that don't close stay as text.
    public static func spans(_ line: String) -> [Span] {
        let c = Array(line)
        var out: [Span] = []
        func add(_ s: Span) {
            if s.text.isEmpty { return }
            if let last = out.last, last.sameStyle(s) { out[out.count - 1].text += s.text } else { out.append(s) }
        }
        /// Where `mark` next starts at or after `from`, with text before it.
        func find(_ mark: [Character], _ from: Int) -> Int? {
            var j = from
            while j + mark.count <= c.count {
                if Array(c[j ..< j + mark.count]) == mark && j > from { return j }
                j += 1
            }
            return nil
        }
        var i = 0
        var bold = false, italic = false, strike = false
        while i < c.count {
            let ch = c[i]
            let next: Character? = i + 1 < c.count ? c[i + 1] : nil
            if ch == "\\", let n = next, "`*_~[]()\\".contains(n) {
                add(Span(String(n), bold: bold, italic: italic, strike: strike)); i += 2; continue
            }
            if ch == "`", let end = find(["`"], i + 1) {
                add(Span(String(c[i + 1 ..< end]), code: true)); i = end + 1; continue
            }
            if ch == "*" && next == "*" && (bold || find(["*", "*"], i + 2) != nil) {
                bold.toggle(); i += 2; continue
            }
            if ch == "~" && next == "~" && (strike || find(["~", "~"], i + 2) != nil) {
                strike.toggle(); i += 2; continue
            }
            if ch == "*", italic || (next.map { !$0.isWhitespace } ?? false) && find(["*"], i + 1) != nil {
                italic.toggle(); i += 1; continue
            }
            if ch == "[", let close = find(["]", "("], i + 1), let end = c[(close + 2)...].firstIndex(of: ")") {
                let url = String(c[(close + 2) ..< end])
                if url.hasPrefix("http://") || url.hasPrefix("https://") {
                    add(Span(String(c[(i + 1) ..< close]), bold: bold, italic: italic, strike: strike, link: url))
                    i = end + 1; continue
                }
            }
            add(Span(String(ch), bold: bold, italic: italic, strike: strike))
            i += 1
        }
        return out.isEmpty ? [Span(line)] : out
    }

    /// A line's words without its Markdown marks (to read aloud).
    public static func plain(_ line: String) -> String { RichText.plain(spans(line)) }

    /// An ```options block's choices: `## name` (with `(recommended)` on
    /// the one to pick), lines about it, `+ pro` and `- con` lines; nil
    /// when it has fewer than two.
    public static func options(_ source: String) -> [Choice]? {
        var out: [Choice] = []
        for line in source.split(separator: "\n", omittingEmptySubsequences: false) {
            let t = line.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { continue }
            if t.hasPrefix("#") {
                var name = String(t.drop { $0 == "#" }).trimmingCharacters(in: .whitespaces)
                var recommended = false
                for mark in ["(recommended)", "[recommended]", "— recommended", "- recommended", "★"] {
                    if let r = name.range(of: mark, options: .caseInsensitive) {
                        name.removeSubrange(r)
                        recommended = true
                    }
                }
                name = name.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: ":-—")).trimmingCharacters(in: .whitespaces)
                out.append(Choice(name: name, recommended: recommended))
                continue
            }
            guard !out.isEmpty else { continue }
            func rest(_ prefixes: [String]) -> String? {
                for p in prefixes where t.hasPrefix(p) { return String(t.dropFirst(p.count)).trimmingCharacters(in: .whitespaces) }
                return nil
            }
            if let pro = rest(["+ ", "✓ "]) {
                out[out.count - 1].pros.append(pro)
            } else if let con = rest(["- ", "− ", "✗ "]) {
                out[out.count - 1].cons.append(con)
            } else {
                out[out.count - 1].about += (out[out.count - 1].about.isEmpty ? "" : " ") + t
            }
        }
        out.removeAll { $0.name.isEmpty }
        return out.count >= 2 ? out : nil
    }
}

/// A mockup's pictures, as harlo took them.
public struct MockupPictures: Equatable, Sendable {
    public var title: String
    public var after: DiagramPicture?
    /// Why there's no picture of it ("" when there is).
    public var afterWhy: String
    public var hasBefore: Bool
    public var before: DiagramPicture?
    public var beforeWhy: String

    public init(_ v: JSON) {
        func picture(_ p: JSON) -> DiagramPicture? {
            guard let png = Data(base64Encoded: p["png"].text), !png.isEmpty else { return nil }
            return DiagramPicture(png: png, width: p["width"].double ?? 0, height: p["height"].double ?? 0)
        }
        title = v["title"].text
        after = picture(v["after"])
        afterWhy = v["after"]["error"].text
        hasBefore = !v["before"].isNull
        before = hasBefore ? picture(v["before"]) : nil
        beforeWhy = v["before"]["error"].text
    }
}

/// Pictures by name: which files are pictures (as harlo counts them), and
/// which of a conversation's pictures a link in a reply means.
public enum PictureNames {
    public static let suffixes: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "heic", "jxl", "tif", "tiff"]

    public static func isPicture(_ path: String) -> Bool {
        let name = path.split(separator: "/").last.map(String.init) ?? path
        guard let dot = name.lastIndex(of: "."), dot != name.startIndex else { return false }
        return suffixes.contains(name[name.index(after: dot)...].lowercased())
    }

    /// The file's name.
    public static func name(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    /// A name harlo made up (a picture it keeps: a photo you sent, one a
    /// tool returned): a long run of hex digits.
    public static func isMadeUp(_ name: String) -> Bool {
        let stem = name.split(separator: ".").first ?? ""
        return stem.count >= 16 && stem.allSatisfy(\.isHexDigit)
    }

    /// The name read aloud: without its suffix, dashes and underscores as
    /// spaces (`sidebar-dark.png`: "sidebar dark").
    /// (A path in the project is read as its file's name: `art/sun.png`,
    /// "sun"; words, such as "You sent · Fix it.", as they are.)
    public static func spoken(_ name: String) -> String {
        guard isPicture(name), !name.contains(" ") else { return name }
        let file = Self.name(name)
        let stem = String(file[..<file.lastIndex(of: ".")!])
        let words = stem.map { $0 == "-" || $0 == "_" ? " " : $0 }
        return words.isEmpty ? file : String(words)
    }

    /// A message's first line, cut at about 28 characters (as the desktop
    /// tells two pictures' names apart).
    public static func firstWords(_ text: String) -> String {
        let line = text.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
        guard line.count > 28 else { return line }
        let cut = String(line.prefix(27))
        var head = cut
        if let space = cut.lastIndex(of: " "), cut.distance(from: cut.startIndex, to: space) > 12 {
            head = String(cut[..<space])
        }
        return head.trimmingCharacters(in: .whitespaces) + "…"
    }

    /// A tool picture's source: the tool and what it was given, as the
    /// desktop says it ("Read shot.png", "Screenshot").
    public static func toolWords(_ tool: String, _ subtitle: String) -> String {
        var t = ToolName.mcp(tool)?.tool ?? tool
        t = t.replacingOccurrences(of: "_", with: " ")
        t = t.prefix(1).uppercased() + t.dropFirst()
        var what = subtitle.contains("/") && !subtitle.contains(" ") ? name(subtitle) : subtitle
        if what.count > 28 { what = String(what.prefix(27)) + "…" }
        return what.isEmpty ? (t.isEmpty ? "a tool" : t) : t + " " + what
    }

    /// `path`'s place in `folder` (`~/…` or a whole path), or nil when it
    /// isn't in it.
    public static func inFolder(_ path: String, _ folder: String) -> String? {
        guard !folder.isEmpty else { return nil }
        var root = folder.hasSuffix("/") ? String(folder.dropLast()) : folder
        if root.hasPrefix("~/") || root == "~" {
            // (`~` is the home folder: /home/you, /root, /Users/you.)
            let tail = String(root.dropFirst())
            let parts = path.split(separator: "/", omittingEmptySubsequences: false)
            let homeLength = parts.count > 1 && parts[1] == "root" ? 2 : 3
            guard parts.count > homeLength, ["home", "root", "Users"].contains(String(parts[1])) else { return nil }
            root = parts.prefix(homeLength).joined(separator: "/") + tail
        }
        guard path.hasPrefix(root + "/") else { return nil }
        let rest = String(path.dropFirst(root.count + 1))
        return rest.isEmpty ? nil : rest
    }

    /// The conversation's picture `link` names (as written: `docs/a.png`,
    /// `./a.png` or the whole path), or nil.
    public static func match(_ link: String, in pictures: [String]) -> String? {
        var tail = link
        while tail.hasPrefix("./") { tail.removeFirst(2) }
        if pictures.contains(tail) { return tail }
        return pictures.first { $0.hasSuffix("/" + tail) }
    }
}

/// A picture from harlo: a file's bytes (JPEG, or PNG with transparency),
/// its type and size in pixels.
public struct Picture: Equatable, Sendable {
    public var data: Data
    public var type: String
    public var width: Double
    public var height: Double

    public init(data: Data, type: String, width: Double, height: Double) {
        self.data = data
        self.type = type
        self.width = width
        self.height = height
    }
}
