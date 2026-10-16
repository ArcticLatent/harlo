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
        ["claude": "CL", "opencode": "OC", "pi": "PI", "omp": "OM", "demo": "DE"][id] ?? String(id.prefix(2)).uppercased()
    }

    static func harnessLabel(_ id: String) -> String {
        ["claude": "Claude", "opencode": "OpenCode", "pi": "Pi", "omp": "Oh My Pi", "demo": "Demo"][id] ?? id
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

    /// Spans as styled text: `size` for prose, JetBrains Mono for code.
    static func attributed(_ spans: [Span], _ p: Palette, size: CGFloat = 16, inCode: Bool = false) -> AttributedString {
        var out = AttributedString()
        for s in spans {
            var a = AttributedString(s.text)
            var font: Font = s.code || inCode ? mono(size - 2, s.bold ? .bold : .regular) : .system(size: size, weight: s.bold ? .semibold : .regular)
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
            .font(.system(size: 12, weight: .medium))
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
            .font(.system(size: 12, weight: .medium))
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
            Image(systemName: icon).font(.system(size: 11, weight: .semibold))
            Text(text).font(.system(size: 13, weight: .medium)).lineLimit(1)
        }
        .padding(.horizontal, 10)
        .frame(height: 30)
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

    static let known = ["claude", "opencode", "pi", "omp", "demo"]
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
                    Image(systemName: "checkmark").font(.system(size: 11, weight: .bold))
                } else {
                    PulsingDot(color: l.color, pulsing: status == "busy" || status == "starting")
                }
                Text(l.text).font(.system(size: 13, weight: .medium))
            }
            .foregroundStyle(Color(l.color))
            .padding(.horizontal, 10)
            .frame(height: 28)
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

    var body: some View {
        HStack(spacing: 10) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 24, height: 24)
            Text(model.host?.name ?? "harlo")
                .font(.system(size: 19, weight: .medium))
                .lineLimit(1)
            StatusDot(status: model.state == .connected ? "live" : "exited")
            Spacer(minLength: 8)
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
                .font(.system(size: 34, weight: .bold))
                .tracking(-0.4)
            if !subtitle.isEmpty {
                Text(subtitle)
                    .font(.system(size: 16))
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
    static func toolSymbol(_ tool: String) -> String {
        switch tool.lowercased() {
        case "read", "view", "cat", "glob", "ls", "find": return "doc.text"
        case "edit", "multiedit", "write", "patch", "apply_patch", "str_replace": return "pencil.line"
        case "bash", "shell", "exec", "run", "command": return "terminal"
        case "grep", "search", "rg": return "magnifyingglass"
        case "fetch", "webfetch", "web_search", "websearch": return "globe"
        case "task", "agent", "subagent": return "person.2"
        case "todo", "todowrite", "todoread": return "checklist"
        default: return "wrench.and.screwdriver"
        }
    }

    /// A tool call's name as words: "Read file", "Edited file", "Run command".
    static func toolWords(_ tool: String, running: Bool) -> String {
        switch tool.lowercased() {
        case "read", "view", "cat": return running ? "Reading file" : "Read file"
        case "edit", "multiedit", "str_replace", "patch", "apply_patch": return running ? "Editing file" : "Edited file"
        case "write": return running ? "Writing file" : "Wrote file"
        case "bash", "shell", "exec", "run", "command": return running ? "Running command" : "Ran command"
        case "grep", "search", "rg": return "Searched"
        case "glob", "find", "ls": return "Listed files"
        case "fetch", "webfetch": return "Fetched"
        case "web_search", "websearch": return "Searched the web"
        case "task", "agent", "subagent": return "Subagent"
        default: return tool.isEmpty ? "Tool" : tool.prefix(1).uppercased() + tool.dropFirst()
        }
    }

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
