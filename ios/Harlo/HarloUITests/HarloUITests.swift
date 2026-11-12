import XCTest

/// The app driven on the simulator against its recording of harlo
/// (`-replay <scene>`, ios/fixtures/recording.jsonl): each screen opened as
/// you would, what should be on it checked, and pictured. The pictures are
/// the test's attachments, named "NN-screen" (the ui-tests job exports them,
/// one folder per phone, appearance and text size); `audit-<screen>` says
/// what Apple's accessibility audit found there.
///
/// The run's text size comes from HARLO_TEXT_SIZE (a content size category,
/// e.g. UICTContentSizeCategoryAccessibilityL); its appearance is the
/// simulator's.
final class HarloUITests: XCTestCase {
    var app = XCUIApplication()
    /// The main session: a fix, with its tool calls, questions and helpers.
    let main = "jumps to the top"

    override func setUp() {
        continueAfterFailure = true
    }

    /// The app at a scene of the recording.
    func launch(_ scene: String) {
        app = XCUIApplication()
        app.launchArguments = ["-replay", scene]
        if let size = ProcessInfo.processInfo.environment["HARLO_TEXT_SIZE"], !size.isEmpty {
            app.launchArguments += ["-UIPreferredContentSizeCategoryName", size]
        }
        app.launch()
    }

    /// Something on screen whose label has `text`.
    func element(_ text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", text)).firstMatch
    }

    @discardableResult
    func see(_ text: String, timeout: Double = 8, file: StaticString = #filePath, line: UInt = #line) -> Bool {
        let found = element(text).waitForExistence(timeout: timeout)
        XCTAssertTrue(found, "on screen: \(text)", file: file, line: line)
        return found
    }

    func tap(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        let e = element(text)
        guard e.waitForExistence(timeout: 8) else {
            XCTFail("nothing to tap: \(text)", file: file, line: line)
            return
        }
        e.tap()
    }

    /// Taps `text` in the conversation once it's above everything under it
    /// (the message box, the plan, a side question): at the largest text
    /// sizes it can sit under them, and the tap would land there (on the
    /// model's button, say).
    func tapInConversation(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        let e = element(text)
        guard e.waitForExistence(timeout: 8) else {
            XCTFail("nothing to tap: \(text)", file: file, line: line)
            return
        }
        let bottom = app.otherElements["sessionBottom"]
        let box = bottom.exists ? bottom : app.textViews.firstMatch.exists ? app.textViews.firstMatch : app.textFields.firstMatch
        for _ in 0..<6 where box.exists && e.frame.maxY > box.frame.minY - 24 {
            let from = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.55))
            from.press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)))
        }
        e.tap()
    }

    /// Waits for `e` to go (false: still there after `timeout`).
    func gone(_ e: XCUIElement, timeout: Double = 8) -> Bool {
        let done = expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: e)
        return XCTWaiter().wait(for: [done], timeout: timeout) == .completed
    }

    /// The screen as it is now, kept as "name".
    func shot(_ name: String, file: StaticString = #filePath, line: UInt = #line) {
        // (Animations settle.)
        Thread.sleep(forTimeInterval: 0.6)
        // A session fits the screen: something kept whole and wider than
        // its row (a long answer at a card's end) widened it past both sides.
        let bottom = app.descendants(matching: .any).matching(identifier: "sessionBottom").firstMatch
        if bottom.exists {
            let screen = app.windows.firstMatch.frame
            XCTAssertTrue(bottom.frame.minX >= screen.minX - 1 && bottom.frame.maxX <= screen.maxX + 1,
                          "\(name): the session fits the screen (\(bottom.frame) in \(screen))", file: file, line: line)
        }
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    /// What Apple's accessibility audit finds on this screen (contrast,
    /// tap targets, labels, text that's cut or doesn't grow), kept as
    /// "audit-name"; it doesn't fail the test.
    func audit(_ name: String) {
        var found: [String] = []
        do {
            try app.performAccessibilityAudit { issue in
                let what = issue.element.map { "\($0.elementType.rawValue) \"\($0.label)\"" } ?? "-"
                found.append("\(issue.compactDescription) — \(what)\n    \(issue.detailedDescription)")
                return true
            }
        } catch {
            found.append("The audit didn't run: \(error)")
        }
        let a = XCTAttachment(string: found.isEmpty ? "Nothing found." : found.joined(separator: "\n"))
        a.name = "audit-" + name
        a.lifetime = .keepAlways
        add(a)
    }

    /// Opens the main session from the dashboard (not its helper's row,
    /// which names it too).
    func openMain(file: StaticString = #filePath, line: UInt = #line) {
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@ AND NOT (label CONTAINS 'Helper of')", main)).firstMatch
        guard row.waitForExistence(timeout: 8) else {
            XCTFail("the main session isn't listed", file: file, line: line)
            return
        }
        row.tap()
    }

    /// "Wrap Long Lines" in a file's or a diff's view options (…).
    func toggleWrap(file: StaticString = #filePath, line: UInt = #line) {
        let menu = app.buttons["View options"]
        guard menu.waitForExistence(timeout: 8) else {
            XCTFail("no view options", file: file, line: line)
            return
        }
        menu.tap()
        tap("Wrap Long Lines", file: file, line: line)
        // (The menu closes and the lines settle.)
        Thread.sleep(forTimeInterval: 0.6)
    }

    /// Lines wrapped to start with, whatever a test before left (the
    /// setting is kept on the phone).
    func wrapped(_ long: XCUIElement, _ screen: CGRect) {
        if long.waitForExistence(timeout: 8), long.frame.maxX > screen.maxX + 1 { toggleWrap() }
    }

    // ---- the screens ---------------------------------------------------------

    func test01Dashboard() {
        launch("end")
        see(main)
        see("Why does the relay drop idle phones?")
        see("Have a subagent look over the README")
        shot("01-dashboard")
        audit("dashboard")
    }

    func test02SessionAndItsTools() {
        launch("session")
        openMain()
        see("Found it")
        // How full the context is: a slim bar over the box.
        let ctx = app.buttons["contextBar"]
        XCTAssertTrue(ctx.waitForExistence(timeout: 5), "the context bar")
        XCTAssertTrue(ctx.label.hasPrefix("Context 9%"), ctx.label)
        XCTAssertGreaterThan(ctx.frame.width, app.frame.width * 0.7, "as wide as the box (\(ctx.frame))")
        XCTAssertGreaterThanOrEqual(ctx.frame.height, 44, "a finger's height to tap (\(ctx.frame))")
        shot("02-session-reply")
        audit("session")
        // The run folded to its summary; opened, each call on its card.
        tap("Read 1 file")
        see("Ran command")
        see("Run the tests")
        shot("03-session-run")
        // The command's card opened: the command and the end of its output.
        tapInConversation("Run the tests")
        see("./run-tests.sh")
        see("112 passed; 0 failed")
        shot("04-command-output")
    }

    func test03Question() {
        launch("ask")
        openMain()
        see("Should the list keep its place")
        see("Match rows by key")
        shot("05-question")
        audit("question")
        tap("Match rows by key")
        shot("06-question-answered")
    }

    func test04Approval() {
        launch("approval")
        openMain()
        see("git clean -fdx")
        shot("07-approval")
        audit("approval")
        let deny = app.buttons["Deny"].firstMatch
        XCTAssertTrue(deny.waitForExistence(timeout: 5), "Deny")
        deny.tap()
        XCTAssertTrue(gone(app.buttons["Deny"].firstMatch), "answered, the card goes")
        shot("08-approval-answered")
    }

    func test05ApprovalsTab() {
        launch("approval")
        app.tabBars.buttons["Approvals"].tap()
        see("git clean -fdx")
        shot("09-approvals-tab")
        audit("approvals")
    }

    func test06Working() {
        launch("working")
        openMain()
        see("Look for other full rebuilds")
        shot("10-working")
    }

    func test07SideQuestionAndBackgroundJob() {
        launch("aside")
        openMain()
        see("Because it only touches the rows")
        see("The background command finished after 3 s: ./build.sh")
        shot("11-side-question")
    }

    func test08HelperAndMCP() {
        launch("helpers")
        openMain()
        see("bring over a helper's changes")
        see("Update the changelog finished")
        shot("12-helper-report")
        tapInConversation("Update the changelog finished")
        // Opened: its plain line, then what the agent got, under its name.
        see("As the agent got it")
        shot("13-harlo-message-open")
        // An MCP server's tool, named in words.
        see("dice roll")
        tapInConversation("dice roll")
        shot("14-mcp-tool")
    }

    func test09Plan() {
        launch("plan")
        openMain()
        // The plan's line above the composer, opened (once more if the
        // first tap came while the transcript was still settling).
        let line = element("of 3 done")
        XCTAssertTrue(line.waitForExistence(timeout: 8), "the plan's line")
        for _ in 0..<2 {
            line.tap()
            if app.navigationBars["Plan"].waitForExistence(timeout: 4) { break }
        }
        XCTAssertTrue(app.navigationBars["Plan"].exists, "the plan opens")
        see("Test with 200 sessions")
        shot("15-plan")
        audit("plan")
    }

    /// The project's wiki from the plan's sheet: how fresh it is, its
    /// pages by folder, a page (a diagram drawn, code in a well), the
    /// pages' graph, and a search that opens its section.
    func test09bWiki() {
        launch("plan")
        openMain()
        let line = element("of 3 done")
        XCTAssertTrue(line.waitForExistence(timeout: 8), "the plan's line")
        for _ in 0..<2 {
            line.tap()
            if app.navigationBars["Plan"].waitForExistence(timeout: 4) { break }
        }
        let open = app.descendants(matching: .any)["wikiOpen"]
        for _ in 0..<6 where !(open.exists && open.isHittable) { app.swipeUp() }
        XCTAssertTrue(open.waitForExistence(timeout: 8), "the plan's sheet has the wiki")
        // (On the small phone the Update row is below the fold.)
        let update = element("Update the wiki")
        for _ in 0..<3 where !(update.exists && update.isHittable) { app.swipeUp() }
        see("1 commit since")
        see("Update the wiki")
        shot("15b-plan-wiki")
        open.tap()
        XCTAssertTrue(app.navigationBars["Wiki"].waitForExistence(timeout: 8), "the wiki opens")
        see("Quickstart")
        see("Architecture")
        see("Workflows")
        shot("15c-wiki-pages")
        audit("wiki-pages")
        tap("Graph")
        see("The wiki's pages and their links", timeout: 15)
        shot("15d-wiki-graph")
        tap("Pages")
        // (The page's row, once the list is back from the graph; a tap
        // while it switches can land nowhere, so again if the page isn't up.)
        let row = app.descendants(matching: .any).matching(identifier: "wikiPage")
            .matching(NSPredicate(format: "label BEGINSWITH %@", "Sidebar")).firstMatch
        let meta = app.descendants(matching: .any)["wikiPageMeta"]
        XCTAssertTrue(row.waitForExistence(timeout: 8), "the Sidebar page's row")
        for _ in 0..<3 where !meta.exists {
            if row.exists && row.isHittable { row.tap() }
            _ = meta.waitForExistence(timeout: 5)
        }
        see("checked against the code")
        see("Rebuilding rows")
        see("Diagram", timeout: 15)
        shot("15e-wiki-page")
        audit("wiki-page")
        // Its sources: the files and lines its claims come from.
        let source = app.descendants(matching: .any)["wikiSource"].firstMatch
        for _ in 0..<6 where !(source.exists && source.isHittable) { app.swipeUp() }
        XCTAssertTrue(source.waitForExistence(timeout: 5), "the page's sources")
        see("src/sidebar.rs")
        see("lines 1–2, 4–9")
        shot("15g-wiki-sources")
        // A search: OpenWiki's sections, a result opening its page.
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let field = app.searchFields["Search the wiki"]
        XCTAssertTrue(field.waitForExistence(timeout: 8), "the wiki's search")
        field.tap()
        field.typeText("rebuilds the whole list\n")
        let hit = app.descendants(matching: .any)["wikiHit"]
        XCTAssertTrue(hit.waitForExistence(timeout: 8), "a search result")
        see("Rebuilding rows")
        shot("15f-wiki-search")
        hit.tap()
        // (Opened at the section found, its heading on screen.)
        XCTAssertTrue(app.navigationBars["Sidebar"].waitForExistence(timeout: 8), "the result opens its page")
        see("Rebuilding rows")
    }

    func test10OtherModelAndMention() {
        launch("end")
        openMain()
        // Earlier replies were the first model's: named beside the agent.
        see("tour-coder")
        shot("16-reply-by-another-model")
        // @: the folder's other sessions beside its files.
        let field = app.textFields.firstMatch.exists ? app.textFields.firstMatch : app.textViews.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5), "the composer")
        field.tap()
        field.typeText("@")
        see("Have a subagent look over")
        shot("17-mention-list")
        tap("Have a subagent look over")
        XCTAssertTrue(((field.value as? String) ?? "").contains("@session:"), "the session goes in as @session: (\(String(describing: field.value)))")
        shot("18-mention-picked")
    }

    func test11ModelPicker() {
        launch("end")
        openMain()
        // (A tap while the conversation settles can miss: again if the sheet
        // didn't open.)
        let chip = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Model:'")).firstMatch
        XCTAssertTrue(chip.waitForExistence(timeout: 8), "the model chip")
        for _ in 0..<2 {
            chip.tap()
            if app.navigationBars["Model"].waitForExistence(timeout: 4) { break }
        }
        see("Tour Coder")
        shot("19-model-picker")
    }

    func test12ChangesAndDiff() {
        launch("end")
        openMain()
        app.buttons["Session menu"].tap()
        tap("Changed files")
        see("sidebar.rs")
        shot("20-changes")
        audit("changes")
        tap("sidebar.rs")
        see("merge_by_key")
        shot("21-diff")
        // Long lines wrap to the screen; the button keeps them whole, to
        // scroll across, and wraps them again.
        let screen = app.windows.firstMatch.frame
        let long = element("each project's sessions, newest first")
        wrapped(long, screen)
        XCTAssertTrue(long.exists && long.frame.maxX <= screen.maxX + 1, "a long line wraps (\(long.frame) in \(screen))")
        toggleWrap()
        XCTAssertTrue(long.frame.maxX > screen.maxX, "kept whole, it runs past the screen (\(long.frame))")
        shot("21-diff-whole")
        toggleWrap()
        XCTAssertTrue(long.frame.maxX <= screen.maxX + 1, "wrapped again (\(long.frame))")
    }

    func test12bFileFromAReply() {
        launch("session")
        openMain()
        // A path in the reply opens the file.
        let link = app.links.matching(NSPredicate(format: "label CONTAINS %@", "src/sidebar.rs")).firstMatch
        XCTAssertTrue(link.waitForExistence(timeout: 8), "the reply's path is a link")
        link.tap()
        see("merge_by_key")
        let screen = app.windows.firstMatch.frame
        let long = element("each project's sessions, newest first")
        let short = element("use crate::projects::project_rows")
        wrapped(long, screen)
        XCTAssertTrue(long.exists && long.frame.maxX <= screen.maxX + 1, "a long line wraps (\(long.frame) in \(screen))")
        // (Its folder above the code, not its name again: that's the title.)
        see("In src")
        shot("21b-file")
        let top = short.frame.minY
        // Kept whole: every line starts at the left, after its number, and
        // the long one runs past the screen; the file still starts at the
        // top (scrolling both ways, a short one sat in the screen's middle).
        toggleWrap()
        XCTAssertEqual(short.frame.minY, top, accuracy: 2, "kept whole, it starts at the top (\(short.frame))")
        XCTAssertTrue(long.frame.maxX > screen.maxX, "kept whole, it runs past the screen (\(long.frame))")
        XCTAssertTrue(short.frame.minX < 80 && abs(short.frame.minX - long.frame.minX) < 1,
                      "lines start at the left, lined up (\(short.frame), \(long.frame))")
        XCTAssertEqual(short.frame.height, long.frame.height, accuracy: 1, "a line's height is one row's")
        shot("21c-file-whole")
        toggleWrap()
        XCTAssertTrue(long.frame.maxX <= screen.maxX + 1, "wrapped again (\(long.frame))")
    }

    func test13ActionLog() {
        launch("end")
        openMain()
        app.buttons["Session menu"].tap()
        tap("Action log")
        see("run-tests.sh")
        shot("22-action-log")
    }

    func test14ClaudeSession() {
        launch("end")
        tap("Have a subagent look over")
        // Its run folded; opened, the subagent's call.
        tapInConversation("started 1 subagent")
        see("Subagent")
        shot("23-claude-session")
    }

    func test14bBackAtTheEnd() {
        launch("end")
        // Opened, left for another session and opened again: at its end
        // each time.
        func atEnd(_ why: String, file: StaticString = #filePath, line: UInt = #line) {
            let last = element("The session list keeps its place now")
            // (20 s: at a large text size the session is 64 pages, and each look
            // at what is on screen takes seconds.)
            XCTAssertTrue(last.waitForExistence(timeout: 20), "the last reply", file: file, line: line)
            // (Settled.)
            Thread.sleep(forTimeInterval: 1.5)
            // (Its end: at a large text size it's taller than the room above
            // the panels.)
            let bottom = app.otherElements["sessionBottom"].frame
            XCTAssertTrue(last.frame.maxY > 0 && last.frame.maxY <= bottom.minY + 1, "\(why): the last reply's end in sight (\(last.frame), the composer at \(bottom.minY))", file: file, line: line)
        }
        openMain()
        atEnd("opened")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap("Have a subagent look over")
        see("started 1 subagent")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        openMain()
        atEnd("opened again")
    }

    func test15OtherTabs() {
        launch("end")
        // The header (harlo's mark, the computer's name, its dot) sits in
        // the same place on every tab.
        // (Other tabs may stay loaded: the one on screen.)
        func header() -> CGRect? {
            let all = app.staticTexts.matching(identifier: "appHeaderName")
            XCTAssertTrue(all.firstMatch.waitForExistence(timeout: 8), "a header")
            return all.allElementsBoundByIndex.first { $0.isHittable }?.frame
        }
        guard let home = header() else { return XCTFail("the dashboard's header") }
        func sameHeader(_ tab: String) {
            app.tabBars.buttons[tab].tap()
            guard let f = header() else { return XCTFail("\(tab)'s header") }
            XCTAssertEqual(f.minX, home.minX, accuracy: 1, "\(tab)'s header across: \(f) against the dashboard's \(home)")
            XCTAssertEqual(f.midY, home.midY, accuracy: 1, "\(tab)'s header down: \(f) against the dashboard's \(home)")
        }
        sameHeader("Approvals")
        sameHeader("Settings")
        sameHeader("New Session")
        // (A field's placeholder isn't its label.)
        let field = app.descendants(matching: .any).matching(NSPredicate(format: "placeholderValue == %@", "What should the agent do?")).firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 8), "the new session's message box")
        shot("24-new-session")
        audit("new-session")
        app.tabBars.buttons["Settings"].tap()
        shot("25-settings")
        audit("settings")
    }

    func test16Pictures() {
        launch("pictures")
        openMain()
        // The reply's picture in its place, as wide as the reply, not its
        // words, and not again under it.
        let reply = app.buttons["replyPicture"]
        XCTAssertTrue(reply.waitForExistence(timeout: 15), "the reply's picture")
        XCTAssertEqual(reply.label, "Picture: The sidebar")
        XCTAssertGreaterThan(reply.frame.width, 150, "the reply's picture is big (\(reply.frame))")
        shot("26-pictures")
        audit("pictures")
        // (At large text sizes the picture can be under the header: brought
        // into view first.)
        for _ in 0..<6 where reply.frame.minY < app.frame.height * 0.12 {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.32)))
        }
        // The viewer: at the one tapped, then across to your photo.
        reply.tap()
        let viewer = app.descendants(matching: .any).matching(identifier: "pictureViewer").firstMatch
        XCTAssertTrue(viewer.waitForExistence(timeout: 8), "the viewer")
        see("2 of 2")
        // (Named by the reply's words.)
        XCTAssertTrue(app.staticTexts["The sidebar"].waitForExistence(timeout: 5), "the picture's name")
        shot("27-picture-viewer")
        audit("picture-viewer")
        // The grid: every picture over its name, the open one marked; a tap
        // opens one.
        app.buttons["pictureGrid"].tap()
        let cells = app.buttons.matching(identifier: "pictureGridCell")
        XCTAssertTrue(cells.firstMatch.waitForExistence(timeout: 5), "the grid")
        see("All pictures")
        XCTAssertEqual(cells.count, 2, "both pictures")
        XCTAssertEqual(cells.element(boundBy: 0).label, "Picture: You sent")
        XCTAssertTrue(cells.element(boundBy: 1).isSelected, "the open picture is marked")
        XCTAssertFalse(app.buttons["pictureShare"].exists, "Share is for one picture")
        // Below the top bar at any text size, not under it.
        XCTAssertGreaterThanOrEqual(cells.element(boundBy: 0).frame.minY, app.buttons["pictureGridBack"].frame.maxY,
                                    "the grid starts below the top bar")
        shot("27b-picture-grid")
        audit("picture-grid")
        // Back: to the picture it came from.
        app.buttons["pictureGridBack"].tap()
        XCTAssertTrue(gone(cells.firstMatch), "Back leaves the grid")
        see("2 of 2")
        app.buttons["pictureGrid"].tap()
        XCTAssertTrue(cells.firstMatch.waitForExistence(timeout: 5), "the grid again")
        cells.element(boundBy: 1).tap()
        XCTAssertTrue(gone(cells.firstMatch), "a tap opens it")
        see("2 of 2")
        viewer.swipeRight()
        see("1 of 2")
        see("You sent")
        XCTAssertTrue(app.buttons["pictureShare"].waitForExistence(timeout: 5), "Share")
        shot("28-picture-photo")
        app.buttons["pictureDone"].tap()
        XCTAssertTrue(gone(viewer), "Done closes it")
        // Your photo, in your message above: the only thumbnail.
        let tiles = app.buttons.matching(identifier: "pictureTile")
        for _ in 0..<6 where !(tiles.firstMatch.exists && tiles.firstMatch.isHittable) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.55)))
        }
        XCTAssertTrue(tiles.firstMatch.waitForExistence(timeout: 5), "your photo's thumbnail")
        XCTAssertEqual(tiles.count, 1, "only your photo is a thumbnail")
        XCTAssertEqual(tiles.firstMatch.label, "Picture: You sent")
    }
}
