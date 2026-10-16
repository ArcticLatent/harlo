import Foundation

/// What a notification from harlo says, sealed for this phone (harlo's
/// `phone_push`): the app's notification extension opens it, and Allow /
/// Deny on the lock screen answer its request.
public struct PushContent: Equatable, Sendable {
    /// "ask", "done", "failed" or "test".
    public var kind: String
    /// The session's key.
    public var session: String
    /// The session's title.
    public var title: String
    public var body: String
    /// The question or approval (an `ask`).
    public var request: UIRequest?
    /// As sent (kept with the notification for its actions).
    public var json: JSON

    public init(_ v: JSON) {
        json = v
        kind = v["k"].text
        session = v["s"].text
        title = v["t"].text
        body = v["b"].text
        request = v["r"]["id"].string == nil ? nil : UIRequest(v["r"])
    }

    /// Opens a notification's `h` with this phone's key and harlo's.
    public static func open(_ sealed: String, identity: KeyPair, harloKey: String) throws -> PushContent {
        PushContent(try Sealed.open(sealed, identity: identity, harloKey: harloKey))
    }

    /// The same, with the keys from `store` (the app's Keychain).
    public static func open(_ sealed: String, store: SecretStore) throws -> PushContent {
        PushContent(try Sealed.open(sealed, store: store))
    }

    /// Allow / Deny can answer it from the notification.
    public var canApprove: Bool {
        guard let r = request else { return false }
        return r.approval || r.method == "confirm"
    }

    /// The notification's lines: title, subtitle, body.
    public var lines: (title: String, subtitle: String, body: String) {
        let name = title.isEmpty ? "harlo" : title
        switch kind {
        case "ask":
            return (name, canApprove ? "Waiting for your approval" : "Waiting for your answer", body)
        case "done":
            return (name, "Finished", body)
        case "failed":
            return (name, "Stopped with an error", body)
        default:
            return (title.isEmpty ? "harlo" : title, "", body)
        }
    }
}

/// This phone's notifications as harlo keeps them: where they go (APNs'
/// token) and what it wants.
public struct PushRegistration: Codable, Equatable, Sendable {
    /// The APNs device token, hex.
    public var token: String
    /// Built for development (an app run from Xcode).
    public var sandbox: Bool
    public var asks: Bool
    public var done: Bool

    public init(token: String, sandbox: Bool, asks: Bool = true, done: Bool = true) {
        self.token = token
        self.sandbox = sandbox
        self.asks = asks
        self.done = done
    }

    public static func hex(_ token: Data) -> String {
        token.map { String(format: "%02x", $0) }.joined()
    }

    var json: JSON {
        ["token": .string(token), "sandbox": .bool(sandbox), "asks": .bool(asks), "done": .bool(done)]
    }
}

/// What harlo seals for this phone outside the connection (notifications,
/// Live Activities), opened with the keys the app keeps.
public enum Sealed {
    public static func open(_ sealed: String, identity: KeyPair, harloKey: String) throws -> JSON {
        let box = try Box(theirPublic: try Base64.decode(harloKey), ours: identity)
        return try JSON.parse(try box.open(sealed))
    }

    public static func open(_ sealed: String, store: SecretStore) throws -> JSON {
        guard let secret = store.load("identity"), let hostData = store.load("host"),
              let host = try? JSONDecoder().decode(PairedHarlo.self, from: hostData)
        else { throw HarloError.notConnected }
        return try open(sealed, identity: try KeyPair(secret: secret), harloKey: host.key)
    }
}

/// A Live Activity's words (harlo's `phone_activity`): what the session is
/// doing, sealed for this phone; the widget opens them.
public struct LiveDetail: Equatable, Sendable {
    /// "Running cargo test", "Waiting for your approval", or (done) its last reply.
    public var now: String
    public var title: String

    public init(_ v: JSON) {
        now = v["now"].text
        title = v["title"].text
    }

    public static func open(_ sealed: String, store: SecretStore) -> LiveDetail? {
        guard !sealed.isEmpty, let v = try? Sealed.open(sealed, store: store) else { return nil }
        return LiveDetail(v)
    }
}
