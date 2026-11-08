import HarloKit
import SwiftUI
import WidgetKit

/// The plan limits widget's face: each account's windows (how much is used,
/// when it resets), as harlo last said them. Small: the fullest window of
/// two accounts; medium: two accounts side by side, two windows each;
/// large: four accounts; the lock screen: the fullest window of all.
struct LimitsWidgetView: View {
    let snapshot: LimitsSnapshot
    let family: WidgetFamily
    var now = Date()
    @Environment(\.colorScheme) private var scheme

    var p: Palette { Theme.standard.palette(dark: scheme == .dark) }

    var body: some View {
        if family == .accessoryRectangular {
            lockScreen
        } else {
            VStack(alignment: .leading, spacing: family == .systemSmall ? 9 : 11) {
                HStack(spacing: 7) {
                    Image("Logo")
                        .resizable()
                        .scaledToFit()
                        .frame(width: 17, height: 17)
                    Text(family == .systemSmall ? "Limits" : "Plan limits")
                        .font(.system(size: 16, weight: .semibold))
                    Spacer(minLength: 4)
                    if family != .systemSmall && !snapshot.accounts.isEmpty {
                        Text("Updated \(snapshot.updated.formatted(date: .omitted, time: .shortened))")
                            .font(.system(size: 11))
                            .foregroundStyle(color(p.faint))
                            .lineLimit(1)
                    }
                }
                if !snapshot.paired {
                    empty("Open harlo to pair it with your computer.")
                } else if snapshot.accounts.isEmpty {
                    empty("No plan limits yet.")
                } else {
                    switch family {
                    case .systemSmall: small
                    case .systemLarge: large
                    default: medium
                    }
                }
            }
            .foregroundStyle(color(p.foreground))
        }
    }

    /// The fullest window of the first two accounts.
    var small: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(snapshot.accounts.prefix(2))) { a in
                if let w = a.fullest {
                    window(w, title: a.name + (w.label.isEmpty ? "" : " · " + w.label), compact: true)
                }
            }
            Spacer(minLength: 0)
        }
    }

    /// Two accounts side by side, two windows each.
    var medium: some View {
        HStack(alignment: .top, spacing: 14) {
            ForEach(Array(snapshot.accounts.prefix(2))) { a in
                account(a, windows: 2)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            if snapshot.accounts.count == 1 { Spacer(minLength: 0) }
        }
        .frame(maxHeight: .infinity, alignment: .top)
    }

    /// Up to four accounts, three windows each, in two columns.
    var large: some View {
        let accounts = Array(snapshot.accounts.prefix(4))
        let rows = stride(from: 0, to: accounts.count, by: 2).map { Array(accounts[$0 ..< min($0 + 2, accounts.count)]) }
        return VStack(alignment: .leading, spacing: 14) {
            ForEach(Array(rows.enumerated()), id: \.offset) { r, row in
                if r > 0 { Rectangle().fill(color(p.hairline)).frame(height: 1) }
                HStack(alignment: .top, spacing: 14) {
                    ForEach(row) { a in
                        account(a, windows: 3)
                            .frame(maxWidth: .infinity, alignment: .topLeading)
                    }
                    if row.count == 1 { Spacer(minLength: 0).frame(maxWidth: .infinity) }
                }
            }
            Spacer(minLength: 0)
        }
    }

    func account(_ a: LimitsSnapshot.Account, windows: Int) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 5) {
                Text(a.name).font(.system(size: 14, weight: .semibold))
                if !a.plan.isEmpty {
                    Text(a.plan.capitalized).font(.system(size: 12)).foregroundStyle(color(p.muted))
                }
                if !a.host.isEmpty {
                    Text("on \(a.host)").font(.system(size: 12)).foregroundStyle(color(p.faint))
                }
            }
            .lineLimit(1)
            ForEach(Array(a.windows.prefix(windows))) { w in
                window(w, title: w.label, compact: false)
            }
        }
    }

    /// A window: its name and how much is used, a bar, when it resets.
    func window(_ w: LimitsSnapshot.Window, title: String, compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 4) {
                Text(title)
                    .font(.system(size: compact ? 12.5 : 12, weight: compact ? .semibold : .regular))
                    .foregroundStyle(color(compact ? p.foreground : p.muted))
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                Spacer(minLength: 2)
                Text(w.usedUp ? "Used up" : w.percent)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundStyle(color(tint(w)))
            }
            Bar(fraction: w.usedFraction, tint: color(tint(w)), track: color(p.hairline))
            if w.resetsAt > 0 {
                Text("Resets " + resetTime(w.resetsAt, now: now))
                    .font(.system(size: 11))
                    .foregroundStyle(color(p.faint))
                    .lineLimit(1)
            }
        }
    }

    /// The lock screen: the fullest window of all.
    @ViewBuilder var lockScreen: some View {
        let fullest: [(LimitsSnapshot.Account, LimitsSnapshot.Window)] = snapshot.accounts.compactMap { a in a.fullest.map { w in (a, w) } }
        if let top = fullest.max(by: { $0.1.usedFraction < $1.1.usedFraction }) {
            lockLine(top.0, top.1)
        } else {
            Text(snapshot.paired ? "No plan limits yet" : "Pair harlo")
                .font(.system(size: 13))
        }
    }

    func lockLine(_ a: LimitsSnapshot.Account, _ w: LimitsSnapshot.Window) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(a.name) · \(w.label)").font(.system(size: 13, weight: .semibold)).lineLimit(1)
            Gauge(value: min(w.usedFraction, 1)) { Text(w.percent) }
                .gaugeStyle(.accessoryLinearCapacity)
            Text(lockNote(w))
                .font(.system(size: 12))
                .lineLimit(1)
        }
    }

    /// "92% used · resets at 18:30", "Used up, resets Thu 09:00".
    func lockNote(_ w: LimitsSnapshot.Window) -> String {
        let reset = w.resetsAt > 0 ? resetTime(w.resetsAt, now: now) : ""
        if w.usedUp { return reset.isEmpty ? "Used up" : "Used up, resets \(reset)" }
        return reset.isEmpty ? "\(w.percent) used" : "\(w.percent) used · resets \(reset)"
    }

    func tint(_ w: LimitsSnapshot.Window) -> RGBA {
        w.usedUp ? p.error : w.nearly ? p.warning : p.primary
    }

    func empty(_ text: String) -> some View {
        VStack {
            Spacer(minLength: 0)
            Text(text)
                .font(.system(size: 13))
                .foregroundStyle(color(p.muted))
                .multilineTextAlignment(.leading)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    func color(_ c: RGBA) -> Color { Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: c.a) }
}

/// A thin bar: how much of a window is used.
private struct Bar: View {
    let fraction: Double
    let tint: Color
    let track: Color

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(track)
                Capsule().fill(tint).frame(width: max(fraction > 0 ? 4 : 0, g.size.width * min(max(fraction, 0), 1)))
            }
        }
        .frame(height: 5)
    }
}
