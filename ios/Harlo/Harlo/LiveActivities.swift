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
    static var staleDate: Date { Date().addingTimeInterval(25 * 60) }

    static func start(for s: OpenSession, model: HarloModel) {
        let key = s.liveKey
        guard enabled, !model.demo, !key.isEmpty, ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        started[key] = Date()
        let state = SessionActivityAttributes.ContentState(phase: "working", since: Date().timeIntervalSince1970, until: 0, done: 0, total: 0, sealed: "", now: "Working")
        let mine = Activity<SessionActivityAttributes>.activities.filter { $0.attributes.key == key }
        // Still showing this session's run: it goes on (harlo gets the token again).
        if let running = mine.first(where: { $0.activityState == .active }) {
            if let token = tokens[key] { Task { await model.setActivityToken(token, for: key) } }
            Task { await running.update(ActivityContent(state: state, staleDate: staleDate)) }
            return
        }
        // A finished one still on the lock screen goes.
        for old in mine { Task { await old.end(nil, dismissalPolicy: .immediate) } }
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
    /// harlo's end didn't come).
    static func tidy(_ sessions: [SessionSummary]) {
        for a in Activity<SessionActivityAttributes>.activities where a.activityState == .active {
            guard let s = sessions.first(where: { $0.key == a.attributes.key }),
                  Date().timeIntervalSince(started[s.key] ?? .distantPast) > 20 else { continue }
            if !["busy", "starting", "input"].contains(s.status) {
                Task { await a.end(nil, dismissalPolicy: .immediate) }
                tokens[a.attributes.key] = nil
            }
        }
    }

    /// Turned off in Settings: those showing go.
    static func endAll() {
        for a in Activity<SessionActivityAttributes>.activities { Task { await a.end(nil, dismissalPolicy: .immediate) } }
        tokens = [:]
    }
}
