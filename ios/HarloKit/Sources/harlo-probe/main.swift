// A phone made of HarloKit, for tools/ios-e2e.sh and tools/ios-record.sh:
// the app's own model (HarloModel) against a real Harlo server on the
// scripted model (tools/fake-model.py), whose messages say what the model
// does (`say …`, `tool edit {…} ;; say …`).
//
// The check pairs from the pairing link, lists the sessions, starts one
// with a photo, answers an approval and a question (one from the list of
// what waits), reads the folder's changes, diffs and files, undoes a turn,
// switches the branch, the approval mode and the model, follows a run while
// it works, survives a dropped connection and a relaunch, finds Harlo at
// its other addresses, and is refused without a pairing code.
//
// The recording is the app's UI tests' and screenshots' tour: sessions on
// the tour's two models doing real work in its project, and at each point
// worth a picture a scene.
//
//   harlo-probe <pairing link>
//   harlo-probe record <pairing link> <recording.jsonl>
import Foundation
import HarloKit

var failures = 0
@MainActor
func check(_ ok: Bool, _ what: String) {
    if !ok { failures += 1 }
    print("IOS-E2E", ok ? "PASS" : "FAIL", what)
}

@MainActor
func waitFor(_ seconds: Double, _ cond: @MainActor () -> Bool) async -> Bool {
    let end = Date.now.addingTimeInterval(seconds)
    while Date.now < end {
        if cond() { return true }
        try? await Task.sleep(for: .milliseconds(100))
    }
    return cond()
}

/// A message for the scripted model: an edit of `path`, then `then`.
func editScript(_ path: String, _ old: String, _ new: String, then: String) -> String {
    let args: JSON = ["path": .string(path), "oldString": .string(old), "newString": .string(new)]
    return "tool edit \(String(decoding: (try? args.data()) ?? Data(), as: UTF8.self)) ;; say \(then)"
}

/// A question with two fields for the scripted model to ask.
let askScript = #"tool question {"questions":[{"question":"Which colour?","header":"Colour","options":[{"label":"Red","description":"warm"},{"label":"Blue"}]},{"question":"Which sizes?","multiple":true,"options":[{"label":"S"},{"label":"L"}]}]} ;; say Got it."#

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
    check(listed && model.sessions.allSatisfy { $0.harness == "harlo" }, "it connects and lists the sessions, each Harlo's (\(model.state), \(model.sessions.map(\.harness)))")

    let themed = await waitFor(5) { model.theme != nil }
    check(themed, "the desktop's theme arrives (\(model.theme?.family ?? "none"))")
    // Picking a penguin on the phone: Harlo keeps it and sends it back.
    await model.setAvatar("scarf")
    let avatar = await waitFor(5) { model.theme?.avatar == "scarf" }
    check(avatar, "a penguin picked on the phone is Harlo's (\(model.theme?.avatar ?? "none"))")

    guard let choices = await model.newSessionChoices(), let repo = choices.projects.first else {
        check(false, "a project for a new session (\(model.alert ?? ""))")
        return
    }
    let agent = choices.defaultAgent
    check(!agent.isEmpty && choices.agents.contains { $0.id == agent }, "agents for a new session (\(choices.agents.map(\.id)), default \(agent))")
    check(choices.approval(for: agent, in: repo) == "write", "a new session starts with the agent's own rules (\(choices.approval(for: agent, in: repo)))")

    // A new session, careful, with a photo (a 1×1 PNG).
    let png = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")!
    let started = await model.startSession(project: repo, agent: agent, model: "", approval: "always-ask", text: "say Hello from the phone.", photos: [Photo(data: png, mimeType: "image/png")])
    guard started, let s = model.session, await waitFor(10, { !s.liveKey.isEmpty }) else {
        check(false, "a new session starts in the project (\(model.alert ?? ""))")
        return
    }
    let replied = await waitFor(15) {
        s.rows.contains { $0.kind == "user" && $0.text.contains("Hello from the phone") }
            && s.rows.contains { $0.kind == "markdown" && $0.html.contains("Hello from the phone.") && $0.state == "done" } && !s.busy
    }
    check(replied, "a new session starts with its first message, and the reply streams back (\(s.rows.map(\.kind)))")
    let mine = s.rows.first { $0.kind == "user" }
    check(mine?.images.count == 1 && model.showsPictures, "the photo goes with it, under the message (\(mine?.images ?? []))")
    if let photo = mine?.images.first {
        let small = await model.picture(photo, thumb: true)
        let whole = await model.picture(photo, thumb: false)
        check(small.picture?.width == 1 && whole.picture?.data.isEmpty == false, "and comes back to look at (\(small.why) \(whole.why))")
        check(s.pictureName(photo) == "You sent", "named by where it came from (\(s.pictureName(photo)))")
    }
    let commands = await waitFor(10) { s.commands.contains { $0.name == "compact" } && s.commands.contains { $0.name == "goal" } }
    check(commands, "what can be typed after / arrives (\(s.commands.map(\.name)))")
    let underProject = await waitFor(5) { model.sessions.contains { $0.key == s.liveKey && $0.project == repo.name } }
    check(underProject, "it's listed under its project (\(model.sessions.map { "\($0.project): \($0.title)" }))")
    let careful = await waitFor(5) { s.modes?.approval == "always-ask" }
    check(careful, "it starts in the approval mode picked (\(s.modes?.approval ?? "-"))")

    // An edit asks first; Allow once lets it run.
    _ = await model.send(editScript("a.txt", "alpha", "from the phone", then: "Edited a.txt."))
    let asked = await waitFor(15) { s.requests.first?.approval == true }
    let ask = s.requests.first
    check(asked && ask?.options == ["Allow once", "Allow always", "Deny"] && ask?.title.contains("edit") == true,
          "an edit asks for approval (\(ask.map { "\($0.title.replacingOccurrences(of: "\n", with: " | ")): \($0.options)" } ?? "none"))")
    check(ask?.asks == "Wants to edit a file" && ask?.detail == "a.txt" && ask?.diff.contains("+from the phone") == true
          && ask?.always.hasPrefix("Allow always applies to ") == true,
          "it says what the agent wants, shows the change and what Allow always covers (\(ask?.asks ?? "-"); \(ask?.always ?? "-"))")
    if let ask {
        _ = await model.answer(ask, ask.approval(true))
        let ran = await waitFor(15) { s.requests.isEmpty && !s.busy && s.rows.last?.kind == "markdown" }
        let edit = s.rows.last { $0.kind == "tool" }
        check(ran && edit?.state == "done" && edit?.result == "+1 −1" && edit?.diff.contains("+from the phone") == true,
              "Allow once answers it: the edit runs and shows its diff (\(edit.map { "\($0.title) \($0.state) \($0.result)" } ?? "none"))")
    }

    // A question with two fields: picked options reach the agent.
    _ = await model.send(askScript)
    let questioned = await waitFor(15) { s.requests.first?.method == "ask" }
    if let q = s.requests.first, questioned {
        check(q.questions.map(\.options) == [["Red", "Blue"], ["S", "L"]], "a question arrives with its options (\(q.questions.map(\.options)))")
        _ = await model.answer(q, UIRequest.askAnswer(q.questions, picked: [["Blue"], ["S", "L"]], typed: []))
        let answered = await waitFor(15) { s.requests.isEmpty && !s.busy && s.rows.last?.html.contains("Got it.") == true }
        check(answered, "the answer reaches the agent and the run goes on (\(s.rows.last?.kind ?? "-"))")
    } else {
        check(false, "a question arrives (\(s.requests.map(\.method)))")
    }

    // What waits on you: the list says where, and it's answered from there
    // with the session closed.
    _ = await model.send(editScript("b.txt", "beta", "from the list", then: "Edited b.txt."))
    let waits = await waitFor(15) { model.waiting.first?.request.approval == true }
    if let w = model.waiting.first, waits {
        check(w.session.key == s.liveKey && w.session.status == "input", "the list says what waits on you, and in which session (\(w.session.status))")
        model.close()
        _ = await model.answer(w.request, w.request.approval(true), in: w.session.key)
        let gone = await waitFor(15) { model.waiting.isEmpty && model.sessions.first { $0.key == w.session.key }?.status != "input" }
        check(gone && model.alert == nil, "an approval answered from the list goes, and Harlo agrees (\(model.alert ?? "no alert"))")
        model.open(w.session)
        let reopened = await waitFor(15) { model.session.map { !$0.liveKey.isEmpty && !$0.loading && !$0.busy && $0.rows.last?.html.contains("Edited b.txt.") == true } ?? false }
        check(reopened, "its session went on (\(model.session?.rows.last?.kind ?? "-"))")
    } else {
        check(false, "the list says what waits on you (\(model.waiting.count))")
    }
    guard let s = model.session else { return }

    await folder(model, s)
    await working(model, s)
    await aside(model, s)

    // The network drops: it reconnects and the session comes back.
    let before = s.rows.count
    await model.dropConnection()
    let back = await waitFor(10) { model.state == .connected && !s.liveKey.isEmpty && !s.loading && s.rows.count == before }
    check(back, "after a dropped connection it reconnects and reopens the session (\(model.state), \(s.rows.count)/\(before) rows)")
    model.suspend()

    // A relaunch: the same Keychain, no code.
    let again = HarloModel(store: store, deviceName: "Probe iPhone")
    check(again.state == .connecting && again.host?.key == offer.key, "a relaunch remembers Harlo")
    again.resume()
    let reconnected = await waitFor(10) { again.state == .connected && !again.sessions.isEmpty }
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

/// The session's folder from the phone: the changes and a diff, the action
/// log, the conversation as Markdown, `@` file search, a file to read (only
/// the folder's), undoing a turn, the branches; the approval mode, the
/// model and thinking level, renaming.
@MainActor
func folder(_ model: HarloModel, _ s: OpenSession) async {
    let changes = await model.changes()
    let names = changes?.files.map(\.path) ?? []
    check(names.contains("a.txt") && names.contains("b.txt") && !(changes?.branch ?? "").isEmpty, "the changed files (\(changes?.branch ?? "-"), \(names))")
    let diff = await model.diff("a.txt")
    let added = diff?.hunks.flatMap(\.lines).filter { $0.kind == .added }.map(\.text) ?? []
    check(added == ["from the phone"] && diff?.removed == 1, "a file's diff (\(added))")

    let log = await model.actionLog()
    check(log?.entries.filter { $0.kind == "edit" }.count == 2 && log?.changed.sorted() == ["a.txt", "b.txt"],
          "the action log (\(log?.entries.map { "\($0.kind) \($0.target)" } ?? []))")
    let md = await model.markdown() ?? ""
    check(md.contains("Hello from the phone.") && md.contains("Edited a.txt."), "the session as Markdown (\(md.count) characters)")

    let found = await model.files(matching: "a.t")
    check(found.first == "a.txt", "@ finds the folder's files (\(found))")
    let text = await model.file("a.txt")
    check(text?.lines.count == 1 && text.map { RichText.plain($0.spans(0)) } == "from the phone", "a file opens to read (\(text?.lines.count ?? 0) lines)")
    let outside = await model.file("../outside.txt")
    check(outside == nil && model.alert != nil, "but only the folder's (\(model.alert ?? "no alert"))")
    model.alert = nil

    // Undo the last turn: b.txt back as it was before it.
    await model.loadUndo()
    check(s.git, "its folder holds a repository: its changes have diffs")
    let last = s.rows.last { $0.kind == "user" && s.undoable[$0.key] != nil }?.key ?? ""
    check(!last.isEmpty, "your messages have snapshots to undo to (\(s.undoable.count))")
    if let repo = ProcessInfo.processInfo.environment["HARLO_E2E_REPO"], !last.isEmpty {
        let b = URL(fileURLWithPath: repo).appendingPathComponent("b.txt")
        let plan = await model.undoPlan(last)
        check(plan?.restore.contains("b.txt") == true && plan?.done == false, "undoing it would bring b.txt back (\(plan?.restore ?? []))")
        let done = await model.undo(last)
        let after = (try? String(contentsOf: b, encoding: .utf8)) ?? ""
        check(done?.done == true && after == "beta\n", "undone: b.txt is as it was (\(after.prefix(30)))")
    }

    // The branch, and switching it (the script made `feature`).
    let branches = await model.branches()
    let home = branches?.current ?? ""
    check(!home.isEmpty && branches?.local.contains { $0.name == "feature" } == true, "the session's branches (\(home), \(branches?.local.map(\.name) ?? []))")
    let said = await model.switchBranch(to: "feature", remote: false, stash: false)
    let onFeature = await model.branches()
    check(onFeature?.current == "feature", "switching the branch, changes brought along (\(said ?? model.alert ?? "-"))")
    model.alert = nil
    _ = await model.switchBranch(to: home, remote: false, stash: false)

    // Approval mode, model and thinking.
    await model.setApproval("write")
    let write = await waitFor(5) { s.modes?.approval == "write" }
    check(write && s.modes?.approvals == ["always-ask", "write", "yolo"], "the approval mode switches (\(String(describing: s.modes)))")
    let models = await model.models()
    check(!models.isEmpty, "the models it can switch to (\(models.map(\.id)))")
    if let other = models.last {
        await model.setModel(other)
        let switched = await waitFor(5) { s.info.modelId.hasSuffix(other.modelId) }
        check(switched, "switching the model (\(s.info.modelId))")
    }
    let levels = await model.thinkingLevels()
    print("IOS-E2E thinking levels \(levels)")

    // Renaming, from the list.
    if let row = model.sessions.first(where: { $0.key == s.liveKey }) { await model.rename(row, to: "Renamed on the phone") }
    let renamed = await waitFor(5) { model.sessions.contains { $0.key == s.liveKey && $0.title == "Renamed on the phone" } }
    check(renamed, "renaming shows in the list (\(model.sessions.map(\.title)))")
}

/// While it works: busy and saying what it does; a message sent then
/// waits its turn, and the run takes it after.
@MainActor
func working(_ model: HarloModel, _ s: OpenSession) async {
    _ = await model.send("pace 150 | say Working through it slowly, one word at a time, so there is time to watch it work.")
    var seen: Set<String> = []
    let busy = await waitFor(10) {
        if let a = s.activity, !a.text.isEmpty { seen.insert(a.text) }
        return s.busy
    }
    check(busy, "it says it's working")
    let sent = await model.send("say And this came after.")
    let queued = await waitFor(5) { !s.queued.isEmpty }
    check(sent, "a message can be sent while it works (\(queued ? "waits: \(s.queued.map(\.text))" : "taken at once"))")
    let finished = await waitFor(30) {
        if let a = s.activity, !a.text.isEmpty { seen.insert(a.text) }
        return !s.busy && s.rows.last?.html.contains("And this came after.") == true
    }
    check(finished, "the run ends, then the message that waited runs (\(s.rows.suffix(2).map { $0.text.prefix(30) }))")
    print("IOS-E2E activity seen \(seen.sorted())")
}

/// Beside the conversation: a refusal that says why, a command sent to the
/// background and stopped from its row, and a goal with its row.
@MainActor
func aside(_ model: HarloModel, _ s: OpenSession) async {
    // Refused with a reason: the agent is told it, and carries on.
    await model.setApproval("always-ask")
    _ = await model.send(editScript("a.txt", "from the phone", "refused", then: "Left it."))
    let asked = await waitFor(15) { s.requests.first?.approval == true }
    if let ask = s.requests.first, asked, model.takesReasons {
        _ = await model.answer(ask, ask.refusal(saying: " Keep it as it is. "))
        let told = await waitFor(15) { s.requests.isEmpty && !s.busy && s.rows.last?.html.contains("Left it.") == true }
        let edit = s.rows.last { $0.kind == "tool" }
        check(told && edit?.state == "error" && edit?.result == "Declined" && edit?.error.hasSuffix("said: Keep it as it is.") == true,
              "a refusal's reason reaches the agent (\(edit?.result ?? "-"): \(edit?.error ?? "-"))")
    } else {
        check(false, "an edit asks, and Harlo takes a refusal's reason (\(s.requests.count), \(model.takesReasons))")
    }

    // A command the agent waits for goes to the background; the turn goes
    // on, and its row stops it.
    await model.setApproval("yolo")
    _ = await model.send(#"tool shell {"command":"sleep 30"} ;; say Moved on."#)
    let waitsFor = await waitFor(15) { !s.backgroundCall.isEmpty }
    check(waitsFor && s.activity?.text == "Running command sleep 30", "it says which command it waits for (\(s.activity?.text ?? "-"))")
    let moved = await model.sendToBackground()
    let listed = await waitFor(15) { s.jobs.contains { $0.title == "Command" } && s.backgroundCall.isEmpty }
    let row = s.jobs.first { $0.title == "Command" }
    check(moved && listed && row?.text == "sleep 30" && row?.action == "Stop" && row?.working == true,
          "sent to the background, it's listed with Stop (\(row?.spoken ?? model.alert ?? "-"))")
    let went = await waitFor(15) { !s.busy && s.rows.last?.html.contains("Moved on.") == true }
    check(went, "and the turn goes on without it (\(s.rows.last?.kind ?? "-"))")
    if let row {
        let stopped = await model.act(on: row)
        let gone = await waitFor(15) { s.jobs.isEmpty }
        check(stopped && gone, "Stop on its row ends it (\(s.jobs.map(\.spoken)))")
    }

    // A goal, typed as the command it is: its row says where it stands
    // (the reviewer here always finds it blocked), and it's cleared the
    // same way.
    _ = await model.send("/goal Ship the release")
    let paused = await waitFor(30) { s.jobs.contains { $0.title == "Goal" && $0.note == "Paused" } }
    let goal = s.jobs.first { $0.title == "Goal" }
    check(paused && goal?.text == "Ship the release" && goal?.detail == "It needs the release date." && goal?.action == "Resume",
          "a goal typed after / shows with where it stands (\(goal?.spoken ?? model.alert ?? "-"))")
    _ = await waitFor(15) { !s.busy }
    _ = await model.send("/goal clear")
    let cleared = await waitFor(15) { !s.jobs.contains { $0.title == "Goal" } }
    check(cleared, "and /goal clear takes it away (\(s.jobs.map(\.spoken)))")
    _ = await waitFor(15) { !s.busy }
}

/// Reaching Harlo wherever it is: its addresses come in the pairing code
/// and when it connects, the app tries them all at once (a dead one doesn't
/// hold it up), and Harlo answers at the others too (Tailscale here).
@MainActor
func reaching(store: MemoryStore, offer: PairingOffer, good: URL) async {
    let m = HarloModel(store: store, deviceName: "Probe iPhone")
    let learned = m.host?.endpoints.filter { $0 != good } ?? []
    print("IOS-E2E other addresses \(learned.map(\.absoluteString))")
    guard let secret = store.load("identity"), let identity = try? KeyPair(secret: secret), var h = m.host else {
        check(false, "the phone keeps its key and Harlo")
        return
    }
    // Each of them answers (these are this machine's own addresses).
    var answered: [String] = []
    for url in learned where url.scheme == "ws" {
        if let (c, _) = try? await HarloClient.connect(to: url, identity: identity, harloKey: offer.key, timeout: 4) {
            answered.append(url.host ?? "")
            await c.close()
        }
    }
    check(answered.count == learned.filter { $0.scheme == "ws" }.count, "Harlo answers at its other addresses (\(answered))")

    // Many requests at once: they go out in the order they were sealed, or
    // Harlo drops the connection as a replay.
    if let (c, _) = try? await HarloClient.connect(to: good, identity: identity, harloKey: offer.key, timeout: 4) {
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
    let dead = URL(string: "ws://10.255.255.1:7676/ws")!
    h.endpoints = [dead] + h.endpoints
    h.lastEndpoint = dead
    let moved = MemoryStore()
    moved.save("identity", secret)
    moved.save("host", try? JSONEncoder().encode(h))
    let elsewhere = HarloModel(store: moved, deviceName: "Probe iPhone")
    let start = Date.now
    elsewhere.resume()
    let quick = await waitFor(8) { elsewhere.state == .connected }
    let took = Date.now.timeIntervalSince(start)
    check(quick && took < 2.5 && elsewhere.host?.lastEndpoint != dead, "a dead address doesn't hold it up (\(String(format: "%.1f", took)) s, via \(elsewhere.host?.lastEndpoint?.host ?? "-"))")
    check(elsewhere.host?.endpoints.contains(dead) == false, "and Harlo's own list replaces addresses it no longer has")

    // The network changes while connected: it checks and stays.
    await elsewhere.networkChanged()
    check(elsewhere.state == .connected, "a network change with Harlo still there keeps the connection")
    elsewhere.suspend()

    // Connected over Tailscale (away from home), with a session open; back
    // on home Wi-Fi, it moves to Harlo's address there and the session
    // carries on.
    guard let tail = learned.first(where: { Endpoints.rank($0) == 1 }) else {
        print("IOS-E2E moving to a better way in: no Tailscale address here, skipped")
        return
    }
    var t = h
    t.endpoints = [tail]
    t.lastEndpoint = tail
    let away = MemoryStore()
    away.save("identity", secret)
    away.save("host", try? JSONEncoder().encode(t))
    let phone = HarloModel(store: away, deviceName: "Probe iPhone")
    phone.resume()
    let viaTail = await waitFor(8) { phone.state == .connected && !phone.sessions.isEmpty }
    check(viaTail && phone.host?.lastEndpoint == tail, "it connects over Tailscale (\(phone.host?.lastEndpoint?.host ?? "-"))")
    if let first = phone.sessions.first { phone.open(first) }
    let opened = await waitFor(5) { phone.session.map { !$0.liveKey.isEmpty && !$0.rows.isEmpty } ?? false }
    let rows = phone.session?.rows.count ?? 0
    let didMove = await phone.moveToBetter()
    let back = await waitFor(5) { phone.state == .connected && phone.session.map { !$0.liveKey.isEmpty && $0.rows.count == rows } ?? false }
    let via = phone.host?.lastEndpoint
    check(opened && didMove && back && via.map { Endpoints.rank($0) == 0 } == true,
          "back home, it moves to Harlo's local address and the session carries on (\(via?.host ?? "-"), \(phone.session?.rows.count ?? 0)/\(rows) rows)")
    let again = await phone.moveToBetter()
    check(!again, "and stays there (nothing better)")
    phone.suspend()
}

// ---- recording -------------------------------------------------------------

/// Records the tour for the app's UI tests and screenshots
/// (tools/ios-record.sh): sessions on the tour's models doing real work in
/// its project (ios/fixtures/tour), and at each point worth a picture a
/// scene.
@MainActor
func record(_ link: String, to path: String) async {
    guard let rec = Recorder(path: path) else {
        check(false, "the recording can be written (\(path))")
        return
    }
    Recorder.shared = rec
    defer { Recorder.shared = nil }
    let model = HarloModel(store: MemoryStore(), deviceName: "iPhone")
    let paired = await model.pair(link)
    let up = await waitFor(10) { model.state == .connected }
    check(paired && up, "pairs with Harlo (\(model.alert ?? ""))")
    guard let choices = await model.newSessionChoices(), let repo = choices.projects.first else {
        check(false, "a project to work in (\(model.alert ?? ""))")
        return
    }
    let agent = choices.defaultAgent
    print("IOS-RECORD agents \(choices.agents.map(\.id)), models \(choices.models(for: agent))")

    /// (Foundation's WebSocket on Linux can drop: the model reconnects.)
    func connected() async -> Bool {
        let ok = await waitFor(20) { model.state == .connected && model.session.map { !$0.liveKey.isEmpty && !$0.loading } ?? true }
        if !ok { print("IOS-RECORD not connected: \(model.state)") }
        return ok
    }
    /// Sends, then waits for the run to end and `until`.
    func turn(_ text: String, _ seconds: Double = 30, until: @MainActor (OpenSession) -> Bool = { _ in true }) async -> Bool {
        guard await connected(), let s = model.session else { return false }
        let before = s.rows.count
        guard await model.send(text) else { return false }
        _ = await waitFor(5) { s.rows.count > before || s.busy }
        return await waitFor(seconds) { !s.busy && s.requests.isEmpty && until(s) }
    }
    func start(_ text: String, model id: String, approval: String) async -> OpenSession? {
        guard await connected(), await model.startSession(project: repo, agent: agent, model: id, approval: approval, text: text),
              let s = model.session, await waitFor(10, { !s.liveKey.isEmpty }) else { return nil }
        _ = await waitFor(30) { !s.busy && s.rows.contains { $0.kind == "markdown" && $0.state == "done" } }
        return s
    }

    // Two other sessions first (older in the list): short questions, the
    // first on the other model (ios-record.sh started it).
    let other = await waitFor(20) { model.sessions.contains { $0.title == "Why does the relay drop idle phones?" && $0.status != "busy" } }
    check(other, "a short session on the other model (\(model.sessions.map(\.title)))")
    let readme = await start("What does the README say about the app?", model: "tour/tour-coder", approval: "write")
    check(readme != nil, "another short session")

    // The main session: a fix, with a read, a search, an edit and the tests.
    guard let s = await start("The session list jumps to the top when a session finishes. Find out why and fix it.", model: "tour/tour-coder", approval: "yolo") else {
        check(false, "the main session starts (\(model.alert ?? ""))")
        return
    }
    _ = await waitFor(30) { !s.busy && s.rows.last?.html.contains("All 112 tests pass") == true }
    check(s.rows.count(where: { $0.kind == "tool" }) == 4, "it reads, searches, edits and runs the tests (\(s.rows.map { $0.kind == "tool" ? "\($0.title) \($0.subtitle)" : $0.kind }))")
    let runs = s.items.compactMap { item -> String? in
        if case .run(_, let summary, _, let open, _) = item, !open { return summary }
        return nil
    }
    check(runs == ["Read 1 file, edited 1 file, ran 1 command, searched 1 time"], "its tool calls fold under the run's summary (\(runs))")
    check(s.rows.contains { $0.command == "./run-tests.sh" }, "a command's row has the command")
    check(s.info.contextWindow > 0 && s.info.contextTokens > 0, "how full its context is (\(s.info.contextTokens) of \(s.info.contextWindow))")
    // What its other screens ask for.
    _ = await model.changes()
    _ = await model.diff("src/sidebar.rs")
    _ = await model.files(matching: "")
    _ = await model.files(matching: "side")
    _ = await model.file("src/sidebar.rs")
    _ = await model.branches()
    let picker = await model.models()
    check(picker.contains { $0.modelId == "tour-helper" }, "the tour's models in the picker (\(picker.map(\.id)))")
    _ = await model.thinkingLevels()
    _ = await model.actionLog()
    _ = await model.markdown()
    await model.loadUndo()
    let titled = await waitFor(10) { model.sessions.contains { $0.title.hasPrefix("The session list jumps to the top") } }
    check(titled, "the main session is listed by its first message (\(model.sessions.map(\.title)))")
    rec.scene("session")

    // A destructive command waits for approval, and is denied.
    await model.setApproval("always-ask")
    _ = await connected()
    _ = await model.send("Clean up the build folder while you're at it.")
    let approval = await waitFor(20) { s.requests.contains(where: \.approval) }
    check(approval, "a destructive command asks first (\(s.requests.map(\.title)))")
    rec.scene("approval")
    if let a = s.requests.first(where: \.approval) { _ = await model.answer(a, a.approval(false)) }
    _ = await waitFor(20) { !s.busy && s.requests.isEmpty }
    await model.setApproval("yolo")

    // A command still running.
    _ = await connected()
    _ = await model.send("Build the release.")
    let working = await waitFor(20) { s.busy && s.rows.contains { $0.kind == "tool" && $0.subtitle.contains("./build.sh") && $0.state != "done" } }
    check(working, "a command runs (\(s.rows.filter { $0.kind == "tool" }.map(\.subtitle).suffix(2)))")
    rec.scene("working")
    let built = await waitFor(40) { !s.busy && s.rows.contains { $0.html.contains("release build finished") } }
    check(built, "the build ends, and the agent says so")

    // Another model now; earlier replies were the first one's.
    if let helper = await model.models().first(where: { $0.modelId == "tour-helper" }) { await model.setModel(helper) }
    let summed = await turn("Summarize what changed.")
    check(summed && s.info.modelId.hasSuffix("tour-helper"), "the model switched (\(s.info.modelId))")
    _ = await model.changes()
    _ = await model.actionLog()
    rec.scene("end")

    // (From here on: scenes of their own, after the tour's end.)
    // An edit waits for approval with its change to read, and is refused
    // with what to do instead.
    await model.setApproval("always-ask")
    _ = await connected()
    _ = await model.send("Call it refresh instead of rebuild.")
    let edit = await waitFor(20) { s.requests.contains { $0.approval && !$0.diff.isEmpty } }
    let asks = s.requests.first(where: \.approval)
    check(edit && asks?.asks == "Wants to edit a file" && asks?.always.isEmpty == false, "an edit asks first, with its change (\(asks?.asks ?? "-"))")
    rec.scene("edit")
    if let asks { _ = await model.answer(asks, asks.refusal(saying: "Keep the name")) }
    let kept = await waitFor(20) { !s.busy && s.requests.isEmpty && s.rows.last?.html.contains("stays") == true }
    check(kept, "refused with a reason, it carries on (\(s.rows.last?.kind ?? "-"))")
    await model.setApproval("yolo")

    // A goal the reviewer finds blocked: paused, with why.
    _ = await connected()
    _ = await model.send("/goal Ship the release")
    let paused = await waitFor(40) { !s.busy && s.jobs.contains { $0.title == "Goal" && $0.note == "Paused" } }
    check(paused, "a goal pauses when only you can go on (\(s.jobs.map(\.spoken)))")

    // A command the agent waits for, sent to the background.
    _ = await connected()
    _ = await model.send("Build it again while I look.")
    let waits = await waitFor(20) { !s.backgroundCall.isEmpty }
    check(waits, "it waits for a command (\(s.activity?.text ?? "-"))")
    rec.scene("command")
    let moved = await model.sendToBackground()
    let listed = await waitFor(20) { !s.busy && s.jobs.contains { $0.title == "Command" } && s.rows.last?.html.contains("in the background") == true }
    check(moved && listed, "in the background it's listed, and the agent goes on (\(s.jobs.map(\.spoken)))")
    rec.scene("background")
    if let job = s.jobs.first(where: { $0.title == "Command" }) { await model.act(on: job) }
    _ = await waitFor(10) { !s.jobs.contains { $0.title == "Command" } }
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
