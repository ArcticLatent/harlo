import Foundation
import XCTest

@testable import HarloKit

final class HarloKitTests: XCTestCase {
    func testBase64ReadsBothAlphabets() throws {
        let bytes = Data([0xfb, 0xff, 0x01])
        XCTAssertEqual(Base64.encode(bytes), "+/8B")
        XCTAssertEqual(Base64.encodeURL(bytes), "-_8B")
        XCTAssertEqual(try Base64.decode("-_8B"), bytes)
        XCTAssertEqual(try Base64.decode("+/8B"), bytes)
        XCTAssertEqual(try Base64.decode("AQ"), Data([1]))
        XCTAssertThrowsError(try Base64.decode("!!"))
    }

    func testJSONKeepsIntegers() throws {
        let v: JSON = ["c": "x", "s": 3, "f": .double(1.5), "ok": true, "none": nil]
        let text = String(decoding: try v.data(), as: UTF8.self)
        XCTAssertEqual(text, #"{"c":"x","f":1.5,"none":null,"ok":true,"s":3}"#)
        XCTAssertEqual(try JSON.parse(text), v)
        XCTAssertEqual(try JSON.parse("{\"s\": 2.0}")["s"].int, 2)
        XCTAssertEqual(v["missing"].text, "")
    }

    func testBoxBothWaysAndTampering() throws {
        let phone = KeyPair.generate(), harlo = KeyPair.generate()
        XCTAssertEqual(try KeyPair(secret: phone.secret).publicKey, phone.publicKey)
        let ours = try Box(theirPublic: harlo.publicKey, ours: phone)
        let theirs = try Box(theirPublic: phone.publicKey, ours: harlo)
        let frame = ours.seal(Data("hello".utf8))
        XCTAssertEqual(try theirs.open(frame), Data("hello".utf8))
        XCTAssertEqual(try Base64.decode(frame).count, 24 + 16 + 5)
        // A flipped bit, or a stranger's key: doesn't open.
        var raw = [UInt8](try Base64.decode(frame))
        raw[30] ^= 1
        XCTAssertThrowsError(try theirs.open(Base64.encode(Data(raw))))
        let stranger = try Box(theirPublic: harlo.publicKey, ours: KeyPair.generate())
        XCTAssertThrowsError(try theirs.open(stranger.seal(Data("x".utf8))))
        XCTAssertThrowsError(try theirs.open(Base64.encode(Data(count: 30))))
        XCTAssertThrowsError(try KeyPair(secret: Data(count: 31)))
    }

    func testChannelRefusesReplaysAndOtherConnections() throws {
        let phone = KeyPair.generate(), harlo = KeyPair.generate()
        var mine = Channel(box: try Box(theirPublic: harlo.publicKey, ours: phone), id: "conn1")
        var theirs = Channel(box: try Box(theirPublic: phone.publicKey, ours: harlo), id: "conn1")
        let f1 = try mine.seal(["op": "sessions"])
        XCTAssertEqual(try theirs.open(f1), ["op": "sessions"])
        XCTAssertThrowsError(try theirs.open(f1))
        let f2 = try mine.seal(["op": "ping"])
        XCTAssertEqual(try theirs.open(f2)["op"], "ping")
        var other = Channel(box: try Box(theirPublic: harlo.publicKey, ours: phone), id: "conn2")
        XCTAssertThrowsError(try theirs.open(try other.seal([:])))
    }

    func testOffersFromLinks() throws {
        let key = Base64.encode(KeyPair.generate().publicKey)
        let offer = { (extra: JSON) -> String in
            var v: JSON = ["v": 1, "k": .string(key), "c": "code1", "n": "desk"]
            for (k, x) in extra.object ?? [:] { v[k] = x }
            return Base64.encodeURL(try! v.data())
        }
        let direct = try PairingOffer(scanned: "http://192.168.1.5:7676/#p=\(offer([:]))")
        XCTAssertEqual(direct.endpoint?.absoluteString, "ws://192.168.1.5:7676/ws")
        XCTAssertEqual([direct.code, direct.name, direct.key], ["code1", "desk", key])
        let funnel = try PairingOffer(scanned: " https://desk.tail1234.ts.net:8443/#p=\(offer([:])) ")
        XCTAssertEqual(funnel.endpoint?.absoluteString, "wss://desk.tail1234.ts.net:8443/ws")
        let relay = try PairingOffer(scanned: "https://relay.example.com/#p=\(offer(["r": "wss://relay.example.com/", "i": "abc"]))")
        XCTAssertEqual(relay.endpoint?.absoluteString, "wss://relay.example.com/ws?id=abc")
        XCTAssertEqual(try PairingOffer(scanned: offer([:])).key, key)
        XCTAssertNil(try PairingOffer(scanned: offer([:])).endpoint)
        XCTAssertThrowsError(try PairingOffer(scanned: "https://example.com/"))
        XCTAssertThrowsError(try PairingOffer(scanned: "https://example.com/#p=nonsense"))
        XCTAssertThrowsError(try PairingOffer(scanned: offer(["v": 2])))
        XCTAssertEqual(fingerprint(key).count, 19)
    }

    /// Among equally good ways in (two home addresses here; see
    /// testWaysInBestFirst for the ranks).
    func testPairedHarloTriesTheLastGoodEndpointFirst() {
        let a = URL(string: "ws://192.168.1.5:7676/ws")!, b = URL(string: "ws://10.0.0.2:7676/ws")!
        var h = PairedHarlo(key: "k", name: "desk", endpoints: [a])
        h.add(b)
        h.add(a)
        XCTAssertEqual(h.tryOrder, [a, b])
        h.lastEndpoint = b
        XCTAssertEqual(h.tryOrder, [b, a])
    }

    func testRowChangesApply() {
        let row = { (k: String) in Row(key: k, kind: "markdown") }
        var rows = [row("a"), row("b"), row("c")]
        rows.apply([.update(at: 1, row: row("B")), .remove(at: 0, count: 1), .insert(at: 2, rows: [row("d")])])
        XCTAssertEqual(rows.map(\.key), ["B", "c", "d"])
        // Out of range: ignored or clamped, never a crash.
        rows.apply([.update(at: 9, row: row("x")), .remove(at: 2, count: 5), .insert(at: 99, rows: [row("e")])])
        XCTAssertEqual(rows.map(\.key), ["B", "c", "e"])
    }

    func testEventsAndRequests() throws {
        let ev = Event(try JSON.parse(#"""
            {"ev":"request","key":"live1","request":{"id":"ui_1","method":"ask","approval":false,"options":[],
             "questions":[{"id":"db","question":"Which database?","options":[{"label":"SQLite"},"Postgres"],"multi":true}]}}
            """#))
        guard case .request(let key, let req)? = ev else { return XCTFail("not a request: \(String(describing: ev))") }
        XCTAssertEqual(key, "live1")
        XCTAssertEqual(req.questions.first?.options, ["SQLite", "Postgres"])
        XCTAssertEqual(req.questions.first?.multi, true)
        let answer = UIRequest.askAnswer(req.questions, picked: [["SQLite"]], typed: ["  "])
        XCTAssertEqual(answer, ["answers": [["id": "db", "selectedOptions": ["SQLite"]]]])
        let rows = Event(try JSON.parse(#"{"ev":"rows","key":"k","changes":[{"op":"insert","at":0,"rows":[{"key":"r1","kind":"tool","group":"r1","summary":"Ran 2 commands","groupOpen":true}]},{"op":"bogus"}]}"#))
        guard case .rows(_, let changes)? = rows, case .insert(_, let inserted)? = changes.first else { return XCTFail("not rows") }
        XCTAssertEqual(changes.count, 1)
        XCTAssertEqual([inserted[0].summary, inserted[0].group], ["Ran 2 commands", "r1"])
        XCTAssertTrue(inserted[0].groupOpen)
        XCTAssertEqual(Event(["ev": "revoked"]), .revoked)
        XCTAssertNil(Event(["ev": "later"]))
        let s = SessionSummary.list(try JSON.parse(#"[{"key":"a","title":"T","where":"~/p","modified":5,"status":"busy"}]"#))
        XCTAssertEqual([s[0].location, s[0].status], ["~/p", "busy"])
        XCTAssertEqual(s[0].modified, 5)
    }
}

final class RichTextTests: XCTestCase {
    func testParagraphsAndInlineStyles() {
        let b = RichText.parse("<p>Hello <strong>world</strong> &amp; <code>x &lt; y</code>, <em>see</em> <a href=\"https://x.dev\">this</a>\nand <a href=\"javascript:alert(1)\">that</a>.</p>")
        guard case .paragraph(let spans)? = b.first, b.count == 1 else { return XCTFail("\(b)") }
        XCTAssertEqual(RichText.plain(spans), "Hello world & x < y, see this and that.")
        XCTAssertEqual(spans.first { $0.bold }?.text, "world")
        XCTAssertEqual(spans.first { $0.code }?.text, "x < y")
        XCTAssertEqual(spans.first { $0.italic }?.text, "see")
        XCTAssertEqual(spans.first { $0.link != nil }?.link, "https://x.dev")
        XCTAssertEqual(spans.filter { $0.link != nil }.count, 1, "javascript: links are plain text")
    }

    func testListsQuotesHeadingsRules() {
        let b = RichText.parse("<ol start=\"3\">\n<li>three</li>\n<li>four\n<ul>\n<li>nested <strong>x</strong></li>\n</ul>\n</li>\n</ol>")
        guard case .list(true, 3, let items)? = b.first else { return XCTFail("\(b)") }
        XCTAssertEqual(items.count, 2)
        XCTAssertEqual(items[1].count, 2)
        guard case .list(false, _, let inner) = items[1][1], case .paragraph(let s)? = inner.first?.first else { return XCTFail("\(items[1])") }
        XCTAssertEqual(RichText.plain(s), "nested x")
        // Loose list items (paragraphs inside).
        let loose = RichText.parse("<ul>\n<li>\n<p>one</p>\n</li>\n<li>\n<p>two</p>\n</li>\n</ul>")
        guard case .list(false, 1, let li)? = loose.first else { return XCTFail("\(loose)") }
        XCTAssertEqual(li.map { $0.count }, [1, 1])
        XCTAssertEqual(RichText.parse("<h2>Title <code>x</code></h2>"), [.heading(level: 2, [Span("Title "), Span("x", code: true)])])
        guard case .quote(let q)? = RichText.parse("<blockquote>\n<p>quoted</p>\n</blockquote>").first else { return XCTFail() }
        XCTAssertEqual(q, [.paragraph([Span("quoted")])])
        XCTAssertEqual(RichText.parse("<hr />"), [.rule])
        XCTAssertEqual(RichText.parse("plain <b>text</b>"), [.paragraph([Span("plain "), Span("text", bold: true)])])
    }

    func testCodeKeepsLinesAndTokens() {
        let b = RichText.parse("<pre style=\"margin:0;\"><span class=\"s2\">fn</span> main() {\n    <span class=\"s3\">&quot;hi&quot;</span>\n}\n</pre>")
        guard case .code(let spans)? = b.first else { return XCTFail("\(b)") }
        XCTAssertEqual(RichText.plain(spans), "fn main() {\n    \"hi\"\n}")
        XCTAssertEqual(spans.first?.token, 2)
        XCTAssertEqual(spans.first { $0.token == 3 }?.text, "\"hi\"")
        XCTAssertTrue(spans.allSatisfy { $0.code })
        // Markdown's own code block (pre > code).
        guard case .code(let md)? = RichText.parse("<pre><code>a  b\n</code></pre>").first else { return XCTFail() }
        XCTAssertEqual(RichText.plain(md), "a  b")
    }

    func testTables() {
        let html = "<table border=\"1\" cellspacing=\"0\" cellpadding=\"6\" style=\"border-collapse: collapse;\"><thead><tr><th>A</th><th>B</th></tr></thead><tbody>\n<tr><td>1</td><td><code>2</code></td></tr>\n</tbody></table>"
        guard case .table(let head, let rows)? = RichText.parse(html).first else { return XCTFail() }
        XCTAssertEqual(head.map(RichText.plain), ["A", "B"])
        XCTAssertEqual(rows.map { $0.map(RichText.plain) }, [["1", "2"]])
    }

    func testBrokenHTMLDoesNotLoseText() {
        XCTAssertEqual(RichText.plain(paragraph(RichText.parse("<p>a < b and <unknown>c</p>"))), "a < b and c")
        XCTAssertEqual(RichText.plain(paragraph(RichText.parse("<p>open <strong>bold"))), "open bold")
        XCTAssertEqual(RichText.plain(paragraph(RichText.parse("<p>&#x1F600; &#65; &bogus; &</p>"))), "😀 A &bogus; &")
    }

    private func paragraph(_ b: [RichBlock]) -> [Span] {
        if case .paragraph(let s)? = b.first { return s }
        return []
    }
}

final class ModelPartsTests: XCTestCase {
    func testRunsFold() {
        let rows = [
            Row(key: "u", kind: "user"),
            Row(key: "t", kind: "thinking", group: "t", summary: "Thought, ran 1 command"),
            Row(key: "c", kind: "tool", group: "t"),
            Row(key: "m", kind: "markdown"),
            Row(key: "x", kind: "tool", group: "x", summary: "Ran 2 commands", groupOpen: true),
            Row(key: "y", kind: "tool", group: "x", groupOpen: true),
        ]
        let items = TranscriptItem.build(rows, opened: [:])
        XCTAssertEqual(items.map(\.id), ["u", "run:t", "m", "run:x"])
        guard case .run(_, let summary, let run, let open, let active) = items[1] else { return XCTFail() }
        XCTAssertEqual([summary, String(run.count), String(open), String(active)], ["Thought, ran 1 command", "2", "false", "false"])
        guard case .run(_, _, _, true, true) = items[3] else { return XCTFail("a run still going shows open") }
        let byHand = TranscriptItem.build(rows, opened: ["t": true, "x": false])
        guard case .run(_, _, _, true, _) = byHand[1], case .run(_, _, _, false, _) = byHand[3] else { return XCTFail("folded by hand") }
    }

    func testApprovalAnswers() {
        let omp = UIRequest(["id": "1", "method": "select", "approval": true, "options": ["Approve", "Deny"]])
        XCTAssertEqual(omp.approval(true), ["value": "Approve"])
        XCTAssertEqual(omp.approval(false), ["value": "Deny"])
        let plain = UIRequest(["id": "2", "method": "confirm"])
        XCTAssertEqual(plain.approval(true), ["confirmed": true])
        XCTAssertEqual(UIRequest.role(of: "Allow tool"), .allow)
        XCTAssertEqual(UIRequest.role(of: "Reject"), .deny)
        XCTAssertEqual(UIRequest.role(of: "Notes"), .plain)
        XCTAssertEqual(UIRequest.role(of: "No, keep it"), .deny)
    }

    /// iOS fails the first LAN connection as "offline" while it asks for
    /// Local Network access; pairing retries those, not refusals.
    func testOfflineIsTold() {
        XCTAssertTrue(HarloClient.isOffline(URLError(.notConnectedToInternet)))
        XCTAssertTrue(HarloClient.isOffline(URLError(.dataNotAllowed)))
        XCTAssertFalse(HarloClient.isOffline(URLError(.cannotConnectToHost)))
        XCTAssertFalse(HarloClient.isOffline(URLError(.cancelled)))
        XCTAssertFalse(HarloClient.isOffline(HarloError.denied("no")))
    }

    func testColoursAsHarloSendsThem() {
        XCTAssertEqual(RGBA(hex: "#ff0000"), RGBA(1, 0, 0))
        // Qt's #AARRGGBB.
        let c = RGBA(hex: "#80ffffff")!
        XCTAssertEqual(c.a, 128.0 / 255, accuracy: 0.001)
        XCTAssertEqual(c.over(RGBA(0, 0, 0)).r, 128.0 / 255, accuracy: 0.001)
        XCTAssertNil(RGBA(hex: "red"))
        let colors: JSON = ["surface": ["background": "#ff101010", "foreground": "#ffeeeeee"], "primary": ["base": "#ffc6a0f6"],
                            "syntax": ["base": ["keyword": "#ffc6a0f6"]]]
        let p = Palette(colors, light: false)
        XCTAssertNotNil(p)
        XCTAssertEqual(p?.token(2), RGBA(hex: "#ffc6a0f6"))
        XCTAssertEqual(p?.status("busy"), p?.primary)
        XCTAssertNil(Palette(["surface": ["background": "#ff101010"]], light: true))
    }

    func testSlashCommandsMatchWhatsTyped() {
        let all = [SlashCommand(name: "plan"), SlashCommand(name: "compact"), SlashCommand(name: "explain-plan")]
        XCTAssertEqual(SlashCommand.matching(all, typed: "/pl").map(\.name), ["plan", "explain-plan"])
        XCTAssertEqual(SlashCommand.matching(all, typed: "/").count, 3)
        XCTAssertTrue(SlashCommand.matching(all, typed: "/plan now").isEmpty)
        XCTAssertTrue(SlashCommand.matching(all, typed: "plan").isEmpty)
    }

    func testChangesInAFolderOfRepositories() {
        let c = Changes(["branch": "", "repos": [["name": "alpha", "branch": "main", "changes": 2, "ahead": 1], ["name": "group/beta", "branch": "dev", "changes": 1], ["name": "gamma", "branch": "main", "changes": 0]],
                         "files": [["path": "alpha/README.md", "status": "M"], ["path": "alpha/notes.md", "status": "?", "untracked": true], ["path": "group/beta/x.rs", "status": "M"]]])
        XCTAssertEqual(c.repos.map(\.name), ["alpha", "group/beta", "gamma"])
        XCTAssertEqual(c.files(in: c.repos[0]).map(\.name), ["README.md", "notes.md"])
        XCTAssertEqual(c.files(in: c.repos[1]).map(\.path), ["group/beta/x.rs"])
        XCTAssertTrue(c.files(in: c.repos[2]).isEmpty)
        XCTAssertTrue(Changes(["branch": "main", "files": []]).repos.isEmpty, "one repository: no list")
    }

    func testActionLog() {
        let log = ActionLog(["commands": 1, "failed": 1, "changed": ["a.rs"], "read": [], "actions": [
            ["ts": 1000, "kind": "edit", "tool": "edit", "target": "a.rs", "result": "+1 −0"],
            ["ts": nil, "kind": "command", "tool": "bash", "target": "make", "result": "exit 2", "error": true],
            ["kind": "edit", "tool": "patch", "target": "x.rs, y.rs", "pending": true]]])
        XCTAssertEqual(log.entries.map(\.kind), ["edit", "command", "edit"])
        XCTAssertEqual(log.entries[0].time, 1000)
        XCTAssertNil(log.entries[1].time)
        XCTAssertTrue(log.entries[1].failed && log.entries[2].running)
        XCTAssertEqual(log.entries.map(\.isFile), [true, false, false], "a patch of several files doesn't open one")
        XCTAssertEqual([log.commands, log.failed], [1, 1])
        XCTAssertEqual(log.changed, ["a.rs"])
    }

    func testNewSessionChoices() {
        let c = NewSessionChoices(["projects": [["id": 3, "name": "harlo", "where": "~/harlo", "agent": "omp", "approvals": ["claude": "write", "pi": ""]]],
                                   "agents": [["id": "claude", "label": "Claude"], ["id": "pi", "label": "Pi"]], "defaultAgent": "pi",
                                   "approvals": ["claude": "always-ask", "pi": ""],
                                   "models": [["harness": "claude", "model": "a/x"], ["harness": "pi", "model": "b/y"], ["harness": "claude", "model": "a/x"]]])
        // omp isn't offered: the first agent that is.
        XCTAssertEqual(c.agent(for: c.projects.first), "claude")
        XCTAssertEqual(c.agent(for: nil), "pi")
        XCTAssertEqual(c.models(for: "claude"), ["a/x"])
        XCTAssertEqual(c.approval(for: "claude", in: nil), "always-ask")
        XCTAssertEqual(c.approval(for: "claude", in: c.projects.first), "write")
        XCTAssertEqual(c.approval(for: "pi", in: nil), "")
        XCTAssertTrue(c.templates.isEmpty)
        // Another machine's project: its agents only.
        let m = NewSessionChoices(["projects": [["id": 1, "name": "web", "agent": "claude"], ["id": 2, "name": "pipeline", "host": "nas", "agent": "claude", "agents": ["pi", "omp"]],
                                                ["id": 3, "name": "tools", "host": "box"]],
                                   "agents": [["id": "claude", "label": "Claude"], ["id": "pi", "label": "Pi"], ["id": "omp", "label": "Oh My Pi"]], "defaultAgent": "claude"])
        XCTAssertEqual(m.agents(for: m.projects[1]).map(\.id), ["pi", "omp"])
        XCTAssertEqual(m.agent(for: m.projects[1]), "pi", "claude isn't on nas: its first agent")
        XCTAssertEqual(m.agents(for: m.projects[2]).count, 3, "not checked yet: all")
        XCTAssertEqual(m.machines.map(\.host), ["", "box", "nas"], "this computer first")
        let t = NewSessionChoices(["templates": [["id": "t1", "name": "Review", "harness": "claude", "model": "a/x", "approval": "write", "text": "Review it"],
                                                 ["id": "", "name": "no id"], ["id": "t3", "name": ""]]]).templates
        XCTAssertEqual(t.map(\.name), ["Review"], "only complete templates")
        XCTAssertEqual([t[0].agent, t[0].model, t[0].approval, t[0].text], ["claude", "a/x", "write", "Review it"])
    }

    func testTheDefaultThemeAndTheSample() {
        XCTAssertEqual(Theme.standard.family, "harlo")
        XCTAssertEqual(Theme.standard.dark.primary, RGBA(hex: "#5b9bf0"))
        XCTAssertTrue(Theme.standard.light.isLight)
    }

    func testThemeAvatar() throws {
        var v = try JSON.parse(#"{"family":"harlo","mode":"dark","avatar":"glasses"}"#)
        v["dark"] = try JSON.parse(##"{"surface":{"background":"#000000","foreground":"#ffffff"},"primary":{"base":"#3366ff"}}"##)
        v["light"] = v["dark"]
        XCTAssertEqual(Theme(v)?.penguin, "glasses")
        v["avatar"] = "unicorn"
        XCTAssertEqual(Theme(v)?.penguin, "classic", "an unknown one: the default")
        let saved = try JSONEncoder().encode(Theme(v)!)
        XCTAssertEqual(try JSONDecoder().decode(Theme.self, from: saved).penguin, "classic")
    }

    @MainActor
    func testSample() {
        let m = HarloModel.sample()
        XCTAssertEqual(m.state, .connected)
        XCTAssertEqual(m.sessions.count, 6)
        m.openSample(asking: true)
        XCTAssertEqual(m.session?.requests.count, 1)
        XCTAssertTrue(m.session?.items.contains { if case .run = $0 { return true }; return false } ?? false)
        XCTAssertFalse(RichText.parse(m.session?.rows.first { $0.kind == "code" }?.html ?? "").isEmpty)
    }

    @MainActor func testWhatNeedsYou() {
        let m = HarloModel.sample()
        let later: JSON = ["id": "q2", "method": "ask", "questions": [["id": "a", "question": "Which?", "options": ["x", "y"]]], "asked": 9_000_000_000_000]
        var list = m.sessions
        list[0] = SessionSummary(["key": "live:1", "title": "One", "harness": "omp", "requests": [later]])
        m.sessions = list
        XCTAssertEqual(m.waiting.map(\.request.id), ["r1", "q2"], "oldest first")
        XCTAssertEqual(m.waiting.first?.session.key, "live:2")
        XCTAssertEqual(m.waiting.first?.request.approval(true), ["value": "Approve"])
        // Answered from the list: hidden until harlo drops it, then forgotten.
        m.answered.insert("r1")
        XCTAssertEqual(m.waiting.map(\.request.id), ["q2"])
        list[1].requests = []
        m.sessions = list
        XCTAssertTrue(m.answered.isEmpty)
    }

    func testHarlosAddresses() throws {
        // A pairing code with the other addresses.
        let raw: JSON = ["v": 1, "k": .string(Base64.encode(Data(repeating: 1, count: 32))), "c": "x",
                         "e": ["ws://100.64.0.2:7676/ws", "ws://192.168.1.5:7676/ws"]]
        let offer = try PairingOffer(encoded: Base64.encodeURL(try raw.data()), page: URL(string: "http://192.168.1.5:7676"))
        XCTAssertEqual(offer.endpoints.map(\.absoluteString), ["ws://192.168.1.5:7676/ws", "ws://100.64.0.2:7676/ws"])
        // harlo's list wins for its own addresses; a relay added by hand stays.
        var h = PairedHarlo(key: "k", name: "", endpoints: [URL(string: "ws://192.168.1.5:7676/ws")!, URL(string: "ws://10.0.0.9:7676/ws")!,
                                                            URL(string: "wss://relay.example/ws?id=a")!])
        h.lastEndpoint = h.endpoints[0]
        h.learn([URL(string: "ws://100.64.0.2:7676/ws")!, URL(string: "ws://192.168.1.5:7676/ws")!])
        XCTAssertEqual(h.endpoints.map(\.absoluteString), ["ws://192.168.1.5:7676/ws", "wss://relay.example/ws?id=a", "ws://100.64.0.2:7676/ws"])
        h.learn([])
        XCTAssertEqual(h.endpoints.count, 3)
    }

    func testWaysInBestFirst() {
        let u = { (s: String) in URL(string: s)! }
        XCTAssertEqual(Endpoints.rank(u("ws://192.168.1.5:7676/ws")), 0)
        XCTAssertEqual(Endpoints.rank(u("ws://10.0.0.2:7676/ws")), 0)
        XCTAssertEqual(Endpoints.rank(u("ws://100.101.2.3:7676/ws")), 1)
        XCTAssertEqual(Endpoints.rank(u("wss://desk.tail1.ts.net:8443/ws")), 2)
        XCTAssertEqual(Endpoints.rank(u("wss://relay.example/ws?id=x")), 3)
        var h = PairedHarlo(key: "k", name: "", endpoints: [u("wss://relay.example/ws?id=x"), u("ws://100.101.2.3:7676/ws"), u("ws://192.168.1.5:7676/ws")])
        h.lastEndpoint = h.endpoints[0]
        XCTAssertEqual(h.tryOrder.map { $0.host ?? "" }, ["192.168.1.5", "100.101.2.3", "relay.example"])
    }

    func testPathsInRepliesOpen() {
        func p(_ t: String) -> String? { FileLinks.path(t).map { $0.line > 0 ? "\($0.path):\($0.line)" : $0.path } }
        XCTAssertEqual(p("src/app.rs:42"), "src/app.rs:42")
        XCTAssertEqual(p("crates/x/src/lib.rs:10:5"), "crates/x/src/lib.rs:10")
        XCTAssertEqual(p("Cargo.toml"), "Cargo.toml")
        XCTAssertEqual(p("./web/index.ts#L7"), "./web/index.ts:7")
        XCTAssertEqual(p("ios/Harlo/Makefile"), "ios/Harlo/Makefile")
        for not in ["self.rows", "v1.2.3", "rebuild()", "https://a.b/c.rs", "a b.rs", "--release", "crates/", "1.5", "foo:bar.rs", "@x/y.ts", "x = 1"] {
            XCTAssertNil(p(not), not)
        }
        let url = FileLinks.url("src/a b.rs", line: 3)!
        XCTAssertEqual(FileLinks.target(url)?.path, "src/a b.rs")
        XCTAssertEqual(FileLinks.target(url)?.line, 3)
        XCTAssertNil(FileLinks.target(URL(string: "https://example.com")!))
        // In a reply: a code span that's a path, and a word with a folder.
        let blocks = FileLinks.link(RichText.parse("<p>See <code>Cargo.toml</code> and crates/a/src/b.rs:9. Not <code>self.x</code> or a/b.</p><pre><code>src/c.rs</code></pre>"))
        guard case .paragraph(let spans) = blocks.first else { return XCTFail("a paragraph") }
        XCTAssertEqual(spans.compactMap { $0.link.flatMap { URL(string: $0) }.flatMap(FileLinks.target)?.path }, ["Cargo.toml", "crates/a/src/b.rs"])
        XCTAssertEqual(RichText.plain(spans), "See Cargo.toml and crates/a/src/b.rs:9. Not self.x or a/b.")
        guard case .code(let code) = blocks.last else { return XCTFail("code") }
        XCTAssertTrue(code.allSatisfy { $0.link == nil }, "code blocks stay as they are")
    }

    func testLongCodeFolds() {
        let spans = [Span("a\nb\n", code: true, token: 2), Span("c\nd", code: true)]
        XCTAssertEqual(RichText.lineCount(spans), 4)
        XCTAssertEqual(RichText.plain(RichText.firstLines(spans, 2)), "a\nb")
        XCTAssertEqual(RichText.plain(RichText.firstLines(spans, 3)), "a\nb\nc")
        XCTAssertEqual(RichText.plain(RichText.firstLines(spans, 9)), "a\nb\nc\nd")
        XCTAssertEqual(RichText.firstLines(spans, 1).first?.token, 2)
    }

    func testNewSessionModelsUsedFirstThenAll() {
        let c = NewSessionChoices(["models": [["harness": "omp", "model": "openai/gpt-5"]],
                                   "agentModels": ["omp": [["id": "anthropic/claude-sonnet-4-5", "name": "Claude Sonnet 4.5"], ["id": "openai/gpt-5", "name": "GPT-5"]]]])
        XCTAssertEqual(c.modelChoices(for: "omp").map(\.id), ["openai/gpt-5", "anthropic/claude-sonnet-4-5"])
        XCTAssertEqual(c.modelChoices(for: "omp").first?.name, "GPT-5")
        XCTAssertTrue(c.modelChoices(for: "pi").isEmpty)
    }

    func testMentions() {
        XCTAssertEqual(Mention.query(in: "look at @src/ma"), "src/ma")
        XCTAssertEqual(Mention.query(in: "@"), "")
        XCTAssertNil(Mention.query(in: "mail me@example.com"))
        XCTAssertNil(Mention.query(in: "@src/a.rs and"))
        XCTAssertEqual(Mention.insert("src/main.rs", into: "look at @src/ma"), "look at @src/main.rs ")
    }

    func testFilesAndBranchesAsHarloSendsThem() {
        let f = FileText(["path": "a.rs", "total": 9000, "cut": true, "lines": ["<span class=\"s2\">fn</span> main() {}"]])
        XCTAssertEqual(f.spans(0).first, Span("fn", code: true, token: 2))
        XCTAssertEqual(RichText.plain(f.spans(0)), "fn main() {}")
        XCTAssertTrue(f.cut)
        let b = Branches(["current": "main", "changed": 2, "local": [["name": "main", "ahead": 1, "date": 5]], "remote": [["name": "origin/x"]]])
        XCTAssertEqual(b.local.first?.ahead, 1)
        XCTAssertEqual(b.remote.map(\.name), ["origin/x"])
        XCTAssertEqual(b.changed, 2)
    }

    func testSessionsByDay() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let now = Date(timeIntervalSince1970: 1_760_000_000)  // 2025-10-09 08:53 UTC
        let t = Int64(now.timeIntervalSince1970)
        func s(_ key: String, _ ago: Int64, status: String = "", pinned: Bool = false) -> SessionSummary {
            SessionSummary(["key": .string(key), "modified": .int(t - ago), "status": .string(status), "pinned": .bool(pinned)])
        }
        let groups = SessionSummary.byDay([s("old", 86400 * 40), s("y", 86400), s("now", 60), s("pin", 86400 * 3, pinned: true), s("week", 86400 * 3),
                                           s("busy", 86400 * 9, status: "busy"), s("month", 86400 * 20)], now: now, calendar: cal)
        XCTAssertEqual(groups.map(\.name), ["Pinned", "Today", "Yesterday", "Previous 7 days", "Previous 30 days", "Older"])
        XCTAssertEqual(groups[1].sessions.map(\.key), ["now", "busy"])
    }

    @MainActor func testMarkedUnreadUntilOpened() {
        let m = HarloModel.sample()
        let s = m.sessions[3]
        XCTAssertFalse(m.isUnread(s))
        XCTAssertTrue(m.isUnread(m.sessions[2]), "harlo's unread")
        m.markUnread(s, true)
        XCTAssertTrue(m.isUnread(s))
        m.open(s)
        XCTAssertFalse(m.isUnread(s))
    }

    func testLiveActivityWordsAreSealed() throws {
        let harlo = KeyPair.generate(), phone = KeyPair.generate()
        let box = try Box(theirPublic: phone.publicKey, ours: harlo)
        let sealed = box.seal(Data(#"{"now":"Running cargo test","title":"Fix it"}"#.utf8))
        let store = MemoryStore()
        store.save("identity", phone.secret)
        store.save("host", try JSONEncoder().encode(PairedHarlo(key: Base64.encode(harlo.publicKey), name: "pc", endpoints: [])))
        XCTAssertEqual(LiveDetail.open(sealed, store: store), LiveDetail(["now": "Running cargo test", "title": "Fix it"]))
        XCTAssertNil(LiveDetail.open("", store: store))
        XCTAssertNil(LiveDetail.open(sealed, store: MemoryStore()))
    }
}

@MainActor
final class DemoTests: XCTestCase {
    func testTheDemoWorksWithoutAHarlo() async throws {
        let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        m.startDemo()
        XCTAssertTrue(m.demo)
        XCTAssertEqual(m.state, .connected)
        XCTAssertEqual(m.host?.name, HarloModel.demoName)
        XCTAssertEqual(m.waiting.count, 2, "two approvals waiting")
        // Never connects, never saves a pairing.
        m.resume()
        XCTAssertNil(m.loop)
        XCTAssertNil(m.store.load("host"))

        // Opening one: what it asked still waits; answering it.
        let asking = try XCTUnwrap(m.sessions.first { $0.status == "input" })
        m.open(asking)
        let s = try XCTUnwrap(m.session)
        XCTAssertEqual(s.requests.count, 1)
        await m.answer(s.requests[0], s.requests[0].approval(true))
        XCTAssertTrue(s.requests.isEmpty)
        XCTAssertEqual(s.rows.last?.text, "Approved: the agent goes on.")
        XCTAssertEqual(m.waiting.count, 1)

        // A message: it shows; a reply follows.
        let before = s.rows.count
        let sent = await m.send("Add a test")
        XCTAssertTrue(sent)
        XCTAssertEqual(s.rows.count, before + 1)
        XCTAssertEqual(s.status, "busy")
        try await Task.sleep(nanoseconds: 1_500_000_000)
        XCTAssertEqual(s.rows.count, before + 2)
        XCTAssertEqual(s.status, "")

        // A new session: on the list, open.
        let started = await m.startSession(project: nil, agent: "claude", model: "", text: "Write the README")
        XCTAssertTrue(started)
        XCTAssertEqual(m.sessions.first?.title, "Write the README")
        XCTAssertEqual(m.session?.rows.first?.text, "Write the README")

        // From the list: a waiting one answered without opening it.
        let other = try XCTUnwrap(m.waiting.first)
        await m.answer(other.request, other.request.approval(false), in: other.session.key)
        XCTAssertTrue(m.waiting.isEmpty)

        // Leaving it: back to pairing.
        m.forget()
        XCTAssertFalse(m.demo)
        XCTAssertNil(m.host)
        XCTAssertEqual(m.state, .unpaired)
        XCTAssertTrue(m.sessions.isEmpty)
    }
}

@MainActor
final class WidgetTests: XCTestCase {
    func testTheWidgetShowsWhatNeedsYouFirst() {
        let m = HarloModel.sample()
        let snap = WidgetSnapshot.of(host: "box", sessions: m.sessions, unread: { m.isUnread($0) })
        XCTAssertEqual(snap.items.first?.status, "input", "waiting first")
        XCTAssertEqual(snap.items.map(\.status).prefix(3), ["input", "busy", "unread"])
        XCTAssertEqual(snap.working, 1)
        XCTAssertEqual(snap.waiting, 1)
        XCTAssertEqual(snap.total, 6)
    }

    func testANotificationMovesItsSessionToTheFront() {
        var snap = WidgetSnapshot.sample()
        let done = PushContent(["k": "done", "s": "s5", "t": "Tidy the README", "b": ""])
        snap.apply(done)
        XCTAssertEqual(snap.items.first?.key, "s5")
        XCTAssertEqual(snap.items.first?.status, "unread")
        snap.apply(PushContent(["k": "ask", "s": "new", "t": "Deploy", "b": ""]))
        XCTAssertEqual(snap.items.first?.key, "new")
        XCTAssertEqual(snap.waiting, 2)
        XCTAssertEqual(snap.total, 7)
    }

    func testTheAppKeepsItAndNotForTheDemo() {
        let store = MemoryStore()
        let m = HarloModel(store: store, deviceName: "iPhone")
        var reloads = 0
        m.widgetChanged = { reloads += 1 }
        m.host = PairedHarlo(key: "k", name: "box", endpoints: [])
        m.sessions = HarloModel.sample().sessions
        XCTAssertEqual(WidgetSnapshot.load(store)?.host, "box")
        XCTAssertEqual(reloads, 1)
        m.sessions = HarloModel.sample().sessions
        XCTAssertEqual(reloads, 1, "the same: not written again")
        m.forget()
        XCTAssertEqual(WidgetSnapshot.load(store)?.items.count, 0, "unpaired: empty")
        let demo = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        demo.startDemo()
        XCTAssertNil(WidgetSnapshot.load(demo.store))
    }
}

final class ReviewTests: XCTestCase {
    func testCommentsBecomeOneMessage() {
        let m = ReviewComment.message([
            ReviewComment(path: "src/b.rs", side: "new", line: 7, code: "  let x = `y`;", text: "Name it better"),
            ReviewComment(path: "src/a.rs", side: "old", line: 3, code: "check()", text: "Keep this\nit matters"),
        ])
        XCTAssertEqual(m, """
        Review comments on your changes. Please address each one:

        - `src/a.rs:3` (a line you removed): `check()`
          Keep this
          it matters
        - `src/b.rs:7`: `let x = 'y';`
          Name it better
        """)
    }
}
