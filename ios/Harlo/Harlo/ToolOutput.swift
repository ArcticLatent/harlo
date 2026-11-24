import HarloKit
import SwiftUI
import UIKit

/// What a tool call did, in its open card: its command highlighted as
/// shell, then an edit's lines, a file's lines highlighted and numbered (a
/// read, a printed file, grep's matches) or other output as text. A long
/// one shows a part, its start or a command's end, until Show all.
struct ToolOutput: View {
    @Environment(\.palette) private var p
    let row: Row
    /// The command, shown when the card's line says what it's for instead.
    let command: String
    @State private var whole = false
    @AppStorage(CodeWrap.key) private var wrap = true

    /// Longer than this folds, to `shown` lines.
    static let long = 14
    static let shown = 10

    /// One line of output: a file's name before its lines, a numbered
    /// line, grep's context around a match, or text.
    struct Line {
        var number = ""
        var spans: [Span]
        var context = false
        var file = false
    }

    var diffLines: [Substring] { row.diff.split(separator: "\n", omittingEmptySubsequences: false) }

    var lines: [Line] {
        if let code = row.code {
            return code.lines.map { l in
                l.file.isEmpty ? Line(number: l.number, spans: l.spans, context: l.context) : Line(spans: [Span(l.file, code: true)], file: true)
            }
        }
        guard !row.text.isEmpty else { return [] }
        return row.shownOutput().split(separator: "\n", omittingEmptySubsequences: false).map { Line(spans: [Span(String($0), code: true)]) }
    }

    /// Kept to its end (a log, a command's last lines): the part shown is
    /// its last lines.
    var fromEnd: Bool {
        row.diff.isEmpty && (row.code?.earlier ?? (row.code == nil && (ToolName.isCommand(row.title) || !row.command.isEmpty)))
    }

    var body: some View {
        let all = lines
        let count = row.diff.isEmpty ? all.count : diffLines.count
        let folded = !whole && count > Self.long
        VStack(alignment: .leading, spacing: 0) {
            Group {
                if !row.diff.isEmpty {
                    DiffLines(diff: folded ? diffLines.prefix(Self.shown).joined(separator: "\n") : row.diff, bare: true)
                        .mask { fade(folded) }
                } else {
                    let shown = folded ? (fromEnd ? Array(all.suffix(Self.shown)) : Array(all.prefix(Self.shown))) : all
                    CodeScroll(wrap: wrap) {
                        VStack(alignment: .leading, spacing: 8) {
                            if !command.isEmpty {
                                commandLine
                            }
                            if !shown.isEmpty {
                                output(shown).mask { fade(folded) }
                            }
                        }
                        .padding(12)
                        .frame(maxWidth: wrap ? .infinity : nil, alignment: .leading)
                    }
                    .background(Color(p.codeBackground))
                }
            }
            if count > Self.long {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { whole.toggle() }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: whole ? "chevron.up" : "chevron.down").scaledFont(10, .semibold)
                        Text(whole ? "Show less" : "Show all \(count) lines").scaledFont(13, .medium)
                    }
                    .foregroundStyle(Color(p.primary))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(Color(p.codeBackground))
                .overlay(alignment: .top) { Rectangle().fill(Color(p.border)).frame(height: 1) }
            }
            if whole, let code = row.code, code.more {
                let left = code.total - code.lines.count
                Text(left > 0 ? "\(left) more \(left == 1 ? "line" : "lines") in harlo on your computer" : "The rest is in harlo on your computer")
                    .scaledFont(12.5)
                    .foregroundStyle(Color(p.muted))
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color(p.codeBackground))
            }
        }
    }

    /// Fading out where it's cut: above a command's end, below anything
    /// else's start.
    func fade(_ folded: Bool) -> LinearGradient {
        let edge: Double = folded ? 0.15 : 1
        return LinearGradient(stops: fromEnd
                              ? [.init(color: .black.opacity(edge), location: 0), .init(color: .black, location: folded ? 0.3 : 0), .init(color: .black, location: 1)]
                              : [.init(color: .black, location: 0), .init(color: .black, location: folded ? 0.7 : 1), .init(color: .black.opacity(edge), location: 1)],
                              startPoint: .top, endPoint: .bottom)
    }

    /// "$ " and the command, highlighted as shell (plain while harlo didn't
    /// send it highlighted).
    var commandLine: some View {
        let spans = row.commandSpans.isEmpty || row.command != command ? [Span(command, code: true)] : row.commandSpans
        return HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text("$ ").font(Style.mono(12)).foregroundStyle(Color(p.muted))
            Text(Style.attributed(wrap ? RichText.breakable(spans) : spans, p, size: 14, inCode: true))
                .fixedSize(horizontal: !wrap, vertical: false)
                .textSelection(.enabled)
                // (Read as it is, without the breaks wrapping adds.)
                .accessibilityLabel(command)
        }
    }

    func output(_ shown: [Line]) -> some View {
        // (Numbers right-aligned in a column as wide as the widest.)
        let digits = shown.map(\.number.count).max() ?? 0
        let matches = row.code?.hasContext == true
        return VStack(alignment: .leading, spacing: 1) {
            ForEach(Array(shown.enumerated()), id: \.offset) { _, line in
                let lead = digits == 0 || line.file ? "" : String(repeating: " ", count: digits - line.number.count) + line.number + "  "
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    if !lead.isEmpty {
                        Text(lead).font(Style.mono(12)).foregroundStyle(Color(p.muted).opacity(0.7)).fixedSize()
                    }
                    if wrap {
                        let split = CodeLines.lines(line.spans).first ?? (indent: 0, spans: [])
                        WrappedLine(indent: split.indent, font: Style.mono(12)) {
                            Text(Style.attributed(split.spans.isEmpty ? [Span(" ", code: true)] : RichText.breakable(split.spans), p, size: 14, inCode: true))
                                .accessibilityLabel(RichText.plain(split.spans))
                        }
                    } else {
                        Text(Style.attributed(line.spans.isEmpty ? [Span(" ", code: true)] : line.spans, p, size: 14, inCode: true))
                            .fixedSize(horizontal: true, vertical: false)
                    }
                }
                // (A file's name and grep's context quieter than its matches.)
                .foregroundStyle(Color(line.file || (line.context && matches) ? p.muted : p.foreground))
                .padding(.top, line.file ? 4 : 0)
            }
        }
        .textSelection(.enabled)
    }
}
