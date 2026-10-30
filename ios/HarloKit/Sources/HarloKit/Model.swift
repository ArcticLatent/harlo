import Foundation
import Observation

/// How the app is doing with harlo.
public enum LinkState: Equatable, Sendable {
    /// No harlo yet: scan its QR code.
    case unpaired
    case connecting
    case connected
    /// harlo can't be reached right now (asleep, another network): trying
    /// again, with why the last try failed.
    case waiting(String)
}

/// What the transcript shows: rows, and runs of tool calls and thoughts
/// folded under their summary (as on the desktop).
public enum TranscriptItem: Identifiable, Equatable, Sendable {
    case row(Row)
    /// `active`: still going (a call runs or waits).
    case run(group: String, summary: String, rows: [Row], open: Bool, active: Bool)

    public var id: String {
        switch self {
        case .row(let r): return r.key
        case .run(let group, _, _, _, _): return "run:" + group
        }
    }

    /// Groups rows into runs; `opened` holds runs opened (true) or folded
    /// (false) by hand.
    public static func build(_ rows: [Row], opened: [String: Bool]) -> [TranscriptItem] {
        var out: [TranscriptItem] = []
        var i = 0
        while i < rows.count {
            let group = rows[i].group
            guard !group.isEmpty else {
                out.append(.row(rows[i]))
                i += 1
                continue
            }
            var j = i
            while j < rows.count, rows[j].group == group { j += 1 }
            let run = Array(rows[i..<j])
            let active = run.contains { $0.groupOpen }
            let summary = run.first { !$0.summary.isEmpty }?.summary ?? ""
            out.append(.run(group: group, summary: summary, rows: run, open: opened[group] ?? active, active: active))
            i = j
        }
        return out
    }
}

/// The session the phone has open.
@MainActor @Observable
public final class OpenSession {
    /// Its key in harlo's list.
    public let key: String
    /// Its live key once harlo opened it (events carry this one).
    public internal(set) var liveKey = ""
    public internal(set) var harness: String
    public internal(set) var title: String
    public internal(set) var model = ""
    /// Its model, thinking level and usage.
    public internal(set) var info = SessionInfo()
    /// Its approval mode and Plan (nil until harlo said).
    public internal(set) var modes: Modes?
    public internal(set) var commands: [SlashCommand] = []
    /// Messages waiting for the agent.
    public internal(set) var steering: [String] = []
    public internal(set) var followUp: [String] = []
    public internal(set) var todos: [TodoPhase] = []
    public internal(set) var subagents: [Subagent] = []
    /// "busy", "idle", "input", …
    public internal(set) var status = ""
    public internal(set) var rows: [Row] = []
    /// Rows before these, only in harlo (a long session's phone gets its
    /// latest).
    public internal(set) var earlier = 0
    /// Questions and approvals waiting, oldest first.
    public internal(set) var requests: [UIRequest] = []
    /// Your messages a turn can be undone to (row key → snapshot).
    public internal(set) var undoable: [String: String] = [:]
    /// Its folder is a git repository (or holds some): its changes have
    /// diffs. Until harlo says, taken as one.
    public internal(set) var git = true
    /// Review comments on the session's changes, until sent.
    public var review: [ReviewComment] = []
    public internal(set) var loading = true
    public internal(set) var error: String?
    /// Runs opened (true) or folded (false) by hand.
    public internal(set) var opened: [String: Bool] = [:]
    /// While it works, a message steers the run (true) or waits for it to
    /// end (false).
    public var steer = false
    /// What's typed and not sent yet (edit & resend puts a message back here).
    public var draft = ""
    /// When each waiting question or approval came (unix ms).
    @ObservationIgnored var asked: [String: Int64] = [:]
    /// Events that came before harlo said which live key is this one's.
    @ObservationIgnored var early: [Event] = []

    public init(key: String, title: String = "", harness: String = "") {
        self.key = key
        self.title = title
        self.harness = harness
    }

    public var busy: Bool { status == "busy" }

    /// What it's doing now, and since when (unix ms; 0 unknown), while it
    /// works or waits for you.
    public struct Activity: Equatable, Sendable {
        public var text: String
        public var since: Int64
    }

    public var activity: Activity? {
        if let r = requests.first {
            return Activity(text: r.approval ? "Waiting for your approval" : "Waiting for your answer", since: asked[r.id] ?? 0)
        }
        guard busy else { return nil }
        // This turn: since your latest message (a call left running from an
        // earlier one, such as a background subagent, isn't what it does now).
        let turn = rows.lastIndex(where: { $0.kind == "user" }).map { rows.index(after: $0) } ?? rows.startIndex
        if let last = rows.last, last.state == "streaming", last.kind != "tool" {
            return Activity(text: last.kind == "thinking" ? "Thinking" : "Writing", since: last.time)
        }
        if let tool = rows[turn...].last(where: { $0.kind == "tool" && $0.state == "running" }) {
            let what = tool.subtitle.isEmpty ? tool.title : "\(tool.title) \(tool.subtitle)"
            return Activity(text: "Running \(what)", since: tool.time)
        }
        return Activity(text: "Working", since: turn > rows.startIndex ? rows[rows.index(before: turn)].time : 0)
    }

    /// Messages waiting for the agent, steering first: (queue, text).
    public var queued: [(queue: String, text: String)] {
        steering.map { ("steering", $0) } + followUp.map { ("followUp", $0) }
    }

    /// The agent lets a waiting message be taken back (omp).
    public var canUnqueue: Bool { harness == "omp" || harness == "demo" }

    /// The agent can branch at one of the user's messages (edit & resend),
    /// and fork from one (omp).
    public var canBranch: Bool { ["pi", "omp", "claude", "opencode", "hermes"].contains(harness) }
    public var canFork: Bool { harness == "omp" }
    public var items: [TranscriptItem] { TranscriptItem.build(rows, opened: opened) }

    /// Opens a folded run, or folds an open one (`open`: how it shows now).
    public func toggleRun(_ group: String, open: Bool) {
        opened[group] = !open
    }

    func opened(liveKey: String, harness: String) {
        self.liveKey = liveKey
        if !harness.isEmpty { self.harness = harness }
        let waiting = early
        early = []
        for ev in waiting { take(ev) }
    }

    func take(_ ev: Event) {
        guard let k = ev.sessionKey else { return }
        if liveKey.isEmpty {
            early.append(ev)
            return
        }
        guard k == liveKey else { return }
        switch ev {
        case .reset(_, let r, let e):
            rows = r
            earlier = e
            loading = false
        case .rows(_, let changes):
            rows.apply(changes)
            loading = false
        case .status(_, let s):
            status = s
        case .meta(_, let i):
            if !i.title.isEmpty { title = i.title }
            model = i.model
            info = i
        case .request(_, let r):
            if !requests.contains(where: { $0.id == r.id }) {
                requests.append(r)
                asked[r.id] = Int64(Date().timeIntervalSince1970 * 1000)
            }
        case .cleared(_, let id):
            requests.removeAll { $0.id == id }
        case .commands(_, let c):
            commands = c
        case .queue(_, let st, let fu):
            steering = st
            followUp = fu
        case .todos(_, let t):
            todos = t
        case .subagents(_, let list):
            subagents = list
        default:
            break
        }
    }
}

/// The app's state: the harlo it's paired with, the connection (kept up
/// while the app is in front), the session list and the open session.
@MainActor @Observable
public final class HarloModel {
    public internal(set) var host: PairedHarlo?
    public internal(set) var state: LinkState
    public internal(set) var sessions: [SessionSummary] = [] {
        didSet {
            // Answered ones are gone once harlo says so.
            let still = Set(sessions.flatMap { $0.requests.map(\.id) })
            answered.formIntersection(still)
            writeWidget()
        }
    }
    /// The home screen widget's sessions changed (the app reloads it).
    @ObservationIgnored public var widgetChanged: (() -> Void)?
    @ObservationIgnored var lastWidget: WidgetSnapshot?
    /// Requests answered from the list that harlo still lists (hidden).
    var answered: Set<String> = []
    public internal(set) var session: OpenSession?
    /// Pairing in progress.
    public private(set) var pairing = false
    /// Something to tell the user once (a failed pairing or send).
    public var alert: String?
    /// The desktop's colours (kept for the next launch).
    public private(set) var theme: Theme?
    /// Where this phone's notifications go, and what it wants (kept; harlo
    /// gets it on each connection).
    public private(set) var push: PushRegistration?
    /// Answers without a harlo (screenshots of the app, the demo).
    @ObservationIgnored public var samples: Samples?
    /// The demo: sample sessions, no harlo (see Demo.swift).
    public internal(set) var demo = false
    @ObservationIgnored var demoCount = 0
    /// Sessions marked unread here (kept), until opened.
    public private(set) var unreadMarks: Set<String> = []

    @ObservationIgnored let store: SecretStore
    @ObservationIgnored let identity: KeyPair
    @ObservationIgnored let deviceName: String
    @ObservationIgnored var client: HarloClient?
    @ObservationIgnored var loop: Task<Void, Never>?
    @ObservationIgnored var better: Task<Void, Never>?
    @ObservationIgnored var active = false
    /// Seconds before the next try: 1, doubling to 15.
    @ObservationIgnored var retry: Double = 1
    /// How long to wait for harlo: on its own address, through a relay.
    @ObservationIgnored public var timeouts: (direct: Double, remote: Double) = (4, 10)
    /// How long pairing keeps trying while the phone says it's offline: the
    /// first try at a LAN address fails that way while iOS asks for Local
    /// Network access.
    @ObservationIgnored public var offlineWait: Double = 45

    public init(store: SecretStore, deviceName: String) {
        self.store = store
        self.deviceName = deviceName
        if let secret = store.load("identity"), let key = try? KeyPair(secret: secret) {
            identity = key
        } else {
            identity = KeyPair.generate()
            store.save("identity", identity.secret)
        }
        theme = store.load("theme").flatMap { try? JSONDecoder().decode(Theme.self, from: $0) }
        push = store.load("push").flatMap { try? JSONDecoder().decode(PushRegistration.self, from: $0) }
        unreadMarks = store.load("unread").flatMap { try? JSONDecoder().decode(Set<String>.self, from: $0) } ?? []
        lastWidget = WidgetSnapshot.load(store)
        let saved = store.load("host").flatMap { try? JSONDecoder().decode(PairedHarlo.self, from: $0) }
        host = saved
        state = saved == nil ? .unpaired : .connecting
    }

    /// This phone's key, as harlo's Settings → Devices could show it.
    public var fingerprint: String { HarloKit.fingerprint(Base64.encode(identity.publicKey)) }

    // ---- the connection ---------------------------------------------------------

    /// The app is in front: connect, and keep reconnecting.
    public func resume() {
        guard !demo else { return }
        active = true
        if loop == nil, host != nil { startLoop(with: nil) }
        // Connected in the background (a notification's Allow): the open
        // session wasn't reopened then.
        if let c = client, state == .connected, let s = session, openedOn !== c {
            Task { await reopen(s) }
        }
        watchForBetter()
    }

    /// The app went to the background: hang up (iOS would cut it anyway).
    public func suspend() {
        guard !demo else { return }
        active = false
        better?.cancel()
        better = nil
        hangUp()
        if host != nil { state = .waiting("") }
    }

    /// Try again now (pull to refresh): reconnects if it isn't connected,
    /// else reloads the list.
    public func reconnect() async {
        guard !demo else { return }
        if let c = client, state == .connected {
            if let s = try? await c.sessions() { sessions = s }
            return
        }
        retry = 1
        if host != nil {
            active = true
            startLoop(with: nil)
        }
    }

    /// The phone's network changed (Wi-Fi to mobile data, a VPN up or
    /// down): a connection that no longer answers is dropped at once
    /// (rather than at the next ping), and a wait for the next try ends.
    public func networkChanged() async {
        guard active, host != nil, !demo else { return }
        if let c = client, state == .connected {
            if await c.alive(timeout: 3) {
                // Still there: but a better way in may be back (home Wi-Fi).
                await moveToBetter()
                return
            }
            if client === c { await c.close() }
        } else if case .waiting = state {
            retry = 1
            startLoop(with: nil)
        }
    }

    /// How often a connection on a lesser way in (Tailscale, Funnel, a
    /// relay) looks for a better one.
    @ObservationIgnored public var betterEvery: Double = 60
    @ObservationIgnored var moving = false

    /// Connected through a lesser way in while a better one answers (back on
    /// home Wi-Fi, harlo's own address over Tailscale instead of the
    /// relay): moves there, reopening the session, without a gap. True if
    /// it moved.
    @discardableResult
    public func moveToBetter() async -> Bool {
        guard !moving, let old = client, state == .connected, var h = host, let now = h.lastEndpoint else { return false }
        let better = h.tryOrder.filter { Endpoints.rank($0) < Endpoints.rank(now) }
        guard !better.isEmpty else { return false }
        moving = true
        defer { moving = false }
        let (key, identity, name) = (h.key, identity, deviceName)
        guard let (c, ready) = try? await Race.first(better, headStart: 0, { url in
            try await HarloClient.connect(to: url, identity: identity, harloKey: key, deviceName: name, timeout: 3)
        }) else { return false }
        // Still connected the old way (nothing changed meanwhile)?
        guard client === old, state == .connected, active else {
            await c.close()
            return false
        }
        h.lastEndpoint = ready.endpoint
        if !ready.harloName.isEmpty { h.name = ready.harloName }
        h.learn(ready.endpoints)
        setHost(h)
        startLoop(with: c)
        await old.close()
        return true
    }

    /// Now and then, while on a lesser way in, looks for a better one.
    private func watchForBetter() {
        better?.cancel()
        better = Task { [weak self] in
            while !Task.isCancelled {
                let every = self?.betterEvery ?? 60
                try? await Task.sleep(nanoseconds: UInt64(every * 1_000_000_000))
                guard let self, !Task.isCancelled else { return }
                if self.active { await self.moveToBetter() }
            }
        }
    }

    /// Closes the connection as if the network dropped (tests): the loop
    /// reconnects.
    public func dropConnection() async {
        await client?.close()
    }

    func hangUp() {
        loop?.cancel()
        loop = nil
        let old = client
        client = nil
        Task { await old?.close() }
    }

    func startLoop(with first: HarloClient?) {
        loop?.cancel()
        loop = Task { [weak self] in await self?.run(first) }
    }

    private func run(_ first: HarloClient?) async {
        var next = first
        while !Task.isCancelled, host != nil {
            let c: HarloClient
            if let n = next {
                c = n
                next = nil
            } else {
                // While waiting, keep saying why (rather than flashing "connecting").
                if case .waiting = state {} else { state = .connecting }
                do {
                    c = try await connectAny()
                } catch HarloError.denied(let reason) {
                    // Removed in harlo (or harlo was reset): pair again.
                    forgetHost()
                    alert = reason
                    return
                } catch {
                    if Task.isCancelled { return }
                    state = .waiting(Self.describe(error))
                    try? await Task.sleep(nanoseconds: UInt64(retry * 1_000_000_000))
                    retry = min(retry * 2, 15)
                    continue
                }
            }
            if Task.isCancelled {
                await c.close()
                return
            }
            client = c
            retry = 1
            state = .connected
            await connected(c)
            for await ev in c.events { handle(ev) }
            if client === c { client = nil }
            if Task.isCancelled || host == nil { return }
            state = .waiting(await c.closeReason ?? "")
            try? await Task.sleep(nanoseconds: 500_000_000)
        }
    }

    /// Tries harlo's addresses all at once (the one that worked last a
    /// moment first) and keeps the first that answers: at home its local
    /// address, elsewhere Tailscale, Funnel or the relay.
    private func connectAny() async throws -> HarloClient {
        guard var h = host else { throw HarloError.notConnected }
        let (key, identity, name, timeouts) = (h.key, identity, deviceName, timeouts)
        let (c, ready) = try await Race.first(h.tryOrder, headStart: 0.3) { url in
            try await HarloClient.connect(to: url, identity: identity, harloKey: key, deviceName: name,
                                          timeout: url.scheme == "ws" ? timeouts.direct : timeouts.remote)
        }
        h.lastEndpoint = ready.endpoint
        if !ready.harloName.isEmpty { h.name = ready.harloName }
        h.learn(ready.endpoints)
        setHost(h)
        return c
    }

    private func connected(_ c: HarloClient) async {
        if let p = push { _ = try? await c.call("push", p.json) }
        if let s = try? await c.sessions() { sessions = s }
        // In the background (a notification's Allow) only what's asked: the
        // open session comes back when the app does.
        if active, let s = session { await reopen(s) }
        if let t = try? await c.theme(), t != theme {
            theme = t
            store.save("theme", try? JSONEncoder().encode(t))
        }
    }

    private func handle(_ ev: Event) {
        switch ev {
        case .sessions(let s):
            sessions = s
        case .revoked:
            forgetHost()
            alert = "This phone was removed in harlo. Pair it again to use it."
        default:
            session?.take(ev)
        }
    }

    private func setHost(_ h: PairedHarlo?) {
        host = h
        store.save("host", h.flatMap { try? JSONEncoder().encode($0) })
    }

    private func forgetHost() {
        hangUp()
        setHost(nil)
        sessions = []
        session = nil
        state = .unpaired
    }

    static func describe(_ error: Error) -> String {
        if let e = error as? HarloError { return e.description }
        return (error as NSError).localizedDescription
    }

    // ---- notifications ----------------------------------------------------------

    /// iOS gave the app its push token (each launch): harlo gets it.
    public func setPushToken(_ token: Data, sandbox: Bool) async {
        var p = push ?? PushRegistration(token: "", sandbox: sandbox)
        p.token = PushRegistration.hex(token)
        p.sandbox = sandbox
        await savePush(p)
    }

    /// Which notifications this phone wants.
    public func setPushWants(asks: Bool, done: Bool) async {
        guard var p = push else { return }
        p.asks = asks
        p.done = done
        await savePush(p)
    }

    private func savePush(_ p: PushRegistration) async {
        guard p != push else { return }
        push = p
        store.save("push", try? JSONEncoder().encode(p))
        if let c = client, state == .connected { _ = try? await c.call("push", p.json) }
    }

    /// Connected within `timeout` seconds (connecting if the app isn't in
    /// front: a notification's action in the background).
    public func waitConnected(timeout: Double = 20) async -> Bool {
        guard host != nil else { return false }
        if state == .connected, client != nil { return true }
        if loop == nil { startLoop(with: nil) }
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if state == .connected, client != nil { return true }
            if host == nil { return false }
            try? await Task.sleep(nanoseconds: 200_000_000)
        }
        return false
    }

    /// Allow or Deny from a notification: connects if needed, answers, and
    /// hangs up again if the app isn't in front. False (and the alert) when
    /// it couldn't.
    @discardableResult
    public func answer(fromNotification content: PushContent, allow: Bool) async -> Bool {
        guard let req = content.request else { return false }
        guard await waitConnected() else {
            alert = "Couldn't reach harlo to answer."
            if !active { suspend() }
            return false
        }
        let before = alert
        await answer(req, req.approval(allow), in: content.session)
        let ok = alert == before || alert == nil
        if !active { suspend() }
        return ok
    }

    // ---- pairing ------------------------------------------------------------------

    /// Pairs from a scanned (or pasted) link. Scanning another of the same
    /// harlo's codes (its Tailscale address, its relay) adds that address.
    @discardableResult
    public func pair(_ scanned: String) async -> Bool {
        let offer: PairingOffer
        do { offer = try PairingOffer(scanned: scanned) } catch {
            alert = Self.describe(error)
            return false
        }
        guard let endpoint = offer.endpoint else {
            alert = "That code has no address to reach harlo at. Scan the QR code in harlo's Pair a device."
            return false
        }
        pairing = true
        defer { pairing = false }
        do {
            let (c, ready) = try await pairingConnect(endpoint, offer)
            let same = host?.key == offer.key
            var h = same ? host! : PairedHarlo(key: offer.key, name: "", endpoints: [])
            for u in offer.endpoints { h.add(u) }
            h.lastEndpoint = endpoint
            h.learn(ready.endpoints)
            h.name = ready.harloName.isEmpty ? offer.name : ready.harloName
            hangUp()
            if !same {
                sessions = []
                session = nil
            }
            setHost(h)
            active = true
            retry = 1
            startLoop(with: c)
            return true
        } catch {
            alert = Self.describe(error)
            return false
        }
    }

    /// Connects with the pairing code, trying again while the phone reports
    /// itself offline (the code isn't spent: nothing reached harlo).
    private func pairingConnect(_ endpoint: URL, _ offer: PairingOffer) async throws -> (HarloClient, Ready) {
        let end = Date().addingTimeInterval(offlineWait)
        while true {
            do {
                return try await HarloClient.connect(to: endpoint, identity: identity, harloKey: offer.key, code: offer.code,
                                                     deviceName: deviceName, timeout: timeouts.remote)
            } catch HarloError.offline(let why) {
                guard Date() < end else {
                    throw HarloError.offline(endpoint.scheme == "ws"
                        ? "\(why) If you turned off Local Network for harlo, turn it on in Settings › Privacy & Security › Local Network."
                        : why)
                }
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        }
    }

    /// Forgets harlo here (it keeps this phone in its list until removed
    /// there).
    public func forget() {
        if demo { return endDemo() }
        forgetHost()
    }

    // ---- sessions ---------------------------------------------------------------

    public func open(_ s: SessionSummary) {
        if unreadMarks.contains(s.key) { markUnread(s, false) }
        if demo { return demoOpen(s) }
        let open = OpenSession(key: s.key, title: s.title, harness: s.harness)
        session = open
        Task { await reopen(open) }
    }

    public func close() {
        session = nil
    }

    /// The connection the open session was opened on.
    @ObservationIgnored weak var openedOn: HarloClient?

    private func reopen(_ s: OpenSession) async {
        guard let c = client else { return }
        openedOn = c
        s.liveKey = ""
        s.early = []
        s.requests = []
        s.error = nil
        do {
            let r = try await c.open(s.key)
            guard session === s else { return }
            s.opened(liveKey: r.key, harness: r.harness)
            s.modes = try? await c.modes(r.key)
        } catch {
            s.error = Self.describe(error)
            s.loading = false
        }
    }

    /// Sends a message: a new turn, or while it works, a follow-up it takes
    /// when done.
    @discardableResult
    public func send(_ text: String, photos: [Photo] = []) async -> Bool {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty || !photos.isEmpty else { return false }
        if demo { return demoSend(t) }
        guard let s = session, let c = client, !s.liveKey.isEmpty else {
            alert = "Not connected to harlo."
            return false
        }
        do {
            try await c.send(s.liveKey, text: t, mode: s.busy ? (s.steer ? .steer : .followUp) : .normal, photos: photos)
            return true
        } catch {
            alert = Self.describe(error)
            return false
        }
    }

    /// Runs `body` with the connection (and the open session's live key when
    /// `needsSession`); a failure becomes the alert.
    private func with<T>(session needsSession: Bool = true, _ body: (HarloClient, String) async throws -> T) async -> T? {
        // (The demo's answers are its own; the rest does nothing.)
        if demo { return nil }
        guard let c = client else {
            alert = "Not connected to harlo."
            return nil
        }
        let key = session?.liveKey ?? ""
        if needsSession && key.isEmpty {
            alert = "The session isn't open yet."
            return nil
        }
        do { return try await body(c, key) } catch {
            alert = Self.describe(error)
            return nil
        }
    }

    // ---- the queue, and branching -------------------------------------------------

    /// Takes back the `index`-th waiting message (see `OpenSession.queued`).
    public func unqueue(_ index: Int) async {
        guard let q = session?.queued, q.indices.contains(index) else { return }
        _ = await with { c, k in try await c.unqueue(k, queue: q[index].queue, text: q[index].text) }
    }

    /// Sends the `index`-th waiting follow-up now, as steering.
    public func sendNow(_ index: Int) async {
        guard let q = session?.queued, q.indices.contains(index), q[index].queue == "followUp" else { return }
        _ = await with { c, k in try await c.promote(k, text: q[index].text) }
    }

    /// Edit & resend: the session goes back to before one of your messages
    /// (`row`: its key); its text comes back to edit and send again. nil: it
    /// couldn't (the alert says why) or was cancelled.
    public func editMessage(_ row: String) async -> String? {
        await with { c, k in try await c.branch(k, row: row, fork: false) } ?? nil
    }

    /// Edit & resend, from the app: the message goes back in the box.
    public func editInComposer(_ row: String) async {
        if let text = await editMessage(row) { session?.draft = text }
    }

    /// omp: a new session with everything up to one of your messages.
    @discardableResult
    public func forkFrom(_ row: String) async -> Bool {
        await with { c, k in try await c.branch(k, row: row, fork: true) } != nil
    }

    /// Asks again with another model: back to before the message, then the
    /// same message to `model`.
    @discardableResult
    public func retry(_ row: String, with model: ModelChoice) async -> Bool {
        guard let text = await editMessage(row) else { return false }
        await setModel(model)
        return await send(text)
    }

    // ---- new sessions -------------------------------------------------------------

    /// Projects, agents and models to start a session with.
    public func newSessionChoices() async -> NewSessionChoices? {
        if let s = samples { return s.choices }
        return await with(session: false) { c, _ in try await c.newSessionChoices() }
    }

    /// Starts a session and opens it.
    @discardableResult
    public func startSession(project: Project?, agent: String, model: String, approval: String = "", text: String, photos: [Photo] = []) async -> Bool {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty || !photos.isEmpty else { return false }
        if demo { return demoStart(project: project, agent: agent, model: model, text: t) }
        guard let r = await with(session: false, { c, _ in
            try await c.newSession(project: project?.id ?? -1, agent: agent, model: model, approval: approval, text: t, photos: photos)
        }) else { return false }
        let title = String(t.split(separator: "\n").first ?? "").prefix(80)
        let open = OpenSession(key: r.key, title: String(title), harness: r.harness)
        session = open
        await reopen(open)
        return true
    }

    // ---- the open session's settings ------------------------------------------------

    public func setApproval(_ mode: String) async {
        guard let s = session else { return }
        if demo { s.modes?.approval = mode; return }
        if let m = await with({ c, k in try await c.setModes(k, approval: mode) }) { s.modes = m }
    }

    public func setPlan(_ on: Bool) async {
        guard let s = session else { return }
        if demo { s.modes?.plan = on; return }
        if let m = await with({ c, k in try await c.setModes(k, plan: on) }) { s.modes = m }
    }

    /// The models the open session can switch to.
    public func models() async -> [ModelChoice] {
        if let s = samples { return s.models }
        return await with { c, k in try await c.models(k) } ?? []
    }

    public func setModel(_ model: ModelChoice) async {
        if demo, let s = session {
            s.model = model.name
            s.info.model = model.name
            s.info.modelId = model.id
            return
        }
        _ = await with { c, k in try await c.setModel(k, model) }
    }

    public func thinkingLevels() async -> [String] {
        if samples != nil { return ["off", "low", "medium", "high"] }
        return await with { c, k in try await c.thinkingLevels(k) } ?? []
    }

    public func setThinking(_ level: String) async {
        if demo { session?.info.thinking = level; return }
        _ = await with { c, k in try await c.setThinking(k, level) }
    }

    /// Which of your messages in the open session a turn can be undone to.
    public func loadUndo() async {
        guard !demo, let s = session, let c = client, !s.liveKey.isEmpty else { return }
        if let r = try? await c.undoTargets(s.liveKey), session === s {
            s.undoable = r.targets
            s.git = r.git ?? true
        }
    }

    /// What undoing from one of your messages would do (nil: the alert says why).
    public func undoPlan(_ row: String) async -> UndoPlan? {
        await with { c, k in try await c.undo(k, row: row, apply: false) }
    }

    /// Puts the session folder's files back as they were before one of your
    /// messages: what it did (nil: the alert says why).
    public func undo(_ row: String) async -> UndoPlan? {
        let done = await with { c, k in try await c.undo(k, row: row, apply: true) }
        if done != nil { await loadUndo() }
        return done
    }

    /// The open session's changed files.
    public func changes() async -> Changes? {
        if let s = samples { return s.changes }
        return await with { c, k in try await c.changes(k) }
    }

    /// The open session as Markdown (to share).
    public func markdown() async -> String? {
        if let s = samples { return s.markdown }
        return await with { c, k in try await c.markdown(k) }
    }

    /// What the open session ran and touched.
    public func actionLog() async -> ActionLog? {
        if let s = samples { return s.actions }
        return await with { c, k in try await c.actions(k) }
    }

    public func diff(_ path: String) async -> FileDiff? {
        if let s = samples { return s.diff }
        return await with { c, k in try await c.diff(k, path: path) }
    }

    // ---- files, branches, Live Activities ------------------------------------------

    /// The open session's files matching what's typed after `@`.
    public func files(matching query: String) async -> [String] {
        if let s = samples { return s.files.filter { query.isEmpty || $0.localizedCaseInsensitiveContains(query) } }
        guard let c = client, let k = session?.liveKey, !k.isEmpty else { return [] }
        return (try? await c.files(k, query: query)) ?? []
    }

    /// A file the open session's replies mention (nil: the alert says why).
    public func file(_ path: String) async -> FileText? {
        if let s = samples { return s.file }
        return await with { c, k in try await c.file(k, path: path) }
    }

    /// The open session's branches (nil: not a repository, or not connected).
    public func branches() async -> Branches? {
        if let s = samples { return s.branches }
        guard let c = client, let k = session?.liveKey, !k.isEmpty else { return nil }
        return try? await c.branches(k)
    }

    /// Switches the open session's repository to another branch: what git
    /// did, or nil (the alert says why).
    public func switchBranch(to name: String, remote: Bool, stash: Bool) async -> String? {
        await with { c, k in try await c.switchBranch(k, to: name, remote: remote, stash: stash) }
    }

    /// A Live Activity's push token for a session (its live key): harlo sends
    /// it updates from then on; "" when it ended on the phone.
    public func setActivityToken(_ token: String, for key: String) async {
        guard let c = client, state == .connected else { return }
        _ = try? await c.activity(key, token: token, sandbox: push?.sandbox ?? false)
    }

    /// Picks your penguin: harlo on the computer takes it too (its left
    /// rail), so both show the same. Shown at once; harlo's answer settles it.
    public func setAvatar(_ name: String) async {
        guard Theme.avatars.contains(name) else { return }
        var t = theme ?? .standard
        t.avatar = name
        theme = t
        if demo { return }
        if let c = client, state == .connected, let back = try? await c.setAvatar(name) { theme = back }
        store.save("theme", try? JSONEncoder().encode(theme))
    }

    // ---- the list -------------------------------------------------------------

    public func rename(_ s: SessionSummary, to name: String) async {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !n.isEmpty else { return }
        if demo, let i = sessions.firstIndex(where: { $0.key == s.key }) { sessions[i].title = n }
        _ = await with(session: false) { c, _ in try await c.rename(s.key, to: n) }
        if session?.key == s.key || session?.liveKey == s.key { session?.title = n }
    }

    public func archive(_ s: SessionSummary) async {
        if demo { sessions.removeAll { $0.key == s.key }; return }
        _ = await with(session: false) { c, _ in try await c.archive(s.key) }
    }

    public func pin(_ s: SessionSummary, _ on: Bool) async {
        if demo, let i = sessions.firstIndex(where: { $0.key == s.key }) { sessions[i].pinned = on; return }
        _ = await with(session: false) { c, _ in try await c.pin(s.key, on) }
    }

    /// Unread: finished since you looked (harlo says), or marked so here.
    public func isUnread(_ s: SessionSummary) -> Bool {
        s.status == "unread" || unreadMarks.contains(s.key)
    }

    /// Marks a session unread (or not) on this phone, until it's opened.
    public func markUnread(_ s: SessionSummary, _ on: Bool) {
        if on { unreadMarks.insert(s.key) } else { unreadMarks.remove(s.key) }
        // Only sessions still listed.
        let listed = Set(sessions.map(\.key))
        if !listed.isEmpty { unreadMarks.formIntersection(listed) }
        store.save("unread", try? JSONEncoder().encode(unreadMarks))
        writeWidget()
    }

    public func stop() async {
        if demo { session?.status = ""; return }
        guard let s = session, let c = client, !s.liveKey.isEmpty else { return }
        try? await c.abort(s.liveKey)
    }

    /// Everything waiting on you, the session it's in, oldest first.
    public var waiting: [(session: SessionSummary, request: UIRequest)] {
        sessions
            .flatMap { s in s.requests.filter { !answered.contains($0.id) }.map { (session: s, request: $0) } }
            .sorted { $0.request.asked < $1.request.asked }
    }

    /// Answers a request from the list, without opening its session.
    public func answer(_ req: UIRequest, _ answer: JSON, in key: String) async {
        if let s = session, s.liveKey == key || s.key == key {
            await self.answer(req, answer)
            answered.insert(req.id)
            return
        }
        answered.insert(req.id)
        if demo { return demoAnswered(req, answer, in: key) }
        if await with(session: false, { c, _ in try await c.answer(key, requestId: req.id, answer: answer) }) == nil {
            answered.remove(req.id)
        }
    }

    public func answer(_ req: UIRequest, _ answer: JSON) async {
        if demo, let s = session {
            s.requests.removeAll { $0.id == req.id }
            answered.insert(req.id)
            return demoAnswered(req, answer, in: s.key)
        }
        guard let s = session, let c = client, !s.liveKey.isEmpty else {
            alert = "Not connected to harlo."
            return
        }
        s.requests.removeAll { $0.id == req.id }
        do { try await c.answer(s.liveKey, requestId: req.id, answer: answer) } catch { alert = Self.describe(error) }
    }
}

extension Event {
    /// The session an event is about (nil: not one session's).
    public var sessionKey: String? {
        switch self {
        case .reset(let k, _, _), .rows(let k, _), .status(let k, _), .meta(let k, _), .request(let k, _), .cleared(let k, _),
             .commands(let k, _), .queue(let k, _, _), .todos(let k, _), .subagents(let k, _): return k
        case .sessions, .revoked: return nil
        }
    }
}

extension UIRequest {
    /// How an option button looks: the go-ahead, the refusal, or neither.
    public enum OptionRole: Sendable { case allow, deny, plain }

    public static func role(of option: String) -> OptionRole {
        let words = Set(option.lowercased().split { !$0.isLetter }.map(String.init))
        if !words.isDisjoint(with: ["approve", "allow", "yes", "accept"]) { return .allow }
        if !words.isDisjoint(with: ["deny", "reject", "no", "decline"]) { return .deny }
        return .plain
    }

    /// The answer for Allow / Deny: the matching option when it has them
    /// (omp's Approve / Deny), else a confirmation.
    public func approval(_ allow: Bool) -> JSON {
        if let o = options.first(where: { Self.role(of: $0) == (allow ? .allow : .deny) }) { return ["value": .string(o)] }
        return ["confirmed": .bool(allow)]
    }

    /// The answer for one of `select`'s options.
    public static func choose(_ option: String) -> JSON { ["value": .string(option)] }

    /// The answer for typed text (`input`, `editor`).
    public static func typed(_ text: String) -> JSON { ["value": .string(text)] }

    public static let dismissed: JSON = ["cancelled": true]
}
