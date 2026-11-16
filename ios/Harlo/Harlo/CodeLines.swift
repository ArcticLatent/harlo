import HarloKit
import SwiftUI
import UIKit

/// Code's lines and their indentation.
enum CodeLines {
    /// How far `s` is indented (a tab as 4), and the rest of it.
    static func indent(_ s: Substring) -> (Int, Substring) {
        var n = 0
        var i = s.startIndex
        while i < s.endIndex, s[i] == " " || s[i] == "\t" {
            n += s[i] == "\t" ? 4 : 1
            i = s.index(after: i)
        }
        return (n, s[i...])
    }

    /// Spans split into lines, each with its indentation taken off.
    static func lines(_ spans: [Span]) -> [(indent: Int, spans: [Span])] {
        var lines: [[Span]] = [[]]
        for s in spans {
            let parts = s.text.split(separator: "\n", omittingEmptySubsequences: false)
            for (k, part) in parts.enumerated() {
                if k > 0 { lines.append([]) }
                if !part.isEmpty { var t = s; t.text = String(part); lines[lines.count - 1].append(t) }
            }
        }
        return lines.map { line in
            var n = 0
            var out: [Span] = []
            var leading = true
            for var s in line {
                if leading {
                    let (k, rest) = indent(Substring(s.text))
                    n += k
                    if rest.isEmpty { continue }
                    s.text = String(rest)
                    leading = false
                }
                out.append(s)
            }
            return (n, out)
        }
    }
}
