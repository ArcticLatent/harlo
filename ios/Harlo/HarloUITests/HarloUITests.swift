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
/// simulator's. (On the main actor, as XCUIApplication is.)
@MainActor
final class HarloUITests: XCTestCase {
    var app = XCUIApplication()
    /// The main session: a fix, with its tool calls and approvals.
    let main = "jumps to the top"

    override func setUp() async throws {
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
    /// (the message box): at the largest text
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

    /// Scrolls the conversation until `text` sits just under the header, for
    /// a shot of what follows it (a run opened, a card's output): opening
    /// one doesn't move the conversation, which stays at its end.
    func showUnderHeader(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        let e = element(text)
        guard e.waitForExistence(timeout: 8) else {
            XCTFail("nothing to show: \(text)", file: file, line: line)
            return
        }
        let top = app.frame.height * 0.17
        let origin = app.coordinate(withNormalizedOffset: .zero)
        let x = app.frame.width * 0.5
        for _ in 0..<6 where abs(e.frame.minY - top) > 30 {
            let distance = max(-app.frame.height * 0.45, min(app.frame.height * 0.45, top - e.frame.minY))
            let from = app.frame.height * 0.5 - distance / 2
            // (Held at the end, so it doesn't fling on.)
            origin.withOffset(CGVector(dx: x, dy: from))
                .press(forDuration: 0.05, thenDragTo: origin.withOffset(CGVector(dx: x, dy: from + distance)), withVelocity: .slow, thenHoldForDuration: 0.2)
        }
        XCTAssertTrue(e.isHittable, "in view: \(text) (\(e.frame))", file: file, line: line)
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

    /// Opens the main session from the dashboard.
    func openMain(file: StaticString = #filePath, line: UInt = #line) {
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@", main)).firstMatch
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
        see("What does the README say about the app?")
        shot("01-dashboard")
        audit("dashboard")
        // (At the largest text sizes the third is below the fold: the list
        // makes it once it's scrolled to.)
        let third = element("Why does the relay drop idle phones?")
        for _ in 0..<3 where !third.waitForExistence(timeout: 2) { app.swipeUp() }
        see("Why does the relay drop idle phones?")
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
        // (On the smallest phone the reply pushes it above the screen, and a
        // plain tap doesn't always scroll it back.)
        showUnderHeader("Read 1 file")
        tap("Read 1 file")
        see("Ran command")
        see("./run-tests.sh")
        showUnderHeader("Read 1 file")
        shot("03-session-run")
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
        see("./build.sh")
        shot("10-working")
    }

    func test10FileMention() {
        launch("end")
        openMain()
        // @: the session folder's files, picked into the message.
        let field = app.textFields.firstMatch.exists ? app.textFields.firstMatch : app.textViews.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5), "the composer")
        field.tap()
        field.typeText("@")
        see("src/sidebar.rs")
        shot("17-mention-list")
        tap("src/sidebar.rs")
        XCTAssertTrue(((field.value as? String) ?? "").contains("@src/sidebar.rs"), "the file goes in as @src/sidebar.rs (\(String(describing: field.value)))")
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
        // (A tap while the session was still settling sometimes didn't open it.)
        if !element("Changed files").waitForExistence(timeout: 4) { app.buttons["Session menu"].tap() }
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

    func test14bBackAtTheEnd() {
        launch("end")
        // Opened, left for another session and opened again: at its end
        // each time.
        func atEnd(_ why: String, file: StaticString = #filePath, line: UInt = #line) {
            let last = element("The session list keeps its place now")
            // (20 s: at a large text size the session is 64 pages, and each look
            // at what is on screen takes seconds.)
            XCTAssertTrue(last.waitForExistence(timeout: 20), "the last reply", file: file, line: line)
            // (Its end: at a large text size it's taller than the room above
            // the panels. Settled: looked at again for up to 10 s on a slow
            // simulator.)
            var inSight = false
            var said = ""
            for _ in 0..<5 {
                Thread.sleep(forTimeInterval: 2)
                let bottom = app.otherElements["sessionBottom"].frame
                let end = last.frame
                said = "\(end), the composer at \(bottom.minY)"
                inSight = end.maxY > 0 && end.maxY <= bottom.minY + 1
                if inSight { break }
            }
            XCTAssertTrue(inSight, "\(why): the last reply's end in sight (\(said))", file: file, line: line)
        }
        openMain()
        atEnd("opened")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap("What does the README say")
        see("Sure: What does the README say")
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
}
