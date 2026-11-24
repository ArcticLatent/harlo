import HarloKit
import SwiftUI
import WidgetKit

struct SessionsProvider: TimelineProvider {
    func placeholder(in context: Context) -> SessionsEntry {
        SessionsEntry(date: Date.now, snapshot: .sample(), family: context.family)
    }

    func getSnapshot(in context: Context, completion: @escaping (SessionsEntry) -> Void) {
        // The gallery: sample sessions; on the home screen, yours.
        let snap = context.isPreview ? .sample() : (load() ?? .sample())
        completion(SessionsEntry(date: Date.now, snapshot: snap, family: context.family))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SessionsEntry>) -> Void) {
        let now = Date.now
        let entry = SessionsEntry(date: now, snapshot: load() ?? .unpaired, family: context.family)
        completion(Timeline(entries: [entry], policy: .after(now.addingTimeInterval(15 * 60))))
    }

    func load() -> WidgetSnapshot? {
        WidgetSnapshot.load(KeychainStore(accessGroup: Live.keychainGroup))
    }
}
