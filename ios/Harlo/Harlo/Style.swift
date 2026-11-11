import HarloKit
import SwiftUI
import UIKit

// The desktop's look: its theme's colours (harlo sends the one chosen
// there; harlo's own, its default, until then), the system font, and
// JetBrains Mono for code.

extension Color {
    init(_ c: RGBA) {
        self.init(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: c.a)
    }
}

private struct PaletteKey: EnvironmentKey {
    static let defaultValue = Theme.standard.dark
}

extension EnvironmentValues {
    /// The theme's colours for the phone's appearance (light or dark).
    var palette: Palette {
        get { self[PaletteKey.self] }
        set { self[PaletteKey.self] = newValue }
    }
}

/// Puts the desktop's theme in the environment, for the phone's light or
/// dark appearance.
struct Themed: ViewModifier {
    @Environment(HarloModel.self) private var model
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        let p = (model.theme ?? .standard).palette(dark: scheme == .dark)
        content
            .environment(\.palette, p)
            .tint(Color(p.primary))
            .foregroundStyle(Color(p.foreground))
    }
}

/// A screen's background and its navigation bar in the theme's colours.
struct Screen: ViewModifier {
    @Environment(\.palette) private var p
    var sidebar = false

    func body(content: Content) -> some View {
        let bg = Color(sidebar ? p.sidebar : p.background)
        content
            .scrollContentBackground(.hidden)
            .background(bg.ignoresSafeArea())
            .toolbarBackground(bg, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
    }
}

extension View {
    func screen(sidebar: Bool = false) -> some View { modifier(Screen(sidebar: sidebar)) }

    /// A text field in a card: the page's colour (not the system's, which
    /// is black in dark mode) inside a hairline.
    func answerField(_ p: Palette) -> some View {
        textFieldStyle(.plain)
            .padding(.horizontal, 10)
            .padding(.vertical, 10)
            .frame(minHeight: 44)
            .background(RoundedRectangle(cornerRadius: 10).fill(Color(p.background)))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Color(p.border), lineWidth: 1))
    }

    /// A card: the theme's raised surface with a hairline.
    func card(_ p: Palette, radius: CGFloat = 12, fill: RGBA? = nil) -> some View {
        background(RoundedRectangle(cornerRadius: radius).fill(Color(fill ?? p.raised)))
            .overlay(RoundedRectangle(cornerRadius: radius).strokeBorder(Color(p.hairline), lineWidth: 1))
    }
}

/// Small shared looks and words.
enum Style {
    /// JetBrains Mono (bundled, as on the desktop).
    static func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        let name: String
        switch weight {
        case .medium: name = "JetBrainsMono-Medium"
        case .semibold: name = "JetBrainsMono-SemiBold"
        case .bold, .heavy, .black: name = "JetBrainsMono-Bold"
        default: name = "JetBrainsMono-Regular"
        }
        return .custom(name, size: size, relativeTo: .body)
    }

    /// An agent's letters, for its avatar.
    static func initials(_ id: String) -> String {
        ["harlo": "HA", "claude": "CL", "demo": "DE"][id] ?? String(id.prefix(2)).uppercased()
    }

    static func harnessLabel(_ id: String) -> String {
        ["harlo": "Harlo", "claude": "Claude", "demo": "Demo"][id] ?? id
    }

    /// "now", "3m", "2h", "5d", "4mo" from unix seconds.
    static func ago(_ seconds: Int64, now: Date) -> String {
        guard seconds > 0 else { return "" }
        let s = max(0, now.timeIntervalSince1970 - Double(seconds))
        if s < 60 { return "now" }
        if s < 3600 { return "\(Int(s / 60))m" }
        if s < 86400 { return "\(Int(s / 3600))h" }
        if s < 86400 * 30 { return "\(Int(s / 86400))d" }
        return "\(Int(s / (86400 * 30)))mo"
    }

    /// An address for people: "192.168.1.5 (direct)", "relay.example.com (relay)".
    static func endpointLabel(_ url: URL) -> String {
        let host = url.host ?? url.absoluteString
        if url.query?.contains("id=") == true { return "\(host) (relay)" }
        if host.hasSuffix(".ts.net") { return "\(host) (Funnel)" }
        if host.hasPrefix("100.") { return "\(host) (Tailscale)" }
        return "\(host) (direct)"
    }

    static var version: String {
        let v = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        let b = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "?"
        return "\(v) (\(b))"
    }

    /// How long since `ms` (unix ms): "8s", "1m 20s", "1h 5m".
    static func elapsed(since ms: Int64, now: Date) -> String {
        duration(max(0, now.timeIntervalSince1970 * 1000 - Double(ms)))
    }

    /// A duration in ms, as `elapsed` says it.
    static func duration(_ ms: Double) -> String {
        let s = Int(ms / 1000)
        if s < 60 { return "\(s)s" }
        if s < 3600 { return s % 60 == 0 ? "\(s / 60)m" : "\(s / 60)m \(s % 60)s" }
        return "\(s / 3600)h \(s % 3600 / 60)m"
    }

    /// "12k", "1.2M".
    static func tokens(_ n: Int64) -> String {
        if n >= 1_000_000 { return String(format: "%.1fM", Double(n) / 1_000_000) }
        if n >= 1000 { return "\(n / 1000)k" }
        return "\(n)"
    }

    /// Spans as styled text: `size` for prose (at the default text size,
    /// grown or shrunk to `typeSize`), JetBrains Mono for code (which
    /// follows the text size itself).
    static func attributed(_ spans: [Span], _ p: Palette, size: CGFloat = 16, inCode: Bool = false,
                           typeSize: DynamicTypeSize = .large) -> AttributedString {
        let prose = scaled(size, typeSize)
        var out = AttributedString()
        for s in spans {
            var a = AttributedString(s.text)
            var font: Font = s.code || inCode ? mono(size - 2, s.bold ? .bold : .regular) : .system(size: prose, weight: s.bold ? .semibold : .regular)
            if s.italic { font = font.italic() }
            a.font = font
            if s.code && !inCode {
                a.backgroundColor = Color(p.codeBackground)
                a.foregroundColor = Color(p.inlineCode)
            }
            if let c = p.token(s.token) { a.foregroundColor = Color(c) }
            if s.strike { a.strikethroughStyle = Text.LineStyle.single }
            if let l = s.link, let url = URL(string: l) {
                a.link = url
                a.foregroundColor = Color(p.link)
                a.underlineStyle = Text.LineStyle.single
            }
            out += a
        }
        return out
    }
}

extension Style {
    /// `size` (in points at the default text size) at the phone's text
    /// size, as text styles grow: for text built in code, where a view
    /// modifier can't reach.
    static func scaled(_ size: CGFloat, _ typeSize: DynamicTypeSize) -> CGFloat {
        let traits = UITraitCollection(preferredContentSizeCategory: category(typeSize))
        return UIFontMetrics(forTextStyle: textStyle(for: size)).scaledValue(for: size, compatibleWith: traits)
    }

    /// The text style whose size is nearest `size`, whose growth it follows.
    static func textStyle(for size: CGFloat) -> UIFont.TextStyle {
        switch size {
        case 30...: return .title1
        case 21..<30: return .title2
        case 19..<21: return .title3
        case 16.5..<19: return .body
        case 15.5..<16.5: return .callout
        case 14..<15.5: return .subheadline
        case 12.5..<14: return .footnote
        case 11.5..<12.5: return .caption1
        default: return .caption2
        }
    }

    static func category(_ t: DynamicTypeSize) -> UIContentSizeCategory {
        switch t {
        case .xSmall: return .extraSmall
        case .small: return .small
        case .medium: return .medium
        case .large: return .large
        case .xLarge: return .extraLarge
        case .xxLarge: return .extraExtraLarge
        case .xxxLarge: return .extraExtraExtraLarge
        case .accessibility1: return .accessibilityMedium
        case .accessibility2: return .accessibilityLarge
        case .accessibility3: return .accessibilityExtraLarge
        case .accessibility4: return .accessibilityExtraExtraLarge
        case .accessibility5: return .accessibilityExtraExtraExtraLarge
        @unknown default: return .large
        }
    }
}

/// The system font at `size` points at the default text size, growing and
/// shrinking with the phone's text size (Dynamic Type) as the nearest text
/// style does.
struct ScaledFont: ViewModifier {
    @ScaledMetric private var size: CGFloat
    private let weight: Font.Weight
    private let design: Font.Design

    init(size: CGFloat, weight: Font.Weight, design: Font.Design) {
        _size = ScaledMetric(wrappedValue: size, relativeTo: Self.style(for: size))
        self.weight = weight
        self.design = design
    }

    static func style(for size: CGFloat) -> Font.TextStyle {
        switch Style.textStyle(for: size) {
        case .title1: return .title
        case .title2: return .title2
        case .title3: return .title3
        case .body: return .body
        case .callout: return .callout
        case .subheadline: return .subheadline
        case .footnote: return .footnote
        case .caption1: return .caption
        default: return .caption2
        }
    }

    func body(content: Content) -> some View {
        content.font(.system(size: size, weight: weight, design: design))
    }
}

extension View {
    /// The system font at `size` (at the default text size), following the
    /// phone's text size.
    func scaledFont(_ size: CGFloat, _ weight: Font.Weight = .regular, design: Font.Design = .default) -> some View {
        modifier(ScaledFont(size: size, weight: weight, design: design))
    }
}

/// A session's state as the desktop's sidebar shows it: a dot, pulsing
/// while it works.
struct StatusDot: View {
    @Environment(\.palette) private var p
    let status: String
    @State private var pulse = false

    var body: some View {
        Circle()
            .fill(Color(p.status(status)))
            .frame(width: 8, height: 8)
            .opacity(status == "busy" && pulse ? 0.35 : 1)
            .animation(status == "busy" ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true) : .default, value: pulse)
            .onAppear { pulse = true }
            .accessibilityLabel(StatusDot.words(status))
    }

    static func words(_ status: String) -> String {
        ["busy": "working", "starting": "starting", "input": "needs you", "unread": "finished, unread", "exited": "stopped"][status] ?? ""
    }
}

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
                    .overlay(Circle().stroke(Color(ring ?? p.sidebar), lineWidth: 2))
                    .offset(x: 1, y: 1)
            }
        }
        .accessibilityElement()
        .accessibilityLabel(Style.harnessLabel(harness) + (StatusDot.words(status).isEmpty ? "" : ", " + StatusDot.words(status)))
    }
}

/// A section label: small, uppercase, spaced (as the desktop's).
struct SectionLabel: View {
    @Environment(\.palette) private var p
    let text: String

    var body: some View {
        Text(text.uppercased())
            .scaledFont(12, .medium)
            .tracking(0.8)
            .foregroundStyle(Color(p.label))
            .textCase(nil)
    }
}

/// An agent's name in a small capsule.
struct AgentTag: View {
    @Environment(\.palette) private var p
    let harness: String

    var body: some View {
        Text(Style.harnessLabel(harness))
            .scaledFont(12, .medium)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Capsule().fill(Color(p.subtle)))
            .overlay(Capsule().strokeBorder(Color(p.border), lineWidth: 1))
            .foregroundStyle(Color(p.muted))
    }
}

/// A small chip button (the composer's model, thinking, mode, Plan).
struct Chip: View {
    @Environment(\.palette) private var p
    let icon: String
    let text: String
    var on = false

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: icon).scaledFont(11, .semibold)
            Text(text).scaledFont(13, .medium).lineLimit(1)
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 30)
        .background(RoundedRectangle(cornerRadius: 9).fill(Color(on ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))))
        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Color(on ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1))
        .foregroundStyle(Color(on ? p.primary : p.muted))
        .contentShape(RoundedRectangle(cornerRadius: 9))
    }
}

// ---- the phone's own look (dashboard, approvals, chats) ----

/// An agent as a tile: its mark, dark, on a rounded square of its colour.
struct AgentTile: View {
    @Environment(\.palette) private var p
    let harness: String
    var size: CGFloat = 44

    var body: some View {
        let t = p.agentTile(harness)
        RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
            .fill(Color(t.fill))
            .frame(width: size, height: size)
            .overlay(
                Image(AgentTile.known.contains(harness) ? "Mark-" + harness : "Mark-demo")
                    .resizable()
                    .renderingMode(.template)
                    .scaledToFit()
                    .frame(width: size * 0.5, height: size * 0.5)
                    .foregroundStyle(Color(t.ink))
            )
            .accessibilityLabel(Style.harnessLabel(harness))
    }

    static let known = ["harlo", "claude", "demo"]
}

/// You: the penguin picked in harlo on the computer (its left rail).
struct UserAvatar: View {
    @Environment(HarloModel.self) private var model
    var size: CGFloat = 32

    var body: some View {
        Image("Penguin-" + (model.theme ?? .standard).penguin)
            .resizable()
            .interpolation(.high)
            .scaledToFit()
            .frame(width: size, height: size)
            .clipShape(Circle())
            .accessibilityLabel("You")
    }
}

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
            .foregroundStyle(Color(l.color))
            .padding(.horizontal, 10)
            .frame(minHeight: 28)
            .background(Capsule().fill(Color(l.color).opacity(0.13)))
            .fixedSize()
        }
    }
}

/// A status's dot, pulsing while it works.
struct PulsingDot: View {
    let color: RGBA
    var pulsing = false
    @State private var dim = false

    var body: some View {
        Circle()
            .fill(Color(color))
            .frame(width: 8, height: 8)
            .opacity(pulsing && dim ? 0.35 : 1)
            .animation(pulsing ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true) : .default, value: dim)
            .onAppear { dim = true }
    }
}

/// A screen's head: harlo's mark and your computer's name (its connection
/// as a dot), and the bell: what needs you (a red dot), tap for Approvals.
struct AppHeader: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    var bell: (() -> Void)?
    /// The dashboard's plan limits shown (nil: no button).
    var limits: Binding<Bool>?

    var body: some View {
        HStack(spacing: 10) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 24, height: 24)
            Text(model.host?.name ?? "harlo")
                .scaledFont(19, .medium)
                .lineLimit(1)
                .accessibilityLabel("\(model.host?.name ?? "harlo"), \(model.state == .connected ? "connected" : "not connected")")
                .accessibilityIdentifier("appHeaderName")
            StatusDot(status: model.state == .connected ? "live" : "exited")
                .accessibilityHidden(true)
            Spacer(minLength: 8)
            // (Only when harlo says limits.)
            if let limits, !model.limits.isEmpty {
                let near = model.limits.machines.contains { $0.accounts.contains { $0.windows.contains(where: \.nearly) } }
                Button {
                    UISelectionFeedbackGenerator().selectionChanged()
                    withAnimation(.easeOut(duration: 0.2)) { limits.wrappedValue.toggle() }
                } label: {
                    Image(systemName: "gauge.with.dots.needle.67percent")
                        .font(.system(size: 17, weight: .regular))
                        .frame(width: 40, height: 40)
                        .background(Circle().fill(Color(limits.wrappedValue ? p.accentSoft.over(p.raised) : p.raised)))
                        .overlay(alignment: .topTrailing) {
                            if near {
                                Circle().fill(Color(p.warning)).frame(width: 9, height: 9).offset(x: -6, y: 6)
                            }
                        }
                        .foregroundStyle(Color(limits.wrappedValue ? p.primary : p.foreground))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(limits.wrappedValue ? "Hide plan limits" : "Show plan limits")
            }
            if let bell {
                Button(action: bell) {
                    Image(systemName: "bell")
                        .font(.system(size: 18, weight: .regular))
                        .frame(width: 40, height: 40)
                        .background(Circle().fill(Color(p.raised)))
                        .overlay(alignment: .topTrailing) {
                            if !model.waiting.isEmpty {
                                Circle().fill(Color(p.error)).frame(width: 9, height: 9).offset(x: -6, y: 6)
                            }
                        }
                        .foregroundStyle(Color(p.foreground))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(model.waiting.isEmpty ? "Approvals" : "\(model.waiting.count) waiting for you")
            }
        }
        // (The buttons' height with or without them, so the name sits in
        // the same place on every tab.)
        .frame(minHeight: 40)
    }
}

/// A screen's big title, and a line under it.
struct BigTitle: View {
    @Environment(\.palette) private var p
    let text: String
    var subtitle = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(text)
                .scaledFont(34, .bold)
                .tracking(-0.4)
            if !subtitle.isEmpty {
                Text(subtitle)
                    .scaledFont(16)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A rounded icon square (a tool call's kind: a file, an edit, a command).
struct IconSquare: View {
    @Environment(\.palette) private var p
    let symbol: String
    var size: CGFloat = 36

    var body: some View {
        RoundedRectangle(cornerRadius: 9, style: .continuous)
            .fill(Color(p.foreground).opacity(0.06))
            .frame(width: size, height: size)
            .overlay(Image(systemName: symbol).font(.system(size: size * 0.42)).foregroundStyle(Color(p.muted)))
    }
}

extension Style {
    /// "9:40 AM" from unix ms (0: "").
    static func clock(_ ms: Int64) -> String {
        guard ms > 0 else { return "" }
        return Date(timeIntervalSince1970: Double(ms) / 1000).formatted(date: .omitted, time: .shortened)
    }

    /// A tool call's kind, as an SF Symbol.
    static func toolSymbol(_ tool: String) -> String { ToolName.symbol(tool) }

    /// A tool call's name as words, as the desktop says it: "Read file",
    /// "Ran command", "harlo: start a session", "context-mode ctx execute".
    static func toolWords(_ tool: String, running: Bool) -> String { ToolName.words(tool, running: running) }

    /// A short duration in ms: "0.2s", "1.4s", "12s", "1m 20s".
    static func seconds(_ ms: Double) -> String {
        ms < 10_000 ? String(format: "%.1fs", ms / 1000) : duration(ms)
    }

    /// "Good morning" … by the hour.
    static func greeting(_ date: Date = Date()) -> String {
        let h = Calendar.current.component(.hour, from: date)
        return h < 5 ? "Good evening" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
    }
}

/// Views side by side, onto the next line when they don't fit (the
/// composer's settings).
struct Flow: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(subviews, width: proposal.width ?? .infinity)
        let width = rows.map { $0.width }.max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: proposal.width ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for (i, size) in zip(row.items, row.sizes) {
                // (Centred in the row's height.)
                subviews[i].place(at: CGPoint(x: x, y: y + (row.height - size.height) / 2), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row {
        var items: [Int] = []
        var sizes: [CGSize] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for (i, v) in subviews.enumerated() {
            var size = v.sizeThatFits(.unspecified)
            size.width = min(size.width, width)
            if !row.items.isEmpty && row.width + spacing + size.width > width {
                rows.append(row)
                row = Row()
            }
            row.width += (row.items.isEmpty ? 0 : spacing) + size.width
            row.height = max(row.height, size.height)
            row.items.append(i)
            row.sizes.append(size)
        }
        if !row.items.isEmpty { rows.append(row) }
        return rows
    }
}
