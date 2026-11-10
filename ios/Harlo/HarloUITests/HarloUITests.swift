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
    func shot(_ name: String) {
        // (Animations settle.)
        Thread.sleep(forTimeInterval: 0.6)
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
        XCTAssertTrue(long.exists && long.frame.maxX <= screen.maxX + 1, "a long line wraps (\(long.frame) in \(screen))")
        app.buttons["Show long lines whole"].tap()
        XCTAssertTrue(app.buttons["Wrap long lines"].waitForExistence(timeout: 4), "the button now wraps them")
        XCTAssertTrue(long.frame.maxX > screen.maxX, "kept whole, it runs past the screen (\(long.frame))")
        shot("21-diff-whole")
        app.buttons["Wrap long lines"].tap()
        XCTAssertTrue(app.buttons["Show long lines whole"].waitForExistence(timeout: 4) && long.frame.maxX <= screen.maxX + 1, "wrapped again")
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

    func test15OtherTabs() {
        launch("end")
        app.tabBars.buttons["New Session"].tap()
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
