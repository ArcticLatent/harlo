import HarloKit
import SwiftUI
import UIKit

/// An agent as an avatar: its letters in a soft circle of its colour, with a
/// session's status on the corner.
struct AgentAvatar: View {
    @Environment(\.palette) private var p
    let harness: String
    var status = ""
    var size: CGFloat = 30
    /// What's behind it (the status dot's ring cuts into it).
    var ring: RGBA?

    var body: some View {
        let t = p.agent(harness)
        ZStack(alignment: .bottomTrailing) {
            Circle()
                .fill(Color(t.fill))
                .frame(width: size, height: size)
                .overlay(
                    Text(Style.initials(harness))
                        .font(.system(size: size * 0.36, weight: .semibold))
                        .foregroundStyle(Color(t.ink))
                )
            if !["", "idle", "live"].contains(status) {
                Circle()
                    .fill(Color(p.status(status)))
                    .frame(width: size * 0.32, height: size * 0.32)
                    .overlay { Circle().stroke(Color(ring ?? p.sidebar), lineWidth: 2) }
                    .offset(x: 1, y: 1)
            }
        }
        .accessibilityElement()
        .accessibilityLabel(Style.harnessLabel(harness) + (StatusDot.words(status).isEmpty ? "" : ", " + StatusDot.words(status)))
    }
}
