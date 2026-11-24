import HarloKit
import SwiftUI
import UIKit

/// A session's state as a pill: running, needs you, done (unread); nil when
/// there's nothing to say (then the row shows when it was last used).
struct StatusPill: View {
    @Environment(\.palette) private var p
    let status: String
    var unread = false

    /// Whether a pill shows for this state.
    static func shows(_ status: String, unread: Bool) -> Bool {
        ["busy", "starting", "input"].contains(status) || unread
    }

    var look: (text: String, color: RGBA, check: Bool)? {
        switch status {
        case "busy", "starting": return ("Running", p.success, false)
        case "input": return ("Needs you", p.warning, false)
        default: return unread ? ("Done", p.primary, true) : nil
        }
    }

    var body: some View {
        if let l = look {
            HStack(spacing: 6) {
                if l.check {
                    Image(systemName: "checkmark").scaledFont(11, .bold)
                } else {
                    PulsingDot(color: l.color, pulsing: status == "busy" || status == "starting")
                }
                Text(l.text).scaledFont(13, .medium)
            }
            // (Its words darkened just enough to read on its own tint over
            // the card: a theme's accent alone can be too light there.)
            .foregroundStyle(Color(l.color.legible(on: p.raised.mix(l.color, 0.13), toward: p.foreground)))
            .padding(.horizontal, 10)
            .frame(minHeight: 28)
            .background { Capsule().fill(Color(l.color).opacity(0.13)) }
            .fixedSize()
        }
    }
}
