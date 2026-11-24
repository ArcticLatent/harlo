import HarloKit
import SwiftUI
import WidgetKit

struct LimitsProvider: TimelineProvider {
    func placeholder(in context: Context) -> LimitsEntry {
        LimitsEntry(date: Date.now, snapshot: .sample(), family: context.family)
    }

    func getSnapshot(in context: Context, completion: @escaping (LimitsEntry) -> Void) {
        // The gallery: sample plans; on the home screen, yours.
        let now = Date.now
        let snap = context.isPreview ? .sample(now: now) : (load() ?? .sample(now: now))
        completion(LimitsEntry(date: now, snapshot: snap.at(now), family: context.family))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<LimitsEntry>) -> Void) {
        let now = Date.now
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
