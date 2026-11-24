import HarloKit
import SwiftUI
import UIKit

/// What to type (photos, slash commands), the session's settings (model,
/// thinking, approval mode, Plan), and send / stop.
/// How full the session's context is: a slim bar the width of the
/// composer, green, then amber and red as it nears where the agent
/// compacts, with that place marked and the stretch past it shaded (as on
/// the desktop); its percentage beside it. A tap gives the numbers and the
/// cost so far.
struct ContextBar: View {
    @Environment(\.palette) private var p
    let info: SessionInfo

    var body: some View {
        if info.contextWindow > 0 && info.contextTokens > 0 {
            let used = info.contextUsed, mark = info.compactMark, way = info.contextWay
            let marked = mark > 0 && mark < 1
            let tint = way > 0.85 ? p.error : way > 0.6 ? p.warning : p.success
            Menu {
                Text("Context: \(Style.tokens(info.contextTokens)) of \(Style.tokens(info.contextWindow)) (\(Int(used * 100))%)")
                if info.compactAt > 0 { Text("Compacts at \(Style.tokens(info.compactAt))") }
                if info.cost > 0 { Text("$\(info.cost, format: .number.precision(.fractionLength(2))) so far") }
            } label: {
                HStack(spacing: 8) {
                    GeometryReader { g in
                        let w = g.size.width
                        ZStack(alignment: .leading) {
                            Capsule().fill(Color(p.muted).opacity(0.18)).frame(height: 4)
                            if marked {
                                // Past where it compacts.
                                Capsule().fill(Color(p.muted).opacity(0.3))
                                    .frame(width: w * (1 - mark), height: 4)
                                    .offset(x: w * mark)
                            }
                            Capsule().fill(Color(tint)).frame(width: max(4, w * used), height: 4)
                            if marked {
                                Capsule().fill(Color(p.muted))
                                    .frame(width: 1.5, height: 8)
                                    .offset(x: w * mark - 0.75)
                            }
                        }
                        .frame(width: w, height: g.size.height)
                    }
                    .frame(height: 8)
                    // (Capped: at the largest sizes it stays one slim row,
                    // and the conversation keeps its room.)
                    Text("\(Int(used * 100))% context")
                        .scaledFont(11.5, .medium)
                        .dynamicTypeSize(...DynamicTypeSize.xxLarge)
                        .monospacedDigit()
                        // (Apple's audit flags it whatever its shade, even the full text
                        // colour: it misreads a menu's label. This one reads 6:1.)
                        .foregroundStyle(Color(way > 0.6 ? tint : p.foreground.mix(p.background, 0.28)))
                        .fixedSize()
                }
                .padding(.horizontal, 16)
                // (A finger's height to tap.)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("contextBar")
            .accessibilityLabel("Context \(Int(used * 100))%, \(Style.tokens(info.contextTokens)) of \(Style.tokens(info.contextWindow))"
                + (info.compactAt > 0 ? ", compacts at \(Style.tokens(info.compactAt))" : ""))
            // (Taking only a slim row: the rest of its height reaches into
            // the space around it. Outside the button, which keeps its 44.)
            .padding(.vertical, -10)
        }
    }
}
