import ActivityKit
import Foundation

/// A session at work, on the lock screen and in the Dynamic Island. The app
/// starts it when you send; harlo (`phone_activity`) sends its updates and
/// its end. Shared by the app and its widget extension.
struct SessionActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        /// "working", "input" (waiting for you), "done", "failed".
        var phase: String
        /// When the run started, and (done) ended: unix seconds.
        var since: Double
        var until: Double
        /// Todos done, of all.
        var done: Int
        var total: Int
        /// What it's doing, sealed for this phone (HarloKit's `LiveDetail`).
        var sealed: String
        /// What it's doing, in the clear: only as the app starts it (harlo's
        /// updates are sealed).
        var now: String?
    }

    /// The session's live key.
    var key: String
    var title: String
    var harness: String
}
