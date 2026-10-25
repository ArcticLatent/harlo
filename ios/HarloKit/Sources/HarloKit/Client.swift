import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// What `ready` says: harlo's computer name and this phone's name there.
public struct Ready: Equatable, Sendable {
    public var harloName: String
    public var deviceName: String
    /// The endpoint that answered.
    public var endpoint: URL
    /// Every place harlo can be reached now (its addresses, Funnel, relay).
    public var endpoints: [URL] = []
}

/// One encrypted connection to harlo (phone_server.rs's `connection`): the
/// plaintext hello, then sealed requests and replies, and harlo's events.
/// Reconnecting is the caller's business: `events` ends when the connection
/// does, with the reason in `closeReason`.
public actor HarloClient {
    private let identity: KeyPair
    private let harloKey: Data
    private let task: URLSessionWebSocketTask
    private let session: URLSession
    private var channel: Channel?
    private var nextId: Int64 = 1
    private var pending: [Int64: CheckedContinuation<JSON, Error>] = [:]
    private var pinger: Task<Void, Never>?
    /// The last frame on its way out.
    private var sending: Task<Void, Never>?
    private let continuation: AsyncStream<Event>.Continuation
    public nonisolated let events: AsyncStream<Event>
    public private(set) var closeReason: String?
    public private(set) var isConnected = false

    /// Connects and says hello: with `code` (and this phone's name) the first
    /// time, to pair; by key alone afterwards. Throws `.denied` with harlo's
    /// reason, or `.closed` when nothing answered within `timeout`.
    public static func connect(to endpoint: URL, identity: KeyPair, harloKey: String, code: String? = nil,
                               deviceName: String = "iPhone", timeout: TimeInterval = 8) async throws -> (HarloClient, Ready)
    {
        let key = try Base64.decode(harloKey)
        guard key.count == 32 else { throw HarloError.badKey }
        let client = HarloClient(endpoint: endpoint, identity: identity, harloKey: key)
        let ready = try await client.hello(code: code, deviceName: deviceName, timeout: timeout, endpoint: endpoint)
        return (client, ready)
    }

    private init(endpoint: URL, identity: KeyPair, harloKey: Data) {
        self.identity = identity
        self.harloKey = harloKey
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        session = URLSession(configuration: config)
        task = session.webSocketTask(with: endpoint)
        // harlo keeps frames to about half a megabyte; iOS's default limit
        // (1 MB) is tight for a big row.
        task.maximumMessageSize = 16 << 20
        (events, continuation) = AsyncStream.makeStream(of: Event.self, bufferingPolicy: .unbounded)
    }

    private func hello(code: String?, deviceName: String, timeout: TimeInterval, endpoint: URL) async throws -> Ready {
        task.resume()
        var hello: JSON = ["type": "hello", "key": .string(Base64.encode(identity.publicKey))]
        if let code, !code.isEmpty {
            hello["code"] = .string(code)
            hello["name"] = .string(deviceName)
        }
        // Nothing within `timeout`: the socket is closed, which ends the wait.
        let timer = Task { [task] in
            try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
            if !Task.isCancelled { task.cancel(with: .goingAway, reason: nil) }
        }
        defer { timer.cancel() }
        let reply: JSON
        do {
            try await task.send(.string(String(decoding: try hello.data(), as: UTF8.self)))
            switch try await task.receive() {
            case .string(let s): reply = try JSON.parse(s)
            case .data(let d): reply = try JSON.parse(d)
            @unknown default: throw HarloError.badFrame("an unexpected frame")
            }
        } catch {
            task.cancel(with: .goingAway, reason: nil)
            let code = task.closeCode.rawValue
            if Self.isOffline(error) { throw HarloError.offline((error as NSError).localizedDescription) }
            throw HarloError.closed(Self.explain(error, closeCode: code))
        }
        switch reply["type"].text {
        case "ready":
            let box = try Box(theirPublic: harloKey, ours: identity)
            channel = Channel(box: box, id: reply["conn"].text)
            isConnected = true
            startReading()
            startPinging()
            return Ready(harloName: reply["name"].text, deviceName: reply["device"].text, endpoint: endpoint,
                         endpoints: (reply["endpoints"].array ?? []).compactMap { $0.string.flatMap(URL.init(string:)) })
        case "denied":
            task.cancel(with: .normalClosure, reason: nil)
            throw HarloError.denied(reply["reason"].string ?? "harlo refused this phone.")
        default:
            task.cancel(with: .protocolError, reason: nil)
            throw HarloError.badFrame("harlo answered something unexpected")
        }
    }

    /// The connection never left the phone: no network, or (the first time
    /// a LAN address is tried) iOS showing its Local Network prompt, during
    /// which connections fail as "offline".
    static func isOffline(_ error: Error) -> Bool {
        let e = error as NSError
        return e.domain == NSURLErrorDomain
            && [NSURLErrorNotConnectedToInternet, NSURLErrorDataNotAllowed, NSURLErrorInternationalRoamingOff].contains(e.code)
    }

    private static func explain(_ error: Error, closeCode: Int) -> String {
        // The relay's "harlo isn't here".
        if closeCode == 4404 { return "harlo isn't connected to the relay right now." }
        if let e = error as? HarloError { return e.description }
        // Closed by the timer (or nothing there to answer).
        if (error as NSError).code == NSURLErrorCancelled { return "harlo didn't answer." }
        return (error as NSError).localizedDescription
    }

    /// Sends a request (`sessions`, `open`, `send`, `abort`, `answer`, `ping`)
    /// and waits for its reply. A reply `{ error }` is returned as is.
    public func call(_ op: String, _ args: JSON = [:]) async throws -> JSON {
        guard isConnected, channel != nil else { throw HarloError.notConnected }
        let id = nextId
        nextId += 1
        var msg = args
        msg["id"] = .int(id)
        msg["op"] = .string(op)
        let frame = try channel!.seal(msg)
        return try await withCheckedThrowingContinuation { cont in
            pending[id] = cont
            // Out in the order sealed: each frame waits for the one before
            // (harlo drops a connection whose frames arrive out of order).
            let before = sending
            sending = Task {
                await before?.value
                await self.write(frame, id: id)
            }
        }
    }

    private func write(_ frame: String, id: Int64) async {
        do {
            try await task.send(.string(frame))
        } catch {
            pending.removeValue(forKey: id)?.resume(throwing: HarloError.notConnected)
        }
    }

    // ---- the protocol's requests -------------------------------------------

    public func sessions() async throws -> [SessionSummary] {
        let r = try await call("sessions")
        if let e = r["error"].string { throw HarloError.closed(e) }
        return SessionSummary.list(r["sessions"])
    }

    /// Opens a session (starting it in harlo if needed): its live key and
    /// agent. Its rows follow as a `reset` event.
    public func open(_ key: String) async throws -> (key: String, harness: String) {
        let r = try await call("open", ["key": .string(key)])
        if let e = r["error"].string { throw HarloError.closed(e) }
        return (r["key"].text, r["harness"].text)
    }

    public func send(_ key: String, text: String, mode: SendMode = .normal, photos: [Photo] = []) async throws {
        var args: JSON = ["key": .string(key), "text": .string(text), "mode": .string(mode.rawValue)]
        if !photos.isEmpty { args["images"] = .array(photos.map(\.json)) }
        try check(await call("send", args))
    }

    /// A reply's `error` as an error.
    @discardableResult
    private func check(_ r: JSON) throws -> JSON {
        if let e = r["error"].string { throw HarloError.closed(e) }
        return r
    }

    /// Projects, agents and models for a new session.
    public func newSessionChoices() async throws -> NewSessionChoices {
        NewSessionChoices(try check(await call("projects")))
    }

    /// Starts a session (`project` -1: the home folder; "" agent or model:
    /// the default) with its first message. Its live key and agent.
    public func newSession(project: Int64, agent: String, model: String, approval: String = "", text: String, photos: [Photo]) async throws -> (key: String, harness: String) {
        let r = try check(await call("new", ["project": .int(project), "agent": .string(agent), "model": .string(model), "approval": .string(approval),
                                             "text": .string(text), "images": .array(photos.map(\.json))]))
        return (r["key"].text, r["harness"].text)
    }

    /// Takes back a message waiting in omp's queue (`queue`: "steering" or
    /// "followUp").
    public func unqueue(_ key: String, queue: String, text: String) async throws {
        try check(await call("unqueue", ["key": .string(key), "queue": .string(queue), "text": .string(text)]))
    }

    /// Sends a waiting follow-up now, as steering (omp).
    public func promote(_ key: String, text: String) async throws {
        try check(await call("promote", ["key": .string(key), "text": .string(text)]))
    }

    /// Branches the session at one of the user's messages (its row key) and
    /// returns that message's text to edit; `fork` copies the session up to
    /// it into a new one instead (omp). nil: cancelled.
    public func branch(_ key: String, row: String, fork: Bool) async throws -> String? {
        let r = try check(await call("branch", ["key": .string(key), "row": .string(row), "fork": .bool(fork)]))
        return r["cancelled"].bool == true ? nil : r["text"].text
    }

    public func modes(_ key: String) async throws -> Modes {
        Modes(try check(await call("modes", ["key": .string(key)])))
    }

    public func setModes(_ key: String, approval: String? = nil, plan: Bool? = nil) async throws -> Modes {
        var args: JSON = ["key": .string(key)]
        if let approval { args["approval"] = .string(approval) }
        if let plan { args["plan"] = .bool(plan) }
        return Modes(try check(await call("setModes", args)))
    }

    public func models(_ key: String) async throws -> [ModelChoice] {
        (try check(await call("models", ["key": .string(key)]))["models"].array ?? []).map(ModelChoice.init)
    }

    public func setModel(_ key: String, _ model: ModelChoice) async throws {
        try check(await call("setModel", ["key": .string(key), "provider": .string(model.provider), "model": .string(model.modelId)]))
    }

    public func thinkingLevels(_ key: String) async throws -> [String] {
        (try check(await call("thinking", ["key": .string(key)]))["levels"].array ?? []).compactMap(\.string)
    }

    public func setThinking(_ key: String, _ level: String) async throws {
        try check(await call("setThinking", ["key": .string(key), "level": .string(level)]))
    }

    public func rename(_ key: String, to name: String) async throws {
        try check(await call("rename", ["key": .string(key), "name": .string(name)]))
    }

    public func archive(_ key: String, _ archived: Bool = true) async throws {
        try check(await call("archive", ["key": .string(key), "archived": .bool(archived)]))
    }

    public func pin(_ key: String, _ pinned: Bool) async throws {
        try check(await call("pin", ["key": .string(key), "pinned": .bool(pinned)]))
    }

    /// Your messages in the open session a turn can be undone to: row key →
    /// snapshot.
    /// Which of your messages a turn can be undone to (row key → snapshot),
    /// and whether the folder is a git repository (its changes have diffs;
    /// nil from a harlo that doesn't say).
    public func undoTargets(_ key: String) async throws -> (targets: [String: String], git: Bool?) {
        let r = try check(await call("undoTargets", ["key": .string(key)]))
        return ((r["targets"].object ?? [:]).compactMapValues(\.string), r["git"].bool)
    }

    /// What undoing from one of your messages does (`apply`: does it).
    public func undo(_ key: String, row: String, apply: Bool) async throws -> UndoPlan {
        UndoPlan(try check(await call(apply ? "undo" : "undoPlan", ["key": .string(key), "row": .string(row)])))
    }

    public func changes(_ key: String) async throws -> Changes {
        Changes(try check(await call("changes", ["key": .string(key)])))
    }

    /// The session as Markdown.
    public func markdown(_ key: String) async throws -> String {
        try check(await call("markdown", ["key": .string(key)]))["markdown"].text
    }

    public func actions(_ key: String) async throws -> ActionLog {
        ActionLog(try check(await call("actions", ["key": .string(key)])))
    }

    public func diff(_ key: String, path: String) async throws -> FileDiff {
        FileDiff(try check(await call("diff", ["key": .string(key), "path": .string(path)])))
    }

    /// The session folder's files matching `query` (`@` mentions), best first.
    public func files(_ key: String, query: String) async throws -> [String] {
        (try check(await call("files", ["key": .string(key), "query": .string(query)]))["files"].array ?? []).compactMap(\.string)
    }

    /// A file of the session's folder, to read.
    public func file(_ key: String, path: String) async throws -> FileText {
        FileText(try check(await call("file", ["key": .string(key), "path": .string(path)])))
    }

    public func branches(_ key: String) async throws -> Branches {
        Branches(try check(await call("branches", ["key": .string(key)])))
    }

    /// Switches the session's repository to `name` (`remote`: `origin/x`,
    /// tracked as `x`), stashing uncommitted changes first with `stash`.
    /// What git did, in words.
    public func switchBranch(_ key: String, to name: String, remote: Bool, stash: Bool) async throws -> String {
        try check(await call("switchBranch", ["key": .string(key), "name": .string(name), "remote": .bool(remote), "stash": .bool(stash)]))["message"].text
    }

    /// A Live Activity for the session: its push token ("" ended).
    public func activity(_ key: String, token: String, sandbox: Bool) async throws {
        try check(await call("activity", ["key": .string(key), "token": .string(token), "sandbox": .bool(sandbox)]))
    }

    /// The desktop's theme (nil: harlo sent none it could read).
    public func theme() async throws -> Theme? {
        Theme(try check(await call("theme")))
    }

    /// Picks your penguin (one of `Theme.avatars`) for harlo too; its theme
    /// with it.
    public func setAvatar(_ name: String) async throws -> Theme? {
        Theme(try check(await call("setAvatar", ["avatar": .string(name)])))
    }

    public func abort(_ key: String) async throws {
        _ = try await call("abort", ["key": .string(key)])
    }

    public func answer(_ key: String, requestId: String, answer: JSON) async throws {
        let r = try await call("answer", ["key": .string(key), "requestId": .string(requestId), "answer": answer])
        if let e = r["error"].string { throw HarloError.closed(e) }
    }

    /// Whether harlo still answers within `timeout` seconds (after the
    /// network changed, a connection can be dead without knowing it).
    public func alive(timeout: Double) async -> Bool {
        guard isConnected else { return false }
        return await firstOf(timeout: timeout) { (try? await self.call("ping"))?["pong"].bool == true }
    }

    public func close() {
        finish("closed")
        task.cancel(with: .normalClosure, reason: nil)
    }

    // ---- reading -------------------------------------------------------------

    private func startReading() {
        // (The task runs on this actor.)
        Task { [task] in
            while true {
                let msg: URLSessionWebSocketTask.Message
                do { msg = try await task.receive() } catch {
                    self.finish(Self.explain(error, closeCode: task.closeCode.rawValue))
                    return
                }
                let text: String
                switch msg {
                case .string(let s): text = s
                case .data(let d): text = String(decoding: d, as: UTF8.self)
                @unknown default: continue
                }
                if !self.handle(text) { return }
            }
        }
    }

    /// One sealed frame from harlo; false: the connection is done.
    private func handle(_ frame: String) -> Bool {
        guard var ch = channel else { return false }
        let msg: JSON
        do {
            msg = try ch.open(frame)
            channel = ch
        } catch {
            finish((error as? HarloError)?.description ?? "a bad frame")
            task.cancel(with: .protocolError, reason: nil)
            return false
        }
        if let id = msg["id"].int, let cont = pending.removeValue(forKey: id) {
            cont.resume(returning: msg)
            return true
        }
        if let ev = Event(msg) {
            continuation.yield(ev)
            if ev == .revoked {
                finish("This phone was removed in harlo.")
                task.cancel(with: .normalClosure, reason: nil)
                return false
            }
        }
        return true
    }

    private func startPinging() {
        // Something every 30 s, so relays and proxies keep a quiet connection.
        pinger = Task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 30_000_000_000)
                if Task.isCancelled { return }
                _ = try? await self.call("ping")
            }
        }
    }

    private func finish(_ reason: String) {
        guard closeReason == nil else { return }
        closeReason = reason
        isConnected = false
        pinger?.cancel()
        for (_, cont) in pending { cont.resume(throwing: HarloError.closed(reason)) }
        pending.removeAll()
        continuation.finish()
        session.finishTasksAndInvalidate()
    }
}

/// The first answer of `body`, or false after `timeout` seconds (`body`
/// keeps running; its answer is dropped).
func firstOf(timeout: Double, _ body: @escaping @Sendable () async -> Bool) async -> Bool {
    let once = Once<Bool>()
    return await withCheckedContinuation { cont in
        once.set(cont)
        Task { once.resume(await body()) }
        Task {
            try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
            once.resume(false)
        }
    }
}

/// A continuation resumed once, by whoever comes first.
final class Once<T: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var cont: CheckedContinuation<T, Never>?
    private var early: T?

    func set(_ c: CheckedContinuation<T, Never>) {
        lock.lock()
        if let v = early {
            lock.unlock()
            c.resume(returning: v)
            return
        }
        cont = c
        lock.unlock()
    }

    func resume(_ value: T) {
        lock.lock()
        guard let c = cont else {
            if early == nil { early = value }
            lock.unlock()
            return
        }
        cont = nil
        early = value
        lock.unlock()
        c.resume(returning: value)
    }
}
