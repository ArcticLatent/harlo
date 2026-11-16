import HarloKit
import SwiftUI

struct AccountLimits: View {
    @Environment(\.palette) private var p
    let account: LimitAccount

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Text(account.name).scaledFont(15, .semibold)
                if !account.plan.isEmpty {
                    Text(account.plan.capitalized).scaledFont(13).foregroundStyle(Color(p.muted))
                }
            }
            ForEach(account.windows) { w in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(String(w.name.prefix(1)).uppercased() + String(w.name.dropFirst()))
                            .scaledFont(13)
                        Spacer()
                        Text(detail(w))
                            .scaledFont(12.5)
                            .foregroundStyle(Color(w.nearly ? (w.usedUp ? p.error : p.warning) : p.muted))
                            .monospacedDigit()
                    }
                    Meter(fraction: w.usedFraction, color: w.usedUp ? p.error : w.nearly ? p.warning : p.primary)
                }
                .accessibilityElement(children: .combine)
            }
        }
    }

    func detail(_ w: LimitWindow) -> String {
        let used = w.usedUp ? "Used up" : "\(Int(min(w.usedFraction, 1) * 100))%"
        return w.resetsAt > 0 ? "\(used) · resets \(resetTime(w.resetsAt))" : used
    }
}
