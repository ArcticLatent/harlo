/// A message waiting for the agent: steering (taken in while it works) or
/// a follow-up (after it finishes).
public struct QueuedMessage: Identifiable, Hashable, Sendable {
    /// "steering" or "followUp".
    public let queue: String
    public let text: String
    /// Its queue, its text and which of the same it is, so a row keeps its
    /// identity when one before it leaves (and two alike are two rows).
    public let id: String

    public var isSteering: Bool { queue == "steering" }

    /// The waiting messages, steering first.
    public static func list(steering: [String], followUp: [String]) -> [QueuedMessage] {
        var seen: [String: Int] = [:]
        return (steering.map { ("steering", $0) } + followUp.map { ("followUp", $0) }).map { queue, text in
            let key = queue + "\n" + text
            let n = seen[key, default: 0]
            seen[key] = n + 1
            return QueuedMessage(queue: queue, text: text, id: "\(key)\n\(n)")
        }
    }
}
