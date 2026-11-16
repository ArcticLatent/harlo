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
