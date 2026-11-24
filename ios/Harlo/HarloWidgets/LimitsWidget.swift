import HarloKit
import SwiftUI
import WidgetKit

/// Your plans' limits on the home screen and the lock screen: how much of
/// each window is used and when it resets, as harlo last said (the app
/// writes them as harlo's plan limits change; see HarloKit's
/// `LimitsSnapshot`). Its timeline has an entry at each reset in the next
/// day, so a window shows as reset once its time passes.
struct LimitsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "HarloLimits", provider: LimitsProvider()) { entry in
            LimitsWidgetView(snapshot: entry.snapshot, family: entry.family, now: entry.date)
                .containerBackground(for: .widget) {
                    // (The lock screen: the system's own.)
                    if entry.family == .accessoryRectangular { Color.clear } else { SessionsBackground() }
                }
                .widgetURL(URL(string: "harlo://limits"))
        }
        .configurationDisplayName("Plan limits")
        .description("How much of your plans' limits is used, and when they reset.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge, .accessoryRectangular])
    }
}
