import HarloKit
import SwiftUI
import UIKit

/// A background command that ended: how it went (red when it failed), the
/// command's first line under it, and a tap for the whole command and its
/// last lines.
struct BackgroundNotice: View {
    @Environment(\.palette) private var p
    let row: Row
    @State private var open = false

    var failed: Bool { row.state == "error" }
    var firstLine: String {
        let first = row.command.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? row.command
        return first.trimmingCharacters(in: .whitespaces) + (row.command.contains("\n") ? " …" : "")
    }

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { open.toggle() }
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Image(systemName: failed ? "exclamationmark.triangle" : row.state == "stopped" ? "stop.circle" : "terminal")
                        .scaledFont(13, .medium)
                        .foregroundStyle(Color(failed ? p.error : p.muted))
                    Text(row.label)
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(Color(failed ? p.error : p.muted))
                        .lineLimit(1)
                    if !row.subtitle.isEmpty {
                        Text(row.subtitle)
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted).opacity(0.75))
                            .lineLimit(1)
                    }
                    Spacer(minLength: 4)
                    Image(systemName: open ? "chevron.up" : "chevron.down")
                        .scaledFont(11, .semibold)
                        .foregroundStyle(Color(p.muted).opacity(0.75))
                }
                if open {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("$ " + row.command)
                            .font(Style.mono(12))
                            .foregroundStyle(Color(p.muted))
                            .textSelection(.enabled)
                        if !row.result.isEmpty {
                            Text(row.result)
                                .font(Style.mono(12))
                                .textSelection(.enabled)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(Color(p.codeBackground), in: RoundedRectangle(cornerRadius: 8))
                } else {
                    Text("$ " + firstLine)
                        .font(Style.mono(12))
                        .foregroundStyle(Color(p.muted).opacity(0.75))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .padding(.leading, 19)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("backgroundNotice")
    }
}
