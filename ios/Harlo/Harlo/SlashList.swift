import HarloKit
import SwiftUI
import UIKit

/// Slash commands matching what's typed.
struct SlashList: View {
    @Environment(\.palette) private var p
    let commands: [SlashCommand]
    var pick: (SlashCommand) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(commands) { c in
                Button { pick(c) } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text("/" + c.name).font(Style.mono(13.5, .semibold))
                            if !c.hint.isEmpty { Text(c.hint).font(Style.mono(11.5)).foregroundStyle(Color(p.muted)) }
                        }
                        if !c.description.isEmpty {
                            Text(c.description).scaledFont(12.5).foregroundStyle(Color(p.muted)).lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if c.id != commands.last?.id { Divider().overlay(Color(p.border)) }
            }
        }
        .card(p, radius: Corner.card)
    }
}
