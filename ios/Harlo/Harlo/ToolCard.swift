import HarloKit
import SwiftUI
import UIKit

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
            if !row.images.isEmpty && model.showsPictures {
                PictureStrip(paths: row.images, side: 72)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 12)
            }
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
                ScrollView(axes) {
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
                .scrollIndicators(.hidden)
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
                .shortOrCut(row.result)
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
