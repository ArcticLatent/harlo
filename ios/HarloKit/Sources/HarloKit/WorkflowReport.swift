import Foundation

/// A workflow agent's report (harlo's `report_done` call), in words.
public struct WorkflowReport: Equatable, Sendable {
    /// The attempt it reports (nil when the call doesn't say).
    public var attempt: Int?
    /// What it says was done.
    public var summary: String
    /// Refused because its attempt is over (a later one is under way):
    /// not used, rather than failed.
    public var superseded: Bool

    /// "Reported attempt 2", or "Reported back".
    public var title: String {
        attempt.map { "Reported attempt \($0)" } ?? "Reported back"
    }
}
