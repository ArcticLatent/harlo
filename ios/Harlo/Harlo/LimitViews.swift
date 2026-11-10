import HarloKit
import SwiftUI

/// Above the composer, from 90% of a plan limit the session uses: how much,
/// and when it resets; once it's used up, Stop and continue (or Continue)
/// at the reset, as harlo's own line does.
struct LimitLine: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        let limit = session.limit
        if limit != nil || session.resumesAt > 0 {
            let out = limit?.out ?? false
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .scaledFont(12)
                    .foregroundStyle(Color(out ? p.error : p.warning))
                Text(words(limit))
                    .scaledFont(13.5)
                    .foregroundStyle(Color(out ? p.error : p.warning))
                    .lineLimit(2)
                Spacer(minLength: 4)
                if session.resumesAt > 0 {
                    button("Don't continue") { await model.cancelContinue() }
                } else if let l = limit, l.out, l.resetAt > now, session.busy || session.info.limitRefused {
                    button((session.busy ? "Stop and continue " : "Continue ") + resetTime(l.resetAt)) { await model.continueAtReset() }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }

    var now: Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

    func words(_ l: SessionLimit?) -> String {
        let gone = l.map { "You've used up your \($0.window.name). " } ?? ""
        if session.resumesAt > 0 {
            return (l?.out == true ? gone : "") + "This session continues \(resetTime(session.resumesAt))"
        }
        guard let l else { return "" }
        let when = l.window.resetsAt > 0 ? " · resets \(resetTime(l.window.resetsAt))" : ""
        return l.out ? "You've used up your \(l.window.name)\(when)"
                     : "You've used \(Int(l.window.usedFraction * 100))% of your \(l.window.name)\(when)"
    }

    func button(_ title: String, _ action: @escaping () async -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            Task { await action() }
        } label: {
            Text(title)
                .scaledFont(12.5, .medium)
                .foregroundStyle(Color(p.primary))
                .multilineTextAlignment(.trailing)
        }
        .buttonStyle(.plain)
    }
}

/// Above the composer, near where the agent compacts the conversation on
/// its own: how many tokens are left.
struct CompactLine: View {
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        let note = session.info.compactNote
        if !note.isEmpty {
            HStack(spacing: 8) {
                Image(systemName: "arrow.down.right.and.arrow.up.left")
                    .scaledFont(12)
                    .foregroundStyle(Color(p.warning))
                Text(note)
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }
}

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
        .card(p, radius: 16)
    }
}

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

/// A thin bar: how much of something is used.
struct Meter: View {
    @Environment(\.palette) private var p
    let fraction: Double
    let color: RGBA

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(p.hairline))
                Capsule().fill(Color(color)).frame(width: g.size.width * max(0, min(1, fraction)))
            }
        }
        .frame(height: 5)
    }
}
