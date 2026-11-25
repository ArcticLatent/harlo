// Pictures a conversation shows: their names, and one as harlo sends it.

import Foundation

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
        t = t.replacing("_", with: " ")
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
