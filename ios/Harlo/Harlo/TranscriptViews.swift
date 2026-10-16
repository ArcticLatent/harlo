import HarloKit
import SwiftUI
import UIKit

/// The agent at the top of its reply: its tile, its name, and when.
struct AgentHead: View {
    @Environment(\.palette) private var p
    let session: OpenSession
    /// When the reply started (unix ms; 0: unknown).
    var time: Int64 = 0

    var body: some View {
        HStack(spacing: 10) {
            AgentTile(harness: session.harness, size: Transcript.tile)
            Text(Style.harnessLabel(session.harness))
                .font(.system(size: 16.5, weight: .semibold))
            if time > 0 {
                Text(Style.clock(time))
                    .font(.system(size: 13.5))
                    .foregroundStyle(Color(p.faint))
            }
        }
        .padding(.top, 4)
    }
}

/// The transcript's measures: the agent's tile, and how far a reply is
/// indented (under its name; tool calls hang their timeline in the gap).
enum Transcript {
    static let tile: CGFloat = 36
    static let indent: CGFloat = 46
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
                        .font(.system(size: 14))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    // How long it took, or (still going) for how long.
                    if active, let start = rows.first?.time, start > 0 {
                        TimelineView(.periodic(from: .now, by: 1)) { ctx in
                            Text("· " + Style.elapsed(since: start, now: ctx.date)).font(.system(size: 14)).monospacedDigit()
                        }
                    } else if let ms = rows.first?.runMs, ms > 0 {
                        Text("· " + Style.duration(Double(ms))).font(.system(size: 14))
                    }
                    if active {
                        StatusDot(status: "busy")
                    }
                }
                .foregroundStyle(Color(p.muted))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.leading, Transcript.indent)
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
        .padding(.leading, Transcript.indent)
        .padding(.bottom, last ? 0 : 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .topLeading) {
            VStack(spacing: 0) {
                line.frame(height: 15).opacity(first ? 0 : 1)
                StepMark(row: row)
                line.frame(maxHeight: .infinity).opacity(last ? 0 : 1)
            }
            .frame(width: Transcript.tile)
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
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    var running: Bool { row.state == "running" }
    var symbol: String { Style.toolSymbol(row.title) }
    var canOpen: Bool { !row.text.isEmpty || !row.diff.isEmpty }
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
                            .font(.system(size: 16, weight: .medium))
                            .lineLimit(1)
                        if !row.subtitle.isEmpty {
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
                            .font(.system(size: 13, weight: .semibold))
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
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(row.text.count > 6000 ? String(row.text.prefix(6000)) + "\n…" : row.text)
                        .font(Style.mono(12))
                        .textSelection(.enabled)
                        .padding(12)
                }
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
                .font(.system(size: 13))
                .foregroundStyle(Color(failed ? p.error : p.muted))
                .lineLimit(1)
                .fixedSize()
        } else if !took.isEmpty {
            HStack(spacing: 5) {
                Image(systemName: failed ? "xmark" : "checkmark")
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(Color(failed ? p.error : p.success))
                Text(took).font(Style.mono(13)).foregroundStyle(Color(p.muted))
            }
            .fixedSize()
        }
    }
}

struct RowView: View {
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        switch row.kind {
        case "user":
            UserBubble(row: row)
        case "markdown":
            RichTextView(blocks: RichCache.blocks(row.html))
        case "code":
            CodeBlock(spans: codeSpans, language: row.title, raw: row.text)
        case "thinking":
            ThinkingRow(row: row)
        case "tool":
            ToolCard(row: row)
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
                .font(.system(size: 13.5))
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
                Text(row.text)
                    .font(.system(size: 16.5))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                if row.time > 0 {
                    Text(Style.clock(row.time))
                        .font(.system(size: 12))
                        .opacity(0.7)
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
}

/// Code, highlighted; a long block shows its first lines until opened.
struct CodeBlock: View {
    @Environment(\.palette) private var p
    let spans: [Span]
    let language: String
    var raw: String = ""
    @State private var whole = false

    /// Longer than this folds, to its first `shown` lines.
    static let long = 40
    static let shown = 24

    var lines: Int { RichText.lineCount(spans) }
    var folded: Bool { !whole && lines > Self.long }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .font(.system(size: 12.5))
                    .foregroundStyle(Color(p.muted))
                if lines > Self.long {
                    Text("· \(lines) lines")
                        .font(.system(size: 12.5))
                        .foregroundStyle(Color(p.muted))
                }
                Spacer()
                Button {
                    UIPasteboard.general.string = raw.isEmpty ? RichText.plain(spans) : raw
                } label: {
                    Image(systemName: "doc.on.doc").font(.caption)
                }
                .accessibilityLabel("Copy code")
            }
            .padding(.horizontal, 10)
            .padding(.top, 7)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(Style.attributed(folded ? RichText.firstLines(spans, Self.shown) : spans, p, size: 15, inCode: true))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(10)
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
                        Image(systemName: whole ? "chevron.up" : "chevron.down").font(.system(size: 10, weight: .semibold))
                        Text(whole ? "Show less" : "Show all \(lines) lines").font(.system(size: 13, weight: .medium))
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
    let block: RichBlock

    var body: some View {
        switch block {
        case .paragraph(let spans):
            Text(Style.attributed(spans, p))
                .lineSpacing(3)
                .textSelection(.enabled)
        case .heading(let level, let spans):
            Text(Style.attributed(spans, p, size: level <= 1 ? 21 : level == 2 ? 19 : 17))
                .bold()
                .padding(.top, 4)
        case .list(let ordered, let start, let items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(items.indices, id: \.self) { i in
                    HStack(alignment: .firstTextBaseline, spacing: 7) {
                        Text(ordered ? "\(start + i)." : "•")
                            .font(.system(size: 16).monospacedDigit())
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
                                Text(Style.attributed(head[c], p, size: 15)).bold()
                            }
                        }
                        Rectangle().fill(Color(p.border)).frame(height: 1)
                    }
                    ForEach(rows.indices, id: \.self) { r in
                        GridRow {
                            ForEach(rows[r].indices, id: \.self) { c in
                                Text(Style.attributed(rows[r][c], p, size: 15))
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
        .font(.system(size: 13, weight: .medium))
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

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(diff.split(separator: "\n", omittingEmptySubsequences: false).prefix(400).enumerated()), id: \.offset) { _, line in
                    let tint: RGBA? = line.hasPrefix("+") ? (p.token(9) ?? p.success) : line.hasPrefix("-") ? (p.token(10) ?? p.error) : nil
                    Text(line.isEmpty ? " " : String(line))
                        .font(Style.mono(11.5))
                        .fixedSize(horizontal: true, vertical: false)
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
