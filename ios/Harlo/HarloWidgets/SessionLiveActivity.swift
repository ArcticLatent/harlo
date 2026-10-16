import ActivityKit
import HarloKit
import SwiftUI
import WidgetKit

@main
struct HarloWidgets: WidgetBundle {
    var body: some Widget {
        SessionLiveActivity()
        SessionsWidget()
    }
}

/// A session at work on the lock screen and in the Dynamic Island: what
/// it's doing (opened with this phone's keys: harlo seals it), its todos
/// and for how long; waiting for you; done.
struct SessionLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SessionActivityAttributes.self) { context in
            LockScreenView(attributes: context.attributes, state: context.state, stale: context.isStale)
                .padding(16)
                .activityBackgroundTint(Color.black.opacity(0.55))
                .activitySystemActionForegroundColor(Live.accent)
                .widgetURL(Live.url(context.attributes.key))
        } dynamicIsland: { context in
            let state = context.state
            let stale = context.isStale
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Live.mark(state.phase)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Live.clock(state)
                        .font(.system(size: 15, weight: .semibold).monospacedDigit())
                        .frame(maxWidth: 70, alignment: .trailing)
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(context.attributes.title)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(Live.words(state, stale: stale))
                            .font(.system(size: 13, design: .monospaced))
                            .foregroundStyle(stale ? .secondary : Live.color(state.phase))
                            .lineLimit(2)
                        if state.total > 0 {
                            ProgressView(value: Double(state.done), total: Double(state.total)) {
                                EmptyView()
                            } currentValueLabel: {
                                Text("\(state.done)/\(state.total) todos").font(.system(size: 11, design: .monospaced))
                            }
                            .tint(Live.accent)
                        }
                    }
                    .padding(.horizontal, 4)
                }
            } compactLeading: {
                Live.mark(state.phase)
            } compactTrailing: {
                if state.total > 0 && state.phase == "working" {
                    Text("\(state.done)/\(state.total)")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Live.accent)
                } else {
                    Live.clock(state)
                        .font(.system(size: 13, weight: .semibold).monospacedDigit())
                        .frame(maxWidth: 52)
                        .foregroundStyle(Live.color(state.phase))
                }
            } minimal: {
                Live.mark(state.phase)
            }
            .widgetURL(Live.url(context.attributes.key))
            .keylineTint(Live.accent)
        }
    }
}

struct LockScreenView: View {
    let attributes: SessionActivityAttributes
    let state: SessionActivityAttributes.ContentState
    /// No news from harlo for a while.
    var stale = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Live.mark(state.phase)
                Text(attributes.title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                Spacer(minLength: 6)
                Live.clock(state)
                    .font(.system(size: 14, weight: .medium).monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: 80, alignment: .trailing)
            }
            Text(Live.words(state, stale: stale))
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(stale ? .secondary : Live.color(state.phase))
                .lineLimit(2)
            HStack(spacing: 8) {
                Text(Live.agent(attributes.harness))
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(.secondary)
                if state.total > 0 {
                    ProgressView(value: Double(state.done), total: Double(state.total))
                        .tint(Live.accent)
                    Text("\(state.done)/\(state.total)")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .foregroundStyle(.white)
    }
}

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

    /// For how long it's been at it, or (done) how long it took.
    @ViewBuilder static func clock(_ s: SessionActivityAttributes.ContentState) -> some View {
        if s.until > 0 || s.phase == "done" || s.phase == "failed" {
            Text(duration(max(0, (s.until > 0 ? s.until : Date().timeIntervalSince1970) - s.since)))
        } else {
            Text(Date(timeIntervalSince1970: s.since), style: .timer)
                .multilineTextAlignment(.trailing)
        }
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
        ["claude": "Claude", "opencode": "OpenCode", "pi": "Pi", "omp": "Oh My Pi", "demo": "Demo"][id] ?? id
    }
}
