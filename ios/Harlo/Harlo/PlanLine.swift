import HarloKit
import SwiftUI

/// The first line of the activity card above the composer, while the
/// session has a shared plan: how far along it is and the step in hand;
/// tap for the whole plan.
struct PlanLine: View {
    @Environment(\.palette) private var p
    let plan: Plan
    var open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Image(systemName: "list.bullet.clipboard")
                    .scaledFont(13)
                    .foregroundStyle(Color(p.primary))
                    .frame(width: 14)
                Text("\(plan.finished)/\(plan.steps.count)")
                    .scaledFont(13, .semibold)
                    .foregroundStyle(Color(p.muted))
                    .monospacedDigit()
                Text(plan.current?.title ?? (plan.finished == plan.steps.count ? "All done" : "Plan"))
                    .scaledFont(13)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up").scaledFont(10, .semibold).foregroundStyle(Color(p.muted))
            }
            // (A full-size target: it reaches 13 pt above and below the
            // line without making the card any taller.)
            .padding(.vertical, 13)
            .contentShape(Rectangle())
            .padding(.vertical, -13)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Plan, \(plan.finished) of \(plan.steps.count) done\(plan.current.map { ", now: \($0.title)" } ?? "")")
        .accessibilityHint("Shows the plan")
    }
}
