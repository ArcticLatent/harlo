import HarloKit
import SwiftUI

/// pi extensions' views by the composer: their header and widgets above
/// it (`below` false), or their widgets and footer below it. A screen or
/// an overlay takes the computer's keys, so here it only says it's open
/// there.
struct ExtPanel: View {
    @Environment(\.palette) private var p
    let session: OpenSession
    let below: Bool

    var shown: [ExtView] {
        session.extViews.filter { v in
            guard !v.hidden else { return false }
            switch v.kind {
            case "footer": return below
            case "header", "screen", "overlay": return !below
            case "widget": return (v.placement == "belowEditor") == below
            default: return false
            }
        }
    }

    var body: some View {
        let views = shown
        if !views.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(views) { v in
                    if v.interactive {
                        Label(v.kind == "overlay" ? "An extension's panel is open on the computer" : "An extension's screen is open on the computer",
                              systemImage: "desktopcomputer")
                            .scaledFont(13.5)
                            .foregroundStyle(Color(p.muted))
                    } else if !v.lines.isEmpty {
                        ExtBlocks(lines: v.lines, blocks: v.blocks)
                    }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, below ? 0 : 8)
            .padding(.bottom, below ? 8 : 0)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(below ? "extPanelBelow" : "extPanelAbove")
        }
    }
}
