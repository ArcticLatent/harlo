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
    /// What this harlo does beyond the first app's ("limits", "compaction",
    /// "serverStart", "machines", "settled"): an older one says none.
    public var features: Set<String> = []
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
    /// Answers from a recording instead of harlo (Replay.swift).
    private var replay: Replay?

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

    /// A connection to a recording: it answers as harlo did, and opening a
    /// session sends that session's events up to the scene.
    /// (Replaying from the start: the model's first calls are answered.)
    public static func replaying(_ replay: Replay) -> HarloClient {
        HarloClient(endpoint: URL(string: "ws://127.0.0.1:9/replay")!, identity: KeyPair.generate(), harloKey: Data(repeating: 7, count: 32), replay: replay)
    }

    /// What a recording answers, with what harlo would send after it.
    private func replayed(_ r: Replay, _ op: String, _ args: JSON) -> JSON {
        let reply = r.answer(op, args)
        switch op {
        case "open":
            for e in r.events(of: reply["key"].text) { if let ev = Event(e) { continuation.yield(ev) } }
        case "answer" where reply["ok"].bool == true:
            continuation.yield(.cleared(key: args["key"].text, id: args["requestId"].text))
        default:
            break
        }
        return reply
    }

    private init(endpoint: URL, identity: KeyPair, harloKey: Data, replay: Replay? = nil) {
        self.identity = identity
        self.harloKey = harloKey
        self.replay = replay
        isConnected = replay != nil
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        session = URLSession(configuration: config)
        task = session.webSocketTask(with: endpoint)
        // harlo keeps frames to about half a megabyte; iOS's default limit
        // (1 MB) is tight for a big row.
        task.maximumMessageSize = 16 << 20
        (events, continuation) = AsyncStream.makeStream(of: Event.self, bufferingPolicy: .unbounded)
        for e in replay?.latestShared() ?? [] { if let ev = Event(e) { continuation.yield(ev) } }
    }

    private func hello(code: String?, deviceName: String, timeout: TimeInterval, endpoint: URL) async throws -> Ready {
        task.resume()
        var hello: JSON = ["type": "hello", "key": .string(Base64.encode(identity.publicKey)),
                           "features": .array(HarloClient.shows.map { .string($0) })]
        if let code, !code.isEmpty {
            hello["code"] = .string(code)
            hello["name"] = .string(deviceName)
        }
        // Nothing within `timeout`: the socket is closed, which ends the wait.
        let timer = Task { [task] in
            try? await Task.sleep(for: .seconds(timeout))
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
            Recorder.shared?.write(["ready": ["name": reply["name"], "features": reply["features"]]])
            let box = try Box(theirPublic: harloKey, ours: identity)
            channel = Channel(box: box, id: reply["conn"].text)
            isConnected = true
            startReading()
            startPinging()
            return Ready(harloName: reply["name"].text, deviceName: reply["device"].text, endpoint: endpoint,
                         endpoints: (reply["endpoints"].array ?? []).compactMap { $0.string.flatMap(URL.init(string:)) },
                         features: Set((reply["features"].array ?? []).compactMap(\.string)))
        case "denied":
            task.cancel(with: .normalClosure, reason: nil)
            throw HarloError.denied(reply["reason"].string ?? "harlo refused this phone.")
        default:
            task.cancel(with: .protocolError, reason: nil)
            throw HarloError.badFrame("harlo answered something unexpected")
        }
    }

    /// What this app shows of what harlo sends (in the hello): harlo leaves
    /// out the rest.
    public static let shows = ["limits", "settled"]

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
        if let r = replay {
            guard isConnected else { throw HarloError.notConnected }
            return replayed(r, op, args)
        }
        guard isConnected, channel != nil else { throw HarloError.notConnected }
        if let rec = Recorder.shared {
            let reply = try await sent(op, args)
            rec.write(["call": .string(op), "args": args, "reply": reply])
            return reply
        }
        return try await sent(op, args)
    }

    private func sent(_ op: String, _ args: JSON) async throws -> JSON {
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
    /// (`host`: without a project, that machine's home folder.)
    public func newSession(project: Int64, host: String = "", agent: String, model: String, approval: String = "", text: String, photos: [Photo]) async throws -> (key: String, harness: String) {
        let r = try check(await call("new", ["project": .int(project), "host": .string(host), "agent": .string(agent), "model": .string(model), "approval": .string(approval),
                                             "text": .string(text), "images": .array(photos.map(\.json))]))
        return (r["key"].text, r["harness"].text)
    }

    /// Takes back a message waiting in the agent's queue (`queue`: "steering" or
    /// "followUp").
    public func unqueue(_ key: String, queue: String, text: String) async throws {
        try check(await call("unqueue", ["key": .string(key), "queue": .string(queue), "text": .string(text)]))
    }

    /// Sends a waiting follow-up now, as steering.
    public func promote(_ key: String, text: String) async throws {
        try check(await call("promote", ["key": .string(key), "text": .string(text)]))
    }

    /// Sends the command the agent waits for (its `call`) to the
    /// background.
    public func background(_ key: String, call: String) async throws {
        try check(await self.call("background", ["key": .string(key), "call": .string(call)]))
    }

    /// Does what a job row offers (`row`: its id).
    public func jobAction(_ key: String, row: String) async throws {
        try check(await call("jobAction", ["key": .string(key), "row": .string(row)]))
    }

    /// Branches the session at one of the user's messages (its row key) and
    /// returns that message's text to edit (`fork`, a copy up to it in a
    /// new session, no agent does now). nil: cancelled.
    public func branch(_ key: String, row: String, fork: Bool) async throws -> String? {
        let r = try check(await call("branch", ["key": .string(key), "row": .string(row), "fork": .bool(fork)]))
        return r["cancelled"].bool == true ? nil : r["text"].text
    }

    public func modes(_ key: String) async throws -> Modes {
        Modes(try check(await call("modes", ["key": .string(key)])))
    }

    public func setModes(_ key: String, approval: String? = nil) async throws -> Modes {
        var args: JSON = ["key": .string(key)]
        if let approval { args["approval"] = .string(approval) }
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

    /// A picture a session shows (`Row.images`), or a picture file in its
    /// folder: a thumbnail, or fitted to a phone's screen.
    public func picture(_ path: String, of key: String, thumb: Bool) async throws -> Picture {
        let r = try check(await call("picture", ["key": .string(key), "path": .string(path), "size": .string(thumb ? "thumb" : "screen")]))
        guard let data = Data(base64Encoded: r["data"].text), !data.isEmpty else { throw HarloError.closed("harlo sent no picture.") }
        return Picture(data: data, type: r["type"].text, width: r["width"].double ?? 0, height: r["height"].double ?? 0)
    }

    /// Connects harlo to one of its other machines (as picking it in harlo
    /// does): what's there.
    public func connect(_ host: String) async throws -> RemoteMachine {
        let r = try check(await call("connect", ["host": .string(host)]))
        return RemoteMachine(host: r["host"].string ?? host, connected: true, home: r["home"].text, agents: (r["agents"].array ?? []).compactMap(\.string))
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

    /// Plan limits on harlo's computer and each machine (then the `limits`
    /// event as they change).
    public func limits() async throws -> PlanLimits {
        PlanLimits(try check(await call("limits")))
    }

    /// A running session's plan limit (nil: under 90%), and when it
    /// continues on its own (unix ms, 0 if it isn't going to).
    public func limit(of key: String) async throws -> (limit: SessionLimit?, resumesAt: Int64) {
        let r = try check(await call("limitOf", ["key": .string(key)]))
        return (SessionLimit(r["limit"]), r["resumesAt"].int ?? 0)
    }

    /// Stops the session if it works, and continues it once its used-up
    /// limit resets (even if harlo restarts meanwhile): when, unix ms.
    public func continueAtReset(_ key: String) async throws -> Int64 {
        try check(await call("resumeAtReset", ["key": .string(key)]))["resumesAt"].int ?? 0
    }

    /// It doesn't continue at the reset after all.
    public func cancelContinue(_ key: String) async throws {
        _ = try check(await call("cancelResume", ["key": .string(key)]))
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
        guard r["ok"].bool == true else { throw HarloError.badFrame("harlo didn't acknowledge the answer") }
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

    #if os(Linux)
    /// Linux's URLSession (harlo-probe) hands a message over 16 KB over in
    /// pieces of 16,380 then 16,384 characters: they're kept until the
    /// last, shorter one.
    private var partial = ""
    #endif

    /// One sealed frame from harlo; false: the connection is done.
    private func handle(_ piece: String) -> Bool {
        guard var ch = channel else { return false }
        #if os(Linux)
        let frame = partial + piece
        partial = ""
        #else
        let frame = piece
        #endif
        let msg: JSON
        do {
            msg = try ch.open(frame)
            channel = ch
        } catch {
            #if os(Linux)
            if [16380, 16384].contains(piece.utf8.count), frame.utf8.count < 16 << 20 {
                partial = frame
                return true
            }
            #endif
            finish((error as? HarloError)?.description ?? "a bad frame")
            task.cancel(with: .protocolError, reason: nil)
            return false
        }
        if let id = msg["id"].int, let cont = pending.removeValue(forKey: id) {
            cont.resume(returning: msg)
            return true
        }
        if let ev = Event(msg) {
            Recorder.shared?.write(["ev": msg])
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
                try? await Task.sleep(for: .seconds(30))
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
            try? await Task.sleep(for: .seconds(timeout))
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
