import HarloKit
import SwiftUI

/// Shared looks and words.
enum Live {
    static let accent: Color = {
        let c = Theme.standard.dark.primary
        return Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: 1)
    }()

    /// The app's Keychain group (its team, from the build): the keys that
    /// open what harlo sealed.
    static var keychainGroup: String? {
        (Bundle.main.object(forInfoDictionaryKey: "HarloTeam") as? String).flatMap { $0.isEmpty ? nil : "\($0).com.arcticlatent.harlo" }
    }

    static func url(_ key: String) -> URL? {
        var c = URLComponents()
        c.scheme = "harlo"
        c.host = "session"
        c.queryItems = [URLQueryItem(name: "key", value: key)]
        return c.url
    }

    static func color(_ phase: String) -> Color {
        switch phase {
        case "input": return .orange
        case "failed": return .red
        case "done": return .green
        default: return .white
        }
    }

    @ViewBuilder static func mark(_ phase: String) -> some View {
        switch phase {
        case "input": Image(systemName: "hand.raised.fill").foregroundStyle(.orange)
        case "failed": Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
        case "done": Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        default: Image(systemName: "sparkle").foregroundStyle(accent)
        }
    }

    /// For how long it's been at it, or (done) how long it took: "Took
    /// 18s" where there's room (a bare "18s" by the title, on the lock
    /// screen, read as "18 seconds ago").
    @ViewBuilder static func clock(_ s: SessionActivityAttributes.ContentState, took: Bool = false) -> some View {
        if !running(s) {
            let d = duration(max(0, (s.until > 0 ? s.until : Date.now.timeIntervalSince1970) - s.since))
            Text(took ? "Took \(d)" : d)
        } else {
            Text(Date(timeIntervalSince1970: s.since), style: .timer)
                .multilineTextAlignment(.trailing)
        }
    }

    /// Still at it: its clock counts up (a live timer).
    static func running(_ s: SessionActivityAttributes.ContentState) -> Bool {
        !(s.until > 0 || s.phase == "done" || s.phase == "failed")
    }

    static func duration(_ seconds: Double) -> String {
        let s = Int(seconds)
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m \(s % 60)s" }
        return "\(s / 3600)h \(s % 3600 / 60)m"
    }

    /// What it's doing: opened with this phone's keys (harlo seals it).
    static func words(_ s: SessionActivityAttributes.ContentState, stale: Bool = false) -> String {
        // Still at work, but harlo hasn't said anything for a while.
        if stale && (s.phase == "working" || s.phase == "input") { return "No news from harlo: is the computer asleep?" }
        let now = s.now ?? LiveDetail.open(s.sealed, store: KeychainStore(accessGroup: keychainGroup))?.now ?? ""
        switch s.phase {
        case "input": return now.isEmpty ? "Waiting for you" : now
        case "done": return now.isEmpty ? "Done" : "Done · " + now
        case "failed": return now.isEmpty ? "Stopped with an error" : "Stopped · " + now
        default: return now.isEmpty ? "Working" : now
        }
    }

    static func agent(_ id: String) -> String {
        ["harlo": "Harlo", "claude": "Claude", "pi": "pi", "demo": "Demo"][id] ?? id
    }
}
