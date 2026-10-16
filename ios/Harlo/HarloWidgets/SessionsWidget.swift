import HarloKit
import SwiftUI
import WidgetKit

/// harlo's sessions on the home screen: what works, what needs you, what
/// finished. The app writes them as its list changes, and the notification
/// extension as harlo's notifications arrive while the app is closed (see
/// HarloKit's `WidgetSnapshot`); the widget reads them every 15 minutes, and
/// whenever either asks it to.
struct SessionsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "HarloSessions", provider: SessionsProvider()) { entry in
            SessionsWidgetView(snapshot: entry.snapshot, family: entry.family, now: entry.date)
                .containerBackground(for: .widget) { SessionsBackground() }
                .widgetURL(entry.snapshot.items.first.map { SessionsWidgetView.url($0.key) })
        }
        .configurationDisplayName("Sessions")
        .description("Your harlo sessions: running, waiting for you, done.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

struct SessionsEntry: TimelineEntry {
    let date: Date
    let snapshot: WidgetSnapshot
    let family: WidgetFamily
}

struct SessionsProvider: TimelineProvider {
    func placeholder(in context: Context) -> SessionsEntry {
        SessionsEntry(date: Date(), snapshot: .sample(), family: context.family)
    }

    func getSnapshot(in context: Context, completion: @escaping (SessionsEntry) -> Void) {
        // The gallery: sample sessions; on the home screen, yours.
        let snap = context.isPreview ? .sample() : (load() ?? .sample())
        completion(SessionsEntry(date: Date(), snapshot: snap, family: context.family))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SessionsEntry>) -> Void) {
        let now = Date()
        let entry = SessionsEntry(date: now, snapshot: load() ?? .unpaired, family: context.family)
        completion(Timeline(entries: [entry], policy: .after(now.addingTimeInterval(15 * 60))))
    }

    func load() -> WidgetSnapshot? {
        WidgetSnapshot.load(KeychainStore(accessGroup: Live.keychainGroup))
    }
}

/// The widget's background: harlo's own, light or dark.
struct SessionsBackground: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let p = Theme.standard.palette(dark: scheme == .dark)
        let c = p.background.mix(p.foreground, scheme == .dark ? 0.03 : 0)
        Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: 1)
    }
}
