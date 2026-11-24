import Foundation

/// A Mermaid diagram as harlo drew it for the phone.
public struct DiagramPicture: Equatable, Sendable {
    public var png: Data
    /// Its size in points (the picture has twice the pixels).
    public var width: Double
    public var height: Double
    /// A flowchart's boxes, to tap.
    public var boxes: [Box]

    /// A box: its node's id and where it is, as fractions of the
    /// picture's width and height.
    public struct Box: Equatable, Sendable {
        public var id: String
        public var x, y, w, h: Double

        public init(id: String, x: Double, y: Double, w: Double, h: Double) {
            self.id = id
            self.x = x
            self.y = y
            self.w = w
            self.h = h
        }
    }

    public init(png: Data, width: Double, height: Double, boxes: [Box] = []) {
        self.png = png
        self.width = width
        self.height = height
        self.boxes = boxes
    }

    /// The boxes as harlo sends them (`[{ id, x, y, w, h }]`), any without
    /// an id or a size left out.
    public static func boxes(_ v: JSON) -> [Box] {
        (v.array ?? []).compactMap { b in
            guard !b["id"].text.isEmpty, let x = b["x"].double, let y = b["y"].double, let w = b["w"].double, let h = b["h"].double, w > 0, h > 0 else { return nil }
            return Box(id: b["id"].text, x: x, y: y, w: w, h: h)
        }
    }
}

extension DiagramPicture {
    /// A flowchart drawn across (LR or RL) turned to run down (TB), past
    /// any `%%` lines before it; nil when it isn't one drawn across.
    public static func down(_ source: String) -> String? {
        let pattern = #"^(\s*(?:%%[^\n]*\n\s*)*)(flowchart|graph)([ \t]+)(LR|RL)\b"#
        guard let re = try? NSRegularExpression(pattern: pattern) else { return nil }
        let all = NSRange(source.startIndex..., in: source)
        guard re.firstMatch(in: source, range: all) != nil else { return nil }
        return re.stringByReplacingMatches(in: source, range: all, withTemplate: "$1$2$3TB")
    }

    /// Whether `down` shows larger than `across` in `width` points (each
    /// fitted to it, never enlarged), by more than 15%: then it's the one
    /// to show.
    public static func showsDown(across: DiagramPicture?, down: DiagramPicture?, width: Double) -> Bool {
        guard let across, let down, width > 0 else { return false }
        let fit = { (d: DiagramPicture) in min(1, width / max(1, d.width)) }
        return fit(down) > 1.15 * fit(across)
    }
}
