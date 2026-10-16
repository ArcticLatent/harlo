import HarloKit
import SwiftUI
import WidgetKit

/// The home screen widget's face (shared by the widget and the app's
/// screenshot tour): harlo's mark and how many work or wait for you, then
/// the sessions, each with its agent's tile and its state. Small: two;
/// medium: four in a grid; large: eight. Tapping one opens it in the app.
struct SessionsWidgetView: View {
    let snapshot: WidgetSnapshot
    let family: WidgetFamily
    var now = Date()
    @Environment(\.colorScheme) private var scheme

    var p: Palette { Theme.standard.palette(dark: scheme == .dark) }

    var count: Int {
        switch family {
        case .systemSmall: return 2
        case .systemLarge: return 8
        default: return 4
        }
    }

    var summary: String {
        let s = snapshot
        // (What needs you first: the small one shows only that.)
        var parts: [String] = []
        if s.waiting > 0 { parts.append(s.waiting == 1 ? "1 needs you" : "\(s.waiting) need you") }
        if s.working > 0 { parts.append("\(s.working) working") }
        if parts.isEmpty { return s.total == 1 ? "1 session" : "\(s.total) sessions" }
        return family == .systemSmall ? parts[0] : parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: family == .systemSmall ? 8 : 10) {
            HStack(spacing: 7) {
                Image("Logo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 17, height: 17)
                if family != .systemSmall {
                    Text("Harlo")
                        .font(.system(size: 16, weight: .semibold))
                }
                Spacer(minLength: 4)
                if !snapshot.host.isEmpty && !snapshot.items.isEmpty {
                    Text(summary)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(color(snapshot.waiting > 0 ? p.warning : p.muted))
                        .lineLimit(1)
                }
            }
            if snapshot.host.isEmpty {
                empty("Open harlo to pair it with your computer.")
            } else if snapshot.items.isEmpty {
                empty("No sessions yet. Start one in harlo.")
            } else if family == .systemSmall {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(snapshot.items.prefix(count))) { item in
                        cell(item, tile: 30)
                    }
                }
                Spacer(minLength: 0)
            } else {
                grid
                if family == .systemLarge {
                    Spacer(minLength: 0)
                    Text("Updated \(snapshot.updated.formatted(date: .omitted, time: .shortened))")
                        .font(.system(size: 11))
                        .foregroundStyle(color(p.faint))
                }
            }
        }
        .foregroundStyle(color(p.foreground))
    }

    /// Two columns, hairlines between the cells.
    var grid: some View {
        let items = Array(snapshot.items.prefix(count))
        let rows = stride(from: 0, to: items.count, by: 2).map { Array(items[$0 ..< min($0 + 2, items.count)]) }
        return VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { r, row in
                if r > 0 { Rectangle().fill(color(p.hairline)).frame(height: 1) }
                HStack(spacing: 0) {
                    ForEach(Array(row.enumerated()), id: \.element.id) { c, item in
                        if c > 0 { Rectangle().fill(color(p.hairline)).frame(width: 1) }
                        Link(destination: Self.url(item.key)) {
                            cell(item, tile: family == .systemLarge ? 38 : 36)
                                .padding(.horizontal, 8)
                                .padding(.vertical, family == .systemLarge ? 11 : 8)
                        }
                    }
                    if row.count == 1 { Spacer(minLength: 0) }
                }
            }
        }
        .padding(.horizontal, -8)
    }

    func cell(_ item: WidgetSnapshot.Item, tile: CGFloat) -> some View {
        HStack(spacing: 10) {
            AgentMark(harness: item.harness, size: tile, palette: p)
            VStack(alignment: .leading, spacing: 3) {
                Text(item.title)
                    .font(.system(size: tile > 32 ? 14.5 : 13.5, weight: .semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                state(item)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Running, needs you, done, failed; or how long ago.
    @ViewBuilder func state(_ item: WidgetSnapshot.Item) -> some View {
        switch item.status {
        case "busy": label("Running", p.success, dot: true)
        case "input": label("Needs you", p.warning, dot: true)
        case "unread": label("Done", p.primary, dot: false, symbol: "checkmark")
        case "failed": label("Failed", p.error, dot: false, symbol: "xmark")
        default:
            Text(Self.ago(item.modified, now: now))
                .font(.system(size: 12.5))
                .foregroundStyle(color(p.faint))
        }
    }

    func label(_ text: String, _ tint: RGBA, dot: Bool, symbol: String = "") -> some View {
        HStack(spacing: 5) {
            if dot {
                Circle().fill(color(tint)).frame(width: 7, height: 7)
            } else {
                Image(systemName: symbol).font(.system(size: 10, weight: .bold))
            }
            Text(text).font(.system(size: 12.5, weight: .medium))
        }
        .foregroundStyle(color(tint))
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

    /// Opens a session in the app (`HarloApp`'s `onOpenURL`).
    static func url(_ key: String) -> URL {
        var c = URLComponents()
        c.scheme = "harlo"
        c.host = "session"
        c.queryItems = [URLQueryItem(name: "key", value: key)]
        return c.url ?? URL(string: "harlo://")!
    }

    /// "now", "3m", "2h", "5d".
    static func ago(_ seconds: Int64, now: Date) -> String {
        let s = max(0, now.timeIntervalSince1970 - Double(seconds))
        if s < 60 { return "now" }
        if s < 3600 { return "\(Int(s / 60))m ago" }
        if s < 86400 { return "\(Int(s / 3600))h ago" }
        return "\(Int(s / 86400))d ago"
    }
}

/// An agent's tile: its mark, dark, on a rounded square of its colour.
struct AgentMark: View {
    let harness: String
    let size: CGFloat
    let palette: Palette

    var body: some View {
        let t = palette.agentTile(harness)
        RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
            .fill(Color(.sRGB, red: t.fill.r, green: t.fill.g, blue: t.fill.b, opacity: t.fill.a))
            .frame(width: size, height: size)
            .overlay(
                Image(["claude", "opencode", "pi", "omp"].contains(harness) ? "Mark-" + harness : "Mark-demo")
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: size * 0.5, height: size * 0.5)
                    .foregroundStyle(Color(.sRGB, red: t.ink.r, green: t.ink.g, blue: t.ink.b, opacity: t.ink.a))
            )
    }
}
