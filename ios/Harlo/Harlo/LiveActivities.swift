import ActivityKit
import Foundation
import HarloKit

/// Live Activities: the app starts one for a session when you send it a
/// message, and hands harlo its push token; harlo sends the updates (what
/// it's doing, its todos, waiting for you) and the end.
@MainActor
enum LiveActivities {
    /// Settings' switch (on unless turned off).
    nonisolated static var enabled: Bool {
        get { UserDefaults.standard.object(forKey: "liveActivities") as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: "liveActivities") }
    }

    /// Push tokens of activities, by session (handed to harlo again when a
    /// running one gets another message).
    private static var tokens: [String: String] = [:]
    /// When each was started or given another message (the list says the
    /// session works a moment later).
    private static var started: [String: Date] = [:]

    /// No news from harlo by then: it shows as stale (harlo's updates move
    /// it on; `phone_activity`'s STALE_AFTER).
    static var staleDate: Date { Date.now.addingTimeInterval(25 * 60) }

    static func start(for s: OpenSession, model: HarloModel) {
        let key = s.liveKey
        guard enabled, !model.demo, !key.isEmpty, ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        started[key] = Date.now
        let state = SessionActivityAttributes.ContentState(phase: "working", since: Date.now.timeIntervalSince1970, until: 0, done: 0, total: 0, sealed: "", now: "Working")
        let mine = Activity<SessionActivityAttributes>.activities.filter { $0.attributes.key == key }
        // Still showing this session's run: it goes on (harlo gets the token again).
        if let running = mine.first(where: { $0.activityState == .active }) {
            if let token = tokens[key] { Task { await model.setActivityToken(token, for: key) } }
            let id = running.id, stale = staleDate
            Task { await update(id, to: state, staleDate: stale) }
            return
        }
        // A finished one still on the lock screen goes.
        let old = mine.map(\.id)
        if !old.isEmpty { Task { await end(old) } }
        let attributes = SessionActivityAttributes(key: key, title: s.title.isEmpty ? "Session" : s.title, harness: s.harness)
        guard let activity = try? Activity.request(attributes: attributes, content: ActivityContent(state: state, staleDate: staleDate), pushType: .token) else { return }
        Task {
            for await data in activity.pushTokenUpdates {
                let token = PushRegistration.hex(data)
                tokens[key] = token
                await model.setActivityToken(token, for: key)
            }
        }
    }

    /// Back in the app: activities of sessions no longer at work end (if
    /// harlo's end didn't come), and finished ones leave the lock screen
    /// (you're in the app now; one left there for hours may later show
    /// empty).
    static func tidy(_ sessions: [SessionSummary]) {
        var gone: [String] = []
        for a in Activity<SessionActivityAttributes>.activities where a.activityState == .ended {
            gone.append(a.id)
            tokens[a.attributes.key] = nil
        }
        for a in Activity<SessionActivityAttributes>.activities where a.activityState == .active || a.activityState == .stale {
            guard let s = sessions.first(where: { $0.key == a.attributes.key }),
                  Date.now.timeIntervalSince(started[s.key] ?? .distantPast) > 20 else { continue }
            if !["busy", "starting", "input"].contains(s.status) {
                gone.append(a.id)
                tokens[a.attributes.key] = nil
            }
        }
        if !gone.isEmpty { Task { await end(gone) } }
    }

    /// Turned off in Settings: those showing go.
    static func endAll() {
        let all = Activity<SessionActivityAttributes>.activities.map(\.id)
        if !all.isEmpty { Task { await end(all) } }
        tokens = [:]
    }

    // An `Activity` isn't Sendable (the iOS 27 SDK says so): what's handed
    // off the main actor is its id, and the activity is looked up where it's
    // updated or ended.

    /// Ends the activities with these ids, right away.
    nonisolated private static func end(_ ids: [String]) async {
        for a in Activity<SessionActivityAttributes>.activities where ids.contains(a.id) {
            await a.end(nil, dismissalPolicy: .immediate)
        }
    }

    /// Shows this state on the activity with this id.
    nonisolated private static func update(_ id: String, to state: SessionActivityAttributes.ContentState, staleDate: Date) async {
        guard let a = Activity<SessionActivityAttributes>.activities.first(where: { $0.id == id }) else { return }
        await a.update(ActivityContent(state: state, staleDate: staleDate))
    }
}
