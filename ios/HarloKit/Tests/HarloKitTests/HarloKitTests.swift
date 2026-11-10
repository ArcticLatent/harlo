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
        // A reply's prose comes as one row: its blocks one after another.
        let reply = RichText.parse("<p>Done:</p>\n<ul>\n<li>one</li>\n</ul>\n<h2>Next</h2>\n<p>More.</p>")
        XCTAssertEqual(reply.count, 4, "\(reply)")
        XCTAssertEqual(reply.last, .paragraph([Span("More.")]))
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
        let options = UIRequest(["id": "1", "method": "select", "approval": true, "options": ["Approve", "Deny"]])
        XCTAssertEqual(options.approval(true), ["value": "Approve"])
        XCTAssertEqual(options.approval(false), ["value": "Deny"])
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
        let c = NewSessionChoices(["projects": [["id": 3, "name": "harlo", "where": "~/harlo", "agent": "other", "approvals": ["claude": "write", "harlo": ""]]],
                                   "agents": [["id": "claude", "label": "Claude"], ["id": "harlo", "label": "Harlo"]], "defaultAgent": "harlo",
                                   "approvals": ["claude": "always-ask", "harlo": ""],
                                   "models": [["harness": "claude", "model": "a/x"], ["harness": "harlo", "model": "b/y"], ["harness": "claude", "model": "a/x"]]])
        // other isn't offered: the first agent that is.
        XCTAssertEqual(c.agent(for: c.projects.first), "claude")
        XCTAssertEqual(c.agent(for: nil), "harlo")
        XCTAssertEqual(c.models(for: "claude"), ["a/x"])
        XCTAssertEqual(c.approval(for: "claude", in: nil), "always-ask")
        XCTAssertEqual(c.approval(for: "claude", in: c.projects.first), "write")
        XCTAssertEqual(c.approval(for: "harlo", in: nil), "")
        XCTAssertTrue(c.templates.isEmpty)
        // Another machine's project: its agents only.
        let m = NewSessionChoices(["projects": [["id": 1, "name": "web", "agent": "claude"], ["id": 2, "name": "pipeline", "host": "nas", "agent": "claude", "agents": ["harlo", "other"]],
                                                ["id": 3, "name": "tools", "host": "box"]],
                                   "agents": [["id": "claude", "label": "Claude"], ["id": "harlo", "label": "Harlo"], ["id": "other", "label": "Other"]], "defaultAgent": "claude"])
        XCTAssertEqual(m.agents(for: m.projects[1]).map(\.id), ["harlo", "other"])
        XCTAssertEqual(m.agent(for: m.projects[1]), "harlo", "claude isn't on nas: its first agent")
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
        XCTAssertEqual(m.sessions.count, 7)
        m.openSample(asking: true)
        XCTAssertEqual(m.session?.requests.count, 1)
        XCTAssertTrue(m.session?.items.contains { if case .run = $0 { return true }; return false } ?? false)
        XCTAssertFalse(RichText.parse(m.session?.rows.first { $0.kind == "code" }?.html ?? "").isEmpty)
    }

    @MainActor func testWhatNeedsYou() {
        let m = HarloModel.sample()
        let later: JSON = ["id": "q2", "method": "ask", "questions": [["id": "a", "question": "Which?", "options": ["x", "y"]]], "asked": 9_000_000_000_000]
        var list = m.sessions
        list[0] = SessionSummary(["key": "live:1", "title": "One", "harness": "other", "requests": [later]])
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
        let c = NewSessionChoices(["models": [["harness": "other", "model": "openai/gpt-5"]],
                                   "agentModels": ["other": [["id": "anthropic/claude-sonnet-4-5", "name": "Claude Sonnet 4.5"], ["id": "openai/gpt-5", "name": "GPT-5"]]]])
        XCTAssertEqual(c.modelChoices(for: "other").map(\.id), ["openai/gpt-5", "anthropic/claude-sonnet-4-5"])
        XCTAssertEqual(c.modelChoices(for: "other").first?.name, "GPT-5")
        XCTAssertTrue(c.modelChoices(for: "harlo").isEmpty)
    }

    func testMentions() {
        XCTAssertEqual(Mention.query(in: "look at @src/ma"), "src/ma")
        XCTAssertEqual(Mention.query(in: "@"), "")
        XCTAssertNil(Mention.query(in: "mail me@example.com"))
        XCTAssertNil(Mention.query(in: "@src/a.rs and"))
        XCTAssertEqual(Mention.insert("src/main.rs", into: "look at @src/ma"), "look at @src/main.rs ")
    }

    func testSessionMentions() {
        let m = SessionMention(["mention": "@session:7c1e9a40", "title": "Find the jump", "harness": "claude", "modified": 1_759_990_000])
        XCTAssertEqual(m.inserted, "session:7c1e9a40")
        XCTAssertEqual(Mention.insert(m.inserted, into: "as @fin"), "as @session:7c1e9a40 ")
        XCTAssertEqual(m.modified, 1_759_990_000)
    }

    func testToolsNamedAsTheDesktopNamesThem() {
        // Harlo's and Claude Code's own.
        XCTAssertEqual(ToolName.words("bash", running: false), "Ran command")
        XCTAssertEqual(ToolName.words("Bash", running: true), "Running command")
        XCTAssertEqual(ToolName.words("run_code", running: false), "Ran a program")
        // Words under a question or a plan, a path or a command under the rest.
        XCTAssertTrue(ToolName.saysWords("ask"))
        XCTAssertTrue(ToolName.saysWords("AskUserQuestion"))
        XCTAssertTrue(ToolName.saysWords("task"))
        XCTAssertTrue(ToolName.saysWords("plan_set"))
        XCTAssertEqual(ToolName.line("mcp__harlo__start_session", "agent=harlo  title=Update the changelog  worktree=changelog  prompt=Add it.").text, "Update the changelog")
        XCTAssertTrue(ToolName.line("start_session", "agent=harlo  title=Update the changelog").words)
        XCTAssertFalse(ToolName.line("bring_over", "live:4").words)
        XCTAssertEqual(ToolName.line("bash", "ls -la").text, "ls -la")
        XCTAssertFalse(ToolName.saysWords("bash"))
        XCTAssertFalse(ToolName.saysWords("read"))
        XCTAssertEqual(ToolName.words("hashline_edit", running: false), "Edited file")
        XCTAssertEqual(ToolName.words("AskUserQuestion", running: false), "Asked you")
        XCTAssertEqual(ToolName.words("web_search", running: false), "Searched the web")
        // harlo's, under either agent's name for them, and the plan's.
        XCTAssertEqual(ToolName.words("mcp__harlo__start_session", running: false), "harlo: start a session")
        XCTAssertEqual(ToolName.words("harlo_bring_over", running: false), "harlo: bring over a helper's changes")
        XCTAssertEqual(ToolName.words("start_session", running: false), "harlo: start a session", "as Harlo calls it")
        XCTAssertEqual(ToolName.symbol("bring_over"), "person.2")
        XCTAssertEqual(ToolName.words("mcp__harlo__plan_done", running: false), "Finish a step")
        XCTAssertEqual(ToolName.words("plan_set", running: false), "Plan")
        // The specs' tools, by Harlo's names and Claude's.
        XCTAssertEqual(ToolName.words("spec_read", running: false), "Read a spec")
        XCTAssertEqual(ToolName.words("mcp__harlo__spec_graph", running: false), "Look at the specs")
        XCTAssertEqual(ToolName.symbol("spec_search"), "doc.text.magnifyingglass")
        // An MCP server's.
        XCTAssertEqual(ToolName.words("mcp__context-mode__ctx_execute", running: false), "context-mode ctx execute")
        XCTAssertEqual(ToolName.symbol("mcp__context-mode__ctx_execute"), "puzzlepiece.extension")
        XCTAssertNil(ToolName.mcp("mcp__x"))
        // One it doesn't know.
        XCTAssertEqual(ToolName.words("web_thing", running: false), "Web thing")
        XCTAssertTrue(ToolName.isCommand("bash"))
        XCTAssertFalse(ToolName.isCommand("read"))
    }

    func testRowsSayWhatACallIsForAndShowACommandsEnd() {
        let lines = (1...3000).map { "line \($0)" }.joined(separator: "\n")
        let r = Row(["key": "t", "kind": "tool", "title": "bash", "text": .string(lines), "label": "Run the tests", "command": "cargo test", "model": "codex/gpt-6.1-sol"])
        XCTAssertEqual([r.label, r.command, r.model], ["Run the tests", "cargo test", "codex/gpt-6.1-sol"])
        let shown = r.shownOutput()
        XCTAssertTrue(shown.hasPrefix("…\nline "))
        XCTAssertTrue(shown.hasSuffix("line 3000"))
        XCTAssertLessThanOrEqual(shown.count, 6002)
        let read = Row(key: "r", kind: "tool", text: lines, title: "read")
        XCTAssertTrue(read.shownOutput().hasPrefix("line 1\n"))
        XCTAssertTrue(read.shownOutput().hasSuffix("\n…"))
        XCTAssertEqual(Row(key: "s", kind: "tool", text: "ok", title: "bash").shownOutput(), "ok")
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
        // (One time for both: samples a second apart differ.)
        let now = Date()
        m.sessions = HarloModel.sample(now: now).sessions
        XCTAssertEqual(WidgetSnapshot.load(store)?.host, "box")
        XCTAssertEqual(reloads, 1)
        m.sessions = HarloModel.sample(now: now).sessions
        XCTAssertEqual(reloads, 1, "the same: not written again")
        m.forget()
        XCTAssertEqual(WidgetSnapshot.load(store)?.items.count, 0, "unpaired: empty")
        let demo = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        demo.startDemo()
        XCTAssertNil(WidgetSnapshot.load(demo.store))
    }
}

@MainActor
final class LimitsWidgetTests: XCTestCase {
    func testItShowsEachAccountOnce() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let ms = Int64(now.timeIntervalSince1970 * 1000)
        let claude = LimitAccount(provider: "Claude", plan: "max", account: "a@x", windows: [
            LimitWindow(label: "Session (5 h)", usedFraction: 0.92, resetsAt: ms + 3_600_000),
            LimitWindow(label: "Weekly · Opus", usedFraction: 1, resetsAt: ms + 86_400_000, status: "exhausted"),
        ])
        let limits = PlanLimits(machines: [
            PlanLimits.Machine(host: "", accounts: [claude, LimitAccount(provider: "openai-codex", plan: "plus", windows: [])], updatedAt: ms),
            // The same account on a machine: once; another one: with its machine.
            PlanLimits.Machine(host: "nas", accounts: [claude, LimitAccount(provider: "openai-codex", plan: "pro", account: "b@x", windows: [
                LimitWindow(label: "Weekly", usedFraction: 0.2, resetsAt: ms + 2 * 86_400_000)])], updatedAt: ms),
        ])
        let snap = LimitsSnapshot.of(limits, now: now)
        XCTAssertTrue(snap.paired)
        XCTAssertEqual(snap.accounts.map(\.name), ["Claude", "ChatGPT"], "no windows: left out; the same account once")
        XCTAssertEqual(snap.accounts[1].host, "nas")
        XCTAssertEqual(snap.accounts[0].windows.map(\.label), ["5-hour", "Weekly Opus"])
        XCTAssertEqual(snap.accounts[0].windows.map(\.percent), ["92%", "100%"])
        XCTAssertTrue(snap.accounts[0].windows[1].usedUp)
        XCTAssertEqual(snap.accounts[0].fullest?.label, "Weekly Opus")
        XCTAssertTrue(snap.accounts[0].windows[0].nearly)
        // Past a reset: that window is reset; the timeline has each reset.
        XCTAssertEqual(snap.resets(after: now), [now.addingTimeInterval(3600), now.addingTimeInterval(86_400)])
        let later = snap.at(now.addingTimeInterval(2 * 3600))
        XCTAssertEqual(later.accounts[0].windows[0].usedFraction, 0)
        XCTAssertEqual(later.accounts[0].windows[0].resetsAt, 0)
        XCTAssertEqual(later.accounts[0].windows[1].usedFraction, 1, "not reset yet")
        XCTAssertEqual(later.resets(after: now.addingTimeInterval(2 * 3600)), [now.addingTimeInterval(86_400)])
    }

    func testTheAppKeepsItAndNotForTheDemo() {
        let store = MemoryStore()
        let m = HarloModel(store: store, deviceName: "iPhone")
        var reloads = 0
        m.limitsWidgetChanged = { reloads += 1 }
        m.host = PairedHarlo(key: "k", name: "box", endpoints: [])
        let now = Date()
        m.limits = HarloModel.sampleLimits(now: now)
        XCTAssertEqual(LimitsSnapshot.load(store)?.accounts.count, 2)
        XCTAssertEqual(reloads, 1)
        m.limits = HarloModel.sampleLimits(now: now)
        XCTAssertEqual(reloads, 1, "the same: not written again")
        m.forget()
        XCTAssertEqual(LimitsSnapshot.load(store)?.paired, false, "unpaired")
        XCTAssertEqual(reloads, 2)
        let demo = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        demo.startDemo()
        XCTAssertNil(LimitsSnapshot.load(demo.store))
        XCTAssertEqual(LimitsSnapshot.sample(now: now).accounts.first?.windows.first?.percent, "92%")
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

final class LimitsTests: XCTestCase {
    func testPlanLimitsFromHarlo() {
        let v = try! JSON.parse(#"""
        { "here": { "accounts": [{ "provider": "Claude", "plan": "max", "windows": [
            { "label": "Session (5 h)", "usedFraction": 1.0, "status": "exhausted", "resetsAt": 1700000000000 },
            { "label": "Weekly · Opus", "usedFraction": 0.95, "resetsAt": null } ] }], "updatedAt": 5 },
          "hosts": { "nas": { "accounts": [{ "provider": "openai-codex", "windows": [] }] }, "empty": { "accounts": [] } } }
        """#)
        let l = PlanLimits(v)
        XCTAssertEqual(l.machines.map(\.host), ["", "nas"])
        let w = l.machines[0].accounts[0].windows
        XCTAssertTrue(w[0].usedUp)
        XCTAssertEqual(w[1].name, "weekly Opus limit")
        XCTAssertEqual(w[1].resetsAt, 0)
        XCTAssertEqual(l.machines[1].accounts[0].name, "ChatGPT")
        XCTAssertFalse(l.isEmpty)
        XCTAssertTrue(PlanLimits(JSON.null).isEmpty)
        XCTAssertEqual(Event(["ev": "limits", "here": ["accounts": []], "hosts": [:]]), .limits(PlanLimits(machines: [.init(host: "", accounts: [], updatedAt: 0)])))
    }

    func testSessionLimit() {
        XCTAssertNil(SessionLimit(.null))
        let l = SessionLimit(["provider": "Claude", "label": "Weekly", "usedFraction": .double(1), "resetsAt": 9, "out": true, "resetAt": 9])
        XCTAssertEqual(l?.window.name, "weekly limit")
        XCTAssertEqual(l?.out, true)
        XCTAssertEqual(l?.resetAt, 9)
    }

    func testCompactNote() {
        var i = SessionInfo()
        i.contextWindow = 200_000
        i.compactAt = 160_000
        i.contextTokens = 100_000
        XCTAssertEqual(i.compactNote, "")
        i.contextTokens = 148_600
        XCTAssertEqual(i.compactNote, "11k tokens left before the context compacts")
        i.contextTokens = 170_000
        XCTAssertEqual(i.compactNote, "The context is at its limit: it compacts next")
        XCTAssertEqual(i.compactMark, 0.8, accuracy: 1e-9)
        let meta = SessionInfo(["compactAt": 160000, "limitRefused": true])
        XCTAssertEqual(meta.compactAt, 160_000)
        XCTAssertTrue(meta.limitRefused)
    }

    func testResetTime() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        XCTAssertEqual(resetTime(1_700_003_600_000, now: now, calendar: cal), "at 23:13")
        XCTAssertEqual(resetTime(1_700_000_000_000 + 2 * 86_400_000, now: now, calendar: cal), "Thu 22:13")
    }

    /// An older app's saved registration has no `limits`: on.
    func testPushRegistrationFromAnOlderApp() throws {
        let old = #"{"token":"ab","sandbox":false,"asks":true,"done":false}"#
        let p = try JSONDecoder().decode(PushRegistration.self, from: Data(old.utf8))
        XCTAssertFalse(p.done)
        XCTAssertTrue(p.limits)
        XCTAssertEqual(p.json["limits"], .bool(true))
        let c = PushContent(["k": "reset", "s": "limit-Weekly", "t": "Claude's weekly limit has reset"])
        XCTAssertTrue(c.isLimit)
        XCTAssertEqual(c.lines.title, "Claude's weekly limit has reset")
    }

    @MainActor func testSamplesHaveLimits() {
        XCTAssertFalse(HarloModel.sample().limits.isEmpty)
    }
}

final class MachinesTests: XCTestCase {
    func testSessionsSayTheirMachineAndShelf() {
        let s = SessionSummary(["key": "k", "host": "nas", "settled": "Merged"])
        XCTAssertEqual(s.host, "nas")
        XCTAssertEqual(s.settled, "Merged")
        let old = SessionSummary(["key": "k"])
        XCTAssertEqual(old.host, "")
        XCTAssertEqual(old.settled, "")
    }

    func testPlacesOnOtherMachines() {
        var c = NewSessionChoices(["projects": [["id": 1, "name": "web", "agent": "claude"], ["id": 2, "name": "pipeline", "host": "nas", "agent": "claude", "agents": ["harlo"]]],
                                   "agents": [["id": "claude", "label": "Claude"], ["id": "harlo", "label": "Harlo"], ["id": "other", "label": "Other"]], "defaultAgent": "claude",
                                   "machines": [["host": "nas", "connected": true, "home": "/home/r", "agents": ["harlo", "other"]], ["host": "box", "connected": false]]])
        XCTAssertEqual(c.machines.map(\.host), ["", "box", "nas"], "machines without projects too, this computer first")
        XCTAssertEqual(c.host(.home("box")), "box")
        XCTAssertEqual(c.host(.project(2)), "nas")
        XCTAssertEqual(c.host(.project(1)), "")
        XCTAssertTrue(c.needsConnecting(.home("box")))
        XCTAssertFalse(c.needsConnecting(.project(2)))
        XCTAssertFalse(c.needsConnecting(.home("")))
        XCTAssertEqual(c.agents(for: .home("nas")).map(\.id), ["harlo", "other"], "that machine's agents")
        XCTAssertEqual(c.agent(for: .home("nas")), "harlo", "claude isn't there: its first")
        XCTAssertEqual(c.agent(for: .home("")), "claude")
        XCTAssertEqual(c.agent(for: .project(2)), "harlo")
        XCTAssertTrue(c.agents(for: .home("box")).isEmpty, "not connected: nothing known")
        c.connected(RemoteMachine(host: "box", connected: true, home: "/root", agents: ["other"]))
        XCTAssertFalse(c.needsConnecting(.home("box")))
        XCTAssertEqual(c.agent(for: .home("box")), "other")
        // An older harlo: no machines, only those with projects.
        let old = NewSessionChoices(["projects": [["id": 2, "name": "pipeline", "host": "nas"]]])
        XCTAssertTrue(old.hosts.isEmpty)
        XCTAssertEqual(old.machines.map(\.host), ["", "nas"])
        XCTAssertFalse(old.needsConnecting(.project(2)))
    }

    func testModelsOnAStoppedServer() {
        XCTAssertEqual(ModelChoice(["provider": "llama", "id": "q", "stoppedServer": "llama.cpp"]).stoppedServer, "llama.cpp")
        XCTAssertEqual(ModelChoice(["provider": "llama", "id": "q"]).stoppedServer, "")
    }

    func testModelServersFromHarlo() {
        let v = try! JSON.parse(#"""
        { "machines": [
            { "host": "", "servers": [{ "url": "http://127.0.0.1:8080/v1", "name": "llama.cpp", "engine": "llama.cpp", "models": ["qwen"], "running": true }] },
            { "host": "nas", "servers": [], "error": "It didn't answer." } ] }
        """#)
        let list = ModelServers.list(v)
        XCTAssertEqual(list.map(\.host), ["", "nas"])
        XCTAssertEqual(list[0].servers.first?.models, ["qwen"])
        XCTAssertEqual(list[0].servers.first?.running, true)
        XCTAssertEqual(list[1].error, "It didn't answer.")
    }

    @MainActor func testDemoStartsAndStopsServers() async {
        let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        m.startDemo()
        let before = await m.modelServers()
        XCTAssertEqual(before?.first?.servers.map(\.running), [true, false])
        let now = await m.setServer(host: "", url: "http://127.0.0.1:11434/v1", running: true)
        XCTAssertEqual(now, true)
        let after = await m.modelServers()
        XCTAssertEqual(after?.first?.servers.map(\.running), [true, true])
        XCTAssertNotNil(m.sessions.first { !$0.settled.isEmpty }, "a settled sample")
    }
}

final class PlanTests: XCTestCase {
    func testPlanFromHarlo() {
        let v = try! JSON.parse(#"""
        { "enabled": true, "commits": true, "reviewers": [{ "id": "claude", "label": "Claude" }, { "id": "other", "label": "Other" }],
          "steps": [{ "id": 1, "title": "One", "status": "done", "commit": "abc123", "summary": "did it" },
                    { "id": 2, "title": "Two", "status": "active", "detail": "how" },
                    { "id": 3, "title": "Three", "status": "skipped", "editedByUser": true },
                    { "id": 4, "title": "Four" }] }
        """#)
        let plan = Plan(v)
        XCTAssertTrue(plan.shows)
        XCTAssertEqual(plan.finished, 2, "done and skipped")
        XCTAssertEqual(plan.current?.title, "Two")
        XCTAssertEqual(plan.steps[3].status, "pending")
        XCTAssertTrue(plan.steps[3].open)
        XCTAssertFalse(plan.steps[0].open)
        XCTAssertEqual(plan.reviewers(for: "claude").map(\.id), ["other", "claude"], "another agent first")
        XCTAssertFalse(Plan(["enabled": false, "steps": []]).shows)
    }

    func testPlanEditsAsHarloTakesThem() {
        XCTAssertEqual(PlanEdit.add(title: "T", detail: "", after: 3).json, ["op": "add", "title": "T", "detail": "", "after": 3])
        XCTAssertEqual(PlanEdit.add(title: "T", detail: "d", after: nil).json, ["op": "add", "title": "T", "detail": "d"])
        XCTAssertEqual(PlanEdit.skip(id: 2, skipped: true).json, ["op": "skip", "id": 2, "skipped": true])
        XCTAssertEqual(PlanEdit.shift(id: 2, by: -1).json, ["op": "shift", "id": 2, "by": -1])
        XCTAssertEqual(PlanEdit.alongside(id: 3, alongside: true).json, ["op": "alongside", "id": 3, "alongside": true])
    }

    func testPlanRunWithHelpers() {
        let v = try! JSON.parse(#"""
        { "enabled": true, "reviewers": [{ "id": "other", "label": "Other" }, { "id": "claude", "label": "Claude" }],
          "run": { "agent": "claude", "agentName": "Claude", "model": "", "paused": "" },
          "steps": [{ "id": 1, "title": "One", "status": "active", "helper": "live:4" },
                    { "id": 2, "title": "Two", "status": "active", "alongside": true, "helper": "live:5", "branch": "plan/two-2" },
                    { "id": 3, "title": "Three" }] }
        """#)
        var plan = Plan(v)
        XCTAssertEqual(plan.run?.agentName, "Claude")
        XCTAssertFalse(plan.canRun, "it's running")
        XCTAssertEqual(plan.steps[0].helper, "live:4")
        XCTAssertTrue(plan.steps[1].alongside)
        XCTAssertEqual(plan.steps[1].branch, "plan/two-2")
        XCTAssertFalse(plan.steps[2].alongside)
        XCTAssertEqual(plan.helperAgents(for: "claude").map(\.id), ["claude", "other"], "the session's own first")
        plan.run?.paused = "Step 2 (Two) failed: tests"
        XCTAssertTrue(plan.canRun, "a paused run goes on")
        plan.run = nil
        XCTAssertTrue(plan.canRun)
        XCTAssertNil(Plan(["enabled": true, "steps": []]).run)
    }

    func testHelpers() {
        let h = Helper(["id": "live:4", "name": "Step 1: One", "agent": "claude", "status": "waiting", "openKey": "live:4", "description": "Did it", "step": 1])
        XCTAssertEqual(h.id, "live:4")
        XCTAssertEqual(h.summary, "Did it")
        XCTAssertEqual(h.step, 1)
        XCTAssertTrue(h.needsSomeone)
        XCTAssertNil(Helper(["id": "live:6", "status": "running"]).step)
        let row = SessionSummary(["key": "live:4", "title": "Step 1: One", "parent": "live:1", "helperOf": "Fix it", "nest": 1])
        XCTAssertEqual(row.parent, "live:1")
        XCTAssertEqual(row.helperOf, "Fix it")
        XCTAssertEqual(row.nest, 1)
        XCTAssertEqual(SessionSummary(["key": "live:1"]).nest, 0)
    }

    func testHelperModelUseAndRequest() {
        let v = try! JSON.parse(#"""
        { "id": "live:7", "openKey": "live:7", "name": "Step 2: Two", "agent": "harlo", "status": "waiting", "step": 2, "review": null,
          "model": "codex/gpt-6.1-sol", "used": { "tokens": 12400, "cost": 0.031, "requests": 3 },
          "asks": { "id": "r1", "approval": true, "options": ["Approve", "Deny"], "text": "Run cargo test?" } }
        """#)
        let h = Helper(v)
        XCTAssertEqual(h.modelLabel, "gpt-6.1-sol")
        XCTAssertEqual(h.used?.label, "12.4k tokens · $0.03")
        XCTAssertEqual(h.asks?.id, "r1")
        XCTAssertEqual(h.asks?.options, ["Approve", "Deny"])
        XCTAssertTrue(h.asks?.approval == true)
        XCTAssertNil(h.reviews)
        let reviewer = Helper(["id": "live:8", "status": "running", "review": 2, "asks": nil])
        XCTAssertEqual(reviewer.reviews, 2)
        XCTAssertNil(reviewer.asks)
        XCTAssertNil(reviewer.used)
        XCTAssertEqual(reviewer.modelLabel, "")
        let total = Helper.Used.total([h.used, nil, Helper.Used(tokens: 600, cost: 0.005, requests: 1)])
        XCTAssertEqual(total.label, "13.0k tokens · $0.04")
        XCTAssertEqual(Helper.Used(cost: 0.004).label, "<$0.01")
        XCTAssertEqual(Helper.Used().label, "")
    }

    func testPlanTeamAndReviews() {
        let v = try! JSON.parse(#"""
        { "enabled": true, "reviewers": [{ "id": "harlo", "label": "Harlo" }],
          "team": { "agent": "harlo", "agentName": "Harlo", "model": "codex/gpt-6.1-sol", "team": true,
                    "reviewer": { "agent": "claude", "agentName": "Claude Code", "model": "claude-opus-5-5" } },
          "run": { "agent": "harlo", "agentName": "Harlo", "paused": "", "review": true, "reviewing": { "2": "live:9" } },
          "steps": [{ "id": 1, "title": "One", "status": "done" },
                    { "id": 2, "title": "Two", "status": "active", "helper": "live:5" },
                    { "id": 3, "title": "Three", "agent": "claude", "model": "claude-sonnet-5-5" }] }
        """#)
        let plan = Plan(v)
        XCTAssertEqual(plan.team?.label, "Harlo · gpt-6.1-sol, reviewed by Claude Code · claude-opus-5-5")
        XCTAssertTrue(plan.team?.custom == true)
        XCTAssertEqual(plan.run?.reviewing[2], "live:9")
        XCTAssertEqual(plan.steps[2].agent, "claude")
        XCTAssertEqual(plan.steps[2].model, "claude-sonnet-5-5")
        // The session's own agent, without a Team.
        let own = Plan(try! JSON.parse(#"{ "team": { "agent": "claude", "agentName": "Claude Code", "model": "", "team": false } }"#))
        XCTAssertEqual(own.team?.label, "Claude Code")
        XCTAssertFalse(own.team?.custom == true)
        XCTAssertNil(Plan(try! JSON.parse(#"{ "team": {} }"#)).team)
    }

    func testHelpersFoldUnderTheirSession() {
        let list = SessionSummary.list([
            ["key": "a", "status": "idle"],
            ["key": "a1", "parent": "a", "nest": 1, "status": "idle"],
            ["key": "a1x", "parent": "a1", "nest": 2, "status": "input"],
            ["key": "a2", "parent": "a", "nest": 1, "status": "idle"],
            ["key": "b", "status": "idle"],
            // (Its parent isn't listed: it lists on its own.)
            ["key": "c1", "parent": "gone", "nest": 1, "status": "busy"],
        ])
        let h = SessionSummary.helpers(list)
        XCTAssertEqual(list.filter { !h.folded($0) }.map(\.key), ["a", "b", "c1"])
        XCTAssertEqual(h.helpers["a"]?.map(\.session.key), ["a1", "a1x", "a2"])
        XCTAssertEqual(h.helpers["a"]?.map(\.depth), [1, 2, 1])
        XCTAssertNil(h.helpers["b"])
        XCTAssertEqual(h.status(list[0]), "input", "a helper waiting for you shows on its session")
        XCTAssertEqual(h.status(list[4]), "idle")
    }

    func testDemoEdits() {
        var plan = HarloModel.samplePlan()
        plan = HarloModel.demoEdit(plan, .add(title: "New", detail: "", after: 1))
        XCTAssertEqual(plan.steps.map(\.title)[1], "New")
        XCTAssertTrue(plan.steps[1].editedByUser)
        plan = HarloModel.demoEdit(plan, .skip(id: 3, skipped: true))
        XCTAssertEqual(plan.steps.first { $0.id == 3 }?.status, "skipped")
        plan = HarloModel.demoEdit(plan, .alongside(id: 3, alongside: false))
        XCTAssertEqual(plan.steps.first { $0.id == 3 }?.alongside, false)
        plan = HarloModel.demoEdit(plan, .alongside(id: 1, alongside: true))
        XCTAssertEqual(plan.steps.first?.alongside, false, "the first step has none before it")
        plan = HarloModel.demoEdit(plan, .remove(id: 3))
        XCTAssertNil(plan.steps.first { $0.id == 3 })
    }

    func testSpecsAndThePlanEvent() {
        let specs = Specs(["enabled": true, "specs": [["path": "SPEC.md", "title": "harlo", "summary": "s"]]])
        // The tree: depth, type, status; what's wrong in the links.
        let tree = Specs(["enabled": true, "specs": [["path": "SPEC.md", "title": "harlo", "type": "product", "depth": 0],
                                                     ["path": "core/SPEC.md", "title": "core", "status": "stale", "depth": 1]],
                          "problems": ["core/SPEC.md: depends-on `x` isn't a spec"], "mermaid": "flowchart TD\n  s0[\"harlo\"]\n"])
        XCTAssertEqual(tree.specs.map(\.depth), [0, 1])
        XCTAssertEqual(tree.specs[0].kind, "product")
        XCTAssertEqual(tree.specs[1].status, "stale")
        XCTAssertEqual(tree.problems, ["core/SPEC.md: depends-on `x` isn't a spec"])
        XCTAssertEqual(tree.spec(of: tree.problems[0])?.title, "core", "a problem opens its spec")
        XCTAssertNil(tree.spec(of: "parents go round in a circle"))
        XCTAssertTrue(tree.mermaid.hasPrefix("flowchart TD"))
        XCTAssertEqual(specs.mermaid, "", "an older harlo sends no graph")
        XCTAssertEqual(specs.specs.first?.path, "SPEC.md")
        if case .plan(let key)? = Event(["ev": "plan", "key": "live:1"]) { XCTAssertEqual(key, "live:1") } else { XCTFail("a plan event") }
    }

    func testHarlosMessagesAreHarlos() throws {
        let v: JSON = ["key": "m3:u", "kind": "notice", "title": "harlo", "subtitle": "live:3",
                       "text": "The Harlo session \"Helper\" (live:3) finished and reported: succeeded."]
        let report = Row(v)
        XCTAssertTrue(report.fromHarlo)
        XCTAssertTrue(report.isSent, "it starts a turn")
        XCTAssertEqual(report.subtitle, "live:3")
        let warning = Row(key: "n1", kind: "notice", text: "Careful", title: "warning")
        XCTAssertFalse(warning.fromHarlo)
        XCTAssertFalse(warning.isSent)
        XCTAssertTrue(Row(key: "u1", kind: "user", text: "hi").isSent)
    }

    func testHarlosApprovalSaysWhy() {
        let danger = UIRequest(["id": "1", "method": "select", "title": "Allow tool: bash\nReason: this can't be undone: it resets the branch", "approval": true])
        XCTAssertEqual(danger.tool, "bash")
        XCTAssertEqual(danger.note, "This can't be undone: it resets the branch")
        let details = UIRequest(["id": "5", "method": "select", "title": "Allow tool: bash\nReason: demo approval\nCommand: ls -la", "approval": true])
        XCTAssertEqual(details.note, "Demo approval", "a command line is a detail, not why")
        let outside = UIRequest(["id": "2", "method": "select", "title": "Allow tool: bash\nOutside the sandbox", "approval": true])
        XCTAssertEqual(outside.note, "Outside the sandbox")
        let plain = UIRequest(["id": "3", "method": "select", "title": "Allow tool: write", "approval": true])
        XCTAssertEqual(plain.tool, "write")
        XCTAssertEqual(plain.note, "")
        XCTAssertEqual(UIRequest(["id": "4", "method": "select", "title": "Split into helpers?"]).tool, "")
    }

    @MainActor func testSideQuestionsCome() {
        let ev = Event(["ev": "aside", "key": "live:1", "aside": ["question": "why?", "text": "Because **so**.", "done": true]])
        guard case .aside(let k, let a)? = ev else { return XCTFail("\(String(describing: ev))") }
        XCTAssertEqual(k, "live:1")
        XCTAssertEqual(a, Aside(["question": "why?", "text": "Because **so**.", "done": true]))
        XCTAssertEqual(a.error, "")
        let s = OpenSession(key: "live:1")
        s.opened(liveKey: "live:1", harness: "harlo")
        s.take(ev!)
        XCTAssertEqual(s.aside?.text, "Because **so**.")
    }

    func testColoursAsHex() {
        XCTAssertEqual(RGBA(hex: "#5b9bf0")?.hex, "#5b9bf0")
    }
}

@MainActor
final class ReplayTests: XCTestCase {
    /// A recording as harlo-probe writes one.
    static let recording = [
        #"{"ready":{"name":"workstation","features":["sessionMentions","limits"]}}"#,
        #"{"call":"sessions","args":{},"reply":{"sessions":[{"key":"live:1","title":"Fix the scrolling","harness":"harlo","status":"input"}]}}"#,
        #"{"call":"open","args":{"key":"live:1"},"reply":{"key":"live:1","harness":"harlo"}}"#,
        #"{"ev":{"ev":"reset","key":"live:1","rows":[{"key":"u1","kind":"user","text":"Fix the scrolling"},{"key":"t1","kind":"tool","title":"bash","label":"Run the tests","command":"cargo test"}]}}"#,
        #"{"ev":{"ev":"request","key":"live:1","request":{"id":"r1","method":"select","title":"Allow tool: bash","options":["Approve","Deny"],"approval":true}}}"#,
        #"{"scene":"approval"}"#,
        #"{"call":"answer","args":{"key":"live:1","requestId":"r1","answer":{"value":"Deny"}},"reply":{"ok":true}}"#,
        #"{"ev":{"ev":"cleared","key":"live:1","id":"r1"}}"#,
        #"{"ev":{"ev":"rows","key":"live:1","changes":[]}}"#,
        #"{"call":"files","args":{"key":"live:1","query":"no"},"reply":{"files":["notes.txt"]}}"#,
        #"{"scene":"answered"}"#,
    ].joined(separator: "\n")

    func testScenesAndAnswers() throws {
        XCTAssertEqual(Replay.scenes(in: Self.recording), ["approval", "answered"])
        XCTAssertThrowsError(try Replay(recording: Self.recording, scene: "nope"))
        let r = try Replay(recording: Self.recording, scene: "approval")
        XCTAssertEqual(r.harloName, "workstation")
        XCTAssertTrue(r.features.contains("sessionMentions"))
        XCTAssertEqual(r.events(of: "live:1").map { $0["ev"].text }, ["reset", "request"], "only up to the scene")
        // A request made only after the scene still answers; one never made says so.
        XCTAssertEqual(r.answer("files", ["key": "live:1", "query": "other"])["files"].array?.count, 1)
        XCTAssertNotNil(r.answer("diff", [:])["error"].string)
    }

    func testTheAppShowsTheRecordingAtAScene() async throws {
        let m = HarloModel.replaying(try Replay(recording: Self.recording, scene: "approval"))
        XCTAssertTrue(m.replaying)
        for _ in 0..<250 where m.sessions.isEmpty { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertEqual(m.state, .connected)
        XCTAssertEqual(m.host?.name, "workstation")
        let summary = try XCTUnwrap(m.sessions.first)
        m.open(summary)
        let s = try XCTUnwrap(m.session)
        for _ in 0..<250 where s.rows.isEmpty || s.requests.isEmpty { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertEqual(s.rows.map(\.kind), ["user", "tool"])
        XCTAssertEqual(s.rows.last?.label, "Run the tests")
        XCTAssertEqual(s.requests.first?.id, "r1")
        // Answering it: harlo's `cleared` follows.
        await m.answer(s.requests[0], s.requests[0].approval(false))
        for _ in 0..<250 where !s.requests.isEmpty { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertTrue(s.requests.isEmpty)
        XCTAssertNil(m.alert)
        // It stays connected whatever happens to the app.
        m.suspend()
        XCTAssertEqual(m.state, .connected)
    }
}

/// The app's recording (scripts/ios-record.sh) played back at each scene, as
/// the UI tests and screenshots see it.
@MainActor
final class RecordingTests: XCTestCase {
    static let text = try? String(contentsOf: URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .appendingPathComponent("../../../fixtures/recording.jsonl").standardized, encoding: .utf8)
    static let main = "The session list jumps to the top"

    func open(_ scene: String) async throws -> (HarloModel, OpenSession) {
        let text = try XCTUnwrap(Self.text, "ios/fixtures/recording.jsonl")
        let m = HarloModel.replaying(try Replay(recording: text, scene: scene))
        for _ in 0..<100 where m.sessions.isEmpty { try await Task.sleep(nanoseconds: 20_000_000) }
        let summary = try XCTUnwrap(m.sessions.first { $0.title.hasPrefix(Self.main) }, "\(scene): \(m.sessions.map(\.title))")
        m.open(summary)
        let s = try XCTUnwrap(m.session)
        for _ in 0..<100 where s.rows.isEmpty || s.loading { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertNil(s.error, scene)
        return (m, s)
    }

    func testEveryScene() async throws {
        let text = try XCTUnwrap(Self.text)
        XCTAssertEqual(Replay.scenes(in: text), ["session", "ask", "approval", "working", "aside", "plan", "helpers", "end"])
        let tools = { (s: OpenSession) in s.rows.filter { $0.kind == "tool" } }

        var (m, s) = try await open("session")
        XCTAssertTrue(m.features.contains("sessionMentions"))
        XCTAssertEqual(m.sessions.count, 3, "\(m.sessions.map(\.title))")
        let tests = try XCTUnwrap(tools(s).first { $0.label == "Run the tests" })
        XCTAssertEqual(tests.command, "./run-tests.sh")
        XCTAssertTrue(tests.shownOutput().hasSuffix("112 passed; 0 failed; 0 ignored; finished in 1.84s"))
        XCTAssertEqual(s.rows.first { $0.kind == "markdown" }?.model, "tour-coder")

        (m, s) = try await open("ask")
        XCTAssertEqual(s.requests.first?.questions.first?.options.count, 2)
        (m, s) = try await open("approval")
        let approval = try XCTUnwrap(s.requests.first)
        XCTAssertTrue(approval.approval && approval.message.contains("git clean"), approval.message)
        XCTAssertEqual(m.waiting.count, 1)
        (m, s) = try await open("working")
        XCTAssertFalse(s.subagents.isEmpty)
        (m, s) = try await open("aside")
        XCTAssertEqual(s.aside?.done, true)
        XCTAssertTrue(s.rows.contains { $0.kind == "notice" && $0.text.contains("The background command finished") })
        (m, s) = try await open("plan")
        for _ in 0..<250 where (s.plan?.steps.count ?? 0) < 3 { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertEqual(s.plan?.steps.count, 3)
        // The plan's sheet reads the specs as it opens: none in the
        // recording, and no alert for it (one would close the sheet).
        await m.loadSpecs()
        XCTAssertNil(m.alert, "specs that can't be read say nothing")
        XCTAssertTrue(tools(s).contains { ToolName.words($0.title, running: false) == "dice roll" })
        (m, s) = try await open("helpers")
        XCTAssertTrue(s.rows.contains { $0.fromHarlo && !$0.label.isEmpty })
        XCTAssertTrue(tools(s).contains { ToolName.words($0.title, running: false) == "harlo: bring over a helper's changes" })
        (m, s) = try await open("end")
        XCTAssertTrue(s.info.modelId.hasSuffix("tour-helper"), s.info.modelId)
        let mentions = await m.sessionMentions(matching: "")
        XCTAssertTrue(mentions.contains { $0.harness == "claude" }, "\(mentions.map(\.title))")
        // The Claude Code session: its subagent's call.
        let claude = try XCTUnwrap(m.sessions.first { $0.harness == "claude" })
        m.open(claude)
        let c = try XCTUnwrap(m.session)
        for _ in 0..<100 where c.rows.isEmpty { try await Task.sleep(nanoseconds: 20_000_000) }
        XCTAssertTrue(c.rows.contains { $0.kind == "tool" && $0.title == "Agent" }, "\(c.rows.map(\.title))")
    }
}

final class VisualsTests: XCTestCase {
    func testOptions() {
        let o = Visuals.options("## Match rows by key (recommended)\nRows are updated in place.\n+ nothing to restore\n+ no redraw\n- more code\n\n## Remember the offset\nSmaller.\nStill redraws.\n- the list flickers\n")
        XCTAssertEqual(o, [
            Choice(name: "Match rows by key", recommended: true, about: "Rows are updated in place.", pros: ["nothing to restore", "no redraw"], cons: ["more code"]),
            Choice(name: "Remember the offset", about: "Smaller. Still redraws.", cons: ["the list flickers"]),
        ])
        XCTAssertNil(Visuals.options("### One ★\n+ x"))
        XCTAssertNil(Visuals.options("Just prose."))
    }

    func testMockupPictures() {
        let png = Data([0x89, 0x50, 0x4E, 0x47]).base64EncodedString()
        let m = MockupPictures(["title": "Settings", "after": ["png": .string(png), "width": 640, "height": 400], "before": ["error": "mockups/old.html isn't there."]])
        XCTAssertEqual(m.title, "Settings")
        XCTAssertEqual(m.after?.width, 640)
        XCTAssertTrue(m.hasBefore)
        XCTAssertNil(m.before)
        XCTAssertEqual(m.beforeWhy, "mockups/old.html isn't there.")
        XCTAssertFalse(MockupPictures(["after": ["png": .string(png)]]).hasBefore)
    }
}
