import HarloKit
import SwiftUI
import UIKit

/// A step's mark: done (a green tick), failed, running, or a thought.
struct StepMark: View {
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        ZStack {
            if row.kind != "tool" {
                Circle().fill(Color(p.raised)).overlay { Circle().strokeBorder(Color(p.hairline), lineWidth: 1) }
                Image(systemName: "sparkle").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color(p.muted))
            } else if row.state == "running" {
                Circle().fill(Color(p.background)).overlay { Circle().strokeBorder(Color(p.primary).opacity(0.5), lineWidth: 1.5) }
                ProgressView().controlSize(.mini)
            } else if row.state == "error" {
                Image(systemName: "xmark.circle.fill").font(.system(size: 22)).foregroundStyle(Color(p.error))
            } else {
                Image(systemName: "checkmark.circle.fill").font(.system(size: 22)).foregroundStyle(Color(p.success))
            }
        }
        .frame(width: 24, height: 24)
        .background { Circle().fill(Color(p.background)) }
    }
}
