import HarloKit
import SwiftUI

/// The dashboard's plan limits: each account's windows, on harlo's computer
/// and on each machine it's connected to.
struct PlanLimitsCard: View {
    @Environment(\.palette) private var p
    let limits: PlanLimits

    var body: some View {
        let machines = limits.machines.filter { !$0.accounts.isEmpty }
        VStack(alignment: .leading, spacing: 14) {
            ForEach(machines) { m in
                VStack(alignment: .leading, spacing: 12) {
                    // (Machines named only when there's more than one.)
                    if machines.count > 1 {
                        SectionLabel(text: m.host.isEmpty ? "This computer" : m.host)
                    }
                    ForEach(m.accounts) { a in
                        AccountLimits(account: a)
                    }
                }
            }
        }
        .padding(14)
        .card(p, radius: Corner.large)
    }
}
