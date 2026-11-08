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

struct LimitsEntry: TimelineEntry {
    let date: Date
    let snapshot: LimitsSnapshot
    let family: WidgetFamily
}

struct LimitsProvider: TimelineProvider {
    func placeholder(in context: Context) -> LimitsEntry {
        LimitsEntry(date: Date(), snapshot: .sample(), family: context.family)
    }

    func getSnapshot(in context: Context, completion: @escaping (LimitsEntry) -> Void) {
        // The gallery: sample plans; on the home screen, yours.
        let now = Date()
        let snap = context.isPreview ? .sample(now: now) : (load() ?? .sample(now: now))
        completion(LimitsEntry(date: now, snapshot: snap.at(now), family: context.family))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<LimitsEntry>) -> Void) {
        let now = Date()
        let snap = load() ?? .unpaired
        // Now, and each reset in the next day (a little after it).
        let times = [now] + snap.resets(after: now).prefix(12).map { $0.addingTimeInterval(5) }
        let entries = times.map { LimitsEntry(date: $0, snapshot: snap.at($0), family: context.family) }
        completion(Timeline(entries: entries, policy: .after(now.addingTimeInterval(30 * 60))))
    }

    func load() -> LimitsSnapshot? {
        LimitsSnapshot.load(KeychainStore(accessGroup: Live.keychainGroup))
    }
}
