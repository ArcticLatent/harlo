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
