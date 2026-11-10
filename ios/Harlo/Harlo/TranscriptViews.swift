import HarloKit
import SwiftUI
import UIKit

/// The agent at the top of its reply: its tile, its name, the reply's
/// model when it isn't the session's now (written before a switch, or by a
/// fallback), and when.
struct AgentHead: View {
    @Environment(\.palette) private var p
    let session: OpenSession
    /// When the reply started (unix ms; 0: unknown).
    var time: Int64 = 0
    /// The model that wrote it ("" unknown).
    var replyModel: String = ""

    /// Its name, when it isn't the session's model.
    var otherModel: String {
        let name = replyModel.split(separator: "/").last.map(String.init) ?? ""
        let now = session.model
        guard !name.isEmpty, !now.isEmpty, replyModel != now, name != now, !now.hasSuffix("/" + name) else { return "" }
        return name
    }

    var body: some View {
        HStack(spacing: 8) {
            AgentTile(harness: session.harness, size: Transcript.tile)
            Text(Style.harnessLabel(session.harness))
                .scaledFont(16.5, .semibold)
            if !otherModel.isEmpty {
                Text(otherModel)
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
            }
            if time > 0 {
                Text(Style.clock(time))
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .padding(.top, 4)
    }
}

/// The transcript's measures: the agent's tile (on the reply's first
/// line, the reply full width under it), and a run's timeline: the rail its
/// marks sit on, and how far its steps are from the edge.
enum Transcript {
    static let tile: CGFloat = 28
    static let rail: CGFloat = 24
    static let step: CGFloat = 32
}

struct ItemView: View {
    let item: TranscriptItem
    let session: OpenSession

    var body: some View {
        switch item {
        case .row(let r):
            RowView(row: r)
        case .run(let group, let summary, let rows, let open, let active):
            RunView(group: group, summary: summary, rows: rows, open: open, active: active, session: session)
        }
    }
}

/// A run of tool calls and thoughts: its summary line, and (open) each
/// step on a timeline: a mark per step (done, failed, running) joined by a
/// line, and the step's card.
struct RunView: View {
    @Environment(\.palette) private var p
    let group: String
    let summary: String
    let rows: [Row]
    let open: Bool
    let active: Bool
    let session: OpenSession

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                withAnimation(.easeOut(duration: 0.15)) { session.toggleRun(group, open: open) }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .frame(width: 12)
                    Text(summary.isEmpty ? "\(rows.count) steps" : summary)
                        .scaledFont(14)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    // How long it took, or (still going) for how long.
                    if active, let start = rows.first?.time, start > 0 {
                        TimelineView(.periodic(from: .now, by: 1)) { ctx in
                            Text("· " + Style.elapsed(since: start, now: ctx.date)).scaledFont(14).monospacedDigit()
                        }
                    } else if let ms = rows.first?.runMs, ms > 0 {
                        Text("· " + Style.duration(Double(ms))).scaledFont(14)
                    }
                    if active {
                        StatusDot(status: "busy")
                    }
                }
                .foregroundStyle(Color(p.muted))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.leading, (Transcript.rail - 12) / 2)
            if open {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.element.id) { i, r in
                        TimelineStep(row: r, first: i == 0, last: i == rows.count - 1)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A step of a run: its mark on the timeline (under the agent's tile), and
/// the step itself (a tool call's card, a thought).
struct TimelineStep: View {
    @Environment(\.palette) private var p
    let row: Row
    var first = false
    var last = false

    var body: some View {
        Group {
            if row.kind == "tool" {
                ToolCard(row: row)
            } else {
                RowView(row: row)
                    .padding(.vertical, 8)
            }
        }
        .padding(.leading, Transcript.step)
        .padding(.bottom, last ? 0 : 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .topLeading) {
            VStack(spacing: 0) {
                line.frame(height: 15).opacity(first ? 0 : 1)
                StepMark(row: row)
                line.frame(maxHeight: .infinity).opacity(last ? 0 : 1)
            }
            .frame(width: Transcript.rail)
        }
    }

    var line: some View {
        Rectangle().fill(Color(p.success).opacity(0.35)).frame(width: 2)
    }
}

/// A step's mark: done (a green tick), failed, running, or a thought.
struct StepMark: View {
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        ZStack {
            if row.kind != "tool" {
                Circle().fill(Color(p.raised)).overlay(Circle().strokeBorder(Color(p.hairline), lineWidth: 1))
                Image(systemName: "sparkle").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color(p.muted))
            } else if row.state == "running" {
                Circle().fill(Color(p.background)).overlay(Circle().strokeBorder(Color(p.primary).opacity(0.5), lineWidth: 1.5))
                ProgressView().controlSize(.mini)
            } else if row.state == "error" {
                Image(systemName: "xmark.circle.fill").font(.system(size: 22)).foregroundStyle(Color(p.error))
            } else {
                Image(systemName: "checkmark.circle.fill").font(.system(size: 22)).foregroundStyle(Color(p.success))
            }
        }
        .frame(width: 24, height: 24)
        .background(Circle().fill(Color(p.background)))
    }
}

/// A tool call on a card: what it did ("Edited file"), on what, how it
/// went (lines added and removed, or a tick and how long), and, opened, its
/// diff or output; a command's result under it.
struct ToolCard: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    /// The line under its name, and whether it's words: harlo's tools name
    /// the session they're about by its title (live:4 → Update the changelog).
    var line: (text: String, words: Bool) {
        let l = ToolName.line(row.title, row.subtitle)
        if !l.words, ToolName.isHarlo(row.title),
           let s = model.sessions.first(where: { $0.key == row.subtitle }), !s.title.isEmpty, s.title != "New session" {
            return (s.title, true)
        }
        return l
    }

    var running: Bool { row.state == "running" }
    var symbol: String { Style.toolSymbol(row.title) }
    var canOpen: Bool { !row.text.isEmpty || !row.diff.isEmpty || !shownCommand.isEmpty }
    /// The command, shown opened when its line says what it's for instead.
    var shownCommand: String { row.label.isEmpty ? "" : row.command }
    var diffResult: Bool { row.result.hasPrefix("+") }
    var took: String {
        row.time > 0 && row.ended > row.time ? Style.seconds(Double(row.ended - row.time)) : ""
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                if canOpen { withAnimation(.easeOut(duration: 0.15)) { open.toggle() } }
            } label: {
                HStack(spacing: 12) {
                    IconSquare(symbol: symbol, size: 38)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(Style.toolWords(row.title, running: running))
                            .scaledFont(16, .medium)
                            .lineLimit(1)
                        if !row.label.isEmpty {
                            // What it's for, in the agent's words (the
                            // command itself is in it, opened).
                            Text(row.label)
                                .scaledFont(14)
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(2)
                        } else if !row.subtitle.isEmpty && line.words {
                            // (A question, a task, a helper's title: words,
                            // cut at the end.)
                            Text(line.text)
                                .scaledFont(14)
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(2)
                        } else if !row.subtitle.isEmpty {
                            Text(row.subtitle)
                                .font(Style.mono(13))
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                    Spacer(minLength: 4)
                    trailing
                    if canOpen {
                        Image(systemName: open ? "chevron.up" : "chevron.down")
                            .scaledFont(13, .semibold)
                            .foregroundStyle(Color(p.muted))
                    }
                }
                .padding(12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open && !row.diff.isEmpty {
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                DiffLines(diff: row.diff, bare: true)
            } else if open {
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                // (Scrolling down too only when it's taller than its box: a
                // vertical scroll view takes all the height it's given.)
                let output = row.shownOutput()
                let tall = output.reduce(into: shownCommand.isEmpty ? 1 : 2) { n, c in if c == "\n" { n += 1 } } > 18
                let axes: Axis.Set = tall ? [.horizontal, .vertical] : [.horizontal]
                ScrollView(axes, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 8) {
                        if !shownCommand.isEmpty {
                            Text("$ " + shownCommand)
                                .font(Style.mono(12))
                                .foregroundStyle(Color(p.muted))
                                .textSelection(.enabled)
                        }
                        if !row.text.isEmpty {
                            // A command's end (its result), anything else's start.
                            Text(output)
                                .font(Style.mono(12))
                                .textSelection(.enabled)
                        }
                    }
                    .padding(12)
                }
                .defaultScrollAnchor(ToolName.isCommand(row.title) || !row.command.isEmpty ? .bottomLeading : .topLeading)
                .frame(height: tall ? 320 : nil)
                .frame(maxHeight: 320)
                .background(Color(p.codeBackground))
            }
            // A command's result, under it ("12 tests passed · 1.4s").
            if symbol == "terminal" && !row.result.isEmpty {
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                Text(row.result + (took.isEmpty ? "" : " · " + took))
                    .font(Style.mono(13))
                    .foregroundStyle(Color(failed ? p.error : row.result.hasSuffix("passed") ? p.success : p.muted))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .card(p, radius: 14)
        .clipShape(RoundedRectangle(cornerRadius: 14))
    }

    var failed: Bool { row.result.hasSuffix("failed") || row.result.hasPrefix("exit") || row.state == "error" }

    @ViewBuilder var trailing: some View {
        if diffResult {
            ResultBadge(text: row.result)
        } else if running {
            EmptyView()
        } else if symbol != "terminal" && !row.result.isEmpty {
            Text(row.result)
                .scaledFont(13)
                .foregroundStyle(Color(failed ? p.error : p.muted))
                .lineLimit(1)
                .fixedSize()
        } else if !took.isEmpty {
            HStack(spacing: 5) {
                Image(systemName: failed ? "xmark" : "checkmark")
                    .scaledFont(13, .bold)
                    .foregroundStyle(Color(failed ? p.error : p.success))
                Text(took).font(Style.mono(13)).foregroundStyle(Color(p.muted))
            }
            .fixedSize()
        }
    }
}

struct RowView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        switch row.kind {
        case "user":
            UserBubble(row: row)
        case "markdown":
            RichTextView(blocks: RichCache.blocks(row.html))
        case "code":
            if row.title.lowercased() == "mermaid" && model.drawsDiagrams {
                DiagramBlock(source: row.text, code: CodeBlock(spans: codeSpans, language: row.title, raw: row.text))
            } else if row.title.lowercased() == "options", let choices = Visuals.options(row.text) {
                OptionsBlock(choices: choices, code: CodeBlock(spans: codeSpans, language: row.title, raw: row.text))
            } else if row.title.lowercased() == "mockup" && model.showsMockups {
                MockupBlock(source: row.text, code: CodeBlock(spans: codeSpans, language: row.title, raw: row.text))
            } else {
                CodeBlock(spans: codeSpans, language: row.title, raw: row.text)
            }
        case "thinking":
            ThinkingRow(row: row)
        case "tool":
            ToolCard(row: row)
        case "notice" where row.fromHarlo:
            HarloMessage(row: row)
        default:
            Text(row.text)
                .font(.footnote)
                .foregroundStyle(Color(row.title == "error" ? p.error : row.title == "warning" ? p.warning : p.muted))
                .textSelection(.enabled)
        }
    }

    var codeSpans: [Span] {
        if case .code(let spans)? = RichCache.blocks(row.html).first { return spans }
        return [Span(row.text, code: true)]
    }
}

/// harlo's message to the agent (a helper's report, the answer to a
/// split): harlo's mark, folded to its first lines (tap for the rest), and
/// the session it's about to open. Not yours: no bubble.
struct HarloMessage: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image("Logo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: Transcript.tile, height: Transcript.tile)
                Text("harlo")
                    .scaledFont(16.5, .semibold)
                Text("to the agent")
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
            }
            Button {
                withAnimation(.easeOut(duration: 0.2)) { open.toggle() }
            } label: {
                // Its plain line, folded; all of it, as the agent got it,
                // opened.
                VStack(alignment: .leading, spacing: 4) {
                    // Its plain line first; opened, what the agent got
                    // under it, smaller.
                    Text(row.label.isEmpty ? row.text : row.label)
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                        .multilineTextAlignment(.leading)
                        .lineLimit(open ? nil : 2)
                    if open && !row.label.isEmpty {
                        Text("As the agent got it")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color(p.muted))
                            .padding(.top, 6)
                        Text(row.text)
                            .font(.caption)
                            .foregroundStyle(Color(p.muted))
                            .multilineTextAlignment(.leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .buttonStyle(.plain)
            if !row.subtitle.isEmpty, let helper = model.sessions.first(where: { $0.key == row.subtitle }) {
                Button("Open helper") {
                    model.open(helper)
                }
                .scaledFont(14, .medium)
                .foregroundStyle(Color(p.primary))
                .buttonStyle(.plain)
            }
        }
        .padding(.top, 4)
    }
}

/// Parsed rich text, kept by its HTML (rows re-render as others stream),
/// the paths in it linked (tapping one opens the file).
@MainActor
enum RichCache {
    private static var cache: [String: [RichBlock]] = [:]

    static func blocks(_ html: String) -> [RichBlock] {
        if let b = cache[html] { return b }
        if cache.count > 2000 { cache.removeAll() }
        let b = FileLinks.link(RichText.parse(html))
        cache[html] = b
        return b
    }
}

struct ThinkingRow: View {
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                open.toggle()
            } label: {
                HStack(spacing: 6) {
                    Text("✦")
                    Text("Thought").italic()
                    Image(systemName: open ? "chevron.down" : "chevron.right").font(.caption2)
                }
                .scaledFont(13.5)
                .foregroundStyle(Color(p.muted))
            }
            .buttonStyle(.plain)
            if open {
                Text(row.text)
                    .font(.footnote)
                    .italic()
                    .foregroundStyle(Color(p.muted))
                    .textSelection(.enabled)
            }
        }
    }
}

/// Your message: a bubble on the right in the accent's colour (when you
/// sent it under it), and your penguin.
struct UserBubble: View {
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        let fill = p.isLight ? p.primary : p.primary.mix(p.background, 0.28)
        let ink: RGBA = fill.luminance > 0.6 ? RGBA(0.08, 0.08, 0.1) : RGBA(1, 1, 1)
        HStack(alignment: .bottom, spacing: 8) {
            Spacer(minLength: 36)
            VStack(alignment: .trailing, spacing: 5) {
                // What came with it: photos and files (by name), and tags
                // ("1 image", "Plan").
                let files = row.title.split(separator: "\n").map(String.init).filter { !$0.isEmpty }
                if !files.isEmpty || !row.subtitle.isEmpty {
                    Flow(spacing: 6) {
                        ForEach(files, id: \.self) { f in
                            attachment(f)
                        }
                        if !row.subtitle.isEmpty {
                            Text(row.subtitle).scaledFont(12.5, .medium).opacity(0.92)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                if !row.text.isEmpty {
                    Text(row.text)
                        .scaledFont(16.5)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
                if row.time > 0 {
                    Text(Style.clock(row.time))
                        .scaledFont(12.5, .semibold)
                }
            }
            .foregroundStyle(Color(ink))
            .padding(.horizontal, 15)
            .padding(.vertical, 11)
            .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Color(fill)))
            .layoutPriority(1)
            UserAvatar(size: 30)
        }
        .padding(.top, 8)
    }

    /// An attachment: a photo (as the phone names them, or an image file)
    /// or a file, by its name.
    func attachment(_ path: String) -> some View {
        let name = path.split(separator: "/").last.map(String.init) ?? path
        let ext = name.split(separator: ".").last.map { $0.lowercased() } ?? ""
        let photo = name.hasPrefix("Photo ") || ["png", "jpg", "jpeg", "gif", "webp", "heic"].contains(ext)
        return HStack(spacing: 5) {
            Image(systemName: photo ? "photo" : "doc").scaledFont(12, .semibold)
            Text(name).scaledFont(13, .medium).lineLimit(1).truncationMode(.middle)
        }
        .padding(.horizontal, 9)
        .frame(minHeight: 26)
        .background(Capsule().fill(.primary.opacity(0.15)))
    }
}

/// Code, highlighted; a long block shows its first lines until opened.
struct CodeBlock: View {
    @Environment(\.palette) private var p
    let spans: [Span]
    let language: String
    var raw: String = ""
    @State private var whole = false
    @AppStorage(CodeWrap.key) private var wrap = true

    /// Longer than this folds, to its first `shown` lines.
    static let long = 40
    static let shown = 24

    var lines: Int { RichText.lineCount(spans) }
    var folded: Bool { !whole && lines > Self.long }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .scaledFont(12.5)
                    .foregroundStyle(Color(p.muted))
                if lines > Self.long {
                    Text("· \(lines) lines")
                        .scaledFont(12.5)
                        .foregroundStyle(Color(p.muted))
                }
                Spacer()
                Button {
                    UIPasteboard.general.string = raw.isEmpty ? RichText.plain(spans) : raw
                } label: {
                    Image(systemName: "doc.on.doc").font(.caption)
                        // (A full-size target in a small header.)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        .padding(.trailing, -12)
                }
                .accessibilityLabel("Copy code")
            }
            .padding(.horizontal, 10)
            let shown = folded ? RichText.firstLines(spans, Self.shown) : spans
            if wrap {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(CodeLines.lines(shown).enumerated()), id: \.offset) { _, line in
                        WrappedLine(indent: line.indent, font: Style.mono(13)) {
                            Text(Style.attributed(line.spans.isEmpty ? [Span(" ")] : line.spans, p, size: 15, inCode: true))
                        }
                    }
                }
                .textSelection(.enabled)
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(Style.attributed(shown, p, size: 15, inCode: true))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: true, vertical: false)
                        .padding(10)
                }
            }
            .mask {
                // Fading out where it's cut.
                LinearGradient(stops: [.init(color: .black, location: 0), .init(color: .black, location: folded ? 0.75 : 1),
                                       .init(color: .black.opacity(folded ? 0.15 : 1), location: 1)],
                               startPoint: .top, endPoint: .bottom)
            }
            if lines > Self.long {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { whole.toggle() }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: whole ? "chevron.up" : "chevron.down").scaledFont(10, .semibold)
                        Text(whole ? "Show less" : "Show all \(lines) lines").scaledFont(13, .medium)
                    }
                    .foregroundStyle(Color(p.primary))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .overlay(alignment: .top) { Rectangle().fill(Color(p.border)).frame(height: 1) }
            }
        }
        .card(p, radius: 10, fill: p.codeBackground)
    }
}

struct RichTextView: View {
    let blocks: [RichBlock]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(blocks.indices, id: \.self) { i in
                BlockView(block: blocks[i])
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct BlockView: View {
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let block: RichBlock

    var body: some View {
        switch block {
        case .paragraph(let spans):
            Text(Style.attributed(spans, p, typeSize: typeSize))
                .lineSpacing(3)
                .textSelection(.enabled)
        case .heading(let level, let spans):
            Text(Style.attributed(spans, p, size: level <= 1 ? 21 : level == 2 ? 19 : 17, typeSize: typeSize))
                .bold()
                .padding(.top, 4)
        case .list(let ordered, let start, let items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(items.indices, id: \.self) { i in
                    HStack(alignment: .firstTextBaseline, spacing: 7) {
                        Text(ordered ? "\(start + i)." : "•")
                            .scaledFont(16).monospacedDigit()
                            .foregroundStyle(Color(p.muted))
                        // (AnyView: lists hold blocks, which hold lists.)
                        AnyView(RichTextView(blocks: items[i]))
                    }
                }
            }
        case .quote(let inner):
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 1.5).fill(Color(p.border)).frame(width: 3)
                AnyView(RichTextView(blocks: inner))
                    .foregroundStyle(Color(p.muted))
            }
        case .code(let spans):
            CodeBlock(spans: spans, language: "")
        case .rule:
            Rectangle().fill(Color(p.border)).frame(height: 1)
        case .table(let head, let rows):
            ScrollView(.horizontal, showsIndicators: false) {
                Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 6) {
                    if !head.isEmpty {
                        GridRow {
                            ForEach(head.indices, id: \.self) { c in
                                Text(Style.attributed(head[c], p, size: 15, typeSize: typeSize)).bold()
                            }
                        }
                        Rectangle().fill(Color(p.border)).frame(height: 1)
                    }
                    ForEach(rows.indices, id: \.self) { r in
                        GridRow {
                            ForEach(rows[r].indices, id: \.self) { c in
                                Text(Style.attributed(rows[r][c], p, size: 15, typeSize: typeSize))
                            }
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
    }
}

/// What a tool call came to: lines added and removed, tests passed, a
/// failure, coloured as the desktop's diffs and statuses.
struct ResultBadge: View {
    @Environment(\.palette) private var p
    let text: String

    var body: some View {
        Group {
            if text.hasPrefix("+"), let space = text.firstIndex(of: " ") {
                HStack(spacing: 4) {
                    Text(text[..<space]).foregroundStyle(Color(p.token(9) ?? p.success))
                    Text(text[text.index(after: space)...]).foregroundStyle(Color(p.token(10) ?? p.error))
                }
            } else if text.hasPrefix("+") {
                Text(text).foregroundStyle(Color(p.token(9) ?? p.success))
            } else {
                Text(text).foregroundStyle(Color(bad ? p.error : text.hasSuffix("passed") ? p.success : p.muted))
            }
        }
        .scaledFont(13, .medium)
        .lineLimit(1)
        .fixedSize()
    }

    var bad: Bool { text.hasSuffix("failed") || text.hasPrefix("exit") }
}

/// An edit's or a write's lines, tinted as the desktop's diffs.
struct DiffLines: View {
    @Environment(\.palette) private var p
    let diff: String
    /// Inside a card already (no card of its own).
    var bare = false
    @AppStorage(CodeWrap.key) private var wrap = true

    var body: some View {
        CodeScroll(wrap: wrap) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(diff.split(separator: "\n", omittingEmptySubsequences: false).prefix(400).enumerated()), id: \.offset) { _, line in
                    let tint: RGBA? = line.hasPrefix("+") ? (p.token(9) ?? p.success) : line.hasPrefix("-") ? (p.token(10) ?? p.error) : nil
                    Group {
                        if wrap, let first = line.first {
                            let (n, rest) = CodeLines.indent(line.dropFirst())
                            WrappedLine(lead: String(first), indent: n, font: Style.mono(11.5)) {
                                Text(rest.isEmpty ? " " : String(rest)).font(Style.mono(11.5))
                            }
                        } else {
                            Text(line.isEmpty ? " " : String(line))
                                .font(Style.mono(11.5))
                                .fixedSize(horizontal: true, vertical: false)
                        }
                    }
                        .padding(.horizontal, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(tint.map { Color($0).opacity(0.14) } ?? Color.clear)
                }
            }
            .padding(.vertical, 6)
        }
        .background(bare ? Color(p.codeBackground) : Color.clear)
        .modifier(CardUnless(bare: bare, radius: 8, fill: p.codeBackground))
    }
}

/// Long lines of code: wrapped to the screen (the default; Settings →
/// Reviewing, or the button on a file or its changes), or kept whole and
/// scrolled across.
enum CodeWrap {
    static let key = "wrapCode"
}

/// Code lines' container: as they are when wrapped, else in a sideways
/// scroll.
struct CodeScroll<Content: View>: View {
    let wrap: Bool
    @ViewBuilder let content: Content

    var body: some View {
        if wrap {
            content
        } else {
            ScrollView(.horizontal, showsIndicators: false) { content }
        }
    }
}

/// A line of code, wrapped with a hanging indent: its further rows start
/// under its own code, not at the margin, so the nesting still reads (and
/// a line too long to fit after its indentation doesn't break there,
/// leaving its first row empty). `lead` is shown before the indentation
/// (a diff's + or −).
struct WrappedLine<Code: View>: View {
    var lead = ""
    let indent: Int
    let font: Font
    @ViewBuilder let code: Code

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            if !lead.isEmpty { Text(lead).font(font).fixedSize() }
            // (Hidden digits: a monospaced font's width, which spaces
            // alone may not be given.)
            if indent > 0 { Text(String(repeating: "0", count: indent)).font(font).fixedSize().hidden() }
            code.frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

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

/// The button that wraps long lines, or unwraps them.
struct WrapButton: View {
    @Binding var wrap: Bool

    var body: some View {
        Button {
            wrap.toggle()
        } label: {
            Image(systemName: wrap ? "arrow.left.and.right" : "text.alignleft")
        }
        .accessibilityLabel(wrap ? "Show long lines whole" : "Wrap long lines")
    }
}

/// A card, unless it's inside one already.
struct CardUnless: ViewModifier {
    @Environment(\.palette) private var p
    let bare: Bool
    var radius: CGFloat = 12
    var fill: RGBA?

    @ViewBuilder func body(content: Content) -> some View {
        if bare { content } else { content.card(p, radius: radius, fill: fill) }
    }
}
