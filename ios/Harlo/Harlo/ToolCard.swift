import HarloKit
import SwiftUI
import UIKit

/// A tool call on a card: what it did ("Edited file"), on what, how it
/// went (lines added and removed, or a tick and how long), and, opened, its
/// diff or output, highlighted (see `ToolOutput`); a command's result under
/// it.
struct ToolCard: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row
    /// Folded until you tap its line (open, a chat of many calls reads as
    /// a wall of output).
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
    var canOpen: Bool { hasOutput || !row.error.isEmpty }
    /// A subagent's card leads to the session it works in.
    var opensChild: Bool { !row.child.isEmpty }
    /// Something for the opened card besides why it failed.
    var hasOutput: Bool { !row.text.isEmpty || !row.diff.isEmpty || !shownCommand.isEmpty }
    /// The command, shown opened when its line says what it's for instead.
    var shownCommand: String { row.label.isEmpty ? "" : row.command }
    var diffResult: Bool { row.result.hasPrefix("+") }
    var took: String {
        row.time > 0 && row.ended > row.time ? Style.seconds(Double(row.ended - row.time)) : ""
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button(action: tapped) {
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
                            // (A question, a task: words,
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
                    if opensChild {
                        Image(systemName: "chevron.right")
                            .scaledFont(13, .semibold)
                            .foregroundStyle(Color(p.muted))
                            .accessibilityHidden(true)
                    } else if canOpen {
                        Image(systemName: open ? "chevron.up" : "chevron.down")
                            .scaledFont(13, .semibold)
                            .foregroundStyle(Color(p.muted))
                    }
                }
                .padding(12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint(opensChild ? "Opens the subagent's session" : "")
            if !row.images.isEmpty && model.showsPictures {
                PictureStrip(paths: row.images, side: 72)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 12)
            }
            if open && hasOutput {
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                ToolOutput(row: row, command: shownCommand)
            }
            // Why it failed, whole (its corner says it in a word).
            if open && !row.error.isEmpty {
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                Text(row.error)
                    .scaledFont(14)
                    .foregroundStyle(Color(p.error))
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
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
        .card(p, radius: Corner.card)
        .clipShape(RoundedRectangle(cornerRadius: Corner.card))
    }

    var failed: Bool {
        row.result.hasSuffix("failed") || row.result.hasPrefix("exit") || row.state == "error"
    }

    /// Opens the subagent's session, or folds and unfolds the card.
    func tapped() {
        if opensChild {
            model.openChild(row.child, title: row.subtitle)
        } else if canOpen {
            withAnimation(.easeOut(duration: 0.15)) { open.toggle() }
        }
    }

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
