// A phone made of HarloKit, for scripts/ios-e2e.sh (a long session is
// checked by scripts/phone-e2e.sh: Foundation's WebSocket on Linux breaks on
// many large frames in a row, which iOS's doesn't): the app's own model
// (HarloModel) pairs with a running harlo from its pairing link, lists its
// sessions, opens the demo session, sends a message and reads the reply,
// answers an approval and a question, survives a dropped connection, and
// comes back after a relaunch by key alone. Then the protocol's refusals.
//
//   harlo-probe <pairing link>
import Foundation
import HarloKit

var failures = 0
func check(_ ok: Bool, _ what: String) {
    if !ok { failures += 1 }
    print("IOS-E2E", ok ? "PASS" : "FAIL", what)
}

@MainActor
func waitFor(_ seconds: Double, _ cond: @MainActor () -> Bool) async -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end {
        if cond() { return true }
        try? await Task.sleep(nanoseconds: 100_000_000)
    }
    return cond()
}

@MainActor
func run(_ link: String) async {
    guard let offer = try? PairingOffer(scanned: link), let endpoint = offer.endpoint else {
        check(false, "the link reads as a pairing offer")
        return
    }
    let store = MemoryStore()
    let model = HarloModel(store: store, deviceName: "Probe iPhone")
    check(model.state == .unpaired, "a new app starts unpaired")

    // Pairing: the code admits this phone's key.
    let paired = await model.pair(link)
    check(paired && model.host?.endpoints.first == endpoint && model.host?.lastEndpoint == endpoint, "the code pairs this phone (\(model.alert ?? model.host?.name ?? ""))")
    let listed = await waitFor(5) { model.state == .connected && !model.sessions.isEmpty }
    check(listed, "it connects and lists the sessions (\(model.state), \(model.sessions.map(\.harness)))")
    guard let demo = model.sessions.first(where: { $0.harness == "demo" }) else { return }

    model.open(demo)
    guard let s = model.session else { return }
    let opened = await waitFor(5) { !s.liveKey.isEmpty && !s.loading }
    check(opened && s.harness == "demo", "a session opens (\(s.harness), \(s.error ?? "no error"))")

    // This harlo's features; plan limits (none on a scratch computer), and a
    // session's (the demo agent has no plan).
    check(["limits", "compaction", "serverStart"].allSatisfy(model.features.contains), "harlo says what it does (\(model.features.sorted()))")
    check(model.limits.isEmpty && model.limits.machines.map(\.host) == [""], "plan limits: this computer's, none here (\(model.limits.machines.map(\.host)))")
    check(s.limit == nil && s.resumesAt == 0, "a session's plan limit: none for an agent without a plan")
    await model.continueAtReset()
    check(model.alert == "This session's limit isn't used up.", "continue at the reset needs a used-up limit (\(model.alert ?? "no alert"))")
    model.alert = nil
    // Machines and model servers: none on a scratch computer.
    check(model.canConnect && model.hasModelServers && model.canPutBack, "harlo connects machines and starts model servers from here (\(model.features.sorted()))")
    let servers = await model.modelServers()
    check(servers?.map(\.host) == [""] && servers?.first?.servers.isEmpty == true, "model servers: this computer's, none set up (\(servers.map { "\($0.map(\.host))" } ?? model.alert ?? "-"))")
    let nowhere = await model.connect("no-such-machine")
    check(nowhere == nil && model.alert == "harlo doesn't have that machine.", "connecting a machine harlo doesn't have says so (\(model.alert ?? "no alert"))")
    model.alert = nil
    let choices = await model.newSessionChoices()
    check(choices?.hosts.isEmpty == true && choices?.machines.map(\.host) == [""], "a new session's places: this computer only (\(choices?.machines.map(\.host) ?? []))")
    check(model.sessions.allSatisfy { $0.host.isEmpty }, "its sessions run on this computer")
    // The plan and wiki (a session outside a project has neither), and a
    // diagram harlo draws for the phone.
    check(model.hasPlans && s.plan?.enabled == false && s.plan?.steps.isEmpty == true, "a session outside a project: its plan is off (\(String(describing: s.plan)))")
    check(model.runsPlans && model.features.contains("helpers") && s.helpers.isEmpty, "harlo runs plans with helpers, and this session started none (\(s.helpers.count))")
    await model.loadWiki()
    check(model.hasWiki && s.wiki != nil && s.wiki?.inProject == false && s.wiki?.pages.isEmpty == true, "its wiki can be asked for: none outside a project (\(String(describing: s.wiki)))")
    let drawn = await model.diagram("flowchart LR\n  A[phone] --> B[harlo]", light: true, colors: ["background": "#ffffff", "text": "#111111"])
    check((drawn.picture?.width ?? 0) > 50 && UInt8(drawn.picture?.png.dropFirst().first ?? 0) == UInt8(ascii: "P"), "harlo draws a Mermaid diagram for the phone (\(drawn.picture.map { "\(Int($0.width))x\(Int($0.height)), \($0.png.count) bytes" } ?? drawn.why))")
    let bad = await model.diagram("not a diagram at all", light: false, colors: [:])
    check(bad.picture == nil && !bad.why.isEmpty, "one it can't draw says why (\(bad.why))")
    let stopped = await model.stoppedModels(agent: "harlo", models: ["nowhere/nothing"])
    check(model.features.contains("stoppedModels") && stopped.isEmpty, "a model on no server of harlo's isn't marked stopped (\(stopped))")

    await model.send("fast: hello from the phone")
    let replied = await waitFor(8) {
        s.rows.contains { $0.kind == "user" && $0.text.contains("hello from the phone") }
            && s.rows.contains { $0.kind == "markdown" && !$0.html.isEmpty && $0.state == "done" }
    }
    check(replied, "a message sent from the phone reaches the session, and the reply streams back (\(s.rows.count) rows)")
    let runs = s.items.compactMap { item -> (String, Int, Bool)? in
        if case .run(_, let summary, let rows, let open, _) = item { return (summary, rows.count, open) }
        return nil
    }
    check(runs.contains { !$0.0.isEmpty && $0.1 >= 2 && !$0.2 }, "a finished run of tool calls folds under its summary (\(runs.map(\.0)))")
    let paragraphs = s.rows.filter { $0.kind == "markdown" }.flatMap { RichText.parse($0.html) }
    check(!paragraphs.isEmpty, "the reply's rich text reads as blocks (\(paragraphs.count))")

    // A side question: answered in its card, not added to the conversation.
    let rowsBefore = s.rows.count
    await model.send("/btw why the demo?")
    let aside = await waitFor(6) { s.aside?.done == true }
    check(aside && s.aside?.question == "why the demo?" && s.aside?.text == "Because the demo says so.", "a side question's answer comes to the phone (\(String(describing: s.aside)))")
    check(s.rows.count == rowsBefore && !s.rows.contains { $0.text.contains("/btw") }, "and stays out of the conversation (\(s.rows.count) rows, \(rowsBefore) before)")
    s.aside = nil

    await model.send("approve please")
    let asked = await waitFor(6) { s.requests.first?.approval == true }
    check(asked, "an approval request arrives (\(s.requests.first.map { "\($0.method) \($0.options)" } ?? "none"))")
    check(s.requests.first?.tool == "bash" && s.requests.first?.note == "Demo approval", "with its tool and why it asks (\(s.requests.first.map { "\($0.tool): \($0.note)" } ?? "none"))")
    if let req = s.requests.first {
        await model.answer(req, req.approval(true))
        let ran = await waitFor(8) { s.requests.isEmpty && s.status != "busy" && s.rows.last?.kind == "markdown" }
        check(ran, "Allow answers it, and the run finishes (\(s.status))")
    }

    // A question with options (an agent's `ask`): picked and typed answers.
    await model.send("ask me")
    let questioned = await waitFor(6) { s.requests.first?.method == "ask" }
    if let ask = s.requests.first, questioned {
        let picks = ask.questions.map { [$0.options.first ?? ""] }
        await model.answer(ask, UIRequest.askAnswer(ask.questions, picked: picks, typed: ["from the probe"]))
        let answered = await waitFor(6) { s.requests.isEmpty && s.rows.contains { $0.kind == "notice" && $0.text.contains(picks[0][0]) } }
        check(answered, "a question arrives with its options, and the answer reaches the agent")
    } else {
        check(false, "a question arrives with its options")
    }

    // Needs you: the list says what waits, and it's answered from there,
    // the session closed.
    if model.session != nil {
        await model.send("approve please")
        let listed = await waitFor(6) { model.waiting.first?.request.approval == true }
        let w = model.waiting.first
        check(listed && (w?.request.asked ?? 0) > 0 && w?.session.status == "input",
              "the list says what waits on you, in which session, since when (\(model.waiting.map { "\($0.session.status) \($0.request.options)" }))")
        if let w {
            model.close()
            await model.answer(w.request, w.request.approval(true), in: w.session.key)
            let hidden = model.waiting.isEmpty
            let gone = await waitFor(8) { model.waiting.isEmpty && model.sessions.first { $0.key == w.session.key }?.status != "input" }
            check(hidden && gone && model.alert == nil, "an approval answered from the list goes at once, and harlo agrees (\(model.alert ?? "no alert"))")
            model.open(w.session)
            let reopened = await waitFor(8) { !(model.session?.liveKey.isEmpty ?? true) && !(model.session?.busy ?? true) && model.session?.requests.isEmpty == true }
            check(reopened && model.session?.rows.last?.kind == "markdown", "its session went on (\(model.session?.rows.last?.kind ?? "-"))")
        }
    }

    await pushes(model, store: store)

    await features(model)

    // The network drops: it reconnects and the session comes back.
    guard let s = model.session else { return }
    let before = s.rows.count
    await model.dropConnection()
    let back = await waitFor(8) { model.state == .connected && !s.liveKey.isEmpty && !s.loading && s.rows.count == before }
    check(back, "after a dropped connection it reconnects and reopens the session (\(model.state), \(s.rows.count)/\(before) rows)")
    model.suspend()

    // A relaunch: the same Keychain, no code.
    let again = HarloModel(store: store, deviceName: "Probe iPhone")
    check(again.state == .connecting && again.host?.key == offer.key, "a relaunch remembers harlo")
    again.resume()
    let reconnected = await waitFor(8) { again.state == .connected && !again.sessions.isEmpty }
    check(reconnected, "and reconnects without a code (\(again.state))")
    again.suspend()

    await reaching(store: store, offer: offer, good: endpoint)

    // A stranger can't use the spent code, or come in without one.
    for (code, what) in [(offer.code, "a used pairing code is refused"), ("", "an unpaired phone is refused")] {
        do {
            _ = try await HarloClient.connect(to: endpoint, identity: KeyPair.generate(), harloKey: offer.key, code: code)
            check(false, what)
        } catch HarloError.denied(let reason) {
            check(true, "\(what) (\(reason))")
        } catch {
            check(false, "\(what) (\(error))")
        }
    }
    let stranger = HarloModel(store: MemoryStore(), deviceName: "Stranger")
    let refused = await stranger.pair(link)
    check(!refused && stranger.state == .unpaired && (stranger.alert ?? "").contains("pairing code"), "the app says why (\(stranger.alert ?? "no alert"))")
}

/// Reaching harlo wherever it is: its addresses come in the pairing code
/// and when it connects, the app tries them all at once (a dead one doesn't
/// hold it up), and harlo answers at the others too (Tailscale here).
@MainActor
func reaching(store: MemoryStore, offer: PairingOffer, good: URL) async {
    check(!offer.alternates.isEmpty, "the pairing code carries harlo's other addresses (\(offer.alternates.map { $0.host ?? "" }))")
    let m = HarloModel(store: store, deviceName: "Probe iPhone")
    let learned = m.host?.endpoints.filter { $0 != good } ?? []
    check(!learned.isEmpty, "the app knows harlo's other addresses (\(learned.map { $0.absoluteString }))")
    // Each of them answers (these are this machine's own addresses).
    var answered: [String] = []
    for url in learned where url.scheme == "ws" {
        if let (c, _) = try? await HarloClient.connect(to: url, identity: try! KeyPair(secret: store.load("identity")!), harloKey: offer.key, timeout: 4) {
            answered.append(url.host ?? "")
            await c.close()
        }
    }
    check(!answered.isEmpty, "harlo answers at its other addresses (\(answered))")

    // Many requests at once (a ping, a list, opening a session): they go
    // out in the order they were sealed, or harlo drops the connection as
    // a replay.
    if let (c, _) = try? await HarloClient.connect(to: good, identity: try! KeyPair(secret: store.load("identity")!), harloKey: offer.key, timeout: 4) {
        let pongs = await withTaskGroup(of: Bool.self) { group in
            for _ in 0..<40 { group.addTask { (try? await c.call("ping"))?["pong"].bool == true } }
            var n = 0
            for await ok in group where ok { n += 1 }
            return n
        }
        let alive = await c.alive(timeout: 3)
        check(pongs == 40 && alive, "40 requests at once all answered, the connection still up (\(pongs)/40)")
        await c.close()
    }

    // The last address that worked is dead now (another network): the
    // others are tried at the same time, so it connects quickly anyway.
    var h = m.host!
    let dead = URL(string: "ws://10.255.255.1:7676/ws")!
    h.endpoints = [dead] + h.endpoints
    h.lastEndpoint = dead
    let moved = MemoryStore()
    moved.save("identity", store.load("identity"))
    moved.save("host", try? JSONEncoder().encode(h))
    let elsewhere = HarloModel(store: moved, deviceName: "Probe iPhone")
    let start = Date()
    elsewhere.resume()
    let quick = await waitFor(8) { elsewhere.state == .connected }
    let took = Date().timeIntervalSince(start)
    check(quick && took < 2.5 && elsewhere.host?.lastEndpoint != dead, "a dead address doesn't hold it up (\(String(format: "%.1f", took)) s, via \(elsewhere.host?.lastEndpoint?.host ?? "-"))")
    check(elsewhere.host?.endpoints.contains(dead) == false, "and harlo's own list replaces addresses it no longer has")

    // The network changes while connected: it checks and stays.
    await elsewhere.networkChanged()
    check(elsewhere.state == .connected, "a network change with harlo still there keeps the connection")
    elsewhere.suspend()

    // Connected over Tailscale (away from home), with a session open; back
    // on home Wi-Fi, it moves to harlo's address there and the session
    // carries on.
    guard let tail = learned.first(where: { Endpoints.rank($0) == 1 }) else {
        check(true, "moving to a better way in (no Tailscale address here; skipped)")
        return
    }
    var t = h
    t.endpoints = [tail]
    t.lastEndpoint = tail
    let away = MemoryStore()
    away.save("identity", store.load("identity"))
    away.save("host", try? JSONEncoder().encode(t))
    let phone = HarloModel(store: away, deviceName: "Probe iPhone")
    phone.resume()
    let viaTail = await waitFor(8) { phone.state == .connected && !phone.sessions.isEmpty }
    check(viaTail && phone.host?.lastEndpoint == tail, "it connects over Tailscale (\(phone.host?.lastEndpoint?.host ?? "-"))")
    if let demo = phone.sessions.first(where: { $0.harness == "demo" }) { phone.open(demo) }
    let opened = await waitFor(5) { phone.session.map { !$0.liveKey.isEmpty && !$0.rows.isEmpty } ?? false }
    let rows = phone.session?.rows.count ?? 0
    let didMove = await phone.moveToBetter()
    let back = await waitFor(5) { phone.state == .connected && phone.session.map { !$0.liveKey.isEmpty && $0.rows.count == rows } ?? false }
    let via = phone.host?.lastEndpoint
    check(opened && didMove && back && via.map { Endpoints.rank($0) == 0 } == true,
          "back home, it moves to harlo's local address and the session carries on (\(via?.host ?? "-"), \(phone.session?.rows.count ?? 0)/\(rows) rows)")
    let again = await phone.moveToBetter()
    check(!again, "and stays there (nothing better)")
    phone.suspend()
}

/// Starting a session and controlling it: the desktop's theme, projects
/// and agents, a first message with a photo, Plan and the approval mode,
/// the model and thinking level, todos, subagents, changed files and a
/// diff, renaming.
@MainActor
func features(_ model: HarloModel) async {
    let themed = await waitFor(5) { model.theme != nil }
    check(themed, "the desktop's theme arrives (\(model.theme?.family ?? "none"))")
    // Picking a penguin on the phone: harlo keeps it and sends it back.
    await model.setAvatar("scarf")
    check(model.theme?.avatar == "scarf", "a penguin picked on the phone is harlo's (\(model.theme?.avatar ?? "none"))")

    guard let choices = await model.newSessionChoices() else {
        check(false, "projects and agents for a new session (\(model.alert ?? ""))")
        return
    }
    let repo = choices.projects.first
    check(repo != nil && choices.agents.contains { $0.id == "demo" }, "projects and agents for a new session (\(choices.projects.map(\.name)), \(choices.agents.map(\.id)))")
    guard let repo else { return }
    let starts = choices.approval(for: "demo", in: repo)
    check(["always-ask", "write", "yolo"].contains(starts) && choices.approval(for: "demo", in: nil) == starts,
          "the permissions a new session starts with (\(starts))")
    let picked = starts == "always-ask" ? "write" : "always-ask"

    // A 1×1 PNG.
    let png = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")!
    let started = await model.startSession(project: repo, agent: "demo", model: "", approval: picked, text: "fast: hello from a new session", photos: [Photo(data: png, mimeType: "image/png")])
    guard started, let s = model.session else {
        check(false, "a new session starts in the project (\(model.alert ?? ""))")
        return
    }
    let answered = await waitFor(8) {
        s.harness == "demo" && s.rows.contains { $0.kind == "user" && $0.text.contains("hello from a new session") }
            && s.rows.contains { $0.kind == "markdown" && $0.state == "done" } && !s.busy
    }
    check(answered, "a new session starts in the project with its first message and a photo (\(s.rows.map(\.kind)))")
    let photo = s.rows.first { $0.kind == "user" }
    check(photo.map { $0.text.contains("Photo 1") || $0.html.contains("Photo 1") || $0.title.contains("Photo") } ?? false,
          "the photo goes with it (\(photo?.text.prefix(80) ?? ""))")
    // The photo comes back as a picture (a thumbnail and fitted to the
    // screen); a file that isn't one doesn't.
    if let pic = photo?.images.first, model.showsPictures {
        let thumb = await model.picture(pic, thumb: true)
        check(thumb.picture.map { $0.width == 1 && $0.height == 1 && !$0.data.isEmpty } ?? false,
              "the photo comes back as a picture (\(thumb.picture.map { "\(Int($0.width))x\(Int($0.height)) \($0.type), \($0.data.count) bytes" } ?? thumb.why))")
        let screen = await model.picture(pic, thumb: false)
        check(screen.picture != nil, "and fitted to the screen (\(screen.picture.map { "\($0.data.count) bytes" } ?? screen.why))")
    } else {
        check(false, "the photo is one of its row's pictures (\(photo?.images ?? []), features \(model.showsPictures))")
    }
    let notOne = await model.picture("notes.txt", thumb: true)
    check(notOne.picture == nil && notOne.why.contains("isn't a picture"), "a file that isn't a picture isn't sent as one (\(notOne.why))")
    let listed = await waitFor(5) { model.sessions.contains { $0.key == s.liveKey && $0.project == repo.name } }
    check(listed, "it's listed under its project")
    let gotModes = await waitFor(5) { s.modes != nil }
    check(gotModes && s.modes?.approval == picked, "it starts with the permissions picked (\(s.modes?.approval ?? "-"), picked \(picked))")

    // Modes.
    let modes = await waitFor(5) { s.modes != nil }
    check(modes && !(s.modes?.approvals.isEmpty ?? true), "its approval mode and Plan (\(String(describing: s.modes)))")
    await model.setPlan(true)
    await model.setApproval("write")
    check(s.modes?.plan == true && s.modes?.approval == "write", "Plan and the approval mode switch (\(String(describing: s.modes)))")
    await model.setPlan(false)

    // Model and thinking.
    let models = await model.models()
    check(models.count == 2, "the models it can switch to (\(models.map(\.id)))")
    if let gpt = models.first(where: { $0.modelId == "gpt-demo" }) { await model.setModel(gpt) }
    let switched = await waitFor(5) { s.info.modelId == "openai/gpt-demo" }
    check(switched, "switching the model (\(s.info.modelId))")
    let levels = await model.thinkingLevels()
    await model.setThinking("high")
    let thought = await waitFor(5) { s.info.thinking == "high" }
    check(levels.contains("high") && thought, "the thinking level (\(levels), now \(s.info.thinking))")

    // Todos, subagents.
    await model.send("fast: todos please")
    let todos = await waitFor(8) { !s.todos.isEmpty }
    check(todos, "todos arrive (\(s.todos.map { "\($0.name) \($0.done)/\($0.tasks.count)" }))")
    _ = await waitFor(8) { !s.busy }
    await model.send("fast: subagents please")
    let subs = await waitFor(8) { s.subagents.count >= 2 }
    check(subs, "subagents arrive (\(s.subagents.map { "\($0.agent) \($0.status)" }))")
    _ = await waitFor(10) { !s.busy }

    // Changed files.
    let changes = await model.changes()
    let names = changes?.files.map(\.path) ?? []
    check(changes?.branch == "main" && names.contains("notes.txt") && names.contains("added.txt"), "the changed files (\(changes?.branch ?? "-"), \(names))")
    let diff = await model.diff("notes.txt")
    let added = diff?.hunks.flatMap(\.lines).filter { $0.kind == .added }.map(\.text) ?? []
    check(added.contains("two, changed") && diff?.removed == 1, "a file's diff (\(added))")

    // What it ran and touched.
    let log = await model.actionLog()
    check((log?.entries.count ?? 0) > 0 && log?.entries.allSatisfy { !$0.kind.isEmpty && !$0.tool.isEmpty } == true,
          "the action log (\(log?.entries.map { "\($0.kind) \($0.target)" } ?? []))")
    let md = await model.markdown() ?? ""
    check(md.hasPrefix("# ") && md.contains("## You") && md.contains("## Demo"), "the session as Markdown (\(md.count) characters)")

    // `@` mentions, and a file a reply names.
    let found = await model.files(matching: "note")
    check(found.first == "notes.txt", "@ finds the session folder's files (\(found))")
    // `@session:`: another agent's session in the folder, and what it
    // said goes to the agent with the message.
    let others = await model.sessionMentions(matching: "plan")
    check(model.features.contains("sessionMentions") && others.first?.title == "Plan the notes" && others.first?.mention == "@session:7c1e9a40" && others.first?.harness == "claude",
          "@ finds the folder's other sessions (\(others.map { "\($0.mention) \($0.title)" }))")
    let none = await model.sessionMentions(matching: "zzz")
    check(none.isEmpty, "only those matching (\(none.count))")
    _ = await waitFor(8) { !s.busy }
    await model.send("fast: what did @session:7c1e9a40 find")
    // (Read in as an attachment, as the desktop sends it.)
    let sent = await waitFor(8) { s.rows.contains { $0.kind == "user" && $0.text.hasSuffix("@session:7c1e9a40 find") && $0.title.contains("session: Plan the notes") } }
    check(sent, "a mentioned session goes with the message (\(s.rows.filter { $0.kind == "user" }.map { "\($0.text.prefix(60)) [\($0.title)]" }.suffix(1)))")
    _ = await waitFor(8) { !s.busy }
    // A shell call's whole command comes with it (the card shows it, and its
    // output from the end).
    let shell = s.rows.first { $0.kind == "tool" && $0.title == "bash" }
    check(shell?.command == "ls -la", "a command's row has the command (\(shell?.command ?? "none"))")

    let text = await model.file("notes.txt")
    check(text?.lines.count == 3 && text.map { RichText.plain($0.spans(1)) } == "two, changed", "a file opens to read (\(text?.lines.count ?? 0) lines)")
    let outside = await model.file("../harlo.txt")
    check(outside == nil && (model.alert ?? "").contains("Only files"), "but only the session folder's (\(model.alert ?? "no alert"))")
    model.alert = nil

    // Undo a turn: the session's first message took a snapshot; a file
    // changed since (as an agent would) comes back as it was.
    await model.loadUndo()
    let first = s.rows.first { $0.kind == "user" }?.key ?? ""
    check(!first.isEmpty && s.undoable[first] != nil, "your message has a snapshot to undo to (\(s.undoable.count))")
    check(s.git, "its folder holds a repository: its changes have diffs")
    if let repo = ProcessInfo.processInfo.environment["HARLO_E2E_REPO"] {
        let notes = URL(fileURLWithPath: repo).appendingPathComponent("notes.txt")
        let before = (try? String(contentsOf: notes, encoding: .utf8)) ?? ""
        try? "an agent was here\n".write(to: notes, atomically: true, encoding: .utf8)
        let plan = await model.undoPlan(first)
        check(plan?.restore.contains("notes.txt") == true && plan?.done == false, "undoing it would bring notes.txt back (\(plan?.restore ?? []))")
        let done = await model.undo(first)
        let after = (try? String(contentsOf: notes, encoding: .utf8)) ?? ""
        check(done?.done == true && after == before, "undone: notes.txt is as it was (\(after.prefix(30)))")
    }
    // The branch, and switching it (the script made `feature`).
    let branches = await model.branches()
    check(branches?.current == "main" && branches?.local.contains { $0.name == "feature" } == true && branches?.changed == 2,
          "the session's branches (\(branches?.current ?? "-"), \(branches?.local.map(\.name) ?? []), \(branches?.changed ?? 0) changed)")
    let said = await model.switchBranch(to: "feature", remote: false, stash: false)
    let onFeature = await model.branches()
    check(onFeature?.current == "feature" && said?.contains("feature") == true, "switching the branch, changes brought along (\(said ?? model.alert ?? "-"))")
    _ = await model.switchBranch(to: "main", remote: false, stash: false)

    // Renaming, from the list.
    if let row = model.sessions.first(where: { $0.key == s.liveKey }) {
        await model.rename(row, to: "Renamed on the phone")
    }
    let renamed = await waitFor(5) { model.sessions.contains { $0.key == s.liveKey && $0.title == "Renamed on the phone" } }
    check(renamed, "renaming shows in the list (\(model.sessions.map(\.title)))")

    await working(model, s)
}

/// While it works: what it's doing, steering or queueing, the queue, each
/// call's result and diff, how long a run took; edit & resend where the
/// agent can't.
@MainActor
func working(_ model: HarloModel, _ s: OpenSession) async {
    // An edit (not fast: there's time to see it work).
    await model.send("edit please")
    var seen: Set<String> = []
    let finished = await waitFor(20) {
        if let a = s.activity, a.since > 0 { seen.insert(a.text.split(separator: " ").first.map(String.init) ?? "") }
        return !s.busy && s.rows.contains { $0.title == "edit" && $0.state == "done" }
    }
    check(finished && seen.contains("Thinking") && seen.contains("Writing"), "while it works it says what it does, and since when (\(seen.sorted()))")
    let edit = s.rows.last { $0.title == "edit" }
    check(edit?.result == "+2 −1" && edit?.diff.contains("+three") == true, "an edit shows its lines added and removed, and its diff (\(edit?.result ?? "-"))")
    let run = s.rows.last { !$0.summary.isEmpty && $0.group == edit?.group }
    check((run?.runMs ?? 0) > 0, "a finished run says how long it took (\(run?.runMs ?? 0) ms)")
    await model.send("fast: tests please")
    _ = await waitFor(10) { !s.busy && s.rows.contains { $0.result == "12 passed" } }
    check(s.rows.contains { $0.result == "12 passed" }, "a test run shows how many passed")

    // Steer while it works.
    await model.send("approve please")
    _ = await waitFor(6) { s.requests.first?.approval == true }
    s.steer = true
    let steered = await model.send("use the other file")
    s.steer = false
    check(steered, "a message steers the run while it works")
    if let req = s.requests.first { await model.answer(req, req.approval(true)) }
    _ = await waitFor(10) { !s.busy }

    // The queue: take a message back, send a follow-up now.
    await model.send("queue demo")
    let queued = await waitFor(5) { s.queued.count == 3 }
    check(queued && s.canUnqueue, "the queue shows (\(s.queued.map(\.text)))")
    await model.unqueue(0)
    let taken = await waitFor(5) { s.queued.count == 2 && s.steering.isEmpty }
    check(taken, "a waiting message can be taken back (\(s.queued.map(\.text)))")
    if let i = s.queued.firstIndex(where: { $0.text == "Update the changelog" }) { await model.sendNow(i) }
    let now = await waitFor(5) { s.steering == ["Update the changelog"] && s.followUp == ["Then run the tests"] }
    check(now, "a follow-up can be sent now (steering \(s.steering), next \(s.followUp))")

    // Edit & resend: the demo agent can't branch, and the app says so.
    if let mine = s.rows.first(where: { $0.kind == "user" }) {
        let text = await model.editMessage(mine.key)
        check(text == nil && (model.alert ?? "").contains("can't"), "edit & resend says when the agent can't (\(model.alert ?? "no alert"))")
        model.alert = nil
    }
}

let args = CommandLine.arguments
guard args.count >= 2 else {
    print("usage: harlo-probe <pairing link> | harlo-probe record <pairing link> <recording.jsonl>")
    exit(2)
}
if args[1] == "record", args.count >= 4 {
    await record(args[2], to: args[3])
    print("IOS-RECORD checks:", failures == 0 ? "all passed" : "\(failures) failed")
    exit(failures == 0 ? 0 : 1)
}
await run(args[1])
print("IOS-E2E checks:", failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)

/// Notifications: the app's token reaches harlo; with the app closed, an
/// approval comes as a notification sealed for this phone (Apple sees a
/// placeholder), Allow on it answers, and the finished run comes too.
/// harlo writes them to a file instead of sending them to Apple
/// (HARLO_TEST_PUSH_FILE). In a session of its own: Foundation's
/// WebSocket on Linux cuts messages at 16 KB, so the probe keeps the
/// sessions it reopens short (an iPhone takes harlo's 512 KB frames).
@MainActor
func pushes(_ model: HarloModel, store: SecretStore) async {
    guard let file = ProcessInfo.processInfo.environment["HARLO_TEST_PUSH_FILE"] else {
        check(false, "notifications: no push file")
        return
    }
    func sent() -> [JSON] {
        ((try? String(contentsOfFile: file, encoding: .utf8)) ?? "").split(separator: "\n").compactMap { try? JSON.parse(String($0)) }
    }
    await model.setPushToken(Data([0xab, 0xcd, 0x01]), sandbox: true)
    check(model.push?.token == "abcd01", "the app keeps its push token (\(model.push?.token ?? "-"))")
    // The app goes to the background; the agent asks a moment later.
    guard await model.startSession(project: nil, agent: "demo", model: "", text: "approve please later"), let s = model.session else {
        check(false, "notifications: a session to ask (\(model.alert ?? ""))")
        return
    }
    model.suspend()
    let came = await waitFor(10) { sent().contains { $0["payload"]["aps"]["category"].string == "approval" } }
    guard came, let push = sent().last(where: { $0["payload"]["aps"]["category"].string == "approval" }) else {
        check(false, "with the app closed, an approval comes as a notification (\(sent().count) sent)")
        return
    }
    let apple = push["payload"]["aps"]
    check(push["token"].string == "abcd01" && push["sandbox"].bool == true && apple["badge"].int == 1 && apple["mutable-content"].int == 1,
          "with the app closed, an approval comes as a notification, to this phone's token, with the badge")
    let placeholder = (try? String(data: JSONEncoder().encode(apple), encoding: .utf8)) ?? ""
    check(!placeholder.contains("ls -la") && !placeholder.contains(s.title) && placeholder.contains("waits for your approval"),
          "Apple sees only a placeholder (\(apple["alert"]["body"].text))")
    guard let content = try? PushContent.open(push["payload"]["h"].text, store: store) else {
        check(false, "the phone opens what it says")
        return
    }
    check(content.kind == "ask" && content.canApprove && content.session == s.liveKey && content.lines.subtitle == "Waiting for your approval"
          && content.request?.options == ["Approve", "Deny"],
          "the phone opens what it says: the session, the approval (\(content.lines.title) · \(content.lines.subtitle) · \(content.body.prefix(40)))")
    let answered = await model.answer(fromNotification: content, allow: true)
    check(answered && model.state != .connected, "Allow on the notification answers it, the app closed again (\(model.alert ?? "no alert"))")
    let done = await waitFor(10) { sent().contains { (try? PushContent.open($0["payload"]["h"].text, store: store))?.kind == "done" } }
    check(done, "the finished run comes as a notification too")
    model.resume()
    let back = await waitFor(8) { model.state == .connected && !s.liveKey.isEmpty && !s.busy && s.requests.isEmpty && model.waiting.isEmpty }
    check(back && s.rows.last?.kind == "markdown", "back in the app, the run went on (\(s.rows.last?.kind ?? "-"))")

    // A Live Activity: harlo follows the run and sends its updates, then its end.
    guard await model.startSession(project: nil, agent: "demo", model: "", text: "edit please later"), let a = model.session,
          await waitFor(5, { !a.liveKey.isEmpty }) else {
        check(false, "a session for a Live Activity (\(model.alert ?? ""))")
        return
    }
    await model.setActivityToken("ac01", for: a.liveKey)
    model.suspend()
    func live() -> [JSON] { sent().filter { $0["activity"].bool == true && $0["token"].string == "ac01" } }
    let ended = await waitFor(30) { live().contains { $0["payload"]["aps"]["event"].string == "end" } }
    let states = live().map { $0["payload"]["aps"]["content-state"] }
    let words = states.compactMap { LiveDetail.open($0["sealed"].text, store: store)?.now }
    check(ended && states.first?["phase"].string == "working" && states.last?["phase"].string == "done" && (states.first?["since"].double ?? 0) > 1e9,
          "a Live Activity gets the run's updates and its end (\(states.map(\.["phase"].text)))")
    let plain = live().map { (try? String(data: JSONEncoder().encode($0), encoding: .utf8)) ?? "" }.joined()
    check(!words.isEmpty && words.allSatisfy { !plain.contains($0) } && words.last?.isEmpty == false,
          "what it's doing is sealed for the phone (\(words))")
    model.resume()
    _ = await waitFor(8) { model.state == .connected }
    // That session is in the home folder, no repository: no View diff.
    await model.loadUndo()
    check(model.session === a && !a.git, "outside a repository, harlo says so (no View diff)")
}

// ---- recording -------------------------------------------------------------

/// Records a tour of harlo's features for the app's UI tests and screenshots
/// (scripts/ios-record.sh): Harlo sessions on the scripted tour model doing
/// real work, a Claude session, and at each point worth a picture a scene.
@MainActor
func record(_ link: String, to path: String) async {
    guard let rec = Recorder(path: path) else {
        check(false, "the recording can be written (\(path))")
        return
    }
    Recorder.shared = rec
    let model = HarloModel(store: MemoryStore(), deviceName: "iPhone")
    let paired = await model.pair(link)
    let up = await waitFor(10) { model.state == .connected }
    check(paired && up, "pairs with harlo (\(model.alert ?? ""))")
    guard let choices = await model.newSessionChoices(), let repo = choices.projects.first else {
        check(false, "a project to work in (\(model.alert ?? ""))")
        return
    }
    let models = choices.models(for: "harlo")
    print("IOS-RECORD agents \(choices.agents.map(\.id)), harlo's models \(models)")

    /// Sends, then waits for the run to end (`until`: and for this).
    func turn(_ text: String, _ seconds: Double = 30, until: @MainActor (OpenSession) -> Bool = { _ in true }) async -> Bool {
        guard await connected(), let s = model.session else { return false }
        let before = s.rows.count
        guard await model.send(text) else { return false }
        _ = await waitFor(5) { s.rows.count > before || s.busy }
        return await waitFor(seconds) { !s.busy && s.requests.isEmpty && until(s) }
    }
    /// (Foundation's WebSocket on Linux can drop: the model reconnects.)
    func connected() async -> Bool {
        let ok = await waitFor(20) { model.state == .connected && model.session.map { !$0.liveKey.isEmpty && !$0.loading } ?? true }
        if !ok { print("IOS-RECORD not connected: \(model.state)") }
        return ok
    }
    func send(_ text: String) async {
        _ = await connected()
        if !(await model.send(text)) { print("IOS-RECORD not sent: \(model.alert ?? "")") }
    }
    func start(_ agent: String, _ text: String, model id: String = "") async -> OpenSession? {
        guard await connected(), await model.startSession(project: repo, agent: agent, model: id, approval: agent == "harlo" ? "yolo" : "", text: text),
              let s = model.session, await waitFor(10, { !s.liveKey.isEmpty }) else { return nil }
        _ = await waitFor(30) { !s.busy && s.rows.contains { $0.kind == "markdown" && $0.state == "done" } }
        return s
    }

    // Two other sessions first (older in the list): a Harlo question and
    // Claude Code with a subagent.
    let other = await start("harlo", "Why does the relay drop idle phones?", model: "tour/tour-coder")
    check(other != nil, "a short Harlo session")
    let claude = await start("claude", "Have a subagent look over the README and delegate the review")
    check(claude != nil && claude?.rows.contains { $0.kind == "tool" } == true, "a Claude Code session with tool calls (\(claude?.rows.map(\.kind) ?? []))")

    // The main session: a fix, with reads, an edit and the tests.
    guard let s = await start("harlo", "The session list jumps to the top when a session finishes. Find out why and fix it.", model: "tour/tour-coder") else {
        check(false, "the main session starts (\(model.alert ?? ""))")
        return
    }
    check(s.rows.filter { $0.kind == "tool" }.count >= 4, "it reads, edits and runs the tests (\(s.rows.map { $0.kind == "tool" ? $0.title : $0.kind }))")
    // What its other screens ask for.
    _ = await model.changes()
    _ = await model.diff("src/sidebar.rs")
    _ = await model.files(matching: "")
    _ = await model.files(matching: "side")
    _ = await model.sessionMentions(matching: "")
    _ = await model.file("src/sidebar.rs")
    _ = await model.branches()
    _ = await model.models()
    _ = await model.thinkingLevels()
    _ = await model.actionLog()
    _ = await model.markdown()
    _ = await model.modelServers()
    await model.loadUndo()
    rec.scene("session")

    // A question for you.
    await send("Before you go on: which approach should we take?")
    let asked = await waitFor(20) { s.requests.contains { !$0.questions.isEmpty } }
    check(asked, "it asks a question (\(s.requests.map(\.method)))")
    rec.scene("ask")
    if let q = s.requests.first(where: { !$0.questions.isEmpty }) {
        await model.answer(q, UIRequest.askAnswer(q.questions, picked: [[q.questions[0].options[0]]], typed: [""]))
    }
    _ = await waitFor(20) { !s.busy && s.requests.isEmpty }

    // A destructive command waits for approval.
    await send("Clean up the build folder while you're at it.")
    let approval = await waitFor(20) { s.requests.contains(where: \.approval) }
    check(approval, "a destructive command asks first (\(s.requests.map(\.title)))")
    rec.scene("approval")
    if let a = s.requests.first(where: \.approval) { await model.answer(a, a.approval(false)) }
    _ = await waitFor(20) { !s.busy && s.requests.isEmpty }

    // A background build and a subagent; the build's end comes later.
    await send("Build the release in the background, and meanwhile look for other full rebuilds.")
    let working = await waitFor(20) { !s.subagents.isEmpty }
    check(working, "a subagent works (\(s.subagents.map(\.status)))")
    rec.scene("working")
    let built = await waitFor(40) { !s.busy && s.rows.contains { $0.html.contains("release build finished") || $0.text.contains("release build finished") } }
    check(built, "the background build's end comes, and the agent says so")

    // A side question.
    await send("/btw why is merging by key faster than rebuilding?")
    let aside = await waitFor(20) { s.aside?.done == true }
    check(aside, "a side question is answered (\(s.aside?.text.prefix(40) ?? "none"))")
    rec.scene("aside")

    // An MCP server's tool, the plan, a helper.
    let rolled = await turn("Roll the dice to pick which list to fix next.")
    check(rolled && s.rows.contains { $0.kind == "tool" && $0.title.contains("roll") }, "an MCP tool (\(s.rows.filter { $0.kind == "tool" }.map(\.title).suffix(2)))")
    let planned = await turn("Plan the rest of the work.")
    _ = await waitFor(5) { (s.plan?.steps.count ?? 0) >= 3 }
    check(planned && (s.plan?.steps.count ?? 0) >= 3, "a plan (\(s.plan?.steps.map(\.title) ?? []))")
    // The project's wiki, as the plan's sheet shows it: its pages and how
    // fresh, each page read, a search OpenWiki answers.
    await model.loadWiki()
    check(s.wiki?.pages.map(\.title) == ["Quickstart", "Sidebar", "Releasing"] && s.wiki?.behind == 1, "the wiki's pages, one commit behind (\(String(describing: s.wiki)))")
    for page in s.wiki?.pages ?? [] { _ = await model.wikiPage(page.path) }
    let sidebarPage = await model.wikiPage("architecture/sidebar.md")
    check(sidebarPage?.blocks.map(\.kind).contains("mermaid") == true && sidebarPage?.blocks.map(\.kind).contains("code") == true, "a page in blocks (\(sidebarPage?.blocks.map(\.kind) ?? []))")
    let found = await model.searchWiki("rebuilds the whole list")
    check(found?.hits.first?.page == "openwiki/architecture/sidebar.md" && found?.hits.first?.heading == "Rebuilding rows" && found?.plain == false, "OpenWiki's search finds the section (\(found?.hits.first.map { "\($0.page)#\($0.section)" } ?? "none"))")
    rec.scene("plan")
    let helped = await turn("Have a helper update the changelog.", 90) { s in s.rows.contains { $0.html.contains("changelog entry is in") } }
    check(helped, "a helper's change brought over (\(s.rows.suffix(4).map { "\($0.kind) \($0.title) \($0.text.prefix(30))" }))")
    rec.scene("helpers")

    // Another model now; earlier replies were the first one's.
    if let helper = await model.models().first(where: { $0.modelId == "tour-helper" }) { await model.setModel(helper) }
    let summed = await turn("Summarize what changed.")
    check(summed && s.info.modelId.hasSuffix("tour-helper"), "the model switched (\(s.info.modelId))")
    _ = await model.sessionMentions(matching: "")
    _ = await model.changes()
    rec.scene("end")

    // Pictures: a photo from the phone, and the one the reply shows; the
    // phone asks for each as a thumbnail and fitted to its screen.
    if let file = ProcessInfo.processInfo.environment["HARLO_RECORD_PHOTO"], let jpeg = FileManager.default.contents(atPath: file) {
        _ = await connected()
        let sent = await model.send("Here's how it looks on my phone.", photos: [Photo(data: jpeg, mimeType: "image/jpeg")])
        let shown = await waitFor(30) { !s.busy && s.pictures.count >= 2 }
        check(sent && shown && model.showsPictures, "the photo and the reply's picture show (\(s.pictures))")
        for path in s.pictures {
            for thumb in [true, false] {
                let got = await model.picture(path, thumb: thumb)
                check(got.picture != nil, "\(path.split(separator: "/").last ?? "") as \(thumb ? "a thumbnail" : "a screen's picture") (\(got.picture.map { "\(Int($0.width))x\(Int($0.height)), \($0.data.count) bytes" } ?? got.why))")
            }
        }
        rec.scene("pictures")
    }
    Recorder.shared = nil
}
