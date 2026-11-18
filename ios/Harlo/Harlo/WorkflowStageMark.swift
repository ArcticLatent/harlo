import HarloKit
import SwiftUI

/// A workflow stage's mark: done (a tick), at work, paused, or still to come.
struct WorkflowStageMark: View {
    @Environment(\.palette) private var p
    let status: String
    var size: CGFloat = 18

    var body: some View {
        ZStack {
            switch status {
            case "done":
                Image(systemName: "checkmark.circle.fill").font(.system(size: size)).foregroundStyle(Color(p.success))
            case "active", "asking":
                // (Amber while its session waits on you.)
                let tint = Color(status == "asking" ? p.warning : p.primary)
                Circle().strokeBorder(tint, lineWidth: 1.5)
                Circle().fill(tint).padding(size * 0.3)
            case "paused":
                Image(systemName: "pause.circle.fill").font(.system(size: size)).foregroundStyle(Color(p.warning))
            default:
                Circle().strokeBorder(Color(p.hairline), lineWidth: 1.5)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
