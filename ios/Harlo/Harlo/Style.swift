import HarloKit
import SwiftUI
import UIKit

// The desktop's look: its theme's colours (harlo sends the one chosen
// there; harlo's own, its default, until then), the system font, and
// JetBrains Mono for code.
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
        if n >= 1_000_000 { return "\((Double(n) / 1_000_000).formatted(.number.precision(.fractionLength(1))))M" }
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
            // (Code's colours a little darker or lighter where a theme's
            // are only just readable on its well: small mono text.)
            if s.code && !inCode {
                a.backgroundColor = Color(p.codeBackground)
                a.foregroundColor = Color(p.inlineCode.legible(on: p.codeBackground, toward: p.foreground, ratio: 5))
            }
            if let c = p.token(s.token) { a.foregroundColor = Color(s.code || inCode ? c.legible(on: p.codeBackground, toward: p.foreground, ratio: 5) : c) }
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
        ms < 10_000 ? "\((ms / 1000).formatted(.number.precision(.fractionLength(1))))s" : duration(ms)
    }

    /// "Good morning" … by the hour.
    static func greeting(_ date: Date = Date.now) -> String {
        let h = Calendar.current.component(.hour, from: date)
        return h < 5 ? "Good evening" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
    }
}
