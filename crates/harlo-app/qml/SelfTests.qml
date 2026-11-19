import QtQuick
import QtQuick.Controls.Basic
import harlo

// harlo's click-driven self-tests (`--selftest`): each scenario a
// SequentialAnimation that runs when `--scenario=` names it, with the
// helpers they share. Made only with --selftest (Main.qml's Loader), so a
// normal start builds none of it. The window and the parts the scenarios
// reach are passed in under their ids' names.
Item {
    id: tests
    required property var win
    required property var activity
    required property var changes
    required property var chat
    required property var clipboard
    required property var files
    required property var frames
    required property var git
    required property var graphicsInfo
    required property var notes
    required property var page
    required property var pairDeviceDialog
    required property var commandPalette
    required property var phoneFirewallToast
    required property var rail
    required property var rightPanel
    required property var searchDialog
    required property var settingsDialog
    required property var setupGuide
    required property var shortcutsDialog
    required property var sidebar
    required property var sidebarPane
    required property var signInDialog
    required property var split
    required property var terminal
    required property var terminalPane
    required property var tipStyle
    required property var updateToasts
    required property var wikiPanel
    required property var windowSettings

    property real t0: 0
    function shot(name) { grabItem(page, name) }
    /// An item's rectangle of the window, taken now (what the test does
    /// next isn't in it).
    function grabItem(item, name) {
        const path = selftestDir + "/" + name + ".png"
        const p = item.mapToItem(null, 0, 0)
        console.log("selftest:", TestInput.grabArea(path, p.x, p.y, item.width, item.height) ? "saved" : "grab failed", path)
    }
    /// The whole window, popups and their shadows included.
    property real hoverLineWidth: -1
    // The window's height before a test made it short.
    property real heightWas: 0
    property var workflowNamed: null
    // The models picked in the workflow dialog.
    property var workflowModels: ({})
    function shotWindow(name) {
        const path = selftestDir + "/" + name + ".png"
        console.log("selftest:", TestInput.grabWindow(path) ? "saved" : "grab failed", path)
    }
    // A popup (menu, dialog): its item is the background's parent.
    function shotPopup(popup, name) { grabItem(popup.background.parent, name) }
    function log(...args) { console.log("selftest:", ...args) }
    // ---- synthetic input: real mouse/key events (TestInput, Qt Test) ----
    property int checksPassed: 0
    property int checksFailed: 0
    function check(ok, what) {
        if (ok) checksPassed++; else checksFailed++
        console.log("selftest:", ok ? "PASS" : "FAIL", what)
    }
    function shown(it) { return it && it.visible && it.opacity > 0 && it.width > 0 && it.height > 0 }
    /// The draft's Run as workflow: labelled when wide, an icon when narrow.
    function draftWorkflow() { const b = findName("draftWorkflowButton"); return shown(b) ? b : findName("draftWorkflowIcon") }
    /// The first shown item (depth first) whose `text` is `text`, under
    /// `root` (default: the whole window, popups included).
    // The window's content and its popup layer (an ApplicationWindow keeps
    // popups out of the content's tree).
    function searchRoots(root) { return root ? [root] : [Overlay.overlay, win.contentItem] }
    function findText(text, root) {
        for (const r of searchRoots(root)) {
            const f = findTextIn(text, r)
            if (f) return f
        }
        return null
    }
    // Containers may have no size of their own (e.g. a scrolling strip),
    // so only the match itself must be shown with a size.
    function findTextIn(text, root) {
        for (const c of root.children) {
            if (!c.visible || c.opacity <= 0) continue
            if (c.text === text && shown(c)) return c
            const f = findTextIn(text, c)
            if (f) return f
        }
        return null
    }
    function findName(name, root) {
        for (const r of searchRoots(root)) {
            const f = findNameIn(name, r)
            if (f) return f
        }
        return null
    }
    function findNameIn(name, root) {
        for (const c of root.children) {
            if (!c.visible || c.opacity <= 0) continue
            if (c.objectName === name && shown(c)) return c
            const f = findNameIn(name, c)
            if (f) return f
        }
        return null
    }
    /// Every visible item named `name` under `root` (the ones in view).
    function findAllNames(name, root) {
        const found = []
        const walk = item => {
            for (const c of item.children) {
                if (!c.visible || c.opacity <= 0) continue
                if (c.objectName === name && shown(c)) found.push(c)
                walk(c)
            }
        }
        walk(root)
        return found
    }
    /// Scrolls every scroll area around `item` so it is in view, as a
    /// person would before clicking it.
    function scrollIntoView(item) {
        for (let f = item.parent; f; f = f.parent) {
            if (f.contentY === undefined || f.contentItem === undefined || f.contentHeight === undefined || f.height <= 0) continue
            // (A list's content can start above 0: its origin.)
            const top = item.mapToItem(f.contentItem, 0, 0).y, start = f.originY || 0
            if (top < f.contentY) f.contentY = Math.max(start, top - 12)
            else if (top + item.height > f.contentY + f.height) f.contentY = Math.min(start + f.contentHeight + (f.bottomMargin || 0) - f.height, top + item.height - f.height + 12)
        }
    }
    /// Clicks the middle of `item` with a real mouse event.
    /// Every git service card under `root` done checking its sign-in.
    function gitHostsChecked(root) {
        return ["github", "gitlab", "forgejo"].every(s => { const c = findName("gitHost-" + s, root); return c !== null && c.st.state !== "checking" })
    }
    /// What's shown under `box` and reaches past its left or right edge
    /// (by more than half a pixel): controls and words, by text.
    function spills(box) {
        const out = []
        const walk = it => {
            for (const c of it.children) {
                if (!c.visible || c.opacity <= 0 || c.width <= 0) continue
                if (c.text !== undefined || c.checked !== undefined) {
                    const p = c.mapToItem(box, 0, 0)
                    if (p.x < -0.5 || p.x + c.width > box.width + 0.5) out.push((c.text || c.objectName || "?") + " @" + Math.round(p.x) + "+" + Math.round(c.width))
                }
                walk(c)
            }
        }
        walk(box)
        return out
    }
    function clickItem(item, what) {
        if (!item) { check(false, "find " + what); return }
        scrollIntoView(item)
        const p = item.mapToItem(null, item.width / 2, item.height / 2)
        TestInput.click(p.x, p.y)
    }
    function clickText(text, root) { clickItem(findText(text, root), "\"" + text + "\"") }
    /// The highest on screen of the shown items reading `text` (a list's
    /// children aren't in its order: rows kept come before rows made).
    function topText(text, root) {
        let best = null
        const walk = item => {
            for (const c of item.children) {
                if (!c.visible || c.opacity <= 0) continue
                if (c.text === text && shown(c) && (!best || c.mapToItem(null, 0, 0).y < best.mapToItem(null, 0, 0).y)) best = c
                walk(c)
            }
        }
        walk(root)
        return best
    }
    /// The nearest ancestor of `item` that has property `prop`.
    function ancestorWith(item, prop) {
        for (let it = item; it; it = it.parent)
            if (it[prop] !== undefined) return it
        return null
    }
    /// Types into the composer and sends with Enter.
    function sendByKeys(text) {
        clickItem(findName("composerInput"), "composer")
        TestInput.typeText(text)
        TestInput.key(Qt.Key_Return, 0)
    }
    function summary() { log("checks:", checksPassed, "passed,", checksFailed, "failed") }
    /// Holds a scenario (`anim`) until `ready()` is true, or `ms` pass,
    /// instead of a fixed pause for something whose time varies. (A short
    /// PauseAnimation must follow: the step after a ScriptAction runs in
    /// the same frame.)
    function waitUntil(anim, ready, ms) {
        if (ready()) return
        anim.pause()
        waiter.anim = anim
        waiter.ready = ready
        waiter.deadline = Date.now() + ms
        waiter.since = Date.now()
        waiter.start()
    }
    Timer {
        id: waiter
        property var anim: null
        property var ready: null
        property real deadline: 0
        property real since: 0
        interval: 200
        repeat: true
        onTriggered: {
            if (!ready() && Date.now() < deadline) return
            stop()
            tests.log("waited", Date.now() - since, "ms")
            anim.resume()
        }
    }
    TextFile { id: testFile }
    function sidebarCount() { let n = 0; while (Workspace.keyAt(n) !== "") n++; return n }
    property int sessionsBefore: 0
    Connections {
        target: chat.transcript
        function onCountChanged() {
            if (tests.t0 > 0 && chat.transcript.count >= 5000) {
                tests.log("5000 rows after", Date.now() - tests.t0, "ms")
                tests.t0 = 0
            }
        }
    }

    // Demo harness: history, streaming, scroll.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "demo"
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.t0 = Date.now(); chat.session.loadDemoHistory(500) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.log("rows after history:", chat.transcript.count, "follow:", chat.transcript.follow) }
        ScriptAction { script: { chat.transcript.scrollToEnd(); chat.submit("stream a demo reply fast", "") } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: tests.shot("demo-streaming") }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: tests.log("rows at end:", chat.transcript.count, "status:", chat.session.status) }
        ScriptAction { script: tests.shot("demo-done") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Real harness, no prompts: project + new session, then models,
    // thinking levels, slash commands, context stats and file search.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "harness"
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.newSession(1, win.argValue("--harness="))
            }
        }
        PauseAnimation { duration: 9000 }
        ScriptAction {
            script: {
                tests.log("status:", chat.session.status, "modelId:", chat.session.modelId, "model:", chat.session.modelName, "thinking:", chat.session.thinking,
                        "commands:", chat.session.commands.length, "context:", chat.session.contextText, "cwd:", chat.session.cwd)
                chat.session.loadModels()
                chat.session.loadThinkingLevels()
                chat.session.searchFiles("transcript")
            }
        }
        PauseAnimation { duration: 3000 }
        ScriptAction {
            script: {
                tests.log("models:", chat.session.models.length, "first:", chat.session.models[0])
                tests.log("thinking levels:", chat.session.thinkingLevels.join(","))
                tests.log("files for 'transcript':", chat.session.fileMatches.slice(0, 3).join(" | "))
                composerProbe.start()
            }
        }
    }
    // Types "/" into the composer to show the command popup.
    SequentialAnimation {
        id: composerProbe
        ScriptAction { script: { chat.composerItem.focusInput(); chat.composerItem.text = "/"; chat.composerItem.cursorToEnd() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.log("completion:", chat.composerItem.completionMode, chat.composerItem.completionItems.length, "popup:", chat.composerItem.completionVisible, "focus:", chat.composerItem.inputFocused, "active:", win.active) }
        ScriptAction { script: tests.shot("harness-" + win.argValue("--harness=")) }
        ScriptAction { script: chat.composerItem.grabCompletion(win.selftestDir + "/completion.png") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Changes panel against a scratch repo (`--cwd`): select, stage a hunk, commit.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "changes"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                tests.log("repo:", git.isRepo, "branch:", git.branch, "files:", git.files.length, "staged:", git.stagedCount, "unstaged:", git.unstagedCount)
                git.select("src/app.rs", false)
            }
        }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("selected:", git.selectedPath, git.diffStats); tests.shot("changes-diff") } }
        ScriptAction { script: git.stageHunk(0) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after stage hunk: staged", git.stagedCount, "unstaged", git.unstagedCount, "msg:", git.message); tests.shot("changes-staged") } }
        ScriptAction { script: git.commit("harlo selftest commit") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after commit: staged", git.stagedCount, "unstaged", git.unstagedCount, "error:", git.error, "msg:", git.message); git.commitAll("harlo selftest commit all") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.log("after commit all: staged", git.stagedCount, "unstaged", git.unstagedCount, "error:", git.error, "msg:", git.message) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // ---- click-driven checks of recent features (real input) ----------
    // Approvals: Approve on the tool row, the waiting footer and the bar;
    // Deny in the bar.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksapproval"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.openDraft(-1); chat.draftView.harness = "demo"; if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 600 }
        // The new-session page's shield: click once, start in the next mode.
        ScriptAction { script: {
            const d = chat.draftView, modes = ["always-ask", "write", "yolo"]
            tests.approvalNext = modes[(modes.indexOf(d.shownApproval) + 1) % 3]
            tests.check(modes.includes(d.shownApproval) && tests.findName("draftPermissionsButton", d).visible, "the new-session page has the shield (" + d.shownApproval + ")")
            tests.clickItem(tests.findName("draftPermissionsButton", d), "the new-session shield") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.draftView.shownApproval === tests.approvalNext, "clicking it switches to " + tests.approvalNext)
            tests.clickItem(tests.findName("draftInput"), "draft input"); TestInput.typeText("fast: please approve the listing"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            tests.check(Workspace.approvalMode(chat.session.key) === tests.approvalNext, "the session starts in that mode (" + Workspace.approvalMode(chat.session.key) + ")")
            tests.check(chat.session.requestIsApproval, "the prompt asks for approval")
            tests.check(tests.findText("Run this command?", chat.transcript) !== null, "the bash row asks 'Run this command?'")
            tests.check(tests.findName("sessionStatus", chat).text === "needs you" && Qt.colorEqual(tests.findName("sessionStatus", chat).color, Theme.warning), "the header says it needs you, not 'working'")
            tests.check(tests.findAllNames("runLive", chat.transcript).filter(d => d.visible).every(d => Qt.colorEqual(d.color, Theme.warning)), "the run's dot is the warning colour while it waits")
            tests.check(tests.findText("Waiting for your approval", chat.transcript) === null, "the footer doesn't say it again")
            // (The bar only when the row is out of view.)
            tests.check(!tests.shown(chat.requestDock), "with its row in view, no bar besides")
            tests.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.check(!chat.session.hasRequest && tests.findText("Waiting for your approval", chat.transcript) === null, "Approve on the row runs it"); tests.sendByKeys("fast: approve again") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(chat.session.requestIsApproval && tests.findText("Run this command?", chat.transcript) !== null, "the next one asks again")
            const q = tests.findAllNames("approvalQuestion", chat.transcript)[0]
            tests.check(q && !q.truncated && q.lineCount >= 1, "its question whole, wrapped when narrow (" + (q ? q.lineCount + " lines" : "none") + ")")
            tests.shotWindow("clicks-approval-again") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Deny", chat.transcript) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.check(!chat.session.hasRequest && tests.findAllNames("toolMark", chat.transcript).some(m => m.name === "close"), "Deny fails the call"); tests.summary(); Qt.quit() } }
    }

    // Motion: the sidebar and the side panel slide open and shut (caught
    // half way), keep their width after; with Animations at 0 they snap.
    property real slideWas: 0
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksmotion"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.newSession(-1, "demo")
            Workspace.chooseLook("motion", "600") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.slideWas = sidebarPane.width; win.togglePanel("activity") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(rightPanel.sliding && rightPanel.width > 0 && rightPanel.width < rightPanel.target, "the side panel slides open (" + Math.round(rightPanel.width) + " of " + Math.round(rightPanel.target) + ")")
            tests.check(activity.width === rightPanel.target, "its panel keeps its width meanwhile (" + activity.width + ")")
            tests.shotWindow("clicks-motion-panel") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(!rightPanel.sliding && rightPanel.width >= 240 && activity.width === rightPanel.width, "then it's open, its panel as wide (" + Math.round(rightPanel.width) + ")")
            win.toggleSidebar() } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(sidebarPane.sliding && sidebarPane.width < tests.slideWas && sidebar.width === sidebarPane.target, "the sidebar slides shut, its contents whole (" + Math.round(sidebarPane.width) + ")")
            tests.shotWindow("clicks-motion-sidebar") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(!sidebar.visible, "then it's gone")
            win.toggleSidebar() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            // (Its width: as fitPanes fits it beside the open panel.)
            tests.check(sidebar.visible && !sidebarPane.sliding && sidebarPane.width >= 160 && sidebar.width === sidebarPane.width, "and back, its contents as wide (" + Math.round(sidebarPane.width) + ")")
            Workspace.chooseLook("motion", "0")
            win.togglePanel("activity") } }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.check(!rightPanel.visible, "Animations at 0: it shuts at once")
            Workspace.chooseLook("motion", "600")
            win.toggleTerminal() } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(terminalPane.sliding && terminalPane.height < terminalPane.target && terminal.height === terminalPane.target, "the terminal slides up, whole (" + Math.round(terminalPane.height) + " of " + Math.round(terminalPane.target) + ")") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(!terminalPane.sliding && terminal.visible && terminal.height === terminalPane.height && terminalPane.height >= 110, "then it's open (" + Math.round(terminalPane.height) + ")")
            win.toggleTerminal() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(!terminal.visible, "and slides shut")
            tests.summary(); Qt.quit() } }
    }

    // Forgejo, one's own server (plain http, its own port): the branch's
    // pull request is opened from the Changes panel through its API (the
    // token git keeps), its checks (commit statuses) show and are watched.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksforgejo"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("forgejo_host", win.argValue("--forgejo-host="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo")
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            tests.check(req.service === "forgejo" && req.repo === "octo/app" && req.head === "fix/login-loop" && req.canCreate && req.url === "", "a Forgejo server of one's own: none yet, and harlo can open one (" + git.requestJson + ")")
            tests.clickItem(tests.findName("requestButton"), "Create pull request") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("requestCreate", changes.requestDialog.footer), "Create") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            tests.check(req.url.endsWith("/octo/app/pulls/3") && req.number === 3, "created through Forgejo's API (" + git.requestJson + ")")
            tests.check(tests.findName("checksSummary").text === "Checks running · 0 of 1 done", "its checks show, still running")
            TestInput.writeFile(win.selftestDir + "/forgejo-statuses.json", JSON.stringify([
                { context: "ci / build (push)", status: "success" },
                { context: "ci / lint (push)", status: "failure", target_url: "http://forgejo/octo/app/actions/runs/1" }]))
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const n = tests.lastNote()
            tests.check(n.kind === "ci" && n.title === "Checks failed: Pull request #3", "harlo notifies when they finish (" + Workspace.lastNotification + ")")
            git.loadRequest() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findName("checksSummary").text === "Failed: ci / lint (push)", "and the panel shows what failed")
            tests.shot("clicks-forgejo")
            tests.summary(); Qt.quit() } }
    }

    // The Changes list keeps its rows and its place as git's status
    // changes: staging a file far down doesn't take the list to the top.
    property var listRow: null
    property real listY: 0
    function changesRowAt(key) {
        const rows = tests.namedAny("changesFiles", changes).model
        for (let i = 0; i < (rows.count !== undefined ? rows.count : rows.length); i++) {
            const r = rows.count !== undefined ? rows.get(i) : rows[i]
            if ((r.key || (r.header ? "h:" + r.header : (r.staged ? "s:" : "u:") + r.path)) === key) return i
        }
        return -1
    }
    SequentialAnimation {
        id: changesListSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickschangeslist"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            for (let i = 10; i < 70; i++) TestInput.writeFile(win.argValue("--cwd=") + "/f" + i + ".txt", "file " + i + "\n")
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo")
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        ScriptAction { script: tests.waitUntil(changesListSteps, () => tests.changesRowAt("u:f69.txt") >= 0, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.namedAny("changesFiles", changes).positionViewAtEnd() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const list = tests.namedAny("changesFiles", changes)
            tests.listRow = list.itemAtIndex(tests.changesRowAt("u:f60.txt")); tests.listY = list.contentY
            tests.check(tests.listRow !== null && list.contentY > 100, "scrolled down the list (" + Math.round(list.contentY) + ")")
            git.stageFile("f65.txt") } }
        ScriptAction { script: tests.waitUntil(changesListSteps, () => tests.changesRowAt("s:f65.txt") >= 0, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const list = tests.namedAny("changesFiles", changes)
            tests.check(list.contentY > 100, "staging a file far down keeps the list where it was (" + Math.round(list.contentY) + ", was " + Math.round(tests.listY) + ")")
            tests.check(list.itemAtIndex(tests.changesRowAt("u:f60.txt")) === tests.listRow, "the other rows stay as they were")
            tests.summary(); Qt.quit() } }
    }

    // A folder of repositories: the Changes panel lists each with how many
    // files it changed, shows the first with changes, switches on a click,
    // and commits in the one shown.
    function repoNames() { return JSON.parse(git.reposJson || "[]").map(r => r.name + ":" + r.changes) }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksrepos"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            const multi = win.argValue("--cwd=") + "/../multi"
            Workspace.addProject(multi)
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo")
            windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(tests.repoNames().join() === "alpha:2,gamma:0,group/beta:1", "the folder's repositories, a level down too (" + tests.repoNames() + ")")
            tests.check(git.isRepo && git.root.endsWith("/alpha") && git.files.length === 2, "the first with changes is shown (" + git.root + ", " + git.files.length + ")")
            const req = tests.findName("repoRequest-gamma", changes)
            tests.check(req !== null && req.children[1].text === "#9" && String(req.children[0].color) === String(Theme.error),
                      "gamma has no changes but an open pull request: listed, with #9 and its failed checks (" + git.requestsJson + ")")
            tests.check(tests.findName("repoShowClean", changes) === null, "nothing is folded away")
            tests.check(win.changeTotal === 3, "the panel's count is all of them (" + win.changeTotal + ")")
            tests.check(tests.repoNames().join() === "alpha:2,gamma:0,group/beta:1", "(the repositories as before)")
            tests.shot("clicks-repos") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("repo-group/beta", changes), "group/beta") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(git.root.endsWith("/group/beta") && git.files.length === 1 && git.files[0].startsWith("README.md"), "a click shows another (" + git.root + ")")
            git.commitAll("Beta's change") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.check(tests.repoNames().join() === "alpha:2,gamma:0,group/beta:0" && git.root.endsWith("/group/beta"), "committing there leaves the others as they were (" + tests.repoNames() + ")")
            tests.clickItem(tests.findName("repoShowClean", changes), "without changes") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findName("repo-group/beta", changes) !== null, "beta, now without changes, folds away and unfolds")
            // Undo across them: a turn, then the agent "edits" two repositories.
            chat.composerItem.text = "fast: change two of them"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            const multi = win.argValue("--cwd=") + "/../multi"
            TestInput.writeFile(multi + "/gamma/README.md", "the agent was here\n")
            TestInput.writeFile(multi + "/group/beta/added.txt", "new\n")
            chat.session.loadUndo() } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const undo = JSON.parse(chat.session.undoJson || "{}"), first = Object.values(undo)[0] || {}
            tests.check(Object.keys(undo).length === 1 && String(first.snapshot).startsWith("@") && first.files === 2, "the turn has a snapshot of the folder's repositories (" + chat.session.undoJson + ")")
            chat.session.previewUndo(Object.keys(undo)[0]) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson)
            tests.check(plan.restore.join() === "gamma/README.md" && plan.remove.join() === "group/beta/added.txt", "undoing it covers both, by their place in the folder (" + chat.session.undoPlanJson + ")")
            chat.session.undoTurn(plan.row) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const multi = win.argValue("--cwd=") + "/../multi"
            tests.check(JSON.parse(chat.session.undoPlanJson).state === "done" && TestInput.readFile(multi + "/gamma/README.md") === "one\nfix\n" && TestInput.readFile(multi + "/group/beta/added.txt") === "", "and puts both back")
            tests.check(TestInput.readFile(multi + "/alpha/notes.md") === "new\n", "your own changes from before the turn stay")
            tests.summary(); Qt.quit() } }
    }

    // A session as Markdown: saved to a file, and copied.
    property string exportKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksexport"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); tests.exportKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("fast: run the tests") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: Workspace.exportMarkdown(tests.exportKey, win.selftestDir + "/session.md") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const md = TestInput.readFile(win.selftestDir + "/session.md")
            tests.check(md.startsWith("# fast: run the tests\n\n*Demo · Claude Demo · "), "Export writes the title and what it ran on (" + md.slice(0, 80) + ")")
            tests.check(md.indexOf("## You") > 0 && md.indexOf("## Demo · ") > 0 && md.indexOf("\n## Summary") < 0 && md.indexOf("- **bash** `cargo test` → 12 passed") > 0, "your message, the reply and its command (" + md.length + " characters)")
            tests.check(Workspace.notice.startsWith("Saved as ") && !Workspace.noticeError, "and says where (" + Workspace.notice + ")")
            Workspace.copyMarkdown(tests.exportKey) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(Workspace.clipboardText === TestInput.readFile(win.selftestDir + "/session.md"), "Copy as Markdown gives the same")
            tests.check(win.paletteActions().some(a => a.title === "Export session as Markdown…"), "the command palette has it")
            tests.summary(); Qt.quit() } }
    }

    // The Activity panel: what a session ran and touched, from its messages.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksactivity"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("fast: edit the notes") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: tests.sendByKeys("fast: run the tests") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: tests.clickItem(tests.findName("rail-activity") || null, "the rail's Activity button") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(windowSettings.rightMode === "activity", "the rail opens the Activity panel")
            const log = activity.log
            tests.check(log && log.actions.length === 2 && log.commands === 1 && log.failed === 0 && log.changed.join() === "notes.md",
                      "it lists the edit and the command (" + JSON.stringify(log) + ")")
            const first = tests.findName("activity-0", activity)
            tests.check(first !== null && log.actions[1].target === "cargo test" && log.actions[1].result === "12 passed", "newest first, with how it went (" + (log ? log.actions[1].result : "") + ")")
            tests.shot("clicks-activity") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("activityFilter-change", activity), "files changed") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(activity.shown.length === 1 && activity.shown[0].kind === "edit", "the filter shows only the changes")
            tests.check(activity.markdown().indexOf("**bash** `cargo test` — 12 passed") > 0, "Copy gives Markdown")
            tests.summary(); Qt.quit() } }
    }

    // What Harlo's sessions cost: the scripted model priced in Model
    // servers ($100 in, $1000 out a million tokens: each of its requests,
    // 1200 in and 20 out, $0.14), its cost on the context line, /btw's
    // request counted, the session's limit stopping it, the context
    // panel's cost; then an online server's price typed in Settings.
    property string harloSpendKey: ""
    SequentialAnimation {
        id: harloSpendSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlospend"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            tests.check(Accounts.setServerPrice("harlo", "fake", 100, 1000) === "", "the scripted model gets a price")
            Workspace.setSpendLimits(0.25, 0)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.harloSpendKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello there") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => (tests.spend().costs || {})[tests.harloSpendKey] > 0 && chat.session.status !== "busy", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const cost = tests.spend().costs[tests.harloSpendKey]
            tests.check(Math.abs(cost - 0.14) < 1e-6 && !tests.spend().held[tests.harloSpendKey], "one request priced: $0.14, under the limit ($" + cost + ")")
            tests.check(chat.session.contextText.indexOf("$0.14") >= 0, "the context line says it (" + chat.session.contextText + ")")
            tests.sendByKeys("/btw what did I say?") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => chat.session.asideJson !== "" && JSON.parse(chat.session.asideJson).done, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.sendByKeys("and once more") }
        PauseAnimation { duration: 300 }
        // ($0.14 each: the prompt, the side question, this one.)
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => !!tests.spend().held[tests.harloSpendKey], 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const why = tests.spend().held[tests.harloSpendKey] || ""
            tests.check(why === "This session has cost $0.42, over its $0.25 limit.", "the side question counted, and past the limit it's held (" + why + ")")
            tests.check(tests.findName("spendBanner") !== null, "the chat says so")
            chat.session.loadContext() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = chat.session.contextJson ? JSON.parse(chat.session.contextJson) : {}
            tests.check(r.costKind === "priced" && Math.abs(r.cost - 0.42) < 1e-6, "the context report has it, priced (" + r.costKind + ", " + r.cost + ")")
            Workspace.setSpendLimits(0, 0)
            settingsDialog.openPage("servers") } }
        PauseAnimation { duration: 800 }
        // An online server: no price yet, so it says what that means.
        ScriptAction { script: {
            // (Its file said nothing of free: harlo said it when it started.)
            const fakeFree = tests.findName("serverFree-fake")
            tests.check(fakeFree !== null && fakeFree.visible && fakeFree.checked, "a server on this computer that takes no key is marked free")
            tests.check(tests.findName("serverFree-gw") === null || !tests.findName("serverFree-gw").visible, "an online one isn't offered as free")
            const row = tests.findName("serverPrice-gw")
            tests.check(row !== null && row.visible, "an online server has one")
            tests.check(tests.findText("No price: what Harlo's sessions on it cost isn't known, so spending limits can't count it.") !== null, "and says what no price means")
            tests.shotPopup(settingsDialog, "clicks-harlo-spend-servers")
            tests.clickItem(tests.findName("serverPriceIn-gw"), "price in")
            TestInput.typeText("3")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("serverPriceOut-gw"), "price out")
            TestInput.typeText("15")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => JSON.parse(Accounts.servers("harlo")).some(s => s.name === "gw" && JSON.stringify(s.prices) === "[[3,15]]"), 5000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const gw = JSON.parse(Accounts.servers("harlo")).find(s => s.name === "gw")
            tests.check(gw && JSON.stringify(gw.prices) === "[[3,15]]", "the price is kept for its models (" + (gw ? JSON.stringify(gw.prices) : "no gw") + ")")
            tests.check(tests.findText("Harlo counts what its sessions cost at this price.") !== null, "and it says it counts")
            tests.shotPopup(settingsDialog, "clicks-harlo-spend-priced")
            tests.clickItem(tests.findName("serverFree-fake"), "free") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const fake = JSON.parse(Accounts.servers("harlo")).find(s => s.name === "fake")
            tests.check(fake && fake.free === false, "unticked, it isn't free any more (" + (fake ? fake.free : "no fake") + ")")
            // Added again, under another name: on this computer, no key: free.
            tests.clickItem(tests.findName("addModelServer"), "Connect…") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.findName("modelServerAddress").text = JSON.parse(Accounts.servers("harlo")).find(s => s.name === "fake").baseUrl
            tests.clickItem(tests.findName("modelServerFind"), "Find models") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSpendSteps, () => Accounts.serverSearch.indexOf("\"ready\"") >= 0, 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const box = tests.findName("modelServerFree")
            tests.check(box !== null && box.checked, "adding one here with no key: Free is ticked (" + (box ? box.checked : "no box") + ")")
            tests.findName("modelServerName").text = "again"
            tests.clickItem(tests.findName("modelServerAdd"), "add") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const again = JSON.parse(Accounts.servers("harlo")).find(s => s.name === "again")
            tests.check(again && again.free === true, "and it's saved as free (" + (again ? again.free : "not added") + ")")
            tests.summary(); Qt.quit() } }
    }

    // A long session (8500 turns, about 51,000 rows): how long it takes to
    // open, and the longest frame while it opens, while a long reply streams
    // in, and while it's scrolled from end to start.
    Timer {
        id: longTick
        interval: 16
        repeat: true
        property real last: 0
        property real worst: 0
        property int slow: 0
        property var gaps: []
        function reset() { last = 0; worst = 0; slow = 0; gaps = [] }
        onTriggered: { const n = Date.now(); if (last > 0) { worst = Math.max(worst, n - last); if (n - last > 50) { slow++; gaps.push([last - tests.longAt, n - last]) } } last = n }
    }
    property real longAt: 0
    // (A turn seen to start and end. The model's log isn't read while it
    // streams: each request carries the whole 20 MB history.)
    property bool longBusy: false
    Connections {
        target: win.argValue("--scenario=") === "clicksharlolong" ? chat.transcript : null
        function onCountChanged() {
            const n = chat.transcript.count
            if (tests.longTimes.shown === undefined && n > 0) tests.longTimes.shown = Date.now() - tests.longAt
            if (tests.longTimes.all === undefined && n >= tests.longTimes.turns * 6) tests.longTimes.all = Date.now() - tests.longAt
        }
    }
    property bool longDone: false
    Connections {
        target: win.argValue("--scenario=") === "clicksharlolong" ? chat.session : null
        function onStatusChanged() {
            if (chat.session.status === "busy") tests.longBusy = true
            else if (tests.longBusy && chat.session.status === "idle") tests.longDone = true
        }
    }
    property var longTimes: ({})
    Timer {
        id: longScroll
        interval: 16
        repeat: true
        // (Up half a screen a frame, a fast flick's speed, for 5 s or to the
        // first row.)
        property int ticks: 0
        onTriggered: { const v = chat.transcript.listView; if (++ticks > 300 || v.atYBeginning) { stop(); return } v.contentY -= v.height / 2 }
    }
    SequentialAnimation {
        id: harloLongSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlolong"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            tests.longTimes.turns = Number(TestInput.readFile(win.selftestDir + "/long-turns"))
            longTick.reset(); longTick.start(); tests.longAt = Date.now()
            // (After this step: opening takes a moment, and the steps after
            // a long one are caught up at once, a wait among them.)
            const path = win.selftestDir + "/data/harlo-agent/sessions/--" + cwd.replace(/^\//, "").replace(/\//g, "-") + "--/long0000-0000-0000-0000-000000000000.jsonl"
            Qt.callLater(() => Workspace.open(path)) } }
        // (Longer than opening blocks: the wait isn't caught up past.)
        PauseAnimation { duration: 1500 }
        // (6 rows a turn.)
        // (And its agent ready: the rows it read itself replace the file's.)
        ScriptAction { script: tests.waitUntil(harloLongSteps, () => chat.transcript.listView.count >= tests.longTimes.turns * 6 && chat.session.status === "idle", 120000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.longTimes.rows = chat.transcript.listView.count
            tests.longTimes.openWorst = longTick.worst; tests.longTimes.openSlow = longTick.slow
            tests.check(tests.longTimes.rows >= tests.longTimes.turns * 6, "the whole session is there (" + tests.longTimes.rows + " rows)") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { longTick.reset(); tests.longAt = Date.now(); tests.sendByKeys("write a long reply") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloLongSteps, () => tests.longDone, 60000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.longTimes.stream = Date.now() - tests.longAt
            tests.longTimes.streamWorst = longTick.worst; tests.longTimes.streamSlow = longTick.slow
            tests.log("long stream gaps", JSON.stringify(longTick.gaps))
            tests.check(chat.transcript.listView.count > tests.longTimes.rows + 30, "the long reply is in (" + chat.transcript.listView.count + " rows)")
            // (Scrolled up as you would: it stops following the end.)
            chat.transcript.listView.follow = false
            longTick.reset(); tests.longAt = Date.now(); longScroll.ticks = 0; longScroll.start() } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: tests.waitUntil(harloLongSteps, () => !longScroll.running, 60000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.longTimes.scroll = Date.now() - tests.longAt
            tests.longTimes.scrollWorst = longTick.worst; tests.longTimes.scrollSlow = longTick.slow; tests.longTimes.scrollTicks = longScroll.ticks
            longTick.stop()
            tests.log("long session", JSON.stringify(tests.longTimes))
            // (Loose bounds, the numbers logged. Before the rows were
            // indexed and the prompt rail made only the ticks near its
            // window, 146 of 301 frames scrolling took over 50 ms, and a
            // turn's end 270 ms.)
            tests.check(tests.longTimes.all < 5000, "it opens in time (all rows in " + tests.longTimes.all + " ms)")
            tests.check(tests.longTimes.streamWorst < 150, "frames keep coming while the reply streams (longest " + tests.longTimes.streamWorst + " ms)")
            tests.check(tests.longTimes.scrollSlow < 30 && tests.longTimes.scrollWorst < 150, "and while it scrolls up (" + tests.longTimes.scrollSlow + " of " + tests.longTimes.scrollTicks + " frames over 50 ms, longest " + tests.longTimes.scrollWorst + " ms)")
            tests.shot("clicks-harlo-long-session")
            tests.summary(); Qt.quit() } }
    }

    // Memory and frame times with four long sessions open (3000 turns,
    // about 18,000 rows each, opened one after another and left open), then
    // one reply that is a single huge code block (2000 lines) and one that
    // is a huge table (1500 rows), each streamed into the last. Numbers
    // logged ("big session"); loose bounds.
    function rssMb() { const m = /VmRSS:\s+(\d+)/.exec(TestInput.readFile("/proc/self/status")); return m ? Math.round(Number(m[1]) / 1024) : -1 }
    property var bigTimes: ({})
    property int bigOpen: 0
    function bigPath(k) {
        const cwd = win.argValue("--cwd=")
        return win.selftestDir + "/data/harlo-agent/sessions/--" + cwd.replace(/^\//, "").replace(/\//g, "-") + "--/long000" + k + "-0000-0000-0000-000000000000.jsonl"
    }
    Connections {
        target: win.argValue("--scenario=") === "clicksharlobig" ? chat.session : null
        function onStatusChanged() {
            if (chat.session.status === "busy") tests.longBusy = true
            else if (tests.longBusy && chat.session.status === "idle") tests.longDone = true
        }
    }
    SequentialAnimation {
        id: harloBigSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlobig"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            tests.bigTimes.turns = Number(TestInput.readFile(win.selftestDir + "/long-turns"))
            tests.bigTimes.rss = [["start", tests.rssMb()]]
            tests.bigTimes.open = [] } }
        // Each opened, read whole and its agent ready, then the next.
        SequentialAnimation {
            loops: 4
            ScriptAction { script: { longTick.reset(); longTick.start(); tests.longAt = Date.now(); const k = tests.bigOpen; Qt.callLater(() => Workspace.open(tests.bigPath(k))) } }
            PauseAnimation { duration: 1500 }
            ScriptAction { script: tests.waitUntil(harloBigSteps, () => chat.session.key !== "" && chat.transcript.listView.count >= tests.bigTimes.turns * 6 && chat.session.status === "idle", 120000) }
            PauseAnimation { duration: 300 }
            ScriptAction { script: {
                tests.bigTimes.open.push([Date.now() - tests.longAt, longTick.worst, longTick.gaps])
                tests.bigTimes.rss.push(["open " + (tests.bigOpen + 1), tests.rssMb()])
                tests.bigOpen++ } }
        }
        ScriptAction { script: { longTick.reset(); tests.longAt = Date.now(); tests.longBusy = false; tests.longDone = false; tests.sendByKeys("write a huge code block") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloBigSteps, () => tests.longDone, 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.bigTimes.code = [Date.now() - tests.longAt, longTick.worst, longTick.slow, longTick.gaps]
            tests.bigTimes.rss.push(["code block", tests.rssMb()])
            tests.check(tests.findText("That's all of it.") !== null || chat.transcript.listView.count > 0, "the code block came")
            longTick.reset(); tests.longAt = Date.now(); tests.longBusy = false; tests.longDone = false
            tests.sendByKeys("write a huge table") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloBigSteps, () => tests.longDone, 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.bigTimes.table = [Date.now() - tests.longAt, longTick.worst, longTick.slow, longTick.gaps]
            tests.bigTimes.rss.push(["table", tests.rssMb()])
            tests.longBusy = false; tests.longDone = false
            tests.sendByKeys("write a huge table") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloBigSteps, () => tests.longDone, 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.bigTimes.rss.push(["table again", tests.rssMb()])
            longTick.stop()
            tests.log("big session", JSON.stringify(tests.bigTimes))
            tests.check(tests.bigOpen === 4, "four long sessions open (" + tests.bigOpen + ")")
            // (Before they came in pieces: over a second each, and the code
            // block took 167 MB more, the table 68.)
            tests.check(tests.bigTimes.code[1] < 150 && tests.bigTimes.table[1] < 150, "frames keep coming while the huge blocks stream (longest " + tests.bigTimes.code[1] + " and " + tests.bigTimes.table[1] + " ms)")
            const grew = (at, before) => tests.bigTimes.rss.find(r => r[0] === at)[1] - tests.bigTimes.rss.find(r => r[0] === before)[1]
            // (The first table takes about 60 MB once, which the next keeps
            // using: what grows is per block, not per version of it.)
            tests.check(grew("code block", "open 4") < 60 && grew("table again", "table") < 20, "and memory grows little (code block " + grew("code block", "open 4") + " MB, a second table " + grew("table again", "table") + " MB)")
            tests.check(tests.bigTimes.open[3][1] < 300, "the session ending with them opens without a stall (longest " + tests.bigTimes.open[3][1] + " ms)")
            tests.summary(); Qt.quit() } }
    }

    // Stop while a turn's snapshot is still being taken (slowed to 5 s):
    // the messages sent meanwhile (one only a file) never reach the model
    // and come back to the box with their files, above a draft typed
    // meanwhile; switching away and back keeps it preparing; the next one
    // goes as usual.
    function modelGot(text) { return tests.modelRequests().some(b => JSON.stringify(b).indexOf(text) >= 0) }
    property string stopKey: ""
    SequentialAnimation {
        id: harloStopSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlostop"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("never send this walrus") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(chat.session.preparing && chat.session.status !== "busy", "preparing while the snapshot is taken, not yet busy (" + chat.session.status + ")")
            tests.sendByKeys("and this narwhal") } }
        PauseAnimation { duration: 150 }
        ScriptAction { script: {
            chat.session.addAttachment(win.argValue("--cwd=") + "/README.md")
            chat.session.send("", "")
            // More than the desktop once kept, then the same words twice,
            // each with its own file.
            for (let i = 1; i <= 16; i++)
                chat.session.send("note " + i, "")
            chat.session.addAttachment(win.argValue("--cwd=") + "/notes.md")
            chat.session.send("same", "")
            chat.session.addAttachment(win.argValue("--cwd=") + "/../wt/README.md")
            chat.session.send("same", "")
            chat.composerItem.text = "a draft typed meanwhile"
            tests.stopKey = chat.session.key
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => chat.session.key !== tests.stopKey && chat.session.key !== "", 3000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: Workspace.open(tests.stopKey) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.key === tests.stopKey && chat.session.preparing, "back again: still preparing (" + chat.session.preparing + ")")
            tests.check(chat.composerItem.text === "a draft typed meanwhile", "and the draft is there (" + chat.composerItem.text + ")")
            const stop = tests.findName("stopButton")
            tests.check(stop !== null && stop.visible, "Stop is offered")
            tests.clickItem(stop, "stop") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(!chat.session.preparing && chat.session.status !== "busy", "stopped at once (" + chat.session.status + ")")
            const notes = Array.from({ length: 16 }, (_, i) => "note " + (i + 1)).join("\n\n")
            tests.check(chat.composerItem.text === "never send this walrus\n\nand this narwhal\n\n" + notes + "\n\nsame\n\nsame\n\na draft typed meanwhile", "every message is back above the draft (" + JSON.stringify(chat.composerItem.text) + ")")
            const back = Array.from(chat.session.attachments, String).map(f => f.split("/").slice(-2).join("/")).sort()
            tests.check(back.join() === "repo/README.md,repo/notes.md,wt/README.md", "and their files are attached again, the first one's too (" + back + ")")
            tests.check(tests.findText("Stopped before they were sent: your messages are back in the box.") !== null, "the chat says so")
            tests.shot("clicks-harlo-stop-unsent") } }
        // (Past the snapshot's 5 s: it ends, and still nothing is sent.)
        PauseAnimation { duration: 5500 }
        ScriptAction { script: {
            tests.check(!tests.modelGot("never send this walrus") && !tests.modelGot("and this narwhal"), "the model never got them (" + tests.modelRequests().length + " requests)")
            chat.composerItem.text = ""
            while (chat.session.attachments.length > 0)
                chat.session.removeAttachment(0)
            tests.sendByKeys("send this one instead") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => tests.modelGot("send this one instead") && chat.session.status !== "busy", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.modelGot("send this one instead"), "the next message goes, after its snapshot")
            tests.check(!tests.modelGot("never send this walrus") && !tests.modelGot("and this narwhal"), "and the stopped ones still never went")
            // Stop with a follow-up queued: it comes back into the box.
            // (Busy in its command: the prompt's request and the write's;
            // yolo, so the write doesn't wait for a yes.)
            Workspace.setApprovalMode(chat.session.key, "yolo")
            tests.requestsBefore = tests.modelRequests().length
            tests.sendByKeys("write and wait") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => chat.session.status === "busy" && tests.modelRequests().length >= tests.requestsBefore + 2, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.sendByKeys("keep this queued orca") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(chat.session.queued.some(q => q.indexOf("keep this queued orca") >= 0), "queued while it works (" + chat.session.queued.join(" | ") + ")")
            tests.clickItem(tests.findName("stopButton"), "stop") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => chat.session.status !== "busy" && chat.composerItem.text !== "", 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.composerItem.text === "keep this queued orca", "Stop gives the queued message back to the box (" + JSON.stringify(chat.composerItem.text) + ")")
            tests.check(chat.session.queued.length === 0, "and nothing stays queued (" + chat.session.queued.join(" | ") + ")")
            tests.check(!tests.modelGot("keep this queued orca"), "the model never got it")
            tests.shot("clicks-harlo-stop-queue-back")
            chat.composerItem.text = ""
            // Stop and send: the run stops, and the message starts the next.
            tests.requestsBefore = tests.modelRequests().length
            tests.sendByKeys("write and wait") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => chat.session.status === "busy" && tests.modelRequests().length >= tests.requestsBefore + 2, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.typeText("go another way")
            const send = tests.findName("sendButton"), p = send.mapToItem(null, send.width / 2, send.height / 2)
            TestInput.rightClick(p.x, p.y) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const items = ["sendFollowUp", "sendSteer", "sendStopAndSend"].map(n => tests.findName(n))
            tests.check(items.every(i => i !== null && tests.shown(i)), "the send button's menu offers follow-up, steer, and stop and send")
            tests.shotWindow("clicks-harlo-send-menu")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.status === "busy", "Esc closed the menu, the run goes on")
            tests.clickItem(tests.findName("composerInput"), "composer")
            tests.stopSentAt = Date.now()
            TestInput.key(Qt.Key_Return, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloStopSteps, () => tests.chatText().indexOf("ok: go another way") >= 0 && chat.session.status !== "busy", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("ok: go another way") >= 0, "Ctrl+Shift+Enter: the model answers the new message")
            tests.check(Date.now() - tests.stopSentAt < 4500, "before the command it stopped would have ended (" + (Date.now() - tests.stopSentAt) + " ms)")
            tests.check(tests.chatText().indexOf("Done waiting.") < 0, "the stopped run didn't go on")
            tests.check(chat.composerItem.text === "", "the box is empty (" + JSON.stringify(chat.composerItem.text) + ")")
            tests.shot("clicks-harlo-stop-and-send")
            tests.summary(); Qt.quit() } }
    }
    property real stopSentAt: 0
    property int requestsBefore: 0

    // Spending limits (the demo agent costs $0.25 a prompt): a session past
    // its limit is stopped and held, Allow more lets it go on; the day's
    // limit holds every session, new ones too.
    property string spendKey: ""
    function spend() { return JSON.parse(Workspace.spendJson || "{}") }
    function userMessages() { return JSON.parse(chat.session.userPrompts()).length }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksspend"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); settingsDialog.openPage("usage") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.clickItem(tests.findName("spendSession"), "per session"); TestInput.typeText("0.50"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.spend().limits.session === 0.5 && tests.spend().limits.day === 0, "the per-session limit saves (" + Workspace.spendJson + ")")
            tests.shotPopup(settingsDialog, "clicks-spend-settings") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { settingsDialog.close(); tests.spendKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("fast: one") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(tests.spend().costs[tests.spendKey] === 0.25 && !tests.spend().held[tests.spendKey], "under the limit: it runs ($" + tests.spend().costs[tests.spendKey] + ")")
            tests.sendByKeys("two: a long answer") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const why = tests.spend().held[tests.spendKey] || ""
            tests.check(why === "This session has cost $0.50, over its $0.50 limit.", "at the limit it's held (" + why + ")")
            tests.check(chat.session.status !== "busy", "and its run stopped (" + chat.session.status + ")")
            tests.check(tests.findName("spendBanner") !== null, "the chat says so")
            tests.shot("clicks-spend-banner") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            chat.composerItem.text = "fast: three"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(chat.composerItem.text === "fast: three" && tests.userMessages() === 2, "a message isn't sent while held; it stays in the box")
            tests.clickItem(tests.findName("spendAllow"), "Allow more") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!tests.spend().held[tests.spendKey] && tests.findName("spendBanner") === null, "Allow $0.50 more lets it go on")
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(tests.userMessages() === 3, "and the message goes")
            // The day: $0.75 so far.
            tests.check(Math.abs(tests.spend().today - 0.75) < 1e-9, "today counts every prompt ($" + tests.spend().today + ")")
            Workspace.setSpendLimits(0, 0.8)
            tests.sendByKeys("fast: four") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const s = tests.spend()
            tests.check(s.day.indexOf("over the $0.80 daily limit") > 0 && s.held[tests.spendKey] === s.day, "past the day's limit every session is held (" + s.day + ")")
            tests.check(tests.findName("spendAllow").text === "Allow $0.80 more today", "with Allow … today")
            tests.spendKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(Workspace.spendHold(tests.spendKey) !== "", "a new session too")
            tests.clickItem(tests.findName("spendAllow"), "Allow more today") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.spendHold(tests.spendKey) === "" && tests.spend().day === "", "Allow … today lifts it")
            tests.summary(); Qt.quit() } }
    }

    // Session templates: save the draft as one, fill the page from it,
    // start from it, replace it with another draft, rename, delete.
    function templateList() { return JSON.parse(Workspace.templatesJson || "[]") }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickstemplates"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.openDraft(-1); chat.draftView.harness = "demo"; if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(!tests.findName("saveTemplate", chat.draftView), "no Save as template on an empty page")
            tests.clickItem(tests.findName("draftInput"), "draft input")
            TestInput.typeText("fast: review the open pull request")
            tests.clickItem(tests.findName("draftPermissionsButton", chat.draftView), "permissions") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("saveTemplate", chat.draftView), "Save as template") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(chat.draftView.templateDialog.opened && tests.findName("templateName").text === "fast: review the open pull", "the dialog opens, named from the message (" + tests.findName("templateName").text + ")")
            tests.shotPopup(chat.draftView.templateDialog, "clicks-templates-save") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("PR review")
            tests.clickItem(tests.findName("templateSave"), "Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = tests.templateList()
            tests.check(l.length === 1 && l[0].name === "PR review" && l[0].harness === "demo" && l[0].text === "fast: review the open pull request" && l[0].approval === chat.draftView.shownApproval,
                      "Save keeps the agent, permissions and message (" + Workspace.templatesJson + ")")
            // A different draft, then the template.
            chat.draftView.approval = ""
            chat.draftView.input.text = "" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("template-PR review", chat.draftView), "the template") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = chat.draftView, t = tests.templateList()[0]
            tests.check(d.input.text === t.text && d.effectiveHarness === "demo" && d.shownApproval === t.approval, "picking it fills the page (" + d.input.text + ", " + d.shownApproval + ")")
            tests.shot("clicks-templates-page") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: TestInput.key(Qt.Key_Return, 0) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(chat.session && chat.session.harness === "demo" && Workspace.approvalMode(Workspace.activeKey) === tests.templateList()[0].approval, "and Enter starts the session with it")
            win.openDraft(-1) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = chat.draftView
            d.input.text = "fast: a better review"
            d.templateMenu.template = tests.templateList()[0]
            d.templateMenu.popup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("templateUpdate"), "Replace with this draft") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findName("templateName").text === "PR review", "replacing keeps its name")
            tests.clickItem(tests.findName("templateSave"), "Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = tests.templateList()
            tests.check(l.length === 1 && l[0].text === "fast: a better review", "Replace with this draft updates it (" + Workspace.templatesJson + ")")
            chat.draftView.templateMenu.template = l[0]
            chat.draftView.templateMenu.popup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("templateDelete"), "Delete") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.templateList().length === 0 && !tests.findName("template-PR review", chat.draftView), "Delete removes it")
            tests.summary(); Qt.quit() } }
    }

    // Prompt navigator: hover, click a list entry, Ctrl+J with arrows, Alt+Up/Down.
    property int railTarget: -1
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksrail"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.newSession(-1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.session.loadDemoHistory(30) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { const r = chat.transcript.rail, p = r.mapToItem(null, r.width - 6, r.height / 2); TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const r = chat.transcript.rail
            tests.check(r.highlighted >= 0, "hovering the ticks highlights one (" + r.highlighted + ")")
            tests.railTarget = Math.max(0, r.highlighted - 1)
            const entry = tests.findText(r.prompts[tests.railTarget].text, r)
            tests.check(entry !== null, "the hover list shows the prompts")
            tests.shot("clicks-rail-hover") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = chat.transcript.rail
            tests.clickItem(tests.findText(r.prompts[tests.railTarget].text, r), "list entry") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.check(chat.transcript.activePrompt === tests.railTarget && !chat.transcript.follow, "clicking an entry jumps to it (" + chat.transcript.activePrompt + ")"); TestInput.moveTo(20, 20) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.check(chat.transcript.rail.highlighted === -1, "moving away closes the list"); TestInput.key(Qt.Key_J, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(chat.transcript.rail.keyboard, "Ctrl+J opens it for the keyboard"); TestInput.key(Qt.Key_Up, 0); TestInput.key(Qt.Key_Up, 0); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.check(chat.transcript.activePrompt === tests.railTarget - 2 && !chat.transcript.rail.keyboard, "Up, Up, Enter jumps two back (" + chat.transcript.activePrompt + ")"); TestInput.key(Qt.Key_Up, Qt.AltModifier) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.check(chat.transcript.activePrompt === tests.railTarget - 3, "Alt+Up: previous prompt (" + chat.transcript.activePrompt + ")"); TestInput.key(Qt.Key_Down, Qt.AltModifier) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.check(chat.transcript.activePrompt === tests.railTarget - 2, "Alt+Down: next prompt (" + chat.transcript.activePrompt + ")"); tests.summary(); Qt.quit() } }
    }

    // Terminal tabs: Ctrl+`, +, Ctrl+Shift+T, exit, the tab's ×, Ctrl+Shift+W.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminal"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.clickItem(tests.findName("composerInput"), "composer"); TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(terminal.visible && terminal.tabs.length === 1 && terminal.current.running, "Ctrl+` opens a terminal"); tests.clickItem(tests.findName("terminalAdd"), "+") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.check(terminal.tabs.length === 2 && terminal.entry.active === 1, "+ adds a tab"); TestInput.key(Qt.Key_T, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.check(terminal.tabs.length === 3, "Ctrl+Shift+T adds another"); tests.shot("clicks-terminal-tabs"); tests.clickItem(terminal.current, "terminal"); TestInput.typeText("exit"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.check(terminal.tabs.length === 2, "typing exit closes that tab"); tests.clickItem(tests.findName("terminalTabClose"), "tab ×") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(terminal.tabs.length === 1, "the tab's × closes it"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(terminal.visible && terminal.current === null && Object.keys(terminal.folders).length === 1 && tests.findName("terminalStart").visible, "a session in another folder starts no shell (" + Object.keys(terminal.folders).length + " folders)")
            tests.clickItem(tests.findName("terminalStart"), "Start a shell") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(terminal.current !== null && terminal.current.running && terminal.current.cwd === Workspace.homeDir() && Object.keys(terminal.folders).length === 2, "Start a shell starts one there")
            tests.clickItem(terminal.current, "terminal"); TestInput.key(Qt.Key_W, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(!terminal.visible, "Ctrl+Shift+W on the last tab closes the panel"); tests.summary(); Qt.quit() } }
    }

    // Subagents: View on a running one, steer it by typing, Esc, Cancel in the tree.
    property var subRow: null
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickssubagents"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.newSession(-1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("start some subagents") }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const row = tests.ancestorWith(tests.findText("BuildOtter", chat.transcript), "viewRequested")
            tests.check(row !== null, "BuildOtter shows under the task row")
            tests.subRow = row
            // Narrow: a running subagent offers Steer (which opens the same view) in place of View.
            tests.clickText(tests.findText("View", row) !== null ? "View" : "Steer", row) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(chat.subagentDialog.visible, "View opens its transcript")
            // (The list is read again a few times a second while they run.)
            tests.check(tests.ancestorWith(tests.findText("BuildOtter", chat.transcript), "viewRequested") === tests.subRow && chat.subagentDock.running > 0, "a running helper's row stays the same row as its list is read again")
            tests.clickItem(tests.findName("steerField"), "steer field"); TestInput.typeText("Only run the unit tests") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.shotPopup(chat.subagentDialog, "clicks-subagent-dialog"); tests.clickText("Steer", chat.subagentDialog.contentItem) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(chat.session.subagentMessage.indexOf("BuildOtter") >= 0, "Steer sends it (" + chat.session.subagentMessage + ")"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.check(!chat.subagentDialog.visible, "Esc closes the dialog"); tests.shotWindow("clicks-subagent-tree")
            {
                // Each row's dot is level with its name.
                const name = tests.findText("ScoutLark", chat.transcript), row = tests.ancestorWith(name, "viewRequested"), dot = tests.findName("subagentDot", row)
                const dy = Math.abs(dot.mapToItem(null, 0, dot.height / 2).y - name.mapToItem(null, 0, name.height / 2).y)
                tests.check(dy <= 1, "a helper's dot is level with its name (" + dy.toFixed(1) + " px off)")
                const nm = tests.findName("subagentName", row)
                tests.check(nm.width >= Math.floor(nm.implicitWidth), "a helper's name keeps its whole width while the line has room (" + nm.width + " of " + nm.implicitWidth.toFixed(1) + ")")
            }
 tests.clickText("Cancel", tests.ancestorWith(tests.findText("BuildOtter", chat.transcript), "viewRequested")) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { const a = chat.subagentDock.agents.find(x => x.id === "BuildOtter"); tests.check(a && a.status === "aborted", "Cancel in the tree aborts it"); tests.summary(); Qt.quit() } }
    }

    // Shortcut rebinding in Settings → Shortcuts, by clicks and key presses.
    function keyOf(label, cap) {
        const row = tests.ancestorWith(tests.findText(label, settingsDialog.contentItem), "hasPending")
        return row ? tests.findText(cap, row) : null
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickskeys"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.check(settingsDialog.visible, "Ctrl+, opens Settings")
            const t0 = Date.now()
            tests.clickItem(tests.findName("settingsNav-shortcuts", settingsDialog.contentItem), "Shortcuts")
            const ms = Date.now() - t0
            tests.log("shortcuts page built in", ms, "ms")
            tests.check(ms < 150, "the Shortcuts page opens at once (" + ms + " ms)")
            tests.check(tests.findText("Toggle sidebar", settingsDialog.contentItem) !== null && tests.findText("Send", settingsDialog.contentItem) === null && tests.findText("Zoom in", settingsDialog.contentItem) === null, "only keys that can be changed are listed")
            tests.check(tests.findText("Show every key", settingsDialog.contentItem) !== null, "fixed keys are a click away")
            // Each page's row in the list: its icon and name centred on
            // the row's highlight.
            const nav = tests.findName("settingsNav-shortcuts", settingsDialog.contentItem)
            const mid = it => it.mapToItem(null, 0, it.height / 2).y
            const icon = nav.contentItem.children[0], name = nav.contentItem.children[1]
            // (Narrow, the list shows only icons.)
            const offs = [icon, name].filter(it => it.visible).map(it => mid(it) - mid(nav.background))
            tests.log("settings nav row: icon and name", offs.map(d => d.toFixed(2)).join(", "), "px off its highlight's centre")
            tests.check(offs.every(d => Math.abs(d) <= 0.5), "a settings page's row centres its icon and name on its highlight (" + offs.map(d => d.toFixed(1)).join(", ") + " px off)") } }
        // A short window: the list of pages scrolls to its last one.
        ScriptAction { script: { tests.heightWas = win.height; win.height = 480 } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const list = tests.findName("settingsNavList", settingsDialog.contentItem)
            const pages = settingsDialog.shownPages
            const last = tests.findName("settingsNav-" + pages[pages.length - 1].id, settingsDialog.contentItem)
            tests.check(list !== null && list.contentHeight > list.height + 1, "in a short window the settings pages are more than fit (" + (list ? list.contentHeight + " in " + list.height : "no list") + ")")
            list.contentY = list.contentHeight - list.height
            const y = last.mapToItem(list, 0, 0).y
            tests.check(y >= 0 && y + last.height <= list.height + 0.5, "the settings page list scrolls to its last page (" + pages[pages.length - 1].title + " at " + y.toFixed(0) + "–" + (y + last.height).toFixed(0) + " of " + list.height.toFixed(0) + ")")
            // Its scroll bar beside the rows, not over their highlights.
            const bar = list.ScrollBar.vertical
            const rowRight = last.mapToItem(null, last.width, 0).x
            if (bar.policy !== ScrollBar.AlwaysOff)
                tests.check(bar.mapToItem(null, 0, 0).x >= rowRight, "the settings list's scroll bar is beside its rows (" + bar.mapToItem(null, 0, 0).x.toFixed(0) + " ≥ " + rowRight.toFixed(0) + ")")
            bar.active = true } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: { tests.shotWindow("settings-short"); const list = tests.findName("settingsNavList", settingsDialog.contentItem); list.contentY = 0; settingsDialog.openPage("about") } }
        PauseAnimation { duration: 300 }
        // A page opened from elsewhere (a link, a search): its row comes into sight.
        ScriptAction { script: {
            const list = tests.findName("settingsNavList", settingsDialog.contentItem)
            const row = tests.findName("settingsNav-about", settingsDialog.contentItem)
            const y = row.mapToItem(list, 0, 0).y
            tests.check(y >= 0 && y + row.height <= list.height + 0.5, "the open page's row scrolls into sight (" + y.toFixed(0) + " in " + list.height.toFixed(0) + ")")
            settingsDialog.openPage("shortcuts")
            win.height = tests.heightWas } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.keyOf("Toggle sidebar", "Ctrl+B"), "Toggle sidebar's Ctrl+B") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(Keymap.recording && tests.findText("Press keys…", settingsDialog.contentItem) !== null, "clicking a key starts recording"); TestInput.key(Qt.Key_K, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!commandPalette.visible, "Ctrl+K while recording doesn't open the palette")
            tests.check(tests.findText("Ctrl+K is taken by “Go to a session or action”.", settingsDialog.contentItem) !== null, "a taken key asks to replace")
            tests.shotPopup(settingsDialog, "clicks-keys-conflict") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("Cancel", settingsDialog.contentItem) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(Keymap.seq("toggleSidebar") === "Ctrl+B", "Cancel keeps Ctrl+B"); tests.clickItem(tests.keyOf("Toggle sidebar", "Ctrl+B"), "Ctrl+B again") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_X, 0) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(tests.findText("X alone is for typing; add Ctrl, Alt or Meta.", settingsDialog.contentItem) !== null && Keymap.recording, "a plain key is refused, still recording"); TestInput.key(Qt.Key_1, Qt.ControlModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(tests.findText("Ctrl+1 is taken by Go to tab 1.", settingsDialog.contentItem) !== null, "a reserved key is refused"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Keymap.seq("toggleSidebar") === "Ctrl+Shift+B" && !Keymap.recording, "Ctrl+Shift+B is recorded")
            tests.check(JSON.parse(Workspace.setting("shortcuts")).toggleSidebar === "Ctrl+Shift+B", "and saved")
            // Replace: Files panel takes Changes panel's key.
            tests.clickItem(tests.keyOf("Files panel", "Ctrl+Shift+E"), "Files panel's Ctrl+Shift+E") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_G, Qt.ControlModifier | Qt.ShiftModifier) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Replace", settingsDialog.contentItem) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Keymap.seq("filesPanel") === "Ctrl+Shift+G" && Keymap.seq("changesPanel") === "", "Replace moves the key and unassigns the other")
            tests.clickItem(tests.keyOf("Terminal", "Ctrl+`"), "Terminal's key") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_Backspace, 0) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Keymap.seq("terminal") === "" && tests.keyOf("Terminal", "Not set") !== null, "Backspace unassigns (shows Not set)")
            tests.clickItem(tests.keyOf("New session", "Ctrl+N"), "New session's Ctrl+N") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.AltModifier) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(settingsDialog, "clicks-keys-edited") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: TestInput.key(Qt.Key_Escape, 0) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(!settingsDialog.visible, "Esc closes Settings")
            tests.check(sidebar.width < 220 || tests.findText("Ctrl+Alt+N", sidebar) !== null, "the sidebar hint follows (Ctrl+Alt+N; hidden in a narrow sidebar)")
            tests.sidebarWas = windowSettings.sidebarOpen
            TestInput.key(Qt.Key_B, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(windowSettings.sidebarOpen === tests.sidebarWas, "old Ctrl+B does nothing"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(windowSettings.sidebarOpen !== tests.sidebarWas, "new Ctrl+Shift+B toggles the sidebar"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickText("Reset all", settingsDialog.contentItem) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Keymap.seq("toggleSidebar") === "Ctrl+B" && Keymap.seq("changesPanel") === "Ctrl+Shift+G" && Keymap.seq("terminal") === "Ctrl+`" && Workspace.setting("shortcuts") === "", "Reset all restores the defaults")
            tests.findName("keyChip-toggleSidebar", settingsDialog.contentItem).forceActiveFocus()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(Keymap.recording && tests.findText("Press keys…", settingsDialog.contentItem) !== null, "Enter on a key starts recording"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(!Keymap.recording && settingsDialog.visible && Keymap.seq("toggleSidebar") === "Ctrl+B", "Esc stops recording and keeps the key")
            tests.shotPopup(settingsDialog, "clicks-keys-page")
            tests.clickText("Show every key", settingsDialog.contentItem) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(shortcutsDialog.visible && tests.findText("Zoom in", shortcutsDialog.contentItem) !== null, "Show every key opens the shortcuts sheet")
            tests.summary(); Qt.quit() } }
    }
    property bool sidebarWas: true

    // Start at login: the switch writes and removes the autostart entry;
    // --background starts hidden, and opens the window without a tray.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksautostart"
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.check(!win.visible, "--background starts without the window") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { tests.check(win.visible, "no tray: the window opens anyway"); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("autostartSwitch", settingsDialog.contentItem), "autostart switch") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(Workspace.autostartEnabled(), "switching it on adds the entry"); testFile.load(win.argValue("--autostart=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(testFile.text.indexOf(" --background\n") > 0 && testFile.text.indexOf("Exec=\"") > 0, "the entry starts harlo with --background")
            tests.shotPopup(settingsDialog, "clicks-autostart") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickItem(tests.findName("autostartSwitch", settingsDialog.contentItem), "autostart switch") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(!Workspace.autostartEnabled() && Workspace.fileStamp(win.argValue("--autostart=")) === "", "switching it off removes the entry"); tests.summary(); Qt.quit() } }
    }

    // The mini-chat window: open by key, chat in it, keep on top, Esc.
    property int rowsBefore: 0
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksminichat"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("fast: first message") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.rowsBefore = chat.transcript.count; TestInput.key(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(win.miniChat && win.miniChat.visible, "Ctrl+Shift+M opens the mini chat")
            tests.check(tests.findText("fast: first message", win.miniChat.contentItem) !== null, "it shows the active session")
            tests.clickItem(tests.findName("composerInput", win.miniChat.contentItem), "mini composer")
            TestInput.typeText("fast: from the mini chat")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(chat.transcript.count > tests.rowsBefore && tests.findText("fast: from the mini chat", chat.transcript) !== null, "a message sent there reaches the session")
            // (Its own window: taken at its next frame, so nothing changes before.)
            win.miniChat.contentItem.grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-minichat.png")) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("keepOnTop", win.miniChat.contentItem), "📌") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check((win.miniChat.flags & Qt.WindowStaysOnTopHint) === 0 && !win.miniChat.stayOnTop && Workspace.setting("mini_chat_on_top") === "0", "📌 turns keep-on-top off (and on by default)"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(!win.miniChat.visible, "Esc hides it"); tests.summary(); Qt.quit() } }
    }

    // Project notes: per project, autosaved, todos, insert into message.
    property string notesKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksnotes"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); tests.notesKey = Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(windowSettings.rightMode === "notes" && tests.findText("repo", notes) !== null, "Ctrl+Shift+N opens the project's notes")
            tests.clickItem(tests.findName("notesEditor", notes), "notes editor")
            TestInput.typeText("Use SQLite for the cache") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(Workspace.projectNotes(1) === "Use SQLite for the cache", "typing autosaves"); tests.clickItem(tests.findName("todoField", notes), "todo field"); TestInput.typeText("write the migration"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(notes.openTodos === 1 && tests.findText("Todos · 1 open", notes) !== null, "Enter adds a todo"); tests.clickText("write the migration", notes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(Workspace.projectNotes(1) === "Use SQLite for the cache\n- [x] write the migration", "ticking it marks the line done (saved)")
            tests.check(tests.findText("Todos · all done", notes) !== null, "the todo count follows")
            tests.shot("clicks-notes") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickText("Insert into message", notes) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(chat.composerItem.text.startsWith("Project notes:\nUse SQLite for the cache"), "Insert into message fills the composer"); chat.composerItem.text = ""; Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(tests.findText("No project", notes) !== null && notes.todos.length === 0 && Workspace.projectNotes(-1) === "", "a session outside projects has its own pad")
            tests.clickItem(tests.findName("notesEditor", notes), "notes editor")
            TestInput.typeText("home notes") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(Workspace.projectNotes(-1) === "home notes", "its notes save separately"); Workspace.open(tests.notesKey) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(notes.todos.length === 1 && tests.findText("repo", notes) !== null, "back in the project, its notes return"); TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(windowSettings.rightMode === "", "Ctrl+Shift+N closes it"); tests.summary(); Qt.quit() } }
    }

    // What's shown follows what it shows: settings changed elsewhere, a
    // project renamed, an open question kept while a file tab covers the
    // chat, and a segmented control moved by its owner.
    property string bindingsKey: ""
    property int segOwner: 0
    property var segItem: null
    Component {
        id: segProbe
        SegmentedControl { labels: ["One", "Two"]; namePrefix: "segProbe-"; current: tests.segOwner; onPicked: i => tests.segOwner = i }
    }
    SequentialAnimation {
        id: bindingSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksbindings"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            tests.bindingsKey = Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            const before = win.checkUpdates
            Workspace.setSetting("check_updates", "0")
            tests.check(before && !win.checkUpdates, "update checks turned off reach the window at once")
            Workspace.setSetting("check_updates", "1")
            tests.check(win.checkUpdates, "and on again")
            Workspace.setSetting("dictation_model", "small")
            tests.check(chat.composerItem.whisperModel === "small", "a dictation model chosen reaches the composer (" + chat.composerItem.whisperModel + ")")
            Workspace.setSetting("dictation_model", "")
            tests.check(chat.composerItem.whisperModel === "base", "cleared, the default (" + chat.composerItem.whisperModel + ")")
            TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findText("repo", notes) !== null, "the notes name the project")
            Workspace.setProjectName(1, "renamed repo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findText("renamed repo", notes) !== null, "renamed, the notes follow")
            TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier)
            chat.submit("please ask me something", "") } }
        ScriptAction { script: tests.waitUntil(bindingSteps, () => chat.session.hasRequest && chat.session.requestMethod === "ask" && chat.askPanel.visible, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const p = chat.askPanel
            p.toggle(0, "Postgres")
            p.setCustom(1, "Offline mode")
            tests.check(JSON.stringify(p.answers).indexOf("Postgres") >= 0, "an answer picked")
            Workspace.openFileTab(win.argValue("--cwd=") + "/README.md") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey.startsWith("file:") && !chat.askPanel.visible, "a file tab covers the question")
            Workspace.open(tests.bindingsKey) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = chat.askPanel, said = JSON.stringify(p.answers)
            tests.check(p.visible && said.indexOf("Postgres") >= 0 && said.indexOf("Offline mode") >= 0, "back in the chat, what was picked and typed is still there (" + said + ")")
            tests.segItem = segProbe.createObject(win.contentItem, { x: 20, y: 20, z: 1000 }) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("segProbe-Two"), "Two") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const one = tests.findName("segProbe-One"), two = tests.findName("segProbe-Two")
            tests.check(tests.segOwner === 1 && two.checked && !one.checked, "a segment clicked tells its owner, and shows chosen")
            tests.segOwner = 0
            tests.check(tests.segItem.current === 0 && one.checked && !two.checked, "the owner moving it back moves the control")
            tests.segItem.destroy()
            tests.summary(); Qt.quit() } }
    }

    // Text harlo doesn't write is shown as it is: a tag in a session title,
    // a to-do, a plan step or a question stays, not read as HTML.
    property string plainKey: ""
    TextMetrics { id: plainProbe }
    /// `label` shows `part` as typed: plain, and as wide as its text
    /// measured plainly (unless cut short).
    function shownPlain(label, part) {
        if (!label || label.text.indexOf(part) < 0 || label.textFormat !== Text.PlainText) return false
        plainProbe.font = label.font
        plainProbe.text = label.text
        return label.truncated || label.lineCount > 1 || Math.abs(label.contentWidth - plainProbe.advanceWidth) < 2
    }
    /// The first item under `root` with property `prop` (depth first).
    function itemWith(prop, root) {
        for (const c of root.children) {
            if (c[prop] !== undefined) return c
            const f = itemWith(prop, c)
            if (f) return f
        }
        return null
    }
    SequentialAnimation {
        id: plainSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksplaintext"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            windowSettings.sidebarOpen = true
            Workspace.addProject(win.argValue("--cwd="))
            tests.plainKey = Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            Workspace.renameSession(tests.plainKey, "Fix the <div> overflow")
            Workspace.planEdit(tests.plainKey, JSON.stringify({ op: "add", title: "Render <b>bold</b> titles" }))
            chat.submit("show the todos", "") } }
        ScriptAction { script: tests.waitUntil(plainSteps, () => chat.session.todosJson.indexOf("<div>") >= 0 && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const dock = tests.itemWith("phases", chat)
            if (dock) dock.expanded = true } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const title = tests.findAllNames("sessionTitle", sidebar).find(l => l.text.indexOf("<div>") >= 0)
            tests.check(tests.shownPlain(title, "Fix the <div> overflow"), "a session title keeps its <div> (" + (title ? title.text + ", " + title.contentWidth.toFixed(0) + " vs " + plainProbe.advanceWidth.toFixed(0) : "not found") + ")")
            const todo = tests.findContaining("<div> handling", chat)
            tests.check(tests.shownPlain(todo, "Patch the parser's <div> handling"), "a to-do keeps its <div> (" + (todo ? todo.text : "not found") + ")")
            const step = tests.findContaining("<b>bold</b>", chat)
            tests.check(tests.shownPlain(step, "Render <b>bold</b> titles"), "a plan step keeps its <b> (" + (step ? step.text : "not found") + ")")
            tests.shotWindow("clicks-plain-text")
            chat.submit("please ask me something", "") } }
        ScriptAction { script: tests.waitUntil(plainSteps, () => chat.askPanel.visible, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // (The demo's questions have no tags: one with them in their place.)
            chat.askPanel.request = { id: "plain", questions: [{ header: "<i>Markup</i>", question: "Keep the <b>bold</b> tags?", options: [{ label: "<div>Yes</div>", description: "Show <br> as typed" }, { label: "No" }] }, { header: "Second", question: "And another?", options: [{ label: "Yes" }] }] } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const p = chat.askPanel
            tests.check(tests.shownPlain(tests.findContaining("Keep the <b>bold</b>", p), "Keep the <b>bold</b> tags?"), "a question keeps its <b>")
            tests.check(tests.shownPlain(tests.findContaining("<div>Yes", p), "<div>Yes</div>"), "an option keeps its <div>")
            tests.check(tests.shownPlain(tests.findContaining("<br>", p), "Show <br> as typed"), "an option's description keeps its <br>")
            tests.check(tests.shownPlain(tests.findContaining("<i>Markup", p), "<i>Markup</i>"), "a question's header keeps its <i>")
            tests.summary(); Qt.quit() } }
    }

    // Narrow windows: a long tool name, a file's long message and the
    // parallel lanes stay inside what holds them.
    SequentialAnimation {
        id: narrowSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksnarrow"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("show a long tool call", "") }
        ScriptAction { script: tests.waitUntil(narrowSteps, () => tests.findAllNames("toolName", chat.transcript).some(l => l.text.indexOf("documentation") >= 0), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const name = tests.findAllNames("toolName", chat.transcript).find(l => l.text.indexOf("documentation") >= 0)
            const head = name.parent, over = tests.spills(head)
            tests.check(over.length === 0, "a long tool name leaves its line's room to the rest (" + over.join(", ") + ")")
            tests.check(name.width <= head.width * 0.45 + 1 && (name.implicitWidth <= name.width || name.truncated), "cut in the middle when it doesn't fit (" + Math.round(name.width) + " of " + Math.round(head.width) + ")")
            tests.shotWindow("clicks-narrow-tool")
            Workspace.openFileTab(win.argValue("--cwd=") + "/" + "a-rather-long-folder-name/".repeat(4) + "missing-file.bin") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            const m = tests.findName("fileMessage"), at = m ? m.mapToItem(null, 0, 0).x : 0
            tests.check(m !== null && m.text !== "" && at + m.width <= win.width, "a file's long message wraps inside the window (" + (m ? Math.round(at) + "+" + Math.round(m.width) + " of " + win.width + ", " + m.lineCount + " lines" : "none") + ")")
            tests.shotWindow("clicks-narrow-file")
            win.openDraft(1) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.draftView.parallel = true }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const lanes = tests.findName("parallelLanes"), over = lanes ? tests.spills(lanes.parent) : ["no lanes"]
            tests.check(over.length === 0, "the parallel lanes fit their card (" + over.join(", ") + ")")
            tests.shotWindow("clicks-narrow-parallel")
            Workspace.newSession(1, "demo") } }
        // A code block of 200 lines comes in two pieces, joined with no seam.
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("show some long code", "") }
        ScriptAction { script: tests.waitUntil(narrowSteps, () => tests.findAllNames("replyCodeView", chat.transcript).some(v => v.y < 20), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pieces = tests.findAllNames("replyCodeView", chat.transcript).filter(v => v.y < 20)
            tests.check(pieces.length > 0 && pieces.every(v => v.y === 0), "a long code block's second piece starts at its top, no seam (" + pieces.map(v => v.y).join(",") + ")")
            tests.summary(); Qt.quit() } }
    }

    // Hidden things do no work: the mini chat lets its session go once
    // hidden, the subagent and branch dialogs make their transcripts only
    // when opened, the picture viewer lets its picture go, the Files panel
    // lists its folder only while shown, the Working pulse stops while its
    // label is hidden, and polling waits while the window is in the tray.
    // With --measure=1: a long session and 20 s in the tray, for numbers.
    property string hiddenA: ""
    property real hiddenCpu: 0
    property real hiddenAt: 0
    readonly property bool hiddenMeasure: win.argValue("--measure=") === "1"
    /// The first item named `name` under `root`, shown or not.
    function namedAny(name, root) {
        for (const c of root.children) {
            if (c.objectName === name) return c
            const f = namedAny(name, c)
            if (f) return f
        }
        return null
    }
    // (Its own and its finished children's: git runs count.)
    function cpuMs() { const f = TestInput.readFile("/proc/self/stat").split(") ")[1].split(" "); return (Number(f[11]) + Number(f[12]) + Number(f[13]) + Number(f[14])) * 10 }
    function anonMb() { const m = /RssAnon:\s+(\d+)/.exec(TestInput.readFile("/proc/self/status")); return m ? Math.round(Number(m[1]) / 1024) : -1 }
    SequentialAnimation {
        id: hiddenSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickshidden"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            tests.hiddenA = Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.session.loadDemoHistory(tests.hiddenMeasure ? 20000 : 200) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.log("measure: A open", tests.rssMb(), "MiB RSS,", tests.anonMb(), "MiB anon")
            tests.check(tests.itemWith("namesFrom", chat.subagentDialog.contentItem) === null && tests.itemWith("namesFrom", chat.branchDialog.contentItem) === null,
                        "the subagent and branch dialogs have no transcript until opened")
            tests.check(files.rooted === "", "the Files panel, never shown, hasn't listed a folder")
            win.toggleMiniChat() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const c = tests.itemWith("pinnable", win.miniChat.contentItem)
            tests.check(c && c.key === tests.hiddenA, "the mini chat shows the session")
            tests.log("measure: mini chat open", tests.rssMb(), "MiB RSS,", tests.anonMb(), "MiB anon")
            win.toggleMiniChat() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Workspace.newSession(1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: Workspace.open(tests.hiddenA) }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            const c = tests.itemWith("pinnable", win.miniChat.contentItem)
            tests.check(c && c.key === "", "hidden, it lets the session go and doesn't follow switches (" + (c ? c.key : "none") + ")")
            tests.log("measure: after switches, mini chat hidden", tests.rssMb(), "MiB RSS,", tests.anonMb(), "MiB anon")
            chat.transcript.pictureViewer.show("A picture", [{ label: "a", url: "qrc:/qt/qml/harlo/icons/app/harlo.svg", width: 64, height: 64 }], 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: chat.transcript.pictureViewer.close() }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(chat.transcript.pictureViewer.pictures.length === 0, "the picture viewer, closed, lets its picture go")
            windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(files.rooted !== "" && files.rooted === files.folder, "shown, the Files panel lists the folder (" + files.rooted + ")")
            windowSettings.rightMode = "" } }
        // (After its closing slide.)
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.check(!files.visible, "closed"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(files.folder !== files.rooted, "hidden, another session's folder isn't listed (" + files.folder + ")")
            windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(files.rooted === files.folder, "shown again, it catches up")
            windowSettings.rightMode = ""
            chat.submit("approve this please", "") } }
        ScriptAction { script: tests.waitUntil(hiddenSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { const w = tests.namedAny("workingLabel", chat.transcript); tests.hiddenCpu = w ? w.opacity : -1 } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // (Hidden behind the approval card, its pulse is stopped at full opacity.)
            const w = tests.namedAny("workingLabel", chat.transcript)
            tests.check(w && !w.visible && w.opacity === 1 && tests.hiddenCpu === 1, "the Working pulse stops while the approval card says it (" + (w ? w.visible + " " + tests.hiddenCpu + " " + w.opacity : "none") + ")")
            chat.session.answerValue("Deny")
            Workspace.openDiffTab(win.argValue("--cwd=") + "/README.md", false) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.hide()
            tests.hiddenAt = Date.now(); tests.hiddenCpu = tests.cpuMs() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.check(!chat.transcript.windowShown, "in the tray, the window's polling knows it's hidden") }
        PauseAnimation { duration: tests.hiddenMeasure ? 20000 : 2000 }
        ScriptAction { script: {
            const ms = Date.now() - tests.hiddenAt, cpu = tests.cpuMs() - tests.hiddenCpu
            tests.log("measure: in the tray", Math.round(ms / 1000), "s, CPU", cpu, "ms")
            win.show()
            tests.summary(); Qt.quit() } }
    }

    // The transcript's hot path: a finished run's box keeps its cards and
    // its place while a reply streams below it, a boxed run's hidden steps
    // build nothing of their own, and CPU over the reply is logged.
    // With --measure=1: a longer session and three replies, for numbers.
    property var hotBox: null
    property var hotCard: null
    property real hotY: 0
    property real hotCpu: 0
    property real hotAt: 0
    property int hotReplies: 0
    property real hotSent: 0
    readonly property bool hotMeasure: win.argValue("--measure=") === "1"
    /// How many items under `root` are named `name`, shown or not.
    function countNamed(name, root) {
        let n = 0
        for (const c of root.children) n += (c.objectName === name ? 1 : 0) + countNamed(name, c)
        return n
    }
    function hotLongBox() { return tests.findAllNames("runBox", chat.transcript).find(b => b.rows.length === 60) || null }
    SequentialAnimation {
        id: hotSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickshotpath"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.session.loadDemoHistory(tests.hotMeasure ? 400 : 30) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: chat.submit("show a long run", "") }
        ScriptAction { script: tests.waitUntil(hotSteps, () => tests.hotLongBox() !== null, 10000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const box = tests.hotLongBox()
            tests.log("measure: runHeaders", tests.countNamed("runHeader", chat.transcript.listView), "agentHeads", tests.countNamed("agentHead", chat.transcript.listView), "rows", chat.transcript.listView.count)
            tests.check(tests.countNamed("runHeader", chat.transcript.listView) < 30, "a boxed run's hidden steps make no run header of their own (" + tests.countNamed("runHeader", chat.transcript.listView) + ")")
            tests.check(tests.countNamed("agentHead", chat.transcript.listView) < 30, "only a reply's first row makes the agent's head (" + tests.countNamed("agentHead", chat.transcript.listView) + ")")
            tests.hotBox = box } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const list = tests.namedAny("runBoxScroll", tests.hotBox); tests.hotCard = list.itemAtIndex(59); tests.hotY = list.contentY } }
        ScriptAction { script: { tests.hotAt = tests.hotSent = Date.now(); tests.hotCpu = tests.cpuMs(); chat.submit("hello again", "") } }
        // (Each reply done, the next sent, till there are enough.)
        ScriptAction { script: tests.waitUntil(hotSteps, () => {
            if (chat.session.status !== "idle" || Date.now() - tests.hotSent < 800) return false
            if (++tests.hotReplies >= (tests.hotMeasure ? 3 : 1)) return true
            tests.hotSent = Date.now(); chat.submit("hello again", ""); return false }, 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.log("measure: replies", tests.hotReplies, "in", Math.round((Date.now() - tests.hotAt) / 100) / 10, "s, CPU", tests.cpuMs() - tests.hotCpu, "ms")
            const box = tests.hotBox, list = box ? tests.namedAny("runBoxScroll", box) : null
            // (Measured, three replies take the box out of the view's reach.)
            if (!tests.hotMeasure) {
                tests.check(box !== null && box.rows.length === 60, "the finished run's box stays as it was while replies stream below it")
                tests.check(list && Math.abs(list.contentY - tests.hotY) < 1, "and keeps its place (" + (list ? Math.round(list.contentY) : "-") + " of " + Math.round(tests.hotY) + ")")
                tests.check(tests.hotCard !== null && list && list.itemAtIndex(59) === tests.hotCard, "its cards aren't made again")
            }
            tests.hotReplies = 0; tests.hotAt = Date.now(); tests.hotCpu = tests.cpuMs() } }
        // Scrolling up through the session, a screen at a time.
        ScriptAction { script: tests.waitUntil(hotSteps, () => {
            const v = chat.transcript.listView
            v.contentY = Math.max(v.originY, v.contentY - v.height * 0.9)
            return ++tests.hotReplies >= (tests.hotMeasure ? 150 : 20) || v.atYBeginning }, 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.log("measure: scrolled", tests.hotReplies, "screens in", Math.round((Date.now() - tests.hotAt) / 100) / 10, "s, CPU", tests.cpuMs() - tests.hotCpu, "ms,", tests.anonMb(), "MiB anon")
            tests.summary(); Qt.quit() } }
    }

    // Scheduled prompts in Settings → Schedules, by clicks and typing.
    // Runs with --clock-ahead=7200 so a schedule an hour out is due.
    property int liveBefore: 0
    function liveCount() { return JSON.parse(Workspace.runningSessions()).length }
    function firstSchedule() { const l = JSON.parse(Workspace.schedulesJson()); return l.length ? l[0] : null }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksschedules"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.setProjectHarness(1, "demo"); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("settingsNav-schedules", settingsDialog.contentItem), "Schedules") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.check(tests.findText("No schedules yet.", settingsDialog.contentItem) !== null, "the page starts empty"); tests.clickText("New schedule") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("Save") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findText("Write the prompt to send.") !== null, "Save without a prompt explains why")
            tests.clickItem(tests.findName("schedulePrompt"), "prompt")
            TestInput.typeText("Summarize the repo")
            tests.check(tests.findText("Write the prompt to send.") === null, "typing clears the error")
            tests.clickText("Every") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.clickItem(tests.findName("scheduleEvery"), "interval"); TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("1"); tests.clickText("Home folder") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("repo") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The editor is its own popup: grab the item that sits in the overlay.
            let it = tests.findName("schedulePrompt")
            while (it && it.parent !== Overlay.overlay) it = it.parent
            if (it) it.grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-schedule-editor.png")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Save") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const s = tests.firstSchedule(), now = Date.now() / 1000
            tests.check(s && s.describe === "Every hour" && s.projectName === "repo" && s.title === "Summarize the repo", "Save adds it (" + (s ? s.describe + " · " + s.projectName : "none") + ")")
            tests.check(s && Math.abs(s.nextRun - now - 3600) < 120, "next run in an hour")
            tests.check(tests.findText("Summarize the repo", settingsDialog.contentItem) !== null, "the card shows it")
            tests.clickItem(tests.findName("scheduleSwitch", settingsDialog.contentItem), "switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(!tests.firstSchedule().enabled && tests.firstSchedule().nextRun === null, "the switch turns it off"); tests.clickItem(tests.findName("scheduleSwitch", settingsDialog.contentItem), "switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(tests.firstSchedule().enabled, "and on again"); tests.liveBefore = tests.liveCount(); tests.clickText("Run now", settingsDialog.contentItem) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(tests.liveCount() === tests.liveBefore + 1 && Workspace.notice === "Scheduled prompt started: Summarize the repo", "Run now starts a session (" + Workspace.notice + ")")
            tests.check(tests.firstSchedule().lastRun > 0, "and records the run")
            tests.shotPopup(settingsDialog, "clicks-schedules")
            tests.liveBefore = tests.liveCount()
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(tests.liveCount() === tests.liveBefore + 1, "a due schedule runs on the timer's check"); tests.clickText("Edit", settingsDialog.contentItem) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.clickItem(tests.findName("scheduleTitle"), "title"); TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("Morning summary"); tests.clickText("Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.check(tests.firstSchedule().title === "Morning summary" && tests.findText("Morning summary", settingsDialog.contentItem) !== null, "Edit renames it"); tests.clickText("Delete", settingsDialog.contentItem) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("Discard") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.check(tests.firstSchedule() === null && tests.findText("No schedules yet.", settingsDialog.contentItem) !== null, "Delete + Discard removes it"); tests.summary(); Qt.quit() } }
    }

    // Plan limits: the sidebar meter and Settings → Usage, from sample data.
    function sampleQuota(reached) {
        return JSON.stringify({ updatedAt: Date.now() - 120000, error: "", accounts: [
            { provider: "openai-codex", plan: "prolite", account: "", limitReached: false,
              windows: [{ label: "7 days", usedFraction: 0.3, resetsAt: Date.now() + 5 * 86400000, status: "ok" }] },
            { provider: "anthropic", plan: "max", account: "", limitReached: reached,
              windows: [{ label: "5 hours", usedFraction: reached ? 1 : 0.8, resetsAt: Date.now() + 2 * 3600000, status: "ok" },
                        { label: "7 days", usedFraction: 0.4, resetsAt: Date.now() + 3 * 86400000, status: "ok" }] } ] })
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksquota"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(tests.findName("planMeter") === null, "no meter without data"); Workspace.quotaJson = tests.sampleQuota(false) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = tests.findName("planMeter")
            tests.check(m !== null && tests.findText("80%", m) !== null, "the meter shows the fullest window (80%)")
            const logo = tests.findName("planMeterLogo", m)
            tests.check(logo !== null && logo.provider === "anthropic" && tests.findName("planMeterClaude", m) === null, "with its provider's logo")
            // A session of the other provider runs: the meter follows it.
            Workspace.meterLast = "openai-codex" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = tests.findName("planMeter")
            tests.check(tests.findText("30%", m) !== null && tests.findName("planMeterLogo", m).provider === "openai-codex", "it shows the provider used last (30%)")
            tests.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const follow = tests.findName("meterFollow"), a = tests.findName("meterPick-anthropic"), o = tests.findName("meterPick-openai-codex")
            tests.check(follow !== null && follow.marked && a !== null && a.hint === "80%" && o !== null && o.hint === "30%", "clicking it lists the providers, following the last used")
            tests.shotWindow("clicks-quota-meter")
            if (a) tests.clickItem(a, "anthropic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = tests.findName("planMeter")
            tests.check(Workspace.meterPin === "anthropic" && tests.findText("80%", m) !== null, "picking one pins it (80%)")
            Workspace.meterLast = "openai-codex"
            tests.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findName("meterPick-anthropic").marked && tests.findText("80%", tests.findName("planMeter")) !== null, "pinned, it stays while others are used")
            tests.clickItem(tests.findName("meterFollow"), "the one used last") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.meterPin === "" && tests.findText("30%", tests.findName("planMeter")) !== null, "and it can follow the last used again")
            tests.clickItem(tests.findName("planMeter"), "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("meterAll"), "All limits…") }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(settingsDialog.visible && settingsDialog.page === "usage", "All limits… opens Settings → Usage")
            tests.check(tests.findText("Plan limits", d) !== null && tests.findText("updated 2 min ago", d) !== null, "the Plan limits section shows, and when it was checked")
            tests.check(tests.findText("anthropic", d) !== null && tests.findText("80% used", d) !== null && tests.findText("30% used", d) !== null && tests.findText("max", d) !== null, "each account and window is listed")
            tests.shotPopup(settingsDialog, "clicks-quota")
            Workspace.quotaJson = tests.sampleQuota(true) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findText("limit reached", settingsDialog.contentItem) !== null && tests.findText("100% used", settingsDialog.contentItem) !== null, "a reached limit is called out")
            tests.summary(); Qt.quit() } }
    }
    // The composer's bottom bar: every control shown at a usable size and
    // inside the window, the provider's logo by the model, and Send by
    // clicks.
    readonly property var barButtons: ["attachButton", "permissionsButton", "modeButton", "modelButton", "micButton", "sendButton", "expandButton"]
    property string approvalBefore: ""
    property string approvalNext: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickscomposer"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const c = chat.composerItem
            for (const name of tests.barButtons) {
                const b = tests.findName(name, c)
                const right = b ? b.mapToItem(null, b.width, 0).x : 0
                tests.check(b !== null && b.height >= 26 && right <= win.width, name + " shown, ≥ 26 px, inside the window")
            }
            tests.check(c.compactBar ? tests.findName("modelButton", c).tip === "anthropic/claude-demo" : tests.findText("Claude Demo", c) !== null, "the model's name shows (in its tooltip when narrow)")
            const modelLabel = tests.findName("modelButton", c).contentItem.children[1]
            tests.check(c.compactBar || !modelLabel.truncated, "the model's name isn't cut short when there's room")
            const pct = tests.findName("contextPercent", c)
            tests.check(pct === null || !pct.visible || /^\d+% context$/.test(pct.text), "the context bar says how full it is")
            const logo = tests.findName("providerLogo", c)
            tests.check(logo !== null && logo.url.startsWith("data:image/svg+xml"), "the provider's logo shows by it")
            tests.check(!tests.findName("sendButton", c).enabled, "Send is off with nothing typed")
            tests.shot("clicks-composer") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.rowsBefore = chat.transcript.count
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.typeText("fast: hello from the bar") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.check(tests.findName("sendButton", chat.composerItem).enabled, "typing enables Send"); tests.clickItem(tests.findName("sendButton", chat.composerItem), "Send") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(chat.transcript.count > tests.rowsBefore && chat.composerItem.text === "", "clicking Send sends and clears the composer")
            tests.clickItem(tests.findName("expandButton", chat.composerItem), "expand") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const c = chat.composerItem
            tests.check(c.expanded && !chat.transcript.visible && c.height > chat.height * 0.6, "expand: the composer takes the chat's height (" + Math.round(c.height) + " of " + Math.round(chat.height) + ")")
            tests.check(c.inputFocused, "and keeps the cursor")
            TestInput.typeText("fast: line one")
            TestInput.key(Qt.Key_Return, Qt.ShiftModifier)
            TestInput.typeText("line two")
            tests.shot("clicks-composer-expanded") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: TestInput.key(Qt.Key_Escape, 0) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!chat.composerItem.expanded && chat.transcript.visible && chat.composerItem.text === "fast: line one\nline two", "Esc goes back to the chat, text kept")
            TestInput.key(Qt.Key_X, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(chat.composerItem.expanded, "Ctrl+Shift+X expands it"); tests.rowsBefore = chat.transcript.count; TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(!chat.composerItem.expanded && chat.transcript.count > tests.rowsBefore, "sending goes back to the chat")
            tests.approvalBefore = chat.composerItem.approvalMode
            tests.check(["always-ask", "write", "yolo"].includes(tests.approvalBefore), "the shield starts in a mode (" + tests.approvalBefore + ")")
            tests.clickItem(tests.findName("permissionsButton", chat.composerItem), "shield") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const modes = ["always-ask", "write", "yolo"]
            const next = modes[(modes.indexOf(tests.approvalBefore) + 1) % 3]
            tests.check(chat.composerItem.approvalMode === next && Workspace.approvalMode(chat.session.key) === next, "clicking the shield switches to " + next)
            tests.check(!chat.session.planMode && !chat.composerItem.planOn && (chat.composerItem.compactBar || tests.findText("Build", chat.composerItem) !== null), "sessions start in Build")
            tests.clickItem(tests.findName("modeButton", chat.composerItem), "Build") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.planMode && (chat.composerItem.compactBar || tests.findText("Plan", chat.composerItem) !== null) && Workspace.planMode(chat.session.key), "clicking Build switches to Plan")
            tests.check(chat.composerItem.inputFocused, "the composer keeps the cursor")
            tests.shot("clicks-composer-plan") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.rowsBefore = chat.transcript.count
            TestInput.typeText("fast: plan the cache")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const c = chat.composerItem, stop = tests.findName("stopButton", c), send = tests.findName("sendButton", c)
            tests.check(stop !== null && send !== null && send.mapToItem(null, send.width, 0).x <= c.mapToItem(null, c.width, 0).x, "while it runs, Stop and Send both fit")
            tests.shot("clicks-composer-busy") } }
        PauseAnimation { duration: 2200 }
        ScriptAction { script: {
            tests.check(tests.findText("fast: plan the cache", chat.transcript) !== null, "a plan message shows without the plan note")
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_Tab, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(!chat.session.planMode && (chat.composerItem.compactBar || tests.findText("Build", chat.composerItem) !== null) && chat.composerItem.text === "", "Tab switches back to Build (no tab typed)")
            tests.summary(); Qt.quit() } }
    }
    // Dictation, with what the microphone "hears" faked (no recording,
    // no whisper run; click-tests.sh serves a stand-in model locally): the
    // first click downloads the model then listens, Enter inserts, Cancel /
    // Esc drop, the shortcut, Settings → Voice.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdictation"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(!Dictation.modelReady("base"), "no speech model yet")
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.typeText("Please:")
            tests.clickItem(tests.findName("micButton", chat.composerItem), "mic")
            // (The local download is quick: check at once.)
            tests.check(chat.composerItem.awaitingModel && Dictation.downloading === "base", "the first click downloads the model") } }
        PauseAnimation { duration: 10 }
        ScriptAction { script: {
            const c = chat.composerItem
            // (Its layout may not have run yet: check the state the bar follows.)
            tests.check(c.dictationActive, "the bar shows it in place of its buttons (" + [c.dictationMine, c.awaitingModel, Dictation.state, Dictation.downloading, Dictation.error, Dictation.downloadError].join(" / ") + ")") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const c = chat.composerItem
            tests.check(Dictation.modelReady("base") && Dictation.downloading === "", "the model arrives")
            tests.check(Dictation.state === "recording" && c.recording, "then it listens by itself")
            tests.check(tests.findName("micLevel", c) !== null && c.micLevel > 0, "with a level meter")
            tests.shot("clicks-dictation") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const c = chat.composerItem
            tests.check(c.dictationSeconds >= 1 && tests.findText("Listening 0:0" + c.dictationSeconds, c) !== null, "and a timer")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Enter inserts the text at the cursor (" + chat.composerItem.text + " / " + Dictation.error + ")")
            tests.check(tests.findText("Please: Add a cache to the loader.", chat.transcript) === null, "and doesn't send it")
            tests.clickItem(tests.findName("micButton", chat.composerItem), "mic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(Dictation.state === "recording", "with the model in, the mic listens at once"); tests.clickItem(tests.findName("dictationCancel", chat.composerItem), "Cancel") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Cancel drops the recording")
            TestInput.key(Qt.Key_D, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.check(Dictation.state === "recording", "Ctrl+Shift+D starts it"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Esc drops it")
            settingsDialog.openPage("voice") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("modelUse-base", d) !== null && tests.findName("modelDownload-small", d) !== null, "Settings → Voice: the downloaded model is in use, the other can be downloaded")
            tests.shotPopup(settingsDialog, "clicks-dictation-settings") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }
    // Remote instances against a throwaway local SSH server (--ssh= its
    // arguments; click-tests.sh starts it): add a host, connect, disconnect.
    SequentialAnimation {
        id: remoteSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksremote"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); settingsDialog.openPage("remote") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("addRemoteHost", settingsDialog.contentItem), "+ Add SSH connection") }
        PauseAnimation { duration: 400 }
        // From ~/.ssh/config (the test's HOME has one).
        ScriptAction { script: {
            const list = tests.findName("sshConfigHosts")
            tests.check(list !== null && list.count === 2 && tests.findText("tester@127.0.0.1:2299") !== null && tests.findText("included-box") !== null,
                      "Add starts on the hosts from ~/.ssh/config, with what they connect to (patterns left out)")
            tests.shotPopup(settingsDialog, "clicks-remote-sshconfig")
            tests.clickItem(tests.findName("useHost-testbox"), "Use") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const h = JSON.parse(Remotes.hostsJson()).find(h => h.name === "testbox")
            tests.check(h !== undefined && h.ssh === "testbox", "Use adds it under its SSH config name")
            Remotes.deleteHost("testbox")
            tests.clickItem(tests.findName("addRemoteHost", settingsDialog.contentItem), "+ Add SSH connection") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("remoteTabManual"), "Type it myself") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("remoteName"), "name")
            TestInput.typeText("box")
            tests.clickItem(tests.findName("remoteSsh"), "ssh command")
            TestInput.typeText("ssh " + win.argValue("--ssh="))
            tests.clickItem(tests.findName("saveRemoteHost"), "Save and connect") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const h = JSON.parse(Remotes.hostsJson())
            tests.check(h.length === 1 && h[0].name === "box", "the host is saved")
            tests.check(h.length === 1 && h[0].state === "connected" && h[0].harlo !== "", "it connects and can run Harlo (" + (h.length ? h[0].state + " " + h[0].message : "") + ")")
            tests.check(tests.findName("remoteHost-box", settingsDialog.contentItem) !== null, "and is listed")
            tests.shotPopup(settingsDialog, "clicks-remote-settings")
            tests.clickItem(tests.findName("remoteConnect-box", settingsDialog.contentItem), "Disconnect") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(JSON.parse(Remotes.hostsJson())[0].state === "", "Disconnect closes it")
            tests.clickItem(tests.findName("remoteConnect-box", settingsDialog.contentItem), "Connect") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(JSON.parse(Remotes.hostsJson())[0].state === "connected", "Connect opens it again")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        // Stage 2: the host as the instance shown (its own projects and
        // sessions), a project there, and a Harlo session.
        ScriptAction { script: tests.clickItem(tests.findName("instanceButton"), "the instance button") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const st = tests.findName("instanceStatus-box"), local = tests.findName("instanceStatus-local")
            tests.check(local && local.text === "This computer" && st && /^Connected · [0-9]+ms ping$/.test(st.text), "the instance menu: Local, and box with its ping (" + (st ? st.text : "none") + ")")
            tests.shotWindow("clicks-instance-menu")
            tests.clickItem(tests.findName("instance-box"), "box") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(Workspace.instance === "box" && tests.findName("instanceLabel").text === "box", "switched: the window shows box")
            tests.check(JSON.parse(Workspace.projectsJson()).length === 0, "with box's projects (none yet)")
            tests.clickItem(tests.findName("addProjectButton"), "+ Project") } }
        PauseAnimation { duration: 600 }
        // (Add a project: its folder from the host's listing.)
        ScriptAction { script: tests.clickItem(tests.findName("addProjectChoose"), "Choose…") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const l = JSON.parse(Remotes.foldersJson || "{}")
            tests.check(sidebar.remoteFolderDialog.visible && !l.loading && l.error === "" && l.dirs.length > 0 && l.display === "box:~", "it browses the host's home (" + l.display + ", " + l.dirs.length + " folders " + l.error + ")")
            // Straight to the test repository (same machine, over SSH).
            Remotes.requestFolders("ssh://box" + win.argValue("--cwd=").replace(/\/repo$/, "")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const row = tests.findText("repo", sidebar.remoteFolderDialog.contentItem)
            tests.check(row !== null, "folders are listed")
            tests.clickItem(row, "repo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.shotPopup(sidebar.remoteFolderDialog, "clicks-remote-folders")
            tests.clickItem(tests.findName("remoteFolderAdd", sidebar.remoteFolderDialog.contentItem), "Add repo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("addProjectAdd"), "Add project") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo")
            tests.remoteProject = p ? Number(p[0]) : -1
            tests.check(p !== undefined && p[2] === "harlo", "the remote folder is a project (Harlo, harlo's own there)")
            const texts = []
            const walk = it => { for (const c of it.children) { if (c.visible && typeof c.text === "string" && c.text.indexOf("box") >= 0) texts.push(c.text); walk(c) } }
            walk(sidebar)
            tests.check(texts.includes("box") && Remotes.displayPath("ssh://box" + win.argValue("--cwd=")) === "box:" + win.argValue("--cwd=").replace(Workspace.homeDir(), "~"), "the project shows its host (box)")
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 600 }
        // (click-tests.sh: the host's check says it has no bubblewrap.)
        ScriptAction { script: {
            const row = tests.findName("hostProvider-harlo", settingsDialog.contentItem)
            tests.check(row !== null && row.detail.indexOf("Sandbox: needs bubblewrap") >= 0, "a host without bubblewrap: Harlo's row says the sandbox needs it (" + (row ? row.detail : "none") + ")")
            tests.shotPopup(settingsDialog, "clicks-remote-providers")
            settingsDialog.close()
            Workspace.newSession(tests.remoteProject, "") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.waitUntil(remoteSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.log("remote session:", chat.session.harness, chat.session.status, chat.session.cwd, chat.session.modelName, Workspace.approvalMode(chat.session.key))
            tests.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "a new session there runs Harlo in the remote folder")
            tests.check(chat.session.modelName !== "", "Harlo answers over SSH (model " + chat.session.modelName + ")")
            tests.check(Workspace.approvalMode(chat.session.key) === "always-ask", "it starts asking first")
            tests.shot("clicks-remote-session") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // Local again: box's projects, sessions and tabs are out of sight.
            tests.instanceKey = chat.session.key
            tests.clickItem(tests.findName("instanceButton"), "the instance button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("instance-local"), "Local") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const tabs = Workspace.tabs.map(t => t.split("\t")[0])
            tests.check(Workspace.instance === "" && JSON.parse(Workspace.projectsJson()).every(p => !p.remote), "Local: this computer's projects only")
            // The phone's New Session: every machine's projects, whichever the window shows.
            const ph = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "projects" })))
            const onBox = (ph.projects || []).find(p => p.host === "box")
            tests.check(onBox && onBox.where.startsWith("box:") && onBox.agents.indexOf("harlo") >= 0, "the phone still gets box's project, with its machine and agents (" + JSON.stringify(onBox) + ")")
            tests.check(tabs.indexOf(tests.instanceKey) < 0 && tests.findText("box", sidebar) === null, "box's tab and sessions are out of sight (" + tabs + ")")
            // Opening box's session (a notification, say) goes back to box.
            Workspace.open(tests.instanceKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const tabs = Workspace.tabs.map(t => t.split("\t")[0])
            tests.check(Workspace.instance === "box" && Workspace.activeKey === tests.instanceKey && tabs.indexOf(tests.instanceKey) >= 0, "opening box's session shows box again, its tab there")
            tests.check(Workspace.instanceOf(tests.instanceKey) === "box", "a session knows its instance") } }
        PauseAnimation { duration: 2500 }
        // Stage 3: the host's saved sessions in the project.
        ScriptAction { script: {
            const row = tests.findText("Remote hello", sidebar)
            tests.check(row !== null, "the host's saved session shows in the project")
            tests.clickItem(row, "Remote hello") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const find = it => { for (const c of it.children) { if (c.visible && c.html !== undefined && c.html.indexOf("Hi from the remote host") >= 0) return c; const f = find(c); if (f) return f } return null }
            tests.check(tests.findText("Remote hello", chat.transcript) !== null && find(chat.transcript) !== null, "opening it reads its transcript over SSH")
            tests.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "as a Harlo session in the remote folder")
            tests.shot("clicks-remote-saved") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // Stage 4: the Changes panel and a terminal on the host.
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        // A remote session with branches: counted, listed and previewed from
        // the host's file, read over SSH in the background; continuing one
        // writes the new session there.
        ScriptAction { script: {
            let key = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("remote-branches.jsonl") >= 0) key = Workspace.keyAt(i)
            tests.check(key !== "", "the host's session with branches is listed")
            if (key !== "") Workspace.open(key) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(chat.branchCount === 2 && tests.findText("2 branches") !== null, "its branches are counted (" + chat.branchCount + ")")
            tests.clickText("2 branches") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const d = chat.branchDialog
            tests.check(d.visible && d.branches.length === 2 && d.current && d.current.lastUser === "Branchy second", "the dialog lists both, the other one picked (" + (d.current ? d.current.lastUser : "") + ")")
            tests.check(d.previewSession.transcriptText().indexOf("ok 2") >= 0 && d.previewSession.transcriptText().indexOf("ok 3") < 0, "and previews it")
            tests.clickText("Continue in a new session", d.contentItem) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const k = Workspace.sessionFile(Workspace.activeKey)
            tests.check(k.indexOf("ssh://box") === 0 && k.indexOf("remote-branches") < 0 && k.endsWith(".jsonl") && tests.chatText().indexOf("ok 2") >= 0 && tests.chatText().indexOf("ok 3") < 0, "continuing it opens a new session, written on the host (" + k + ")")
            tests.check(TestInput.readFile(k.replace("ssh://box", "")).indexOf("Branchy second") >= 0, "whose file is there") } }
        // The host's sessions are in the message search too.
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.findForTest("Branchy edited") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const hits = tests.foundHits || []
            tests.check(hits.some(h => String(h.path).indexOf("ssh://box") === 0 && String(h.path).indexOf("remote-branches.jsonl") > 0), "message search finds the host's sessions (" + hits.length + " hits)") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(git.isRepo && git.branch === "main" && git.root === "ssh://box" + win.argValue("--cwd=") && git.files.join("\n").indexOf("notes.md") >= 0,
                      "Changes shows the remote repository (" + git.root + " " + git.branch + " " + git.files.length + " " + git.message + ")")
            // Its branches, read over SSH off the UI thread.
            git.loadBranches() } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const b = git.branchesList !== "" ? JSON.parse(git.branchesList) : {}
            tests.check(b.current === "main" && (b.local || []).some(x => x.name === "wt-branch"), "the host's branches are listed (" + git.branchesList.slice(0, 80) + ")")
            // A changed file on the host, side by side.
            tests.clickText("notes.md", changes) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(Workspace.activeKey === "diff:0:ssh://box" + win.argValue("--cwd=") + "/notes.md" && dv.rows.length === 1 && dv.rightPane.editor.text === "notes", "the diff tab shows a file on the host (" + Workspace.activeKey + ", " + dv.rows.length + " rows)")
            Workspace.closeTab(Workspace.activeKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(terminal.visible && terminal.current.running && terminal.current.cwd === "ssh://box" + win.argValue("--cwd="), "the terminal opens on the host (" + terminal.current.cwd + ")")
            tests.clickItem(terminal.current, "terminal")
            TestInput.typeText("touch remote-term-ok")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.shot("clicks-remote-terminal"); git.refresh(true) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.check(git.files.join("\n").indexOf("remote-term-ok") >= 0, "its shell runs in the remote folder")
            git.commitAll("remote commit") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(!git.error && git.files.length === 0, "commit all commits there (" + git.message + ")")
            // Stage 5: files on the host.
            terminal.visible && win.toggleTerminal()
            windowSettings.rightMode = "files"
            chat.session.searchFiles("READ") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.check(chat.session.fileMatches.indexOf("README.md") >= 0, "@ completion lists the host's files (" + chat.session.fileMatches.join(",") + ")")
            const row = tests.findText("README.md", files)
            tests.check(row !== null && tests.findText(".git", files) === null, "the Files panel lists the remote folder")
            tests.clickItem(row, "README.md") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const find = it => { for (const c of it.children) { if (c.visible && c.text === "hello\n" && c.cursorPosition !== undefined) return c; const f = find(c); if (f) return f } return null }
            tests.check(Workspace.activeKey === "file:ssh://box" + win.argValue("--cwd=") + "/README.md" && find(win.contentItem) !== null, "it opens in the editor, read over SSH (" + Workspace.activeKey + ")")
            tests.shot("clicks-remote-file")
            // Edited and saved: over SSH, in the background.
            chat.fileEditor.editor.text = "hello from harlo\n"
            chat.fileEditor.save() } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const readme = win.argValue("--cwd=") + "/README.md"
            tests.check(TestInput.readFile(readme) === "hello from harlo\n" && !chat.fileEditor.modified, "saving writes it on the host")
            // Changed on the host (an agent there, say): no edits here, so it reloads.
            TestInput.writeFile(readme, "changed on the host\n") } }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: {
            tests.check(chat.fileEditor.editor && chat.fileEditor.editor.text === "changed on the host\n", "a change on the host reloads it (" + (chat.fileEditor.editor ? JSON.stringify(chat.fileEditor.editor.text) : "") + ")")
            // With edits here, a change there is pointed out instead.
            chat.fileEditor.apply([[0, chat.fileEditor.editor.length, "my edit\n"]])
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "changed again\n") } }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: {
            tests.check(chat.fileEditor.diskNotice === "changed" && chat.fileEditor.editor.text === "my edit\n", "with unsaved edits, it says the file changed there (" + chat.fileEditor.diskNotice + ")")
            tests.summary(); Qt.quit() } }
    }
    property int remoteProject: -1
    // Notifications with the demo harness (self-tests record them instead
    // of showing them): a template, completion and question for a session
    // you aren't viewing, the focus rule, the session in view, off, test.
    property string notifyKeyA: ""
    property string notifyBefore: ""
    function lastNote() { try { return JSON.parse(Workspace.lastNotification) } catch (e) { return {} } }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksnotify"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); settingsDialog.openPage("notifications") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("notifyEnabled", d).checked && tests.findName("notifyWhileFocused", d).checked && tests.findName("notifyEvent-subagent", d).checked, "notifications are on by default (while focused too, like before)")
            tests.check(tests.findName("notifyTitle-completion", d).placeholderText === "{agent_name} is ready", "templates show their defaults")
            tests.clickItem(tests.findName("notifyTitle-completion", d), "completion title")
            TestInput.typeText("{session_name} is done")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(JSON.parse(Workspace.notifySettingsJson()).events.completion.title === "{session_name} is done", "editing a template saves it")
            tests.shotPopup(settingsDialog, "clicks-notify-settings") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { settingsDialog.close(); Workspace.setWindowActive(true); tests.notifyKeyA = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.sendByKeys("fast: hello"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            const n = tests.lastNote()
            tests.check(n.kind === "completion" && n.key === tests.notifyKeyA && n.title === "fast: hello is done" && n.body === "Claude Demo completed the task",
                      "a finished session you aren't viewing notifies, with the template (" + Workspace.lastNotification + ")")
            Workspace.open(tests.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.sendByKeys("approve please"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const n = tests.lastNote()
            tests.check(n.kind === "question" && n.title === "Input needed" && n.body !== "", "a question notifies (" + n.body + ")")
            // While focused, off: quiet while harlo is focused.
            const c = JSON.parse(Workspace.notifySettingsJson()); c.whileFocused = false; Workspace.setNotifySettings(JSON.stringify(c))
            tests.notifyBefore = Workspace.lastNotification
            Workspace.open(tests.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.sendByKeys("fast: again"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            tests.check(Workspace.lastNotification === tests.notifyBefore, "with “while focused” off, nothing while harlo is focused")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.whileFocused = true; Workspace.setNotifySettings(JSON.stringify(c))
            Workspace.open(tests.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.sendByKeys("fast: watched") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            tests.check(Workspace.lastNotification === tests.notifyBefore, "never for the session you're looking at")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.enabled = false; Workspace.setNotifySettings(JSON.stringify(c))
            tests.sendByKeys("fast: off"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            tests.check(Workspace.lastNotification === tests.notifyBefore, "nothing when notifications are off")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.enabled = true; Workspace.setNotifySettings(JSON.stringify(c))
            settingsDialog.openPage("notifications") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("notifyTest", settingsDialog.contentItem), "Send test notification") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.lastNote().kind === "test" && tests.lastNote().title === "Test notification", "Send test notification sends one")
            tests.clickItem(tests.findName("quietOn", settingsDialog.contentItem), "Quiet hours") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const q = JSON.parse(Workspace.notifySettingsJson()).quiet
            tests.check(q.on && q.from === 1320 && q.to === 480 && !q.asks, "Quiet hours on: 22:00 to 08:00 at first (" + JSON.stringify(q) + ")")
            tests.check(tests.findName("quietFrom", settingsDialog.contentItem).displayText === "22:00", "its times show")
            tests.shotPopup(settingsDialog, "clicks-notify-quiet")
            // Quiet from this half hour for an hour: now.
            const now = new Date(), m = now.getHours() * 60 + now.getMinutes()
            const c = JSON.parse(Workspace.notifySettingsJson()); c.quiet.from = m - m % 30; c.quiet.to = (m - m % 30 + 60) % 1440
            Workspace.setNotifySettings(JSON.stringify(c))
            settingsDialog.close()
            tests.notifyBefore = Workspace.lastNotification
            Workspace.open(tests.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.sendByKeys("fast: at night"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            tests.check(Workspace.lastNotification === tests.notifyBefore, "quiet hours: a finished run doesn't notify")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.quiet.asks = true; Workspace.setNotifySettings(JSON.stringify(c))
            Workspace.open(tests.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.sendByKeys("approve please"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(tests.lastNote().kind === "question" && Workspace.lastNotification !== tests.notifyBefore, "with “let questions through”, a question still does")
            tests.summary(); Qt.quit() } }
    }
    // MCP settings with HOME in the test folder (click-tests.sh) and a fake
    // server script (--mcp-server=): add, test, edit, per harness, project
    // scope, import, delete.
    property string mcpProject: ""
    function mcpList(scope) { return JSON.parse(Mcp.listJson(scope)).servers }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksmcp"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            // Only Harlo here (clicksmcpagents covers Claude).
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd="))
            tests.mcpProject = Workspace.projectRoots(1)[0]
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("No MCP servers here yet.", d) !== null, "no servers yet")
            tests.clickItem(tests.findName("mcpAdd", d), "Add MCP server") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.clickItem(tests.findName("mcpName", d), "name")
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("fake")
            tests.clickItem(tests.findName("mcpCommand", d), "command")
            TestInput.typeText(win.argValue("--mcp-server="))
            tests.clickItem(tests.findName("mcpSave", d), "Create") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const l = tests.mcpList("global")
            tests.check(l.length === 1 && l[0].name === "fake" && l[0].harnesses.join() === "harlo", "Create saves it for Harlo (" + JSON.stringify(l.map(s => s.name + ":" + s.harnesses)) + ")")
            const st = JSON.parse(Mcp.statusJson("global")).fake
            tests.check(st && st.state === "connected" && st.tools.length === 2, "and tests it: connected, 2 tools (" + JSON.stringify(st) + ")")
            const pill = tests.findName("mcpStatus-fake", settingsDialog.contentItem)
            tests.check(pill !== null && pill.text === "Connected · 2 tools", "the card shows the status")
            const card = tests.findName("mcpServer-fake", settingsDialog.contentItem), over = card ? tests.spills(card) : ["no card"]
            tests.check(over.length === 0, "its badges and switch stay inside the card (" + over.join(", ") + ")")
            tests.shotPopup(settingsDialog, "clicks-mcp-grid")
            tests.clickItem(tests.findName("mcpToggle-fake", settingsDialog.contentItem), "the card's switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = tests.mcpList("global")
            tests.check(!l[0].form.enabled && tests.findName("mcpStatus-fake", settingsDialog.contentItem).text === "Disabled", "the card's switch turns it off")
            tests.clickItem(tests.findName("mcpToggle-fake", settingsDialog.contentItem), "the card's switch") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const st = JSON.parse(Mcp.statusJson("global")).fake
            tests.check(tests.mcpList("global")[0].form.enabled && st && st.state === "connected", "and on again, tested at once")
            tests.clickItem(tests.findName("mcpServer-fake", settingsDialog.contentItem), "fake") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("look", d) !== null && tests.findText("Looks.", d) !== null, "the server's page lists its tools")
            const bar = tests.findName("mcpActionBar", d)
            const flick = tests.ancestorWith(tests.findName("mcpName", d), "contentY")
            tests.check(bar !== null && bar.parent.objectName === "settingsFooter" && bar.mapToItem(null, 0, bar.height).y <= d.mapToItem(null, 0, d.height).y + 1
                      && flick.contentHeight > flick.height, "Save / Cancel / Delete float at the bottom while the page scrolls")
            tests.shotPopup(settingsDialog, "clicks-mcp-server")
            tests.check(tests.findName("mcpForHarlo", d) !== null && tests.findName("mcpForHarlo", d).checked, "it's ticked for Harlo")
            tests.clickItem(tests.findName("mcpTimeout", d), "timeout")
            TestInput.typeText("45")
            tests.clickItem(tests.findName("mcpSave", d), "Save") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const l = tests.mcpList("global")
            tests.check(l.length === 1 && l[0].harnesses.join() === "harlo" && l[0].form.timeoutSecs === 45, "the timeout is saved (" + JSON.stringify(l[0]) + ")")
            settingsDialog.contentItem && tests.clickItem(tests.findName("mcpScope", settingsDialog.contentItem), "scope") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("repo") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("No MCP servers here yet.", d) !== null && tests.findText("fake", d) === null, "the project scope has its own servers")
            tests.clickItem(tests.findName("mcpAdd", d), "Add") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("mcpImport", settingsDialog.contentItem), "Import JSON…") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = tests.findName("mcpImportText")
            t.text = '{"mcpServers":{"one":{"command":"' + win.argValue("--mcp-server=") + '"},"web":{"type":"http","url":"http://127.0.0.1:9/mcp"}}}'
            tests.clickItem(tests.findName("mcpImportGo"), "Import") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            const l = tests.mcpList(tests.mcpProject)
            tests.check(l.length === 2 && l.every(s => s.harnesses.join() === "harlo"), "importing two servers adds both to the project (" + JSON.stringify(l.map(s => s.name)) + ")")
            const st = JSON.parse(Mcp.statusJson(tests.mcpProject))
            tests.check(st.one && st.one.state === "connected" && st.web && st.web.state === "failed", "and tests them (" + (st.web ? st.web.error : "") + ")")
            tests.clickItem(tests.findName("mcpServer-web", settingsDialog.contentItem), "web") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickItem(tests.findName("mcpDelete", settingsDialog.contentItem), "Delete") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("Discard") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = tests.mcpList(tests.mcpProject)
            tests.check(l.length === 1 && l[0].name === "one", "Delete removes it")
            tests.check(Workspace.fileStamp(tests.mcpProject + "/.harlo/mcp.json") !== "", "project servers are in .harlo/mcp.json")
            tests.summary(); Qt.quit() } }
    }
    // Phone access from the desktop: the sidebar's button, the consent
    // screen, the QR code and link, Settings → Phone. (scripts/
    // phone-e2e.sh tests a phone pairing for real.)
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksphone"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000))
            tests.clickItem(tests.findName("pairDeviceButton"), "phone button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(pairDeviceDialog.visible && tests.findText("Turn on phone access?") !== null && !Phone.running, "the phone button asks to turn phone access on")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-consent") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("phoneTurnOn"), "Turn on") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const qr = tests.findName("pairQr"), link = tests.findName("pairLink")
            tests.check(Phone.enabled && Phone.running && Phone.error === "", "Turn on starts it (" + Phone.error + ")")
            tests.check(qr !== null && String(qr.source).startsWith("data:image/svg+xml") && link !== null && /^http:\/\/[0-9.]+:[0-9]+\/#p=/.test(link.text),
                      "a QR code and link appear (" + (link ? link.text.substring(0, 40) : "") + ")")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-qr") } }
        PauseAnimation { duration: 400 }
        // The firewall (a stand-in ufw that blocks the port): pointed out,
        // and opened with one click (pkexec asks for the password).
        ScriptAction { script: {
            tests.check(tests.findName("firewallNote") !== null && tests.findText("This computer's firewall (ufw) blocks phones, so the link won't open on yours yet.") !== null,
                      "a firewall that blocks phones is pointed out")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-firewall") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("allowFirewall"), "Allow phones on this network") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            tests.check(tests.findName("firewallNote") === null && Phone.firewallError === "" && !Phone.firewallBusy && tests.findName("pairQr") !== null,
                      "Allow opens it and the note goes (" + Phone.firewallError + ")")
            // The app tries every address: the other kind (Tailscale or this
            // network), when this computer has one, is pointed out too.
            const kinds = JSON.parse(Phone.addressesJson()).map(a => String(a.label).startsWith("Tailscale"))
            tests.otherAddress = kinds.indexOf(true) >= 0 && kinds.indexOf(false) >= 0
            if (tests.otherAddress) {
                tests.check(tests.findName("firewallOtherNote") !== null, "the firewall on the other address is pointed out too")
                tests.clickItem(tests.findName("allowFirewallOther"), "Allow on the other address")
            } } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: if (tests.otherAddress) tests.check(tests.findName("firewallOtherNote") === null, "and Allow opens that one too") }
        PauseAnimation { duration: 300 }
        // From anywhere, 1: harlo's relay (relay.harlo.run, or --relay=).
        ScriptAction { script: tests.clickItem(tests.findName("pairAddress"), "the address picker") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Anywhere  ·  Harlo relay") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findText("Use Harlo's relay?") !== null && tests.findName("pairQr") === null, "harlo's relay asks first")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-relay-consent")
            tests.clickItem(tests.findName("pairRelayOn"), "Use relay") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const link = tests.findName("pairLink")
            tests.check(Phone.relayOn && link !== null && link.text.startsWith(tests.relayUnderTest + "/#p="), "the QR code points at the relay (" + (link ? link.text.substring(0, 40) : "") + ")")
            tests.check(Phone.relayState === "online" && tests.findName("pairRelayState") === null, "harlo is connected to the relay (" + Phone.relayState + " " + Phone.relayError + ")")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-relay-qr")
            tests.clickItem(tests.findName("pairAddress"), "the address picker") } }
        // From anywhere, 2: Tailscale Funnel (a stand-in tailscale).
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Anywhere  ·  Tailscale Funnel") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Phone.funnelAvailable && tests.findText("Use Tailscale Funnel?") !== null && tests.findName("pairQr") === null, "Funnel asks first")
            tests.clickItem(tests.findName("pairFunnelOn"), "Use Funnel") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const link = tests.findName("pairLink")
            tests.check(Phone.funnelOn && Phone.funnelState === "online" && link !== null && link.text.startsWith("https://desk.tail1234.ts.net:8443/#p="),
                      "the QR code points at the funnel (" + Phone.funnelState + " " + Phone.funnelError + " " + (link ? link.text.substring(0, 44) : "") + ")")
            tests.shotPopup(pairDeviceDialog, "clicks-phone-funnel-qr") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { pairDeviceDialog.close(); settingsDialog.openPage("devices") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("phoneAccess", d).checked && tests.findText("No phones paired yet.", d) !== null, "Settings → Phone: on, no phones yet")
            tests.check(tests.findName("relaySwitch", d).checked && tests.findName("funnelSwitch", d).checked, "the relay and Funnel show as on")
            tests.shot("clicks-phone-settings") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("funnelSwitch", settingsDialog.contentItem), "the Funnel switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!Phone.funnelOn && Phone.funnelState === "off", "the switch turns Funnel off")
            tests.clickItem(tests.findName("phoneAccess", settingsDialog.contentItem), "the switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!Phone.enabled && !Phone.running && Phone.relayState === "off", "the switch turns it off, the relay too")
            tests.summary(); Qt.quit() } }
    }
    readonly property string relayUnderTest: win.argValue("--relay=") || "https://relay.harlo.run"
    // Claude sessions, against a stand-in claude (scripts/fake-claude.py,
    // --fake-claude=): a reply, an approval, a question, model / Plan /
    // permission switches (the stand-in's replies report them), stopping,
    // and the session in the sidebar, reopened with its history and resumed.
    property string claudeKey: ""
    // A test's message search (off the UI thread, as the dialog's): its hits
    // land here.
    property var foundHits: null
    function findForTest(words) { tests.foundHits = null; Workspace.findMessages(words, 20, 900001) }
    Connections {
        target: Workspace
        enabled: win.selftestDir !== ""
        function onMessagesFound(gen, json) { if (gen === 900001) tests.foundHits = JSON.parse(json) }
    }
    function chatText() { return chat.session.transcriptText() }
    SequentialAnimation {
        id: claudeSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksclaude"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("hello there") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(chat.session.harness === "claude", "a Claude session (" + chat.session.harness + ")")
            tests.check(tests.chatText().indexOf("Hello from fake claude (model claude-sonnet-5, mode default)") >= 0, "it streams the reply (" + tests.chatText().slice(-160) + ")")
            Workspace.renameSession(tests.claudeKey, "My Claude chat") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(chat.session.title === "My Claude chat", "renamed (" + chat.session.title + ")")
            tests.sendByKeys("please approve writing a file") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            // (On its tool row when that's in view; the bar otherwise.)
            tests.check(chat.session.requestIsApproval && (tests.findText("Allow Write to run?", chat.transcript) !== null || tests.findText("Approval needed: Write", chat.requestDock) !== null), "a Write asks for approval")
            tests.shotWindow("clicks-claude-approval")
            tests.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(!chat.session.hasRequest && tests.chatText().indexOf("Wrote hi.txt.") >= 0, "Approve lets it write")
            tests.sendByKeys("approve it again") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const always = tests.findName("alwaysAllowInline", chat.transcript)
            tests.check(always !== null && always.ToolTip.text === "Remember: acceptEdits mode (for this session)", "it offers Always allow with Claude's suggestion (" + (always ? always.ToolTip.text : "none") + ")")
            if (always) tests.clickItem(always, "Always allow") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(!chat.session.hasRequest && tests.chatText().indexOf("remembering acceptEdits") >= 0, "Always allow hands Claude its suggestion back (" + tests.chatText().slice(-60) + ")")
            tests.sendByKeys("plan it please") } }
        // A long plan to approve: drawn as Markdown, scrolling in its card,
        // its answers in view.
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const scroll = tests.findName("requestScroll")
            const start = tests.findText("Start with this plan")
            const prose = tests.findAllNames("requestProse", scroll || win.contentItem).map(t => t.getText(0, t.length))
            tests.check(chat.session.hasRequest && scroll !== null && scroll.scrolls && scroll.height <= Math.max(140, chat.height * 0.45) + 1, "a long plan scrolls in its card (" + (scroll ? Math.round(scroll.height) + " of " + Math.round(scroll.contentHeight) : "none") + ")")
            tests.check(start !== null && start.mapToItem(null, 0, start.height).y <= win.height && tests.findText("Keep planning") !== null, "its answers stay in view")
            tests.check(prose.some(t => t === "Context") && !prose.some(t => t.indexOf("## ") >= 0) && prose.some(t => t.indexOf("Step 1.") >= 0 && t.indexOf("**") < 0), "it reads as Markdown: headings and a list, not ## or ** (" + prose.slice(0, 3).join(" | ") + ")")
            tests.shotWindow("clicks-claude-plan")
            if (scroll) scroll.contentY = scroll.contentHeight - scroll.height } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const last = tests.findAllNames("requestProse", tests.findName("requestScroll")).find(t => t.getText(0, t.length) === "The last line of the plan.")
            tests.check(last !== null && last !== undefined, "scrolled down, its end shows")
            tests.clickText("Start with this plan") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(!chat.session.hasRequest && tests.chatText().indexOf("Plan approved.") >= 0, "Start with this plan approves it (" + tests.chatText().slice(-60) + ")")
            tests.sendByKeys("ask me which database") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(chat.session.hasRequest && chat.session.requestMethod === "ask" && tests.findText("Which database?", chat.askPanel) !== null, "its question shows in the ask panel")
            tests.clickText("Postgres", chat.askPanel) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Send answers", chat.askPanel) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("You picked Postgres.") >= 0, "the answer goes back (" + tests.chatText().slice(-80) + ")")
            chat.session.setModel("anthropic", "claude-haiku-4-5")
            tests.clickItem(tests.findName("modeButton", chat.composerItem), "Plan") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: tests.sendByKeys("what now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("(model claude-haiku-4-5, mode plan)") >= 0, "the model and Plan reach Claude (" + tests.chatText().slice(-110) + ")")
            tests.clickItem(tests.findName("modeButton", chat.composerItem), "Build")
            tests.clickItem(tests.findName("permissionsButton", chat.composerItem), "the shield") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: tests.sendByKeys("and now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("mode acceptEdits)") >= 0, "the shield's edits mode reaches Claude (" + tests.chatText().slice(-80) + ")")
            tests.sendByKeys("run a build in the background") } }
        // A background command's end: a notice as it comes (and the same
        // one read back from the file, below).
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            const said = 'Background command "sleep 1; echo built" completed (exit code 0)'
            tests.check(tests.chatText().split(said).length === 2 && tests.chatText().indexOf("It finished: built.") >= 0, "a background command's end is a notice as it comes (" + tests.chatText().slice(-160) + ")")
            tests.check(tests.findContaining("<task-notification>", chat.transcript) === null, "not a message of yours")
            const bg = tests.findAllNames("backgroundNotice", chat.transcript).filter(n => n.visible)
            const head = bg.length ? tests.findName("backgroundHeadline", bg[0]) : null
            const cmd = bg.length ? tests.findName("backgroundCommand", bg[0]) : null
            tests.check(bg.length === 1 && head && head.text === "Background command finished" && cmd && cmd.text === "$ sleep 1; echo built", "drawn as how it went over its command (" + (head ? head.text : "none") + " | " + (cmd ? cmd.text : "none") + ")")
            tests.shotWindow("clicks-claude-background")
            tests.sendByKeys("run a script in the background") } }
        // A long one that fails: its first line, a click away from all of it.
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const bg = tests.findAllNames("backgroundNotice", chat.transcript).filter(n => n.visible)
            const last = bg[bg.length - 1]
            const head = last ? tests.findName("backgroundHeadline", last) : null
            const cmd = last ? tests.findName("backgroundCommand", last) : null
            tests.check(bg.length === 2 && head && head.text === "Background command failed" && head.color === Theme.error, "a failed one says so, in red (" + (head ? head.text : "none") + ")")
            tests.check(cmd && cmd.text === "$ cd /tmp; python3 - <<'EOF' …" && cmd.lineCount === 1, "its command on one line (" + (cmd ? cmd.text : "none") + ")")
            tests.check(last && tests.findContaining("exit code 1", last) !== null, "with its exit code")
            tests.bgNotice = last
            if (head) tests.clickItem(head, "the failed command's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const open = tests.bgNotice ? tests.findName("backgroundOpen", tests.bgNotice) : null
            tests.check(open && open.visible && tests.findContaining('print("checking")', open) !== null, "a click shows all of it")
            tests.shotWindow("clicks-claude-background-open")
            // /marketplace: harlo runs claude plugin marketplace add, and
            // Claude never gets the words.
            tests.pluginAsked = tests.chatText().split("Hello from fake claude").length
            tests.sendByKeys("/marketplace add obra/superpowers-marketplace") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(claudeSteps, () => tests.findAllNames("backgroundHeadline", chat.transcript).some(h => h.text === "Marketplace added"), 10000) }
        // (It starts again, idle, to load what was added.)
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const cmd = tests.findAllNames("backgroundCommand", chat.transcript).find(c => c.visible && c.text.indexOf("claude plugin") >= 0)
            tests.check(cmd && cmd.text === "$ claude plugin marketplace add obra/superpowers-marketplace", "/marketplace add runs claude plugin marketplace add (" + (cmd ? cmd.text : "none") + ")")
            tests.check(tests.chatText().split("Hello from fake claude").length === tests.pluginAsked && chat.session.status === "idle", "not sent to Claude (" + chat.session.status + ")")
            tests.sendByKeys("go slow please") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            tests.check(chat.session.status === "busy", "a slow reply runs")
            tests.clickItem(tests.findName("stopButton", chat.composerItem), "Stop") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(chat.session.status === "idle", "Stop interrupts it (" + chat.session.status + ")")
            // Stop the process (the sidebar's Stop process): the session is
            // then its file.
            Workspace.closeSession(tests.claudeKey) } }
        // The scanner lists it from its file; open it again.
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            // Its own transcript (under the test's Claude config folder) only.
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("/claude-config/projects/") >= 0) saved = Workspace.keyAt(i)
            tests.check(saved !== "", "the session is in the sidebar (" + saved + ")")
            if (saved === "") { tests.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("hello there") >= 0 && tests.chatText().indexOf("You picked Postgres.") >= 0, "reopened, it shows its history")
            tests.check(chat.session.title === "My Claude chat", "and its name (" + chat.session.title + ")")
            // Not started yet (nothing sent): its / list is the one Claude
            // gave last time in this folder, with harlo's /plugin.
            tests.check(chat.session.status !== "idle" && chat.session.commands.some(c => c.indexOf("review\t") === 0) && chat.session.commands.some(c => c.indexOf("plugin\t") === 0),
                      "before it starts, its / list is Claude's from last time (" + chat.session.status + ": " + chat.session.commands.map(c => c.split("\t")[0]).join(", ") + ")")
            tests.check(tests.chatText().split('Background command "sleep 1; echo built" completed (exit code 0)').length === 2 && tests.findContaining("<task-notification>", chat.transcript) === null, "the background command's notice, once, read back")
            if (chat.session.harness !== "claude") { tests.check(false, "the reopened session is the Claude one"); tests.summary(); Qt.quit() }
            else tests.sendByKeys("one more") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Hello from fake claude") >= 0 && chat.session.status === "idle" && tests.chatText().split("Hello from fake claude").length > 1, "and it carries on (resumed)")
            tests.findForTest("picked Postgres") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const hits = tests.foundHits || []
            tests.check(hits.some(h => String(h.path).indexOf("/claude-config/projects/") >= 0), "message search finds Claude's sessions (" + hits.length + " hits)")
            tests.shot("clicks-claude")
            // Archived from the phone while open (and done): it leaves the lists.
            tests.claudeKey = chat.session.key
            const r = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "archive", key: tests.claudeKey, archived: true })))
            tests.check(r.ok === true, "the phone archives it (" + JSON.stringify(r) + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const keys = []
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) keys.push(Workspace.keyAt(i))
            tests.check(keys.indexOf(tests.claudeKey) < 0 && !keys.some(k => k.indexOf("/claude-config/projects/") >= 0), "archived, the open session leaves the sidebar (" + keys + ")")
            tests.summary(); Qt.quit() } }
    }


    // Provider marks: a Claude project's sessions of each provider show
    // their provider's mark (the status on its corner) in the sidebar and
    // the tabs, the project shows what its new sessions run, and the header
    // names the provider.
    // (No root: the window and its popups.)
    function findAllNamed(name, root, out) {
        if (!root) {
            for (const r of searchRoots(null)) findAllNamed(name, r, out)
            return out
        }
        for (const c of root.children) {
            if (!c.visible || c.opacity <= 0) continue
            if (c.objectName === name && shown(c)) out.push(c)
            findAllNamed(name, c, out)
        }
        return out
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksmarks"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const id = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setProjectHarness(id, "claude")
            Workspace.newSession(id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("hello there") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.sendByKeys("hello there") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            // Claude Code on Claude: Claude's mark; Demo on its Claude
            // model: the model's maker, Demo as a tag.
            const marks = tests.findAllNamed("sessionProvider", sidebar, []).map(m => m.provider).sort()
            const makers = tests.findAllNamed("sessionMaker", sidebar, []).map(m => m.provider)
            tests.check(marks.join(",") === "claude" && makers.join(",") === "anthropic" && tests.findAllNamed("sessionAgentTag", sidebar, []).length === 1,
                        "each session shows its model's maker, its agent as a tag, Claude Code on Claude just Claude's mark (" + marks + " | " + makers + ")")
            const p = tests.findName("projectProvider-repo", sidebar)
            // (A narrow sidebar leaves the mark out: the name keeps its room.)
            tests.check(p !== null ? p.provider === "claude" : sidebar.width < Theme.px(240), "the project shows what its new sessions run (" + (p ? p.provider : "narrow: left out") + ")")
            tests.check(tests.findText("Demo", tests.findName("sessionProviderChip")) !== null, "the header names the provider")
            const tabMarks = tests.findAllNamed("tabProvider", win.contentItem, []).map(m => m.provider).sort()
            tests.check(tabMarks.join(",") === "claude,demo", "and so do the tabs (" + tabMarks + ")")
            const cl = tests.findAllNamed("sessionProvider", sidebar, []).find(m => m.provider === "claude")
            const at = cl.mapToItem(null, cl.width / 2, cl.height / 2)
            TestInput.moveTo(at.x, at.y) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            // The rows are one line (the avatar says the model and agent),
            // and no hover tip covers them.
            const titles = tests.findAllNamed("sessionTitle", sidebar, [])
            const rows = titles.map(t => { let r = t; while (r && r.rowKey === undefined) r = r.parent; return r })
            tests.check(titles.length === 2 && rows.every(r => r && r.height <= 46) && tests.findAllNamed("sessionDetail", sidebar, []).length === 0,
                        "each session row is one line (" + rows.map(r => r ? r.height : "none") + ")")
            tests.check(!ToolTip.toolTip.visible, "pointed at, no tip covers the rows")
            tests.shotWindow("clicks-marks")
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = tests.findAllNamed("menuProvider", null, []).map(m => m.provider)
            tests.check(marks.filter(m => m === "claude").length === 2 && marks.indexOf("harlo") >= 0, "the project menu shows the providers' marks (" + marks + ")")
            tests.shotWindow("clicks-marks-menu")
            sidebar.projectMenu.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: tests.clickItem(tests.findName("draftHarness", chat.draftView), "the provider chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = tests.findAllNamed("menuProvider", null, []).map(m => m.provider)
            tests.check(marks.indexOf("claude") >= 0 && marks.indexOf("harlo") >= 0, "so does New session's (" + marks + ")")
            tests.shotWindow("clicks-marks-draft")
            TestInput.key(Qt.Key_Escape, 0)
            // Each agent × maker: the model's maker's logo with the agent as
            // a tag, except Claude Code on Claude, and no maker known.
            const makers = ["", "anthropic", "openai", "google", "zai-coding-plan", "kimi-for-coding", "deepseek", "xai", "minimax-coding-plan", "mistral", "opencode"]
            // (Laid out on a sheet for the shot: a row per agent, the first
            // with a status on each.)
            tests.marksSheet = matrixSheet.createObject(win.contentItem, { width: 24 + makers.length * 38, height: 92 })
            tests.marksMatrix = []
            for (const [row, agent] of ["harlo", "claude"].entries())
                for (const [col, m] of makers.entries())
                    tests.marksMatrix.push(avatarProbe.createObject(win.contentItem, { provider: agent, maker: m, x: 20 + col * 38, y: 20 + row * 42, ring: Theme.background, badge: row === 0 ? "unread" : "" })) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.shotWindow("clicks-marks-matrix")
            tests.marksSheet.destroy()
            const wrong = [], offCentre = []
            for (const a of tests.marksMatrix) {
                const first = a.maker !== "" && !(a.provider === "claude" && a.maker === "anthropic")
                // (Its own parts, shown or not.)
                const part = n => a.children.find(c => c.objectName === n)
                const logo = part("sessionMaker"), mark = part("sessionProvider"), tag = part("sessionAgentTag")
                const ok = first ? logo.visible && tag.visible && !mark.visible && logo.url !== "" && logo.children[0].status === Image.Ready
                                 : mark.visible && !logo.visible && !tag.visible
                if (!ok) wrong.push(a.provider + "×" + (a.maker || "none"))
                // Each mark centred on whole pixels: the same gap each side.
                const inner = tag.children[0], small = inner.children[0]
                for (const [m, box] of [[logo, a], [mark, a], [small, inner]])
                    if (m.x * 2 + m.width !== box.width || m.y * 2 + m.height !== box.height || m.x !== Math.round(m.x) || m.y !== Math.round(m.y))
                        offCentre.push(a.provider + "×" + (a.maker || "none") + " " + m.width + " in " + box.width + " at " + m.x + "," + m.y)
                a.destroy()
            }
            tests.check(offCentre.length === 0, "every mark sits centred on whole pixels, the agent's in its badge too (" + offCentre.slice(0, 3) + ")")
            tests.check(tests.marksMatrix.length === 22 && wrong.length === 0, "every agent × maker: the maker's logo loaded, the agent as a tag; Claude on Claude or no model: the agent's mark (wrong: " + wrong + ")")
            tests.marksMatrix = []
            // A session's model → its maker: by the model's name, else its
            // provider's logo, else none.
            const cases = { "openai-codex/gpt-6": "openai", "opencode-go/glm-5.3": "zai-coding-plan", "opencode-go/claude-haiku-5-5": "anthropic",
                            "opencode-go/hy4": "opencode", "zai-coding-plan/glm-5.3": "zai-coding-plan", "openrouter/moonshotai/kimi-k3": "kimi-for-coding",
                            "llama.cpp/qwen4-coder": "llama.cpp", "my-server/my-model": "", "": "" }
            const off = Object.keys(cases).filter(k => Workspace.modelMaker(k) !== cases[k]).map(k => k + "→" + Workspace.modelMaker(k))
            tests.check(off.length === 0, "each model's maker, from its name or its provider (" + off + ")")
            tests.summary(); Qt.quit() } }
    }
    property var marksMatrix: []
    property var marksSheet: null
    Component {
        id: avatarProbe
        AgentAvatar { width: 26; height: 26 }
    }
    Component {
        id: matrixSheet
        Rectangle { color: Theme.background }
    }

    // Claude's plan limits (Stage 4), as Claude Code reports them (its
    // /usage; the stand-in claude here): the sidebar meter and Settings →
    // Usage show them; when Claude Code can't say, the last numbers stay.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksclaudeusage"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.refreshQuota() } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const d = JSON.parse(Workspace.quotaJson), a = d.accounts[0]
            tests.check(d.accounts.length === 1 && a.provider === "Claude" && a.plan === "Max" && a.account === "me@example.com", "Claude's plan and account (" + JSON.stringify(a || d) + ")")
            tests.check(a && a.windows.map(w => w.label).join(",") === "Session (5 h),Weekly,Weekly · Opus" && a.windows[1].usedFraction === 0.81, "its windows")
            tests.check(tests.findName("planMeter") !== null && tests.findName("planMeterClaude") !== null, "the sidebar meter shows, with Claude's mark")
            settingsDialog.openPage("usage") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("Weekly · Opus", d) !== null && tests.findText("Max", d) !== null, "Settings → Usage lists them")
            tests.shotWindow("clicks-claude-usage")
            Workspace.refreshQuota() } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const d = JSON.parse(Workspace.quotaJson)
            tests.check(d.error.indexOf("rate limited") >= 0 && d.accounts.length === 1 && d.accounts[0].windows.length === 3, "Claude Code can't say: the last numbers stay (" + d.error + ")")
            tests.check(!TestInput.readFile(win.selftestDir + "/claude-config/.credentials.json"), "harlo never needed Claude's sign-in file")
            settingsDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // Subagents of Claude sessions (Stage 4), against the stand-in:
    // "delegate" starts one; the dock lists it by its description, its
    // transcript opens, a reopened session lists it from disk.
    function subagentsOf() { try { return JSON.parse(chat.session.subagentsJson || "[]") } catch (e) { return [] } }
    property string delegateKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdelegate"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.delegateKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.subagentDock.expanded = true; tests.sendByKeys("please delegate this") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const a = tests.subagentsOf()
            tests.check(a.length === 1 && a[0].name === "Look around" && a[0].agent === "Explore" && a[0].status === "completed", "Claude: the subagent is listed, done (" + JSON.stringify(a) + ")")
            tests.check(a.length === 1 && String(a[0].sessionFile).indexOf("/subagents/agent-") > 0 && tests.chatText().indexOf("The subagent found notes.md.") >= 0, "with its transcript, and the reply")
            tests.check(Workspace.meterLast === "Claude", "the meter follows Claude's limits now (" + Workspace.meterLast + ")")
            tests.check(tests.findText("Look around", chat.subagentDock) !== null, "the dock names it by its description")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            tests.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "View: its own conversation (" + t.slice(0, 120).replace(/\n/g, " | ") + ")")
            tests.check(tests.findName("steerField") === null && tests.findText("Stop helper") === null, "no Steer or Cancel for Claude's")
            tests.shotWindow("clicks-delegate-claude")
            chat.subagentDialog.close()
            Workspace.closeSession(tests.delegateKey) } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("/claude-config/projects/") >= 0) saved = Workspace.keyAt(i)
            if (saved !== "") Workspace.open(saved) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const a = tests.subagentsOf()
            tests.check(chat.session.harness === "claude" && a.length === 1 && a[0].name === "Look around" && a[0].status === "completed" && !!a[0].sessionFile, "reopened: listed from disk (" + JSON.stringify(a) + ")")
            tests.summary(); Qt.quit() } }
    }

    // MCP servers for Claude and Harlo (Stage 4): a new server is for the
    // installed agents; it lands in each one's own dialect (Claude's global
    // ones through `claude mcp`, the stand-in here); Harlo's switch works,
    // Claude has none; removing reaches both; a project gets .mcp.json and
    // .harlo/mcp.json.
    function readJson(path) { try { return JSON.parse(TestInput.readFile(path) || "{}") } catch (e) { return {} } }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksmcpagents"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("mcpAdd", settingsDialog.contentItem), "Add MCP server") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, c = tests.findName("mcpForClaude", d), h = tests.findName("mcpForHarlo", d)
            tests.check(c !== null && c.checked && h !== null && h.checked, "a new server is for Claude and Harlo (the installed agents that are on)")
            tests.shotWindow("clicks-mcp-agents")
            tests.clickItem(tests.findName("mcpBack", d), "back")
            const err = Mcp.save("global", "", JSON.stringify({ name: "files", enabled: true, transport: "stdio", command: ["npx", "-y", "files-mcp"], env: [["TOKEN", "t"]], url: "", headers: [], cwd: "", timeoutSecs: 20 }), JSON.stringify(["claude", "harlo"]))
            tests.check(err === "", "saved (" + err + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const claude = tests.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            const hs = tests.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            tests.check(JSON.stringify(claude.files) === JSON.stringify({ type: "stdio", command: "npx", args: ["-y", "files-mcp"], env: { TOKEN: "t" } }), "Claude's dialect, through claude mcp (" + JSON.stringify(claude.files) + ")")
            tests.check(JSON.stringify(hs.files) === JSON.stringify({ command: "npx", args: ["-y", "files-mcp"], env: { TOKEN: "t" }, timeout: 20, enabled: true }), "Harlo's dialect (" + JSON.stringify(hs.files) + ")")
            const l = tests.mcpList("global")
            tests.check(l.length === 1 && l[0].harnesses.join(",") === "claude,harlo" && !l[0].differs, "listed once, for both")
            tests.check(Mcp.setEnabled("global", "files", false) === "", "switched off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const hs = tests.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            const claude = tests.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            tests.check(hs.files.enabled === false && !!claude.files && tests.mcpList("global")[0].differs, "Harlo's is off; Claude's stays (no switch)")
            tests.check(Mcp.remove("global", "files") === "", "removed") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const hs = tests.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            const claude = tests.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            tests.check(!hs.files && !claude.files && tests.mcpList("global").length === 0, "gone from both")
            const project = win.argValue("--cwd=")
            tests.check(Mcp.save(project, "", JSON.stringify({ name: "web", enabled: true, transport: "http", command: [], env: [], url: "https://example.com/mcp", headers: [["Authorization", "Bearer x"]], cwd: "" }), JSON.stringify(["claude", "harlo"])) === "", "a project server") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const project = win.argValue("--cwd=")
            const claude = tests.readJson(project + "/.mcp.json").mcpServers || {}, hs = tests.readJson(project + "/.harlo/mcp.json").mcpServers || {}
            tests.check(claude.web && claude.web.type === "http" && claude.web.headers.Authorization === "Bearer x" && hs.web && hs.web.url === "https://example.com/mcp", "in the project's .mcp.json and .harlo/mcp.json")
            settingsDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // Edit & resend for Claude (Stage 4): hovering a prompt
    // offers it; it forks the session before that prompt (the original
    // stays as it was) and puts the prompt back in the composer; the next
    // message goes on in the fork.
    property string forkFile: ""
    function hoverEdit(text) {
        const bubble = tests.findText(text, chat.transcript)
        if (!bubble) return false
        const at = bubble.mapToItem(null, 4, 4)
        TestInput.moveTo(at.x, at.y)
        return true
    }
    SequentialAnimation {
        id: effortCheck
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("after the effort change") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("effort low") >= 0 && t.indexOf("second, edited") >= 0 && t.indexOf("second message") < 0, "a restart (effort) resumes the fork (" + t.slice(-90) + ")")
            tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: forkSteps
        property string harness: "claude"
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksfork"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, forkSteps.harness) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: tests.sendByKeys("first message") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("second message") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.forkFile = Workspace.sessionFile(Workspace.activeKey)
            tests.check(tests.hoverEdit("second message"), forkSteps.harness + ": the prompt is there") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Edit & resend", chat.transcript) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const now = Workspace.sessionFile(Workspace.activeKey)
            tests.check(chat.composerItem.text === "second message", "the prompt is back in the composer (" + chat.composerItem.text + ")")
            tests.check(tests.chatText().indexOf("first message") >= 0 && tests.chatText().indexOf("second message") < 0, "the transcript ends before it")
            tests.check(now !== "" && now !== tests.forkFile, "in a new session (" + now.split("/").pop() + ")")
            chat.composerItem.text = ""
            tests.sendByKeys("second, edited") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("first message") >= 0 && t.indexOf("second, edited") >= 0 && t.indexOf("second message") < 0 && chat.session.status === "idle", "and goes on there")
            tests.shot("clicks-fork-" + forkSteps.harness)
            if (forkSteps.harness === "claude") {
                tests.check(TestInput.readFile(tests.forkFile).indexOf("second message") >= 0, "the original is as it was")
                // Effort restarts Claude (resuming the fork).
                chat.session.setThinkingLevel("low")
                forkSteps.harness = "claude-effort"
                forkSteps.stop(); effortCheck.start()
            } else {
                tests.summary(); Qt.quit()
            } } }
    }

    // Claude on a remote host (Stage 4), over the test's SSH server with the
    // stand-in claude: a session in a remote project runs there, its
    // transcript is the host's (ssh://…), a name is kept there, and the host's
    // Claude sessions are listed, reopened with their history and resumed.
    property string remoteClaudeKey: ""
    property string instanceKey: ""
    property string remoteWorktree: ""
    SequentialAnimation {
        id: remoteClaudeSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksremoteclaude"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            tests.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        // (The first connection is slow when the machine is busy.)
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            tests.check(JSON.parse(Remotes.hostsJson())[0].state === "connected", "connected")
            settingsDialog.openPage("remote") } }
        PauseAnimation { duration: 600 }
        // Settings → Other machines: the host's agents.
        ScriptAction { script: {
            const d = settingsDialog.contentItem, h = JSON.parse(Remotes.hostsJson())[0]
            tests.check(tests.findName("hostAgent-box-claude", d) !== null && tests.findText("Claude 2.1.296", d) !== null, "the host's agents are listed, with versions")
            tests.check(tests.findName("hostSignIn-box-claude", d) !== null && h.claudeAuth === "no", "Claude not signed in there: Sign in")
            tests.check(tests.findName("hostInstall-box-claude", d) === null && tests.findName("hostRecheck-box", d) !== null, "and Check again")
            tests.shotWindow("clicks-remote-agents")
            settingsDialog.close()
            Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = "ssh://box" + win.argValue("--cwd="); sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findText("New Claude session") !== null && tests.findText("New Harlo session") !== null, "a remote project offers Claude (found there) and Harlo (harlo's own)")
            sidebar.projectMenu.close()
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            tests.remoteClaudeKey = Workspace.newSession(p.id, "claude") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.sendByKeys("hello there") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const file = Workspace.sessionFile(Workspace.activeKey)
            tests.shotWindow("clicks-remote-claude-chat")
            tests.check(chat.session.harness === "claude" && tests.chatText().indexOf("Hello from fake claude") >= 0, "Claude answers over SSH (" + tests.chatText().slice(-80) + ")")
            tests.check(file.indexOf("ssh://box" + win.selftestDir + "/claude-config/projects/") === 0, "its transcript is the host's (" + file + ")")
            // Orchestration there: the bridge on the host, harlo's socket
            // forwarded to it; the helper starts on the host too.
            tests.sendByKeys("please orchestrate a helper") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const t = tests.chatText(), kids = JSON.parse(Workspace.orchestratedJson(tests.remoteClaudeKey))
            tests.check(/Started a helper with harlo \([1-9][0-9]* tools\): live:/.test(t), "on the host, the agent finds harlo's tools (" + t.slice(-110) + ")")
            tests.check(kids.length === 1 && Workspace.instanceOf(kids[0].id) === "box", "and its helper runs on box (" + JSON.stringify(kids) + ")")
            tests.check(t.indexOf("The Claude session") >= 0 && t.indexOf("harlo · ") >= 0, "whose reply comes back to it")
            tests.check(Workspace.sessionFile(tests.remoteClaudeKey) !== "", "(still the host's session)")
            // Code lookup on: harlo's tools offer it, answered from box's code.
            Workspace.setSetting("lookup_" + JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, "1")
            tests.sendByKeys("lookup note") } }
        ScriptAction { script: tests.waitUntil(remoteClaudeSteps, () => tests.chatText().indexOf("Lookup tools:") >= 0, 60000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(/Lookup tools: \S*lookup; retry: fn retry  src\/jobs\.rs:1-3/.test(t), "Code lookup on: Claude on box gets lookup, its card from box's code (" + t.slice(t.indexOf("Lookup tools:"), t.indexOf("Lookup tools:") + 120) + ")")
            Workspace.renameSession(tests.remoteClaudeKey, "Remote chat") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const file = Workspace.sessionFile(Workspace.activeKey).replace("ssh://box", "")
            tests.check(TestInput.readFile(file).indexOf('"customTitle":"Remote chat"') >= 0, "the name is written there")
            tests.sendByKeys("please delegate this") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const a = tests.subagentsOf()
            tests.check(a.length === 1 && a[0].status === "completed" && String(a[0].sessionFile).indexOf("ssh://box") === 0, "a subagent there, its transcript the host's (" + (a[0] ? a[0].sessionFile : "none") + ")")
            Workspace.closeSession(tests.remoteClaudeKey) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("ssh://box" + win.selftestDir + "/claude-config/projects/") === 0) saved = Workspace.keyAt(i)
            tests.check(saved !== "", "the host's Claude session is in the sidebar (" + saved + ")")
            if (saved === "") { tests.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(chat.session.harness === "claude" && tests.chatText().indexOf("hello there") >= 0 && chat.session.title === "Remote chat", "reopened: its history and name (" + chat.session.title + ")")
            const a = tests.subagentsOf()
            tests.check(a.length === 1 && a[0].name === "Look around" && a[0].status === "completed" && String(a[0].sessionFile).indexOf("ssh://box") === 0, "and its subagent, from the host (" + JSON.stringify(a.map(x => [x.name, x.status])) + ")")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            tests.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "whose own conversation opens from there")
            chat.subagentDialog.close()
            if (chat.session.harness !== "claude") { tests.summary(); Qt.quit() }
            else tests.sendByKeys("one more") } }
        PauseAnimation { duration: 4000 }
        // An edit there: numbered from the host's file (read over SSH).
        ScriptAction { script: tests.sendByKeys("show the tools please") }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const edit = tests.findAllNames("previewCode", chat.transcript).find(v => v.signs)
            const en = edit ? edit.lines.map(l => l.n) : []
            tests.check(edit && JSON.stringify(en) === JSON.stringify(["3", "", "", "4", "5", "6"]) && edit.lines[0].p.indexOf("ssh://box") === 0,
                      "an edit on the host, numbered from its file there (" + JSON.stringify(en) + ")") } }
        ScriptAction { script: {
            tests.check(tests.chatText().split("Hello from fake claude").length > 2 && chat.session.status === "idle", "and it carries on there (resumed)")
            tests.shotWindow("clicks-remote-claude")
            // A worktree on the host: the dialog lists its branches (read in
            // the background), the project's setup runs there.
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            Workspace.setProjectSetup(p.id, "echo ok > setup-ran")
            sidebar.worktreeDialogs.openNew(p.id, p.name, "claude") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const box = sidebar.worktreeDialogs.baseBox
            tests.check(box.count >= 2 && box.currentText === "main", "New worktree lists the host's branches, current first (" + box.count + " " + box.currentText + ")")
            sidebar.worktreeDialogs.newDialog.close()
            Workspace.newWorktreeSession(JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, "feature-x", "main", "claude") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const dir = win.selftestDir + "/remote-data/harlo/worktrees/"
            tests.remoteClaudeKey = Workspace.activeKey
            tests.check(String(chat.session.cwd).indexOf("ssh://box" + dir) === 0 && String(chat.session.cwd).endsWith("/feature-x"), "a worktree session runs in the host's worktree (" + chat.session.cwd + ")")
            const wt = String(chat.session.cwd).replace("ssh://box", "")
            tests.check(TestInput.readFile(wt + "/README.md") === "hello\n", "the worktree is checked out there")
            tests.check(TestInput.readFile(wt + "/setup-ran") === "ok\n", "and the project's setup ran in it")
            tests.sendByKeys("hello from the worktree") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Hello from fake claude") >= 0, "Claude answers in it")
            tests.remoteWorktree = String(chat.session.cwd).replace("ssh://box", "")
            Workspace.removeWorktree(tests.remoteClaudeKey, true, true) } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            tests.check(TestInput.readFile(tests.remoteWorktree + "/README.md") === "", "removing it removes it there")
            // Settings on box: its agents, and its own MCP files.
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, row = tests.findName("hostProvider-claude", d)
            tests.check(Workspace.instance === "box" && row !== null && row.detail.indexOf("Version 2.1.296") === 0 && row.detail.indexOf("not signed in") > 0, "Providers shows box's agents (" + (row ? row.detail : "none") + ")")
            tests.check(tests.findName("provider-claude", d) === null, "not this computer's list")
            tests.shotWindow("clicks-remote-providers")
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const err = Mcp.save("host:box", "", JSON.stringify({ name: "boxtools", enabled: true, transport: "stdio", command: ["box-mcp"], url: "", headers: [], env: [], cwd: "", timeoutSecs: null, exposure: "", description: "" }), JSON.stringify(["harlo", "claude"]))
            const home = win.selftestDir + "/claude-config"
            const hs = JSON.parse(TestInput.readFile(home + "/.config/harlo-agent/mcp.json") || "{}").mcpServers || {}
            const claude = JSON.parse(TestInput.readFile(home + "/.claude.json") || "{}").mcpServers || {}
            tests.check(err === "" && hs.boxtools && hs.boxtools.command === "box-mcp", "MCP on box writes box's own Harlo config (" + err + ")")
            tests.check(claude.boxtools !== undefined, "and box's Claude (claude mcp there)")
            const l = JSON.parse(Mcp.listJson("host:box"))
            tests.check(l.servers.some(x => x.name === "boxtools" && x.harnesses.indexOf("claude") >= 0 && x.harnesses.indexOf("harlo") >= 0), "which the page lists for box")
            tests.shotWindow("clicks-remote-mcp")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The host's Claude is older than the newest: offered, and
            // updated there.
            const m = tests.findName("updateMessage-box/claude")
            tests.check(m !== null && m.text === "Version 2.1.300 is available on box (you have 2.1.296).", "the host's agents are checked for updates (" + (m ? m.text : Updates.entriesJson) + ")")
            tests.clickItem(tests.findName("updateGo-box/claude"), "Update on box") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            const m = tests.findName("updateMessage-box/claude")
            tests.check(m !== null && m.text === "Claude on box is now 2.1.300. New sessions use it.", "and updated there (" + (m ? m.text : "none") + ")")
            tests.summary(); Qt.quit() } }
    }


    // Orchestration: an agent drives harlo through its MCP bridge (the
    // stand-in Claude as the client): it starts a helper session, which
    // shows in its dock; the helper's reply comes back to it; View opens the
    // helper as a tab.
    property string orchestrator: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksorchestrate"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.subagentDock.expanded = true; tests.sendByKeys("please orchestrate a helper") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(/Started a helper with harlo \([1-9][0-9]* tools\): live:/.test(t), "the agent finds harlo's tools and starts a session (" + t.slice(-90) + ")")
            const kids = JSON.parse(Workspace.orchestratedJson(tests.orchestrator))
            tests.check(kids.length === 1 && kids[0].name === "Helper" && kids[0].agent === "claude", "it's the agent's subagent (" + JSON.stringify(kids) + ")")
            tests.check(tests.findText("Helper", chat.subagentDock) !== null, "and shows in its dock") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = tests.chatText(), kids = JSON.parse(Workspace.orchestratedJson(tests.orchestrator))
            tests.check(kids.length === 1 && kids[0].status === "completed", "the helper finishes (" + (kids[0] ? kids[0].status : "") + ")")
            tests.check(t.indexOf("The Claude session") >= 0 && t.indexOf("harlo · ") >= 0 && t.indexOf("Thanks; harlo says") >= 0 && t.indexOf("Hello from fake claude") >= 0, "and its reply comes back to the agent (" + t.slice(-120) + ")")
            const who = tests.findName("subagentWho", chat)
            tests.check(who !== null && who.text.indexOf("Claude Code") === 0 && who.text.indexOf("claude-") < 0, "the helper is named as elsewhere, not by its ids (" + (who ? who.text : "none") + ")")
            const st = tests.findName("subagentStatus", chat)
            tests.check(st === null || !st.visible, "finished needs no word: its faint dot says it")
            tests.shotWindow("clicks-orchestrate")
            // A finished helper's View shows on its row's hover.
            const row = tests.findName("subagentWho", chat.subagentDock)
            tests.check(row !== null && tests.findName("subagentView", chat.subagentDock) === null, "a finished helper's View waits for a hover")
            if (row) { const p = row.mapToItem(null, row.width / 2, row.height / 2); TestInput.moveTo(p.x, p.y) } } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("subagentView", chat.subagentDock), "View") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(tests.orchestrator))
            tests.check(Workspace.activeKey === kids[0].id && tests.chatText().indexOf("Say hello as the helper") >= 0, "View opens the helper's session (" + Workspace.activeKey + ")")
            // Turned off: a running agent's bridge offers nothing and harlo
            // refuses; new sessions get no bridge, and there's no Delegate.
            Workspace.setOrchestrationEnabled(false)
            Workspace.open(tests.orchestrator) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(tests.findName("delegateButton") === null, "off: no Delegate button")
            tests.sendByKeys("please orchestrate again") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("(0 tools): Teamwork is turned off in harlo") >= 0, "off: a running agent gets no tools, and calls are refused (" + t.slice(-110) + ")")
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("please orchestrate") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("No harlo tools here.") >= 0, "off: a new session has no harlo tools (" + tests.chatText().slice(-60) + ")")
            tests.summary(); Qt.quit() } }
    }

    // The real Claude Code (not in the click suite; spends a little):
    // one tiny turn on Haiku in --cwd.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "claudereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { chat.session.setModel("anthropic", "claude-haiku-4-5"); tests.sendByKeys("Reply with exactly: OK") } }
        PauseAnimation { duration: 20000 }
        ScriptAction { script: {
            tests.log("status:", chat.session.status, "model:", chat.session.modelId, "context:", chat.session.contextText)
            tests.log("transcript:", tests.chatText())
            Qt.quit() } }
    }


    // A real subagent (not in the click suite): --harness= claude|harlo,
    // --model=provider/id; one tiny delegated task.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "delegatereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.delegateKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            tests.sendByKeys("Start one subagent (your subagent/Agent tool, the general agent) with the description 'Say banana' and the task 'Reply with the single word banana.' Then tell me what it said, in one word.") } }
        PauseAnimation { duration: 75000 }
        ScriptAction { script: {
            tests.log("status:", chat.session.status, "model:", chat.session.modelId)
            tests.log("subagents:", chat.session.subagentsJson)
            tests.log("transcript:", tests.chatText().replace(/\n/g, " | ").slice(-700))
            Qt.quit() } }
    }

    // Edit & resend against a real agent (not in the click suite):
    // --harness=, --model=provider/id; three one-word turns.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "forkreal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            tests.sendByKeys("Reply with exactly: apple") } }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: tests.sendByKeys("Reply with exactly: banana") }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: { tests.forkFile = Workspace.sessionFile(Workspace.activeKey); tests.hoverEdit("Reply with exactly: banana") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Edit & resend", chat.transcript) }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            tests.log("composer:", chat.composerItem.text, "| file:", tests.forkFile.split("/").pop(), "->", Workspace.sessionFile(Workspace.activeKey).split("/").pop())
            tests.log("after fork:", tests.chatText().replace(/\n/g, " | "))
            chat.composerItem.text = ""
            tests.sendByKeys("Reply with exactly: cherry") } }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: {
            tests.log("status:", chat.session.status, "| transcript:", tests.chatText().replace(/\n/g, " | ").slice(-500))
            Qt.quit() } }
    }

    // A real agent on a remote host (not in the click suite): --ssh= the
    // host, --harness=, --model=provider/id; one tiny turn there.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "remotereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")); Remotes.connectHost("box") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { tests.log("host:", JSON.stringify(JSON.parse(Remotes.hostsJson())[0].versions)); Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.newSession(JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 8000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            tests.sendByKeys("Reply with exactly: OK") } }
        PauseAnimation { duration: 60000 }
        ScriptAction { script: {
            tests.log("status:", chat.session.status, "| file:", Workspace.sessionFile(Workspace.activeKey), "| transcript:", tests.chatText().replace(/\n/g, " | ").slice(-400))
            Qt.quit() } }
    }

    // Hover tips: the shared ToolTip, dressed like the pills.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickstooltip"
        PauseAnimation { duration: 900 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const b = tests.findName("modeButton", chat.composerItem), p = b.mapToItem(null, b.width / 2, b.height / 2)
            // (Offscreen, controls don't follow the mouse unless told to.)
            b.hoverEnabled = true
            TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tipStyle.tip
            tests.check(t.visible && t.text.indexOf("Build: the agent makes changes") === 0, "hovering shows the tip (" + t.text + ")")
            tests.check(t.leftPadding === 10 && t.contentItem && t.contentItem.font.pixelSize === Theme.px(12) && t.background && t.background.variant === "outline", "dressed like the pills")
            tests.check(t.contentItem.parent !== tipStyle && t.contentItem.text === t.text, "its text is in it")
            tests.shotWindow("clicks-tooltip")
            // A long one wraps instead of running across the window.
            tests.findName("modeButton", chat.composerItem).ToolTip.text = "Use the task tool to run two subagents in parallel: one lists every file in this repo with its size, the other counts the lines of Rust in each crate and reports the totals." } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = tipStyle.tip
            tests.check(t.visible && t.width <= Theme.px(420) && t.contentItem.lineCount >= 3 && t.height >= t.contentItem.lineCount * Theme.px(12),
                      "a long tip wraps (" + t.width + "x" + t.height + ", " + t.contentItem.lineCount + " lines)")
            const r = t.contentItem.mapToItem(null, 0, 0)
            tests.check(r.x >= 0 && r.x + t.contentItem.width <= win.width, "inside the window")
            tests.shotWindow("clicks-tooltip-long")
            tests.summary(); Qt.quit() } }
    }

    // Agent updates: a newer Claude (as the registry says) is offered in
    // the corner; Dismiss holds for that version; Update runs it in the
    // background and says what it is now; View output shows the log.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksupdates"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            updateToasts.doneTimeout = 1500
            Updates.checkLocal() } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const m = tests.findName("updateMessage-local/claude")
            tests.check(m !== null && m.text === "Version 2.1.300 is available (you have 2.1.296).", "a newer Claude is offered (" + (m ? m.text : Updates.entriesJson) + ")")
            const toast = tests.findName("update-local/claude"), at = toast ? toast.mapToItem(null, 0, 0).x : -1
            tests.check(toast && at >= 0 && at + toast.width <= win.width, "the notice is inside the window (" + Math.round(at) + "+" + (toast ? Math.round(toast.width) : 0) + " of " + win.width + ")")
            tests.shotWindow("clicks-update-offer")
            tests.clickItem(tests.findName("updateDismiss-local/claude"), "Dismiss") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(tests.findName("update-local/claude") === null, "Dismiss puts it away"); Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findName("update-local/claude") === null, "and that version isn't offered again")
            Workspace.setSetting("update_dismissed", "{}")
            Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.clickItem(tests.findName("updateGo-local/claude"), "Update") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const m = tests.findName("updateMessage-local/claude")
            tests.check(m !== null && m.text === "Claude is now 2.1.300. New sessions use it.", "Update runs, then says what it is now (" + (m ? m.text : "none") + ")")
            tests.shotWindow("clicks-update-done")
            tests.clickItem(tests.findName("updateOutput-local/claude"), "View output") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(windowSettings.terminalOpen, "View output opens the terminal")
            tests.check(tests.findName("update-local/claude") !== null, "done: it stays while the pointer is on it")
            TestInput.moveTo(20, 20) } }
        PauseAnimation { duration: 2200 }
        ScriptAction { script: {
            tests.check(tests.findName("update-local/claude") === null, "then goes by itself")
            Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findName("update-local/claude") === null, "up to date now: nothing offered")
            tests.summary(); Qt.quit() } }
    }

    // The website's screenshots (not a test; see click-tests.sh's siteshots):
    // the main window with a session and its changes, the diff tab, the
    // Midnight theme with the Activity panel, the new-session page, the Wiki
    // panel beside a lookup in it, and a plan under way with a diagram and
    // options, and last two agents at work at once (site-hero). Each dark,
    // then light (name-light) in the same state, for the site's themes. Run
    // with QT_SCALE_FACTOR=2 for pictures the site can crop and still show
    // sharp.
    property string siteKey: ""
    property real wikiScale: 1
    SequentialAnimation {
        id: siteSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "siteshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            // (Claude Code is the stand-in: never the claude installed here.)
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            // (A narrow window, --winwidth=524, for the phone-sized pictures.)
            win.width = Number(win.argValue("--winwidth=")) || 1440
            win.height = 900
            // (Narrow, for the phone-sized pictures: the diff inline, and the
            // text larger, so it reads at a phone's width.)
            if (win.width < 1000) {
                Workspace.setSetting("diff_layout", "unified")
                Workspace.chooseTextScale(1.25)
            }
            const base = win.argValue("--cwd=").replace(/\/repo$/, "")
            Workspace.addProject(base + "/webapp")
            Workspace.addProject(base + "/api")
            Workspace.addProject(base + "/shipyard")
            Workspace.saveTemplate(JSON.stringify({ name: "Review a pull request", harness: "claude", approval: "always-ask", text: "Review the open pull request: correctness first, then tests." }))
            Workspace.saveTemplate(JSON.stringify({ name: "Fix the failing tests", harness: "harlo", approval: "write", text: "Run the tests and fix what fails." })) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickItem(tests.findText("Fix the login redirect loop", sidebar), "the session") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.siteKey = Workspace.activeKey
            const run = tests.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", chat.transcript)
            if (run) tests.clickItem(run, "the run")
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-main"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-main-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: tests.clickText("guard.ts", changes) }
        PauseAnimation { duration: 800 }
        // (Narrow, without the sidebar: the code's lines fit.)
        ScriptAction { script: { windowSettings.rightMode = ""; if (win.width < 1000) windowSettings.sidebarOpen = false } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.shotWindow("site-diff"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.shotWindow("site-diff-light"); Workspace.chooseThemeMode("dark"); windowSettings.sidebarOpen = true } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            Workspace.chooseTheme("Midnight")
            Workspace.open(tests.siteKey)
            windowSettings.rightMode = "activity" } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-midnight"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-midnight-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.chooseTheme("harlo"); windowSettings.rightMode = ""; win.openDraft(1) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.shotWindow("site-new"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.shotWindow("site-new-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.clickItem(tests.findText("Where is a pallet's weight worked out?", sidebar), "the wiki lookup") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 1500 }
        // (The page Claude Code read, at its section on weighing.)
        ScriptAction { script: wikiPanel.openWiki((wikiPanel.pages.find(p => /manifest/.test(p.path)) || wikiPanel.pages[0]).path, "", false) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const b = ((wikiPanel.pageView || {}).blocks || []).find(x => /weigh/i.test(x.anchor))
            if (b) wikiPanel.openWiki(wikiPanel.openPage, b.anchor, false) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-wiki"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-wiki-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // (Narrow, from the question: the phone crop shows what was asked,
        // the wiki lookup and the page.)
        ScriptAction { script: {
            if (win.width >= 1000) return
            const v = chat.transcript.listView
            v.follow = false
            v.positionViewAtBeginning() } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { if (win.width < 1000) { tests.shotWindow("site-wiki-top"); Workspace.chooseThemeMode("light") } } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.width < 1000) { tests.shotWindow("site-wiki-top-light"); Workspace.chooseThemeMode("dark") } } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            windowSettings.rightMode = ""
            tests.clickItem(tests.findText("Carriers want manifests as CSV", sidebar), "the plan") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-plan"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-plan-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // (Narrow, the option cards stand one above another: the plan folded
        // to its header so both show whole, scrolled to the first, the
        // recommended one, for the plan's phone picture.)
        ScriptAction { script: {
            if (win.width >= 1000) return
            const fold = tests.findName("planToggle", chat)
            if (fold) tests.clickItem(fold, "fold the plan") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            if (win.width >= 1000) return
            const v = chat.transcript.listView
            const card = tests.findName("optionCard", chat.transcript)
            v.follow = false
            if (card) v.contentY = Math.max(v.originY, card.mapToItem(v.contentItem, 0, 0).y - 50)
            // (The round Latest arrow would sit on the second card.)
            const latest = tests.findName("latestButton", chat)
            if (latest) latest.opacity = 0 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { if (win.width < 1000) { tests.shotWindow("site-plan-cards"); Workspace.chooseThemeMode("light") } } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.width >= 1000) return
            tests.shotWindow("site-plan-cards-light"); Workspace.chooseThemeMode("dark")
            const latest = tests.findName("latestButton", chat)
            if (latest) latest.opacity = 1 } }
        PauseAnimation { duration: 800 }
        // Both agents at work (site-hero): the Harlo agent caching lookups
        // in the api project while Claude Code reviews its login fix in
        // webapp, whose changes wait in the Changes panel. (The stand-ins'
        // "cache the user lookups" and "review the login fix".) On a wide run
        // the window is the page column's size, 1102×720, so the site shows
        // it at its own size, not shrunk.
        ScriptAction { script: { if (win.width >= 1000) { win.width = 1102; win.height = 720; windowSettings.sidebarWidth = 250; windowSettings.changesWidth = 290 } } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            windowSettings.rightMode = ""
            const api = JSON.parse(Workspace.projectsJson()).find(p => p.path.endsWith("/api"))
            tests.siteKey = Workspace.newSession(api.id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(siteSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("Cache the user lookups") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const web = JSON.parse(Workspace.projectsJson()).find(p => p.path.endsWith("/webapp"))
            tests.siteKey = Workspace.newSession(web.id, "claude") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(siteSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { tests.sendByKeys("Review the login fix in src/auth/guard.ts"); windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        // (Named as you would, so its header doesn't say New session.)
        ScriptAction { script: Workspace.renameSession(tests.siteKey, "Review the login fix") }
        // (The review finished; the Harlo agent still at work.)
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.waitUntil(siteSteps, () => chat.session.status === "idle", 45000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-hero"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.shotWindow("site-hero-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }

    // Screenshots of every Settings page for a design critique (not a test:
    // SCENARIOS=settingsshots KEEP=1 scripts/click-tests.sh 1280 524): each
    // page from its top, then a screen further down while there's more, then
    // a few in the light theme.
    property var setQueue: []
    property bool glassLink: false
    property var setSearch: null
    function findMoreText(root) {
        for (const c of root.children) {
            if (!c.visible) continue
            if (c.plain !== undefined && c.parts !== undefined && c.parts[1] !== "" && shown(c)) return c
            const f = findMoreText(c)
            if (f) return f
        }
        return null
    }
    function setShotPage(id, part) {
        const f = tests.findName("settingsPage", settingsDialog.contentItem)
        tests.shotWindow("set-" + id + (part > 0 ? "-" + (part + 1) : ""))
        if (f && part < 5 && f.contentY + f.height < f.contentHeight - 8) {
            f.contentY = Math.min(f.contentHeight - f.height, f.contentY + f.height - 80)
            tests.setQueue.unshift(() => tests.setShotPage(id, part + 1))
        }
    }
    Timer {
        id: setTimer
        interval: 700
        repeat: true
        onTriggered: {
            const step = tests.setQueue.shift()
            if (!step) { stop(); tests.summary(); Qt.quit(); return }
            step()
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "settingsshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            win.height = 900
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            const base = win.argValue("--cwd=").replace(/\/repo$/, "")
            Workspace.addProject(base + "/webapp")
            Workspace.addProject(base + "/api")
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const q = []
            for (const pg of settingsDialog.pages) {
                q.push(() => settingsDialog.openPage(pg.id))
                q.push(() => {
                    const f = tests.findName("settingsPage", settingsDialog.contentItem)
                    // (Opened after a page shot scrolled down.)
                    tests.check(!f || f.contentY === 0, pg.title + " opens at its top")
                    tests.setShotPage(pg.id, 0)
                })
            }
            // Narrow: the search button opens the page list over the page.
            q.push(() => {
                const b = tests.findName("settingsNavSearch", settingsDialog.contentItem)
                tests.check(settingsDialog.compact === (b !== null), "the nav's search button shows only when narrow")
                if (b) tests.clickItem(b, "search settings")
            })
            q.push(() => {
                if (!settingsDialog.compact) return
                const nav = tests.findName("settingsNav", settingsDialog.contentItem)
                tests.check(settingsDialog.navOpen && nav && nav.width === 236 && tests.findText("Your projects", settingsDialog.contentItem) !== null, "it opens with the pages' names")
                tests.shotWindow("set-nav-open")
                tests.clickItem(tests.findName("settingsNav-git", settingsDialog.contentItem), "Git")
            })
            q.push(() => {
                if (!settingsDialog.compact) return
                tests.check(!settingsDialog.navOpen && settingsDialog.page === "git", "picking a page closes it")
            })
            // States that say why: Harlo isn't signed in here.
            q.push(() => settingsDialog.openPage("providers"))
            q.push(() => {
                const st = tests.findName("providerStatus-harlo", settingsDialog.contentItem)
                tests.check(st && st.text !== "Error" && st.text !== "", "Providers says why Harlo isn't ready (" + (st ? st.text : "") + ")")
                tests.check(tests.findName("rowSignIn-harlo", settingsDialog.contentItem) !== null, "and offers Sign in on its row")
                // More shows the rest of a row's words, Less hides it again.
                const m = tests.findMoreText(settingsDialog.contentItem)
                tests.check(m !== null, "a row has More")
                if (m) {
                    const before = m.plain.length
                    m.linkActivated("more")
                    tests.check(m.open && m.plain.length > before && m.text.indexOf(">Less<") > 0, "More shows the rest")
                    m.linkActivated("more")
                    tests.check(!m.open && m.plain.length === before, "Less hides it")
                }
                settingsDialog.openPage("general")
            })
            q.push(() => tests.check(tests.findName("defaultModelSignIn", settingsDialog.contentItem) !== null, "General's Model row says where to sign in"))
            // Devices with phone access off: said once per section, not per row.
            q.push(() => settingsDialog.openPage("devices"))
            q.push(() => {
                const notes = tests.findAllNames("needsPhone", settingsDialog.contentItem)
                tests.check(Phone.enabled || (notes.length === 2 && tests.findText("Turn on phone access first.", settingsDialog.contentItem) === null), "Devices says once per section that phone access is off (" + notes.length + ")")
                settingsDialog.openPage("voice")
            })
            // A search lands on the row it matched, lit for a moment.
            q.push(() => {
                settingsDialog.openPage("general")
                if (settingsDialog.compact) settingsDialog.navOpen = true
                const f = tests.findName("settingsSearch", settingsDialog.contentItem)
                tests.setSearch = f
                f.text = "sandbox"
                f.accepted()
            })
            q.push(() => {
                const pg = tests.findName("settingsPage", settingsDialog.contentItem)
                const flash = tests.findName("settingsSeekFlash", settingsDialog.contentItem)
                tests.check(settingsDialog.page === "providers" && pg && flash && flash.opacity > 0 && flash.mapToItem(pg, 0, 0).y >= 0 && flash.mapToItem(pg, 0, 0).y < pg.height, "searching sandbox lands on its row on Agents (" + settingsDialog.page + ", " + (pg ? pg.contentY : -1) + ")")
                tests.shotWindow("set-seek")
                tests.setSearch.text = ""
                settingsDialog.navOpen = false
                settingsDialog.openPage("appearance")
            })
            // Glass that can't be drawn: off, with a way to the renderer.
            q.push(() => {
                const b = tests.findName("glassRenderer", settingsDialog.contentItem)
                const sw = tests.findName("glassSwitch", settingsDialog.contentItem)
                tests.check(b === null || (!sw.enabled && !sw.checked), "Glass is off and disabled when it can't be drawn")
                tests.glassLink = b !== null
                if (b) tests.clickItem(b, "Renderer…")
            })
            q.push(() => {
                const pg = tests.findName("settingsPage", settingsDialog.contentItem)
                if (tests.glassLink) tests.check(settingsDialog.page === "general" && pg.contentY > 0, "Renderer… opens General at its renderer row")
                settingsDialog.openPage("voice")
            })
            // Voice: a chosen model that isn't here isn't "in use".
            q.push(() => tests.check(tests.findText("Base model (in use)", settingsDialog.contentItem) === null && tests.findText("Base model", settingsDialog.contentItem) !== null, "Voice doesn't call a missing model in use"))
            for (const id of ["general", "appearance", "providers"]) {
                q.push(() => { Workspace.chooseThemeMode("light"); settingsDialog.openPage(id) })
                q.push(() => { tests.shotWindow("set-" + id + "-light"); Workspace.chooseThemeMode("dark") })
            }
            tests.setQueue = q
            setTimer.start() } }
    }

    // Screenshots for a design critique (not a test: SCENARIOS=critiqueshots
    // KEEP=1 scripts/click-tests.sh 1280 524): a Claude session of prose
    // with headings and several prompts, its prompt list, a dozen helpers,
    // each right-hand panel, the sidebar beside it, and the new-session page.
    function critReplies() { const t = tests.chatText(); return t.split("Hello from fake claude").length + t.split("What is left to do").length - 2 }
    function critTop(item) {
        if (!item) return
        for (let f = item.parent; f; f = f.parent) {
            if (f.contentY === undefined || f.contentItem === undefined || f.contentHeight === undefined || f.height <= 0) continue
            f.contentY = Math.max(f.originY || 0, item.mapToItem(f.contentItem, 0, 0).y - 16)
            return
        }
    }
    // Brings a line inside a text to the top of its flickable (a reply's
    // prose is one text: its later headings are inside it).
    function critTopAt(item, text) {
        if (!item || item.getText === undefined) return
        const at = item.getText(0, item.length).indexOf(text)
        if (at < 0) return
        const r = item.positionToRectangle(at)
        for (let f = item.parent; f; f = f.parent) {
            if (f.contentY === undefined || f.contentItem === undefined || f.contentHeight === undefined || f.height <= 0) continue
            f.contentY = Math.max(f.originY || 0, item.mapToItem(f.contentItem, 0, r.y).y - 16)
            return
        }
    }
    function critShot(name) { tests.shotWindow("crit-" + name) }
    readonly property var critPrompts: [
        "write it up: how the transcript is put together, with headings, lists and a table please",
        "I still don't like this look, it's confusing. any programming language output like this should have line numbers like an editor does",
        "we should have more language support for the syntax highlighting, toml and qml at least",
        "do you see the problem here? there is no separation between the shell command and line numbers, it looks like one block",
        "start many helpers to look around the code",
        "okay good, let's get a critique of all of it",
        "write it up again"]
    property int critSent: 0
    SequentialAnimation {
        id: critSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "critiqueshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            win.height = 900
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            const base = win.argValue("--cwd=").replace(/\/repo$/, "")
            Workspace.addProject(base + "/webapp")
            Workspace.addProject(base + "/api")
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.path === win.argValue("--cwd="))
            Workspace.newSession(p.id, "claude") } }
        PauseAnimation { duration: 1500 }
        SequentialAnimation {
            loops: 7
            ScriptAction { script: { tests.sendByKeys(tests.critPrompts[tests.critSent]); tests.critSent++ } }
            ScriptAction { script: tests.waitUntil(critSteps, () => tests.critReplies() >= tests.critSent && chat.session.status === "idle", 30000) }
            PauseAnimation { duration: 400 }
        }
        // Prose: the first write-up from its title, then its end.
        ScriptAction { script: { chat.transcript.follow = false; tests.critTop(tests.findContaining("The transcript, from events to rows", chat.transcript)) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.critShot("prose-top") }
        ScriptAction { script: { const t = tests.findContaining("The transcript, from events to rows", chat.transcript); tests.critTopAt(t && t.getText ? t : t && t.parent, "Reading it") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.critShot("prose-end"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.critTop(tests.findContaining("The transcript, from events to rows", chat.transcript)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.critShot("prose-top-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // The prompt list (Ctrl+J), and the rail beside the session.
        ScriptAction { script: chat.transcript.rail.openKeyboard() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.critShot("rail-list"); chat.transcript.rail.close() } }
        PauseAnimation { duration: 400 }
        // The helpers, opened.
        ScriptAction { script: { chat.transcript.scrollToEnd(); const d = tests.findName("subagentDock"); if (d) d.expanded = true } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.critShot("helpers")
            // A helper's task that only repeats its name isn't shown again.
            const d = tests.findName("subagentDock")
            tests.check(d !== null && (tests.findText("Explore", d) !== null) === (win.width > 900), "helpers show their kind while their names fit beside it")
            const count = (t, r) => { let n = 0; for (const c of r.children) { if (!c.visible || c.opacity <= 0) continue; if (c.text === t) n++; n += count(t, c) } return n }
            const cut = (r) => { let n = []; for (const c of r.children) { if (!c.visible || c.opacity <= 0) continue; if (c.font && c.font.bold && c.truncated) n.push(c.text); n = n.concat(cut(c)) } return n }
            if (win.width > 900) tests.check(d !== null && cut(d).length === 0, "with room, no helper's name is cut (" + (d ? cut(d).join(", ") : "") + ")")
            tests.check(d !== null && count("Analyze the transcript layout", d) === 1, "a helper's name shows once, not again as its task (" + (d ? count("Analyze the transcript layout", d) : -1) + ")")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.critShot("helpers-light"); Workspace.chooseThemeMode("dark"); const d = tests.findName("subagentDock"); if (d) d.expanded = false } }
        PauseAnimation { duration: 600 }
        // Each right-hand panel.
        ScriptAction { script: windowSettings.rightMode = "context" }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.critShot("right-context"); windowSettings.rightMode = "activity" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.critShot("right-activity")
            // Only filters that narrow the list; one tool throughout needs no name under each row.
            tests.check(tests.findName("activityFilter-all") !== null && tests.findName("activityFilter-command") === null && tests.findName("activityFilter-read") === null && tests.findName("activityFilter-other") === null, "Activity offers no empty filter, nor one equal to all")
            tests.check(tests.findText("Agent", tests.findName("activityList")) === null, "and its rows don't each repeat their tool")
            windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.critShot("right-changes"); windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.critShot("right-files"); windowSettings.rightMode = "notes" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.critShot("right-notes"); windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.critShot("right-wiki"); windowSettings.rightMode = "context"; Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.critShot("right-context-light"); Workspace.chooseThemeMode("dark"); windowSettings.rightMode = "" } }
        PauseAnimation { duration: 800 }
        // The new-session page.
        ScriptAction { script: win.openDraft(JSON.parse(Workspace.projectsJson()).find(p => p.path === win.argValue("--cwd=")).id) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.critShot("new"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.critShot("new-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }

    // harlo's own update (an AppImage here): offered from its release,
    // downloaded, checked against its signed manifest, put in place; then
    // Restart.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksselfupdate"
        PauseAnimation { duration: 900 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); settingsDialog.openPage("general") } }
        PauseAnimation { duration: 600 }
        // Settings → General → Check now.
        ScriptAction { script: tests.clickItem(tests.findName("checkUpdatesNow", settingsDialog.contentItem), "Check now") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(!Updates.checking && Updates.checkStatus === "1 update found: it's offered at the bottom right of the window.", "Check now says what it found (" + Updates.checkStatus + ")")
            // (Scrolled to the row's end, its answer in the picture.)
            let f = tests.findName("checkUpdatesNow", settingsDialog.contentItem)
            while (f && f.contentY === undefined) f = f.parent
            if (f) f.contentY = Math.max(0, f.contentHeight - f.height)
            tests.shotPopup(settingsDialog, "clicks-selfupdate-check")
            settingsDialog.close() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const m = tests.findName("updateMessage-local/harlo")
            tests.check(m !== null && /^Version 9\.9\.9 is available \(you have \d+\.\d+\.\d+\)\.$/.test(m.text), "a newer harlo is offered (" + (m ? m.text : Updates.entriesJson) + ")")
            // What's new since this version: both releases, cut short, the rest a click away.
            const notes = tests.findName("updateNotes-local/harlo")
            tests.check(notes !== null && notes.visible && notes.text.startsWith("harlo 9.9.9\n• The stand-in release: denying a permission ends the run, and archiving an open session hides it.\n")
                      && notes.text.indexOf("harlo 9.9.8") > 0 && notes.text.endsWith("…") && notes.text.indexOf("Long ago") < 0, "it says what's new, in short (" + (notes ? JSON.stringify(notes.text) : "none") + ")")
            const more = tests.findName("updateReadMore-local/harlo")
            tests.check(more !== null && more.visible && JSON.parse(Updates.entriesJson).find(e => e.key === "local/harlo").notesUrl === "https://github.com/ArcticLatent/harlo/blob/v9.9.9/CHANGELOG.md", "Read more opens the full changelog")
            tests.shotWindow("clicks-selfupdate-notes")
            tests.clickItem(tests.findName("updateGo-local/harlo"), "Update") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const m = tests.findName("updateMessage-local/harlo")
            tests.check(m !== null && m.text === "harlo 9.9.9 is installed. Restart harlo to use it.", "downloaded, checked and put in place (" + (m ? m.text : "none") + ")")
            tests.check(TestInput.readFile(win.selftestDir + "/harlo-test.AppImage") === "harlo 9.9.9 new\n", "the AppImage is the new one")
            tests.check(tests.findName("updateRestart-local/harlo") !== null, "with Restart harlo")
            tests.shotWindow("clicks-selfupdate") } }
        PauseAnimation { duration: 11000 }
        ScriptAction { script: {
            tests.check(tests.findName("update-local/harlo") !== null, "and it stays (it waits for the restart)")
            tests.summary(); Qt.quit() } }
    }

    // The diff tab: a changed file opens side by side from the Changes
    // panel; another file replaces it in the same tab; rows pair up, fill
    // and mark the changed part; a hunk stages from between the sides;
    // the view follows the file as it changes.
    SequentialAnimation {
        id: diffSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdiff"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            // (Side by side here: inline has its own steps at the end.)
            Workspace.setSetting("diff_layout", "split")
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView, rows = dv.rows
            tests.check(Workspace.activeKey === "diff:0:" + win.argValue("--cwd=") + "/src/app.rs" && dv.visible, "a changed file opens in the diff tab (" + Workspace.activeKey + ")")
            tests.check(dv.blocks.length === 4 && rows.length === 43, "four changes, the sides level (" + dv.blocks.length + " changes, " + rows.length + " rows)")
            const edited = rows.find(r => r[1] === 3), gone = rows.find(r => r[1] === 10), added = rows.filter(r => r[0] === 2 && r[5] === 1)
            tests.check(edited && edited[0] === 1 && edited[5] === 1 && edited[3] === -1 && edited[8] === 6 && edited[9] === 14, "an edited line pairs up, the added part marked (" + edited + ")")
            tests.check(gone && gone[0] === 1 && gone[5] === 2 && added.length === 3, "a removed line faces filler, added lines too (" + added.length + ")")
            tests.check(dv.leftPane.editor.text.indexOf("line 10") >= 0 && dv.rightPane.editor.text.indexOf("line 10\n") < 0 && dv.rightPane.editor.text.indexOf("new B") >= 0, "old on the left, new on the right")
            tests.check(tests.findName("diffRuler") !== null && tests.findName("hunk-stage-0") !== null, "the change map, and a hunk's buttons")
            tests.check(dv.leftPane.width * 2 + 44 <= dv.width + 1, "the sides fit the view (" + dv.leftPane.width + " of " + dv.width + ")")
            {
                // Both sides on whole pixels (at half a pixel the text blurs).
                const lx = dv.leftPane.editor.mapToItem(null, 0, 0).x, rx = dv.rightPane.editor.mapToItem(null, 0, 0).x
                tests.check(lx === Math.round(lx) && rx === Math.round(rx) && !dv.rightPane.editor.smooth, "both sides on whole pixels, drawn unfiltered (" + lx + ", " + rx + ")")
            }
            const f = dv.rightPane.flick, third = Math.max(0, Math.min(f.contentHeight - f.height, dv.blocks[0].start * dv.lineHeight - f.height / 3))
            tests.check(f.height > 0 && Math.abs(f.contentY - third) < 1, "it opens at the first change, a third of the way down (" + f.contentY + ", not " + third + ")")
            tests.shotWindow("clicks-diff")
            tests.clickItem(tests.findName("diffNext"), "next change") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(chat.diffView.rightPane.flick.contentY > 0 || chat.diffView.rightPane.flick.contentHeight <= chat.diffView.rightPane.flick.height, "next change scrolls (" + chat.diffView.rightPane.flick.contentY + ")")
            const dv = chat.diffView
            tests.check(Math.abs(dv.rightPane.rowsAt() - dv.rightPane.flick.contentY) < 1, "the rows scroll with the text (" + dv.rightPane.rowsAt() + ", " + dv.rightPane.flick.contentY + ")")
            tests.shotWindow("clicks-diff-next")
            tests.diffCollapseClick() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(dv.rows.length === 27 && dv.folds.length === 2 && dv.folds[0].count === 14 && dv.folds[1].count === 4, "collapsed: the changes, 3 lines around each, the rest folded (" + dv.rows.length + " rows, " + dv.folds.map(f => f.count) + ")")
            tests.check(dv.rightPane.editor.text.split("\n").length === 27 && dv.hunks.every(h => h[1] >= 0 && h[1] < 27), "the text and hunk buttons follow")
            tests.check(Workspace.setting("diff_collapse") === "1", "and it's remembered")
            tests.shotWindow("clicks-diff-collapsed")
            tests.clickItem(tests.findName("diffFold-r-13"), "a fold") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(dv.rows.length === 40 && dv.folds.length === 1, "a click opens a fold (" + dv.rows.length + " rows)")
            tests.diffCollapseClick() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(chat.diffView.rows.length === 43 && Workspace.setting("diff_collapse") === "0", "off: every line again")
            tests.clickText("README.md", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const diffTabs = Workspace.tabs.filter(t => t.startsWith("diff:"))
            tests.check(diffTabs.length === 1 && Workspace.activeKey.endsWith("/README.md"), "another file replaces it in the same tab (" + diffTabs.length + " diff tabs)")
            tests.check(chat.diffView.rightPane.editor.text === "hello world" && chat.diffView.leftPane.editor.text === "hello", "and shows that file")
            tests.clickText("todo.txt", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const rows = chat.diffView.rows
            tests.check(rows.length === 2 && rows.every(r => r[0] === 2 && r[5] === 1), "a new file: all added, nothing on the left")
            tests.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickItem(tests.findName("hunk-stage-0"), "stage the first hunk") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.check(git.stagedCount === 1, "the hunk is staged (" + git.stagedCount + " staged)")
            // The Staged list comes first: its app.rs.
            tests.clickItem(tests.topText("app.rs", changes), "the staged app.rs") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const rows = chat.diffView.rows
            tests.check(Workspace.activeKey.startsWith("diff:1:") && rows.some(r => r[5] === 1) && tests.findName("hunk-unstage-0") !== null, "the staged side, with Unstage on its hunk (" + Workspace.activeKey + ")")
            // An agent changes the file: the view follows.
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello again\n")
            tests.clickText("README.md", changes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello once more\n") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            tests.check(chat.diffView.rightPane.editor.text === "hello once more", "it follows the file as it changes (" + chat.diffView.rightPane.editor.text + ")")
            tests.clickItem(tests.topText("app.rs", changes), "the staged app.rs") } }
        PauseAnimation { duration: 1500 }
        // Inline: from the More menu, one column.
        ScriptAction { script: tests.clickItem(tests.findName("diffMore"), "More") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("diffLayoutUnified"), "Always inline") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const dv = chat.diffView, u = dv.uni ? dv.uni.rows : []
            const pairs = dv.rows.filter(r => r[0] === 1 && r[5] === 1).length
            tests.check(dv.unified && !dv.leftPane.visible && u.length === dv.rows.length + pairs && Workspace.setting("diff_layout") === "unified", "inline: one column, a line for each side of a change (" + u.length + " rows)")
            const gone = u.findIndex(r => r[14] === 1), added = u.findIndex(r => r[5] === 1)
            // (The staged side of app.rs: line 3 as it was, then as it is.)
            const lines = dv.rightPane.editor.text.split("\n")
            tests.reviewKey = lines[gone] || ""
            tests.check(gone >= 0 && added === gone + 1 && lines[gone] === "line 3" && lines[added] === "line 3 changed", "the removed line, then the added one, in the one text (" + lines[gone] + " / " + lines[added] + ")")
            tests.check((tests.findName("hunk-stage-0") !== null || tests.findName("hunk-unstage-0") !== null) && dv.blocks.length > 0 && dv.hunks.every(h => h[1] >= 0 && h[1] < u.length), "hunk buttons and the change map follow")
            tests.shotWindow("clicks-diff-inline")
            tests.clickAt(tests.findName("gutter-u-" + gone, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = chat.diffView.commentBox
            tests.check(box.visible && box.side === "old" && box.code === tests.reviewKey && box.line === 3, "a comment on a removed line, inline (" + box.side + " " + box.code + " " + box.line + ")")
            box.close()
            chat.diffView.setLayout("auto")
            Workspace.openDiffTab(win.argValue("--cwd=") + "/gone/deep/old.svg", false) } }
        ScriptAction { script: tests.waitUntil(diffSteps, () => chat.diffView.info !== null && chat.diffView.path.endsWith("old.svg"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // Its folder is gone too: the diff comes from the repository still.
            const dv = chat.diffView
            const old = (dv.unified ? dv.rightPane : dv.leftPane).editor.text
            tests.check(dv.info !== null && dv.info.note === "" && dv.rows.length === 2 && old.indexOf("second") >= 0,
                      "a file deleted with its folders still shows its diff (" + (dv.info ? dv.rows.length + " rows, " + JSON.stringify(old) : "loading") + ")")
            tests.summary(); Qt.quit() } }
    }

    // The composer's Delegate menu: Second opinion and Committee send the
    // agent harlo's written instruction (agents and models picked, the
    // composer's text as the question).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdelegatemenu"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            chat.composerItem.text = "is the cache right?"
            tests.clickItem(tests.findName("delegateButton"), "the Delegate button") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.clickItem(tests.findName("delegateAdvisor"), "Second opinion…") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const agent = tests.findName("delegateAgentAsk"), note = tests.findName("delegateNote")
            tests.check(agent && agent.currentText === "Harlo", "Second opinion: another agent by default (" + (agent ? agent.currentText : "") + ")")
            tests.check(note && note.text === "is the cache right?", "the composer's text is the question")
            tests.shotWindow("clicks-delegate-advisor")
            tests.clickItem(tests.findName("delegateStart"), "Ask") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("Get a second opinion from Harlo (agent \"harlo\", its default model)") >= 0 && t.indexOf("The question: is the cache right?") >= 0, "the agent gets harlo's instruction (" + t.slice(0, 120) + ")")
            tests.check(chat.composerItem.text === "", "and the composer is empty")
            tests.clickItem(tests.findName("delegateButton"), "the Delegate button") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.clickItem(tests.findName("delegateCommittee"), "Committee…") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const a = tests.findName("delegateAgentMember1"), b = tests.findName("delegateAgentMember2"), m = tests.findName("delegateModelMember1")
            tests.check(a && b && a.currentText === "Claude" && b.currentText === "Harlo", "Committee: this agent and another (" + (a ? a.currentText : "") + ", " + (b ? b.currentText : "") + ")")
            tests.check(m && m.count > 2, "with Claude's models to pick from (" + (m ? m.count : 0) + ")")
            m.currentIndex = 1
            m.activated(1)
            tests.findName("delegateNote").text = "flaky tests"
            tests.clickItem(tests.findName("delegateStart"), "Start committee") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(/- Claude \(agent "claude", model "[^"]+"\)/.test(t) && t.indexOf("- Harlo (agent \"harlo\"") >= 0 && t.indexOf("The problem: flaky tests") >= 0, "both members, with the chosen model, go to the agent (" + t.slice(-300) + ")")
            tests.summary(); Qt.quit() } }
    }

    // The Delegate menu with real agents (not in the click suite): Claude
    // (Haiku) asks Harlo (its default model) for a second opinion.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "delegatemenureal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            chat.session.setModel("anthropic", "claude-haiku-4-5")
            chat.composerItem.text = "In Python, is `x += 1` the same as `x = x + 1` for ints? One short sentence."
            chat.composerItem.delegatePopover.show("advisor") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            const p = chat.composerItem.delegatePopover
            const row = tests.findName("delegateAgentAsk").parent
            row.pick("harlo")
            tests.log("sending:", JSON.stringify(row.chosen()))
            p.start() } }
        PauseAnimation { duration: 100000 }
        ScriptAction { script: {
            tests.log("children:", Workspace.orchestratedJson(tests.orchestrator))
            tests.log("asked:", chat.session.hasRequest, "| status:", chat.session.status, "| transcript:", tests.chatText().replace(/\n/g, " | ").slice(-1500))
            Qt.quit() } }
    }

    // Graphics (not in the click suite; started several times by hand):
    // what draws the window, the saved choice, the notice; --choose= saves
    // a choice for the next start.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "graphicsprobe"
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            if (win.argValue("--choose=") !== "") Workspace.setSetting("graphics", win.argValue("--choose="))
            tests.log("graphics: api", graphicsInfo.api, "(OpenGL " + GraphicsInfo.OpenGL + ", Vulkan " + GraphicsInfo.Vulkan + ", Software " + GraphicsInfo.Software + ") drawn", win.drawnOnce,
                    "| saved", Workspace.setting("graphics"), "| notice", Workspace.notice)
            tests.shot("graphics-probe") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            if (win.argValue("--restart=") === "1") Workspace.restartApp()
            Qt.quit() } }
    }

    // Settings → General → Graphics: Automatic at first; another choice
    // is saved and asks for a restart; back to the one it started with, no
    // restart. (Offscreen, Qt draws in software.)
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksgraphics"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            settingsDialog.openPage("general") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("graphics-auto", d) && tests.findName("graphics-auto", d).selected, "Automatic is chosen at first")
            const row = tests.findName("graphicsRow", d)
            tests.check(row && row.detail.endsWith("Now: Software."), "it says what draws the window now (" + (row ? row.detail.slice(-30) : "") + ")")
            tests.check(tests.findName("graphicsRestartRow", d) === null, "no restart asked")
            tests.clickItem(tests.findName("graphics-vulkan", d), "Vulkan") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(Workspace.setting("graphics") === "vulkan" && tests.findName("graphics-vulkan", d).selected, "Vulkan is saved for the next start")
            tests.check(tests.findName("graphicsRestartRow", d) !== null && tests.findText("Restart Harlo", d) !== null, "and a restart is offered")
            tests.shotWindow("clicks-graphics")
            tests.clickItem(tests.findName("graphics-auto", d), "Automatic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.setting("graphics") === "auto" && tests.findName("graphicsRestartRow", settingsDialog.contentItem) === null, "back to the one it started with: no restart")
            tests.summary(); Qt.quit() } }
    }

    // Tool calls (the stand-in Claude): a finished run open in a box of its
    // own height that scrolls (the wheel moves the box, then the session);
    // a write's lines numbered and coloured, more on a click; an edit's
    // lines out and in; a command under what it's for, its last lines; a
    // failed one; a read on one line; no notice for a foreground command.
    SequentialAnimation {
        id: toolCardSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickstoolcards"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("show the tools please") }
        ScriptAction { script: tests.waitUntil(toolCardSteps, () => tests.chatText().indexOf("Limits now come from") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.sendByKeys("show the tools again") }
        ScriptAction { script: tests.waitUntil(toolCardSteps, () => tests.chatText().split("Limits now come from").length === 3 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const t = chat.transcript
            const boxes = tests.findAllNames("runBox", t)
            tests.check(boxes.length >= 1, "a finished run shows open, in its box (" + boxes.length + ")")
            const box = boxes[boxes.length - 1]
            const what = tests.findAllNames("toolWhat", box).map(l => l.plain)
            tests.check(what.indexOf("Running the full test suite") >= 0, "a command shows what it's for (" + what.join(" | ") + ")")
            tests.check(what.indexOf("src/config.rs") >= 0 && what.indexOf("README.md") >= 0, "paths from the session's folder")
            const previews = tests.findAllNames("toolPreview", box)
            tests.check(previews.length === 2, "the write and the edit show their lines (" + previews.length + ")")
            const more = tests.findAllNames("toolMoreLines", box)
            tests.check(more.length === 1 && more[0].text === "↓ 5 more lines", "the write's first 6 of 11 lines, and 5 more (" + more.map(b => b.text) + ")")
            const edit = tests.findAllNames("previewCode", box).find(v => v.signs)
            const en = edit ? edit.lines.map(l => l.n) : []
            tests.check(edit && edit.numbers && JSON.stringify(en) === JSON.stringify(["3", "", "", "4", "5", "6"]) && tests.findAllNames("codeNumber", edit).length === 4,
                      "an edit numbered where its new lines are in the file, its removed lines not (" + JSON.stringify(en) + ")")
            const results = tests.findAllNames("toolResult", box).map(l => l.text)
            tests.check(results.some(r => r.indexOf("11 lines") >= 0) && results.some(r => r.indexOf("+2") >= 0 && r.indexOf("−2") >= 0)
                      && results.some(r => r.indexOf("7 passed") >= 0) && results.some(r => r.indexOf("exit 101") >= 0), "what each came to (" + results.join(" | ") + ")")
            const tails = tests.findAllNames("toolTail", box).map(l => l.text)
            tests.check(tails.some(x => x.indexOf("7 passed") >= 0), "a command's last lines (" + tails.join(" | ") + ")")
            const code = tests.findAllNames("codeView", box).sort((a, b) => a.mapToItem(box, 0, 0).y - b.mapToItem(box, 0, 0).y).reduce((all, v) => all.concat(v.lines), [])
            tests.check(code.length === 7 && code[0].n === "1" && code[0].h === "hello", "a read's line, numbered (" + JSON.stringify(code[0]) + ")")
            tests.check(code[1].n === "3" && code[1].mark === "match" && code[3].mark === "context" && code[3].h.indexOf("}") >= 0,
                      "grep's line numbers as a gutter, its matches tinted among their context (" + JSON.stringify(code.slice(1, 4)) + ")")
            tests.check(code[4].n === "2" && code[6].n === "4" && code[4].h.indexOf("<span") >= 0 && /^    let/.test(code[4].h.replace(/<[^>]+>/g, "")),
                      "a printed file's lines numbered from where it starts, highlighted, indented (" + JSON.stringify(code.slice(4)) + ")")
            const sel = tests.findAllNames("codeText", box)
            tests.check(sel.length >= 2 && sel.every(e => e.selectByMouse && e.readOnly), "a listing's text can be selected")
            tests.check(tests.findAllNames("toolTail", box).every(l => !/^\d+[:-]|^    let/.test(l.text)), "a listing isn't shown again as plain lines")
            tests.check(tests.findAllNames("toolCommandRule", box).length >= 1, "a rule between a command and its listing")
            const cmds = tests.findAllNames("toolCommand", box)
            const sedIn = cmds.some(c => c.text.indexOf("sed -n") >= 0)
            tests.check(win.width < 700 ? cmds.length >= 3 && sedIn : cmds.length === 2 && !sedIn,
                      "the commands under what they're for; a short one on its line, in the well too when its line cuts it short (" + cmds.length + ")")
            const grepCmd = cmds.find(c => c.text.indexOf("grep -n") >= 0)
            tests.check(grepCmd && grepCmd.wrapMode === Text.WrapAtWordBoundaryOrAnywhere, "a command in the well breaks at a space")
            const sedHead = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("sed -n") >= 0)
            const sedWrap = sedHead ? tests.findName("codeWrap", sedHead.parent) : null
            const sedCard = sedHead ? sedHead.parent.parent.parent : null
            const sedCode = sedCard ? tests.findName("codeView", sedCard) : null
            tests.check(sedCode && (sedWrap !== null) === sedCode.cut && (win.width > 700 || sedCode.cut), "a call whose lines are cut offers Wrap on its line, only then (" + (sedWrap ? sedWrap.text : "none") + ")")
            tests.check(tests.chatText().indexOf("Running the full test suite\n") < 0 && tests.findContaining("▸ cargo", t) === null, "no notice for a command in the foreground")
            // The wheel: the box first, then the session past its top.
            const f = tests.findName("runBoxScroll", box)
            tests.toolBoxStart = [f.contentY, parseFloat(t.debugContentY())]
            tests.check(f.contentY > 0 && f.height < f.contentHeight, "taller than its box, it starts at its end (" + f.contentY + " of " + f.contentHeight + ")")
            const rail = t.rail
            tests.check(!rail.visible || rail.mapToItem(t, 0, 0).x >= t.width - t.gutter, "the prompt rail sits in the gutter, never on the text (" + Math.round(rail.mapToItem(t, 0, 0).x) + " vs " + Math.round(t.width - t.gutter) + ")")
            tests.shotWindow("clicks-tool-cards")
            const p = f.mapToItem(null, f.width / 2, f.height / 2)
            TestInput.wheel(p.x, p.y, 120) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = tests.findAllNames("runBox", t), f = tests.findName("runBoxScroll", boxes[boxes.length - 1])
            tests.check(f.contentY < tests.toolBoxStart[0] && Math.abs(parseFloat(t.debugContentY()) - tests.toolBoxStart[1]) < 1, "the wheel scrolls the box, not the session (" + f.contentY + ", " + t.debugContentY() + ")")
        } }
        // The wheel scrolls smoothly: a notch at a time, as a hand turns it.
        SequentialAnimation {
            loops: 12
            ScriptAction { script: tests.toolWheel(1) }
            PauseAnimation { duration: 250 }
        }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = tests.findAllNames("runBox", t), f = tests.findName("runBoxScroll", boxes[boxes.length - 1])
            // (Its top is originY: a list laid out from its end can start above 0.)
            tests.check(f.atYBeginning && parseFloat(t.debugContentY()) < tests.toolBoxStart[1], "at the box's top, the session scrolls on (" + f.contentY + " of origin " + f.originY + ", " + t.debugContentY() + ")")
            tests.scrollIntoView(f)
            const more = tests.findAllNames("toolMoreLines", boxes[boxes.length - 1])
            if (more.length) tests.clickItem(more[0], "5 more lines") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = tests.findAllNames("runBox", t), box = boxes[boxes.length - 1]
            tests.check(tests.findAllNames("toolMoreLines", box).some(b => b.text === "↑ Fewer lines"), "opened: all of it")
            tests.shotWindow("clicks-tool-cards-open")
            tests.toolBoxCount = boxes.length
            tests.toolBoxGroup = box.group
            tests.clickItem(tests.findName("runHeader", box), "the run's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript
            tests.check(tests.findAllNames("runBox", t).length === tests.toolBoxCount - 1, "its line folds the run")
            // (A folded run's rows, or those in a box, make no cards of their own.)
            // (Hidden ones too: findAllNames passes them over.)
            const cards = []
            const walk = it => { for (const c of it.children) { if (c.objectName === "toolHead" || c.objectName === "thoughtHead") cards.push(c); walk(c) } }
            walk(t)
            tests.check(cards.length > 0 && cards.every(h => h.visible), "no hidden cards are made (" + cards.filter(h => !h.visible).length + " of " + cards.length + " hidden)")
            tests.check(Theme.contrastOf(Theme.textFaint, Theme.background) >= 4.49 && Theme.contrastOf(Theme.textMuted, Theme.background) > Theme.contrastOf(Theme.textFaint, Theme.background), "faint text is readable (4.5:1) and below muted, dark")
            tests.clickItem(chat.transcript.runHeaderOf(tests.toolBoxGroup), "the folded run's line") } }
        // Opened again, its box is where it was left (its top), not at its end.
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const boxes = tests.findAllNames("runBox", chat.transcript), box = boxes.find(b => b.group === tests.toolBoxGroup)
            const f = box ? tests.findName("runBoxScroll", box) : null
            tests.check(f && f.contentY === 0 && f.contentHeight > f.height, "opened again, the run's box is where it was left (" + (f ? f.contentY + " of " + f.contentHeight : "no box") + ")")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.contrastOf(Theme.textFaint, Theme.background) >= 4.49 && Theme.contrastOf(Theme.textFaint, Theme.sidebar) >= 4.49, "faint text is readable (4.5:1), light")
            tests.shotWindow("clicks-tool-cards-light")
            Workspace.chooseThemeMode("dark") } }
        // A run of 200 calls: its box makes the cards near its view, and the
        // keys still go through every step.
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.sendByKeys("run two hundred commands") }
        ScriptAction { script: tests.waitUntil(toolCardSteps, () => tests.chatText().indexOf("Ran all two hundred") >= 0 && chat.session.status === "idle", 30000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = tests.findAllNames("runBox", t), box = boxes[boxes.length - 1], f = tests.findName("runBoxScroll", box)
            const made = () => { const out = []; const walk = it => { for (const c of it.children) { if (c.objectName === "toolHead") out.push(c); walk(c) } }; walk(box); return out }
            const what = h => tests.findAllNames("toolWhat", h).map(l => l.plain).join(" ")
            tests.bigMade = made().length
            const calls = box.rows.filter(r => r.kind !== "thinking").length
            tests.check(f && calls === 200 && tests.bigMade > 0 && tests.bigMade < 80, "a run of 200 calls makes only the cards near its box's view (" + tests.bigMade + " of " + calls + " calls, " + box.rows.map(r => r.kind).filter(k => k !== "tool").join(",") + ")")
            tests.check(made().some(h => /\bStep 199\b/.test(what(h))) && !made().some(h => /\bStep 0\b/.test(what(h))), "it opens at its end, its first steps not made yet (" + made().map(what).join(" | ").slice(0, 300) + ")")
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_Up, Qt.ControlModifier | Qt.ShiftModifier)
            const head = win.activeFocusItem
            tests.check(head && head.objectName === "runHeader" && head.group === box.group, "Ctrl+Shift+Up: the long run's line")
            TestInput.key(Qt.Key_Down, 0)
            const first = win.activeFocusItem
            tests.check(first && first.objectName === "toolHead" && /\bStep 0\b/.test(what(first)), "Down: its first step, made when it's wanted (" + (first ? what(first) : "") + ")")
            for (let k = 0; k < 120; k++) TestInput.key(Qt.Key_Down, 0)
            const far = win.activeFocusItem
            tests.check(far && far.objectName === "toolHead" && /\bStep 120\b/.test(what(far)), "Down ×120: the 121st, the box following (" + (far ? what(far) : "") + ")")
            const p = far.mapToItem(f, 0, 0)
            tests.check(p.y >= -1 && p.y + far.height <= f.height + 1, "the step in the box's view")
            tests.check(made().length < 80, "still only those near the view (" + made().length + ")")
            for (let k = 0; k < 20; k++) TestInput.key(Qt.Key_Up, 0)
            const back = win.activeFocusItem
            tests.check(back && /\bStep 100\b/.test(what(back)), "Up ×20: back to the 101st (" + (back ? what(back) : "") + ")")
            TestInput.key(Qt.Key_Escape, 0)
            tests.summary(); Qt.quit() } }
    }
    // Listings: each kind of code a command or a read can show (cat -n,
    // a read, grep in several files, sed with a long line, a TOML file's
    // head, an unknown file) and code blocks in the reply, in each theme.
    SequentialAnimation {
        id: listingSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickslistings"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("show the listings please") }
        ScriptAction { script: tests.waitUntil(listingSteps, () => tests.chatText().indexOf("Here are the listings") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const code = tests.findAllNames("codeView", box).sort((a, b) => a.mapToItem(box, 0, 0).y - b.mapToItem(box, 0, 0).y).reduce((all, v) => all.concat(v.lines), [])
            tests.check(code.length === 33, "cat -n, the read, grep, sed, head and cat show 6, 6, 6, 6, 6 and 3 lines (" + code.length + ")")
            tests.check(code[0].n === "1" && code[0].h.indexOf("<span") >= 0 && code[5].n === "6", "cat -n: its numbers, TypeScript highlighted (" + JSON.stringify(code[0]) + ")")
            tests.check(code[6].n === "1" && code[9].h.indexOf("<span") >= 0, "a read: its first lines at once, QML highlighted (" + JSON.stringify(code[9]) + ")")
            tests.check(code[12].mark === "file" && code[12].h.indexOf("src/main.rs") >= 0 && code[13].n === "3" && code[15].mark === "gap" && code[16].mark === "file" && code[16].h.indexOf("src/app.ts") >= 0 && code[17].h.indexOf("<span") >= 0,
                      "grep in several files: each file's lines under its name, in its language (" + JSON.stringify(code.slice(12, 18).map(l => [l.n, l.mark])) + ")")
            tests.check(code[18].n === "38" && code[22].h.indexOf("file_print") >= 0, "sed: numbered from 38")
            tests.check(code[24].n === "1" && code[24].h.indexOf("<span") >= 0 && code[30].n === "1" && code[30].h.indexOf("<span") < 0, "TOML highlighted; an unknown file numbered, plain (" + JSON.stringify([code[24], code[30]]) + ")")
            tests.check(tests.findAllNames("toolTail", box).length === 0, "no listing as plain last lines")
            tests.check(win.width < 1000 || tests.findAllNames("codeMoreRight", box).length >= 1, "a line longer than the well: its edge fades, more to the side")
            const blocks = tests.findAllNames("replyCodeView", chat.transcript)
            tests.check(blocks.length === 3 && blocks[0].count === 4 && blocks[0].numbers && blocks[1].count === 1, "a reply's code blocks: numbered (" + blocks.map(b => b.count) + ")")
            tests.check(["TypeScript", "Shell", "TOML"].every(n => tests.findText(n, chat.transcript) !== null), "code blocks name their language")
            tests.listingAt("cat -n")
            tests.shotWindow("listings-catn-read") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { tests.listingAt("Find every use"); tests.shotWindow("listings-grep-sed") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { tests.listingAt("head -16"); tests.shotWindow("listings-head-cat") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Code and text start on whole pixels (at a fraction, text jumps a pixel as things around it repaint).
            const xs = tests.findAllNames("codeText", chat.transcript).map(e => e.mapToItem(null, 0, 0).x)
            tests.check(xs.length > 0 && xs.every(x => Math.abs(x - Math.round(x)) < 0.01), "code starts on whole pixels (" + xs.join(", ") + ")")
            const a = tests.findContaining("Run it with", chat.transcript)
            tests.check(a !== null, "the reply's code blocks")
            if (a) tests.scrollIntoView(a)
            tests.shotWindow("listings-reply-code")
            // Lines cut at the right: that box offers Wrap (only when cut).
            const ts = tests.findAllNames("replyCode", chat.transcript).find(b => tests.findText("TypeScript", b) !== null)
            const wrapBtn = ts ? tests.findName("codeWrap", ts) : null
            const tsCode = ts ? tests.findName("replyCodeView", ts) : null
            tests.check(tsCode && (wrapBtn !== null) === tsCode.cut && (win.width > 1000 || tsCode.cut) && (!wrapBtn || wrapBtn.text === "Wrap"), "a code box whose lines are cut offers Wrap, one that fits doesn't (" + (wrapBtn ? wrapBtn.text : "none") + ")")
            tests.wrapBox = wrapBtn ? ts : null
            if (wrapBtn) tests.clickItem(wrapBtn, "Wrap") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            if (tests.wrapBox) {
                const ts = tests.wrapBox, code = ts ? tests.findName("replyCodeView", ts) : null, btn = ts ? tests.findName("codeWrap", ts) : null
                tests.check(code && code.wrap && !code.cut && btn && btn.text === "No wrap" && !Theme.wrapCode, "Wrap wraps that box only, and says No wrap (" + (btn ? btn.text : "none") + ")")
                tests.shotWindow("listings-reply-wrapped")
                if (btn) tests.clickItem(btn, "No wrap")
            }
            tests.listingAt("qml/Badge.qml", 60) } }
        // (A press while the list still moves only stops it: a moment first.)
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // (Its line, not its path: the path opens the file.)
            tests.clickItem(tests.findAllNames("toolResult", tests.listingBox()).find(l => l.text === "13 lines"), "the read's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const code = tests.findAllNames("codeView", box).sort((a, b) => a.mapToItem(box, 0, 0).y - b.mapToItem(box, 0, 0).y).reduce((all, v) => all.concat(v.lines), [])
            const qml = code.filter(l => /Item|QtQuick/.test(l.h))
            tests.check(code.length === 40 && qml.length >= 2 && qml[0].h.indexOf("<span") >= 0, "the read, opened: its 13 lines numbered, QML highlighted (" + code.length + ")")
            tests.listingAt("qml/Badge.qml")
            tests.shotWindow("listings-read-open")
            Workspace.chooseLook("wrap", "true") } }
        // Word wrap on: a long line takes two rows, the next line's number
        // where that line starts.
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const sed = tests.findAllNames("codeView", tests.listingBox()).find(v => v.lines.length && v.lines[0].n === "38")
            const one = sed.lineHeight(0), long = sed.lineHeight(4)
            tests.check(Theme.wrapCode && long > one * 1.5 && Math.abs(sed.lineTop(5) - (sed.lineTop(4) + long)) < 1, "wrapped: the long line takes " + Math.round(long / one) + " rows, 43 below it (" + [one, long, sed.lineTop(4), sed.lineTop(5)] + ")")
            // Its second row hangs under its indentation and 2 more.
            const ed = tests.findName("codeText", sed), s4 = sed.starts[4], y0 = ed.positionToRectangle(s4).y
            let at = s4
            while (at < ed.length && ed.positionToRectangle(at).y <= y0 + 1) at++
            const x2 = ed.positionToRectangle(at).x, want = ed.positionToRectangle(s4 + 6).x
            tests.check(x2 > 0 && Math.abs(x2 - want) < 2, "…its next row under its indentation and 2 (" + [x2, want] + ")")
            const blocks = tests.findAllNames("replyCodeView", chat.transcript)
            tests.check(blocks.length === 3 && blocks[0].count === 4 && blocks[2].count === 2, "a reply's code blocks, wrapped: still numbered (" + blocks.map(b => b.count) + ")")
            tests.listingAt("Look at how")
            tests.shotWindow("listings-wrap")
            Workspace.chooseLook("wrap", "null")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.listingAt("cat -n"); tests.shotWindow("listings-catn-read-light") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { tests.listingAt("Find every use"); tests.shotWindow("listings-grep-sed-light") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const a = tests.findContaining("Run it with", chat.transcript)
            if (a) tests.scrollIntoView(a)
            tests.shotWindow("listings-reply-code-light")
            Workspace.chooseThemeMode("dark")
            // Copy: on hover only, a reply's as a tool's; a tool's in its
            // call's line, never over its code.
            TestInput.moveTo(4, 4)
            const block = tests.findAllNames("replyCode", chat.transcript)[0]
            tests.check(block && tests.findAllNames("copyCode", block).length === 0, "a reply's Copy: hidden until hovered")
            if (block) {
                const p = block.mapToItem(null, block.width / 2, block.height / 2)
                TestInput.moveTo(p.x, p.y)
                tests.check(tests.findAllNames("copyCode", block).length === 1, "…and shown on hover")
            }
            TestInput.moveTo(4, 4)
            tests.check(tests.findAllNames("copyCode", tests.listingBox()).every(c => !c.visible), "a tool's Copy: hidden until hovered ("
                      + tests.findAllNames("copyCode", tests.listingBox()).map(c => [c.copied, c.activeFocus, win.activeFocusItem ? win.activeFocusItem.objectName + ":" + win.activeFocusItem.focusReason : "-"]) + ")")
            tests.listingAt("Look at how")
            const sedCard = tests.findAllNames("toolWhat", tests.listingBox()).find(l => l.plain.indexOf("Look at how") >= 0)
            const at = sedCard.mapToItem(null, sedCard.width / 2, sedCard.height / 2)
            tests.hoverLineWidth = sedCard.width
            TestInput.moveTo(at.x, at.y + 30) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const copy = tests.findAllNames("copyCode", box).find(c => c.visible)
            const head = copy ? (function up(i) { return !i || i.objectName === "toolHead" ? i : up(i.parent) })(copy.parent) : null
            tests.check(copy && head, "hovered: its Copy, in the call's line (" + (copy ? copy.parent : "none") + ")")
            const sedWhat = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Look at how") >= 0)
            tests.check(sedWhat && sedWhat.width === tests.hoverLineWidth, "its Copy keeps its place: nothing in the line moves on hover (" + (sedWhat ? sedWhat.width : -1) + " vs " + tests.hoverLineWidth + ")")
            const sed = tests.findAllNames("codeView", box).find(v => v.lines.length && v.lines[0].n === "38")
            tests.check(copy && sed && copy.mapToItem(null, 0, copy.height).y <= sed.mapToItem(null, 0, 0).y, "the Copy sits above the code")
            tests.shotWindow("listings-copy-hover")
            // The Latest pill: on the composer's top edge, centred on the column.
            const latest = tests.findName("latestButton", chat)
            const lp = latest ? latest.mapToItem(null, latest.width / 2, latest.height / 2) : null
            const cp = chat.composerItem.mapToItem(null, chat.composerItem.width / 2, 0)
            tests.check(latest && Math.abs(lp.y - cp.y) < 2 && Math.abs(lp.x - cp.x) < 2, "Latest: on the composer's edge, centred (" + (lp ? [lp.x, lp.y, cp.x, cp.y] : "none") + ")")
            // Paths a listing names: on a remote session's host.
            const t = chat.transcript
            tests.check(t.fullPath("src/a.rs", "", "ssh://box/home/u/p") === "ssh://box/home/u/p/src/a.rs" && t.fullPath("/etc/hosts", "", "ssh://box/home/u/p") === "ssh://box/etc/hosts"
                      && t.fullPath("x.rs", "cd sub && cat -n x.rs", "/home/u/p") === "/home/u/p/sub/x.rs" && t.fullPath("/a/b", "", "/home/u/p") === "/a/b",
                      "paths: on the session's host, from a command's cd (" + [t.fullPath("src/a.rs", "", "ssh://box/home/u/p"), t.fullPath("/etc/hosts", "", "ssh://box/home/u/p")] + ")")
            const grepHead = tests.findAllNames("codeView", box).sort((a, b) => a.mapToItem(box, 0, 0).y - b.mapToItem(box, 0, 0).y).reduce((all, v) => all.concat(v.lines), []).find(l => l.mark === "file")
            tests.check(grepHead && grepHead.h.indexOf("harlo-open:3:") >= 0 && grepHead.h.indexOf("↗") >= 0, "a grep file's name: a link, marked ↗")
            // A line number: opens the file at that line.
            const n38 = tests.findAllNames("codeNumber", sed).find(l => l.text === "38")
            tests.check(n38 && sed.lines[0].p.endsWith("/src/listing.rs"), "sed's numbers: links to src/listing.rs (" + (sed ? sed.lines[0].p : "") + ")")
            if (n38) tests.clickItem(n38, "line 38") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            const line = ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1
            tests.check(chat.filePath.endsWith("/src/listing.rs") && line === 38, "line 38: the file opens at it (" + chat.filePath + ", line " + line + ")")
            const top = ed.contentY, lh = ed.lineHeight
            tests.check(top > 0 && top < 37 * lh - 2 * lh, "…with lines above it in view (" + Math.round(top / lh) + " lines scrolled)")
            tests.check(ed.arrivalLine === 37 && ed.arrivalGlow > 0, "…tinted where you arrived (line " + (ed.arrivalLine + 1) + ")")
            tests.shotWindow("listings-open-at-line")
            Workspace.open(chat.session.key) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            // The keyboard: Tab reaches a run's line (not each call); Down
            // goes into its steps, Enter opens one, Escape goes back.
            const box = tests.listingBox(), runHead = tests.findName("runHeader", box)
            const heads = tests.findAllNames("toolHead", box)
            tests.check(runHead && runHead.activeFocusOnTab && heads.every(h => !h.activeFocusOnTab), "Tab: the run's line, not each call")
            // (From the composer, by keys only.)
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_Up, Qt.ControlModifier | Qt.ShiftModifier)
            tests.check(win.activeFocusItem === runHead, "Ctrl+Shift+Up: from the composer to the run's line (" + (win.activeFocusItem ? win.activeFocusItem.objectName : "") + ")")
            tests.check(runHead.Accessible.name.indexOf("Run: ") === 0, "the run's line has a name for screen readers (" + runHead.Accessible.name + ")")
            TestInput.key(Qt.Key_Down, 0)
            const first = win.activeFocusItem
            tests.check(first && first.objectName === "toolHead" && tests.findAllNames("toolWhat", first)[0].plain.indexOf("cat -n") >= 0, "Down: its first step")
            for (let k = 0; k < 4; k++) TestInput.key(Qt.Key_Down, 0)
            const head = win.activeFocusItem
            tests.keyHead = head
            tests.check(head && tests.findAllNames("toolWhat", head)[0].plain.indexOf("head -16") >= 0, "Down ×4: its fifth (" + (head ? head.objectName : "") + ")")
            TestInput.key(Qt.Key_Return, 0)
            TestInput.key(Qt.Key_C, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const head = tests.keyHead
            tests.check(head && head.activeFocus && head.parent.open, "Enter opens it, its line ringed (" + (head ? [head.activeFocus, head.parent.open] : "") + ")")
            tests.check(tests.findAllNames("copyCode", head).some(c => c.copied), "Ctrl+C copies its listing")
            tests.listingAt("head -16")
            tests.shotWindow("listings-keyboard")
            TestInput.key(Qt.Key_Escape, 0)
            tests.check(win.activeFocusItem && win.activeFocusItem.objectName === "runHeader", "Escape: back to the run's line")
            TestInput.key(Qt.Key_Escape, 0)
            tests.check(win.activeFocusItem && win.activeFocusItem.objectName === "composerInput", "Escape again: the composer")
            // Typing on a run goes to the composer.
            TestInput.key(Qt.Key_Up, Qt.ControlModifier | Qt.ShiftModifier)
            TestInput.typeText("hi")
            tests.check(chat.composerItem.text === "hi" && win.activeFocusItem.objectName === "composerInput", "typing on a run: into the composer (" + chat.composerItem.text + ")")
            TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0)
            tests.sendByKeys("read five files") } }
        ScriptAction { script: tests.waitUntil(listingSteps, () => tests.chatText().indexOf("Read them all") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const shown = tests.findAllNames("codeView", box).filter(v => v.visible && v.lines.length).sort((a, b) => a.mapToItem(box, 0, 0).y - b.mapToItem(box, 0, 0).y)
            tests.check(shown.length === 3 && shown[0].lines[0].h.indexOf("c.rs") >= 0, "five reads: the last 3 show their lines (" + shown.map(v => v.lines.length && v.lines[0].h.replace(/<[^>]+>/g, "")) + ")")
            tests.listingAt("a.rs")
            const aRead = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("a.rs") >= 0)
            const ap = aRead.mapToItem(null, 4, aRead.height / 2)
            TestInput.moveTo(ap.x, ap.y)
            tests.check(tests.findAllNames("showLines", box).length === 1 && !tests.findAllNames("copyCode", box).some(c => c.visible), "an older read, hovered: Show lines, no Copy")
            tests.shotWindow("listings-five-reads")
            tests.sendByKeys("read a long file") } }
        ScriptAction { script: tests.waitUntil(listingSteps, () => tests.chatText().indexOf("the long one") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            chat.transcript.follow = false
            const r = tests.findAllNames("toolResult", chat.transcript).find(l => l.text.indexOf("1200 lines") >= 0)
            tests.check(r !== undefined, "the long read")
            const more = tests.findAllNames("toolMoreLines", chat.transcript).find(l => l.text.indexOf("1194") >= 0)
            tests.check(more !== undefined, "its preview: 6 lines, ↓ 1194 more (the whole file's count)")
            tests.scrollIntoView(r)
            const t0 = Date.now()
            tests.clickItem(r, "the long read's line")
            tests.perfOpen = Date.now() - t0 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const v = tests.findAllNames("codeView", chat.transcript).find(c => c.lines.length === 400)
            tests.check(v !== undefined, "opened: 400 lines (" + tests.findAllNames("codeView", chat.transcript).map(c => c.lines.length) + ")")
            const lf = tests.findName("runBoxScroll", tests.listingBox())
            tests.check(lf && lf.contentY === 0, "a run of one call, opened: its top in view (" + (lf ? lf.contentY : "none") + ")")
            if (v) {
                tests.check(v.lines[0].n === "1" && v.lines[399].n === "400", "…from its first line")
                // The numbers and tints placed again (as on a resize): ten times.
                const t0 = Date.now()
                for (let k = 0; k < 10; k++) v.laidOut++
                console.log("PERF relayout x10 ms", Date.now() - t0, "open ms", tests.perfOpen)
            }
            const beyond = tests.findAllNames("toolBeyond", chat.transcript)[0], go = tests.findAllNames("toolBeyondOpen", chat.transcript)[0]
            tests.check(beyond && beyond.text === "800 more lines" && go && go.text === "Open at line 401", "past 400: said, with a way on (" + (beyond ? beyond.text : "none") + " · " + (go ? go.text : "") + ")")
            if (go) { tests.scrollIntoView(go); tests.shotWindow("listings-past-400"); tests.clickItem(go, "Open at line 401") } } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            const line = ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1
            tests.check(chat.filePath.endsWith("/src/long.rs") && line === 401, "Open at line 401: the file there (" + chat.filePath + ", " + line + ")")
            Workspace.open(chat.session.key) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.sendByKeys("search a big tree") }
        ScriptAction { script: tests.waitUntil(listingSteps, () => tests.chatText().indexOf("Searched it") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            chat.transcript.follow = false
            const box = tests.listingBox()
            const card = what => { const w = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf(what) >= 0); return w ? w.parent.parent.parent : null }
            const grep = card("Search the whole tree"), nl = card("Number the log")
            tests.check(grep && grep.code && grep.code.lines[0].n === "3" && !grep.code.earlier, "400 matches: kept from the first (" + (grep && grep.code ? grep.code.lines[0].n : "none") + ")")
            const all = grep ? chat.session.codeText("Bash", grep.command, "", chat.session.callOutput(grep.rowKey)).split("\n").length : 0
            tests.check(all === 400, "Copy: all 400 matches, not just what's kept (" + all + ")")
            const nlLast = nl && nl.code ? nl.code.lines[nl.code.lines.length - 1].n : "none"
            const up = tests.findAllNames("toolEarlierLines", box)
            tests.check(nl && nl.code && nl.code.earlier && nlLast === "1200" && up.length === 1 && /^↑ \d+ of 1194 earlier lines$/.test(up[0].text) && tests.findAllNames("toolEarlier", box).length === 0,
                      "a long log, kept to its end: its last lines, the earlier ones above them (" + nlLast + " · " + (up[0] ? up[0].text : "none") + ")")
            tests.listingAt("Search the whole tree")
            const w = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Search the whole tree") >= 0)
            if (w) tests.clickItem(w, "the search's line") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const beyond = tests.findAllNames("toolBeyond", box)[0]
            tests.check(beyond && beyond.text === "More lines not shown here" && tests.findAllNames("toolBeyondOpen", box).length === 0, "opened: more past these, no file jump for search results (" + (beyond ? beyond.text : "none") + ")")
            } }
        // (The log's card, past the opened listing, is made as the box comes
        // to it, and placed again once what's above it has its height.)
        ScriptAction { script: tests.waitUntil(listingSteps, () => tests.listingAt("Number the log"), 5000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const box = tests.listingBox()
            tests.listingAt("Number the log")
            tests.shotWindow("listings-search-log")
            const up = tests.findAllNames("toolEarlierLines", box)[0]
            const nlw = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Number the log") >= 0)
            const lv = nlw ? tests.findAllNames("codeView", nlw.parent.parent.parent)[0] : null
            tests.holdY = lv ? lv.mapToItem(null, 0, lv.height).y : -1
            if (up) tests.clickItem(up, "the log's earlier lines") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = tests.listingBox()
            const w = tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Number the log") >= 0)
            const nl = w ? w.parent.parent.parent : null
            const lines = nl && nl.code ? nl.code.lines : []
            tests.check(lines.length > 6 && lines[lines.length - 1].n === "1200" && Number(lines[0].n) > 1 && tests.findAllNames("toolEarlier", box).length === 1
                      && tests.findAllNames("toolMoreLines", box).some(b => b.text === "↑ Fewer lines") && tests.findAllNames("toolEarlierLines", box).length === 0,
                      "opened, the log's kept lines to its end, and earlier lines not here said (" + (lines[0] ? lines[0].n : "none") + "–" + (lines.length ? lines[lines.length - 1].n : "") + ")")
            const lv = nl ? tests.findAllNames("codeView", nl)[0] : null
            const y = lv ? lv.mapToItem(null, 0, lv.height).y : -2
            tests.check(tests.holdY > 0 && Math.abs(y - tests.holdY) < 2, "opening the earlier lines keeps the last line where it was (" + Math.round(tests.holdY) + " → " + Math.round(y) + ")")
            tests.listingAt("Number the log")
            tests.shotWindow("listings-search-log-open")
            tests.summary(); Qt.quit() } }
    }
    // A long session's replies far back keep their long text in a page file:
    // shown, run, found, listed and copied as if they never left.
    SequentialAnimation {
        id: pagingSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickspaging"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            const path = win.selftestDir + "/data/harlo-agent/sessions/--" + cwd.replace(/^\//, "").replace(/\//g, "-") + "--/paged000-0000-0000-0000-000000000000.jsonl"
            Qt.callLater(() => Workspace.open(path)) } }
        PauseAnimation { duration: 1500 }
        // (5 rows a turn; then paged once they're sent.)
        ScriptAction { script: tests.waitUntil(pagingSteps, () => chat.transcript.listView.count >= 2000 && chat.session.status === "idle" && chat.session.pagedRows() > 0, 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const n = chat.session.pagedRows(), all = chat.transcript.listView.count
            tests.check(n > 500 && n < all - 500, "far back, rows keep their long text on disk; the latest stay whole (" + n + " of " + all + ")")
            tests.check(tests.textHas("zebra399q", chat.transcript), "the latest reply shows")
            chat.transcript.follow = false
            chat.transcript.listView.positionViewAtBeginning() } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.check(tests.textHas("Fixed part 0: the zebra0q loop stopped one short", chat.transcript), "at the top, the first reply reads as it was")
            const head = tests.findAllNames("runHeader", chat.transcript)[0]
            const run = head ? JSON.parse(chat.session.runJson(head.group)) : []
            const out = run.length ? chat.session.callOutput(run[0].rowKey) : ""
            tests.check(run.length === 2 && run[0].command === "grep -n needle src/part0/file0.rs" && run[0].text.indexOf("12: needle 0-0 at line 12") >= 0 && out === run[0].text,
                      "its run's calls: command and output read back, copied whole (" + run.length + " · " + (run.length ? run[0].command : "") + ")")
            tests.shotWindow("clicks-paging-top")
            if (head) tests.clickItem(head, "the first run's line") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const box = tests.findAllNames("runBox", chat.transcript)[0]
            tests.check(box && tests.findAllNames("toolWhat", box).length === 2, "opened, its two calls' cards")
            tests.shotWindow("clicks-paging-run")
            chat.openFind()
            TestInput.typeText("zebra3q") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const fb = chat.findBar
            tests.check(fb.hits.length === 1 && chat.transcript.wordMarks.length > 0 && tests.textHas("zebra3q", chat.transcript), "find: a reply far back, found, shown and marked (" + fb.hits.length + " · " + chat.transcript.wordMarks.length + ")")
            tests.shotWindow("clicks-paging-find")
            TestInput.key(Qt.Key_Escape, 0)
            const prompts = JSON.parse(chat.session.userPrompts())
            tests.check(prompts.length === 400 && prompts[0].text === "Look at part 0 and fix what's wrong there, please.", "the prompt rail has every prompt (" + prompts.length + ")")
            chat.transcript.revealIndex(prompts[200].i) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.check(tests.textHas("Look at part 200 and fix", chat.transcript) && tests.textHas("zebra200q", chat.transcript), "jumped to a prompt far back: it and its reply")
            chat.transcript.listView.positionViewAtEnd()
            chat.transcript.follow = true } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.check(tests.textHas("zebra399q", chat.transcript), "back at the end, the latest reply")
            tests.summary(); Qt.quit() } }
    }
    /// Some shown text under `root` has `part` in it (rich text included).
    function textHas(part, root) {
        const walk = item => {
            for (const c of item.children) {
                if (!c.visible || c.opacity <= 0) continue
                if (typeof c.text === "string" && c.text.indexOf(part) >= 0 && shown(c)) return true
                if (walk(c)) return true
            }
            return false
        }
        return walk(root)
    }
    property real perfOpen: 0
    property real holdY: 0
    property var harloJobCard: null
    property var bgNotice: null
    property var pluginNote: null
    property int pluginAsked: 0
    property string harloRightWas: ""
    property var wrapBox: null

    property var keyHead: null
    function listingBox() {
        const boxes = tests.findAllNames("runBox", chat.transcript)
        return boxes[boxes.length - 1]
    }
    // Brings a card of the last run to its box's top, and the box into view.
    function listingAt(what, below) {
        const box = listingBox(), f = tests.findName("runBoxScroll", box)
        const find = () => tests.findAllNames("toolWhat", box).find(l => l.plain.indexOf(what) >= 0)
        let w = find()
        // (Far from the box's view, its card isn't made yet: scrolled to it,
        // as a hand would.)
        if (!w) {
            const j = (box.rows || []).findIndex(r => [r.label, r.subtitle, r.command, r.title].some(x => (x || "").indexOf(what) >= 0))
            // (Laid out by the next frame: look again then.)
            // (Once: then the box settles, what's above the card taking its
            // height, before it's looked for again.)
            if (j >= 0 && tests.seeking !== what) { tests.seeking = what; chat.transcript.follow = false; tests.scrollIntoView(f); f.pinned = false; f.positionViewAtIndex(j, ListView.Beginning) }
            if (j >= 0) return false
        }
        if (!w) { tests.check(false, "find the card " + what); return false }
        tests.seeking = ""
        // (As a hand would: the session stops following its end, and the
        // box stays where it's put instead of going back to its end.)
        chat.transcript.follow = false
        tests.scrollIntoView(f)
        f.pinned = false
        // (A list's content can start above 0: its origin.)
        const top = f.originY || 0
        f.contentY = Math.max(top, Math.min(top + f.contentHeight + (f.bottomMargin || 0) - f.height, w.mapToItem(f.contentItem, 0, 0).y - (below || 8)))
        return true
    }
    property var toolBoxStart: []
    // Turns the wheel up over the last run box, `n` notches.
    function toolWheel(n) {
        const boxes = tests.findAllNames("runBox", chat.transcript), f = tests.findName("runBoxScroll", boxes[boxes.length - 1])
        const p = f.mapToItem(null, f.width / 2, f.height / 2)
        for (let k = 0; k < n; k++) TestInput.wheel(p.x, p.y, 120)
    }
    property int toolBoxCount: 0
    property string toolBoxGroup: ""
    property int bigMade: 0
    property string seeking: ""

    // Settings → Teamwork: the skills earlier versions installed are
    // gone (a user's own of the same name kept), and the page's switches.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksorchestration"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            const h = win.selftestDir + "/home"
            tests.check(TestInput.readFile(h + "/.claude/skills/harlo-advisor/SKILL.md") === "" && TestInput.readFile(h + "/.agents/skills/harlo/SKILL.md") === "", "the skills harlo installed are removed at start")
            tests.check(TestInput.readFile(h + "/.agents/skills/harlo-handoff/SKILL.md") === "mine\n", "the user's own skill of the same name kept")
            settingsDialog.openPage("orchestration") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("orchSelfStart", d) !== null && tests.findName("orchReviewSteps", d) !== null && tests.findText("Skills", d) === null, "the page: the switch and how helpers start, no skills")
            tests.shotWindow("clicks-orchestration")
            tests.findName("orchSelfStart", d).activated(0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(Workspace.setting("orchestration_on_their_own") === "asked" && tests.findName("orchSelfStart", d).currentIndex === 0, "Only when I ask is saved and shown")
            tests.clickItem(tests.findName("orchEnabled", d), "Orchestration's switch, off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(!Workspace.orchestrationOn && Workspace.setting("orchestration") === "0" && !(tests.findName("orchSelfStart", d) || {}).visible, "off: saved, and the page shows only the switch")
            tests.shotWindow("clicks-orchestration-off")
            tests.clickItem(tests.findName("orchEnabled", d), "Orchestration's switch, on") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.orchestrationOn && Workspace.setting("orchestration") === "1", "on again")
            tests.summary()
            Qt.quit() } }
    }

    // Orchestration on a host with the real Claude (not in the click
    // suite; --ssh= a test sshd): the Python bridge there, harlo's socket
    // forwarded; one tiny Haiku turn calls list_agents.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "orchestrateremotereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")); Remotes.connectHost("box") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: { tests.log("host:", JSON.parse(Remotes.hostsJson())[0].state); Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: { chat.session.setModel("anthropic", "claude-haiku-4-5"); tests.sendByKeys("Call the list_agents tool of the harlo MCP server, then reply with just the agent ids it returned, comma-separated.") } }
        PauseAnimation { duration: 45000 }
        ScriptAction { script: {
            tests.log("asked:", chat.session.hasRequest, "| status:", chat.session.status, "| transcript:", tests.chatText().replace(/\n/g, " | ").slice(-600))
            Qt.quit() } }
    }

    // Orchestration with a real agent (not in the click suite): one tiny
    // turn (--harness=, --model= provider/model) that calls harlo's
    // list_agents (approved when asked).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "orchestratereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            tests.sendByKeys("Call the list_agents tool of the harlo MCP server, then reply with just the agent ids it returned, comma-separated.") } }
        PauseAnimation { duration: 20000 }
        ScriptAction { script: { tests.log("asked:", chat.session.hasRequest, chat.session.requestTitle); if (chat.session.hasRequest) chat.session.answerValue("Approve") } }
        PauseAnimation { duration: 25000 }
        ScriptAction { script: {
            tests.log("status:", chat.session.status, "| transcript:", tests.chatText().replace(/\n/g, " | ").slice(-500))
            Qt.quit() } }
    }

    // Settings → Agents: each provider's state, its
    // details, and its switch, which decides what New session offers: a
    // stand-in claude (--fake-claude=) and Harlo, turned off.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksproviders"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, st = id => { const l = tests.findName("providerStatus-" + id, d); return l ? l.text : "(none)" }
            tests.check(st("claude") === "Available · 17 models", "Claude: available, 17 models (" + st("claude") + ")")
            tests.check(st("harlo") !== "(none)" && st("harlo") !== "Not installed", "Harlo: harlo's own (" + st("harlo") + ")")
            tests.clickItem(tests.findName("provider-claude", d), "the Claude row") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const opus = tests.findText("Opus 4.8", d)
            tests.check(tests.findText("Version 2.1.296", d) !== null && (opus === null || !opus.visible), "its details: version, and its models folded")
            tests.clickItem(tests.findName("providerModels-claude", d), "Show its 17 models") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, opus = tests.findText("Opus 4.8", d)
            tests.check(opus !== null && opus.visible, "its models on demand")
            tests.shotWindow("clicks-providers")
            tests.clickItem(tests.findName("providerSwitch-harlo", d), "Harlo's switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const ids = JSON.parse(Workspace.chatProvidersJson()).map(p => p.id)
            const st = tests.findName("providerStatus-harlo", settingsDialog.contentItem)
            tests.check(st && st.text === "Disabled" && ids.indexOf("harlo") < 0, "turned off: Disabled, and not offered (" + ids + ")")
            tests.check(ids.indexOf("claude") >= 0, "Claude is offered (" + ids + ")")
            settingsDialog.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            // No default set: it shows (and starts) the first one on and
            // installed, Claude here, as harlo does.
            tests.check(chat.draftView.effectiveHarness === "claude" && Workspace.defaultAgent() === "claude", "with no default it shows the agent harlo starts (" + chat.draftView.effectiveHarness + ")")
            tests.clickItem(tests.findName("draftHarness", chat.draftView), "the provider chip") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findText("Claude") !== null && tests.findText("Harlo") === null, "New session offers what's on")
            TestInput.key(Qt.Key_Escape, 0)
            Workspace.setProviderEnabled("harlo", true)
            tests.summary(); Qt.quit() } }
    }

    // The new-session page's model: the agent's models (as Settings →
    // Providers reads them), picked from a filtered list; the session
    // starts with it (the stand-in claude says its model).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdraftmodel"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.refreshProviders()
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.openDraft(JSON.parse(Workspace.projectsJson())[0].id)
            chat.draftView.harness = "claude" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const chip = tests.findName("draftModel")
            tests.check(chip !== null && chip.text === "Default model", "a model chip: the agent's default")
            tests.clickItem(chip, "the model chip") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.check(chat.draftView.modelChoices.length === 17, "its 17 models to pick (" + chat.draftView.modelChoices.length + ")") }
        ScriptAction { script: {
            tests.clickItem(tests.findName("draftModelFilter"), "the filter")
            TestInput.typeText("haiku") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const haiku = chat.draftView.modelChoices.find(m => m.name.toLowerCase().indexOf("haiku") >= 0)
            tests.check(haiku !== undefined, "a Haiku to pick")
            tests.shotWindow("clicks-draft-model")
            tests.clickItem(haiku ? tests.findName("draftModel-" + haiku.id) : null, "Haiku") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = chat.draftView.model
            tests.check(m !== "" && tests.findName("draftModel").text === chat.draftView.modelName, "picked: the chip says it (" + m + ")")
            tests.clickItem(tests.findName("draftInput"), "the message box")
            TestInput.typeText("hello")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const haiku = chat.draftView.modelChoices.find(m => m.name.toLowerCase().indexOf("haiku") >= 0)
            tests.check(chat.session.harness === "claude" && haiku && tests.chatText().indexOf("(model " + haiku.id + ",") >= 0, "the session starts with it (" + tests.chatText().slice(-120) + ")")
            tests.summary(); Qt.quit() } }
    }

    // Settings → Appearance: the colour scheme and theme cards,
    // contrast, diff colours, chat width, animations, the fonts' sizes and
    // word wrap; each takes effect at once.
    // The setup guide: shown on a first start, not to a harlo set up before;
    // the agents (a fake Claude and Harlo, both there: no Install
    // buttons), the default agent and its model (then the new-session
    // page starts with it), git (the services' cards, the default, a
    // server; no sign-in), the theme; Finish marks it done; Settings opens
    // it again.
    SequentialAnimation {
        id: setupSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickssetup"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.refreshProviders() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(Workspace.setting("setup_done") === "", "not set up yet")
            win.maybeShowSetup() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(setupGuide.visible && setupGuide.step === 0, "a first start shows the guide")
            tests.check(["claude", "harlo"].every(id => tests.findName("setupAgent-" + id) !== null), "a card for each agent")
            tests.check(["claude", "harlo"].every(id => tests.findName("setupInstall-" + id) === null), "no Install for an installed one")
            tests.clickItem(tests.findName("setupDefault-claude"), "Claude by default") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(Workspace.setting("default_harness") === "claude", "the default agent is saved")
            const combo = tests.findName("setupModel"), p = JSON.parse(Workspace.providersJson()).find(x => x.id === "claude")
            const i = p.models.findIndex(n => n.toLowerCase().indexOf("haiku") >= 0)
            tests.check(combo !== null && combo.visible && combo.count === p.modelIds.length + 1 && i >= 0, "its models to pick (" + (combo ? combo.count : 0) + ")")
            tests.setupModel = p.modelIds[i]
            combo.activated(i + 1) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.setting("default_model_claude") === tests.setupModel, "the model is saved (" + Workspace.setting("default_model_claude") + ")")
            tests.shotPopup(setupGuide, "clicks-setup-agents") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("setupNext"), "Next") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(setupGuide.step === 1 && ["github", "gitlab", "forgejo"].every(s => tests.findName("gitHost-" + s) !== null), "git: a card for each service")
            const gh = JSON.parse(Setup.gitStatus("github"))
            tests.check(gh.state === "ready" && gh.account === "", "GitHub: not signed in here (" + JSON.stringify(gh) + ")")
            tests.check(tests.findName("gitHost-github").chosen, "GitHub by default")
            tests.clickItem(tests.findName("gitHostUse-gitlab"), "GitLab by default")
            const server = tests.findName("gitHostServer-forgejo")
            server.text = "https://Git.Example.org/"
            server.editingFinished() } }
        PauseAnimation { duration: 2000 }
        // (The services checked: on a busy computer that takes a while.)
        ScriptAction { script: tests.waitUntil(setupSteps, () => tests.gitHostsChecked(setupGuide.contentItem), 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const over = ["github", "gitlab", "forgejo"].map(v => tests.findName("gitHost-" + v, setupGuide.contentItem)).filter(c => c).map(c => tests.spills(c)).reduce((a, b) => a.concat(b), [])
            tests.check(over.length === 0, "the git services' buttons stay inside their cards (" + over.join(", ") + ")")
            tests.check(Workspace.setting("git_host") === "gitlab" && tests.findName("gitHost-gitlab").chosen, "the default service is saved")
            tests.check(JSON.parse(Setup.gitStatus("forgejo")).host === "git.example.org", "Forgejo's server, as a host (" + Setup.gitStatus("forgejo") + ")")
            tests.shotPopup(setupGuide, "clicks-setup-git") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("setupNext"), "Next") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(setupGuide.step === 2, "the theme page")
            tests.clickItem(tests.findName("setupScheme-light"), "Light")
            tests.clickItem(tests.findName("setupTheme-Dracula"), "Dracula") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(Workspace.themeMode === "light" && Workspace.themeFamily === "Dracula", "the theme is chosen (" + Workspace.themeFamily + ", " + Workspace.themeMode + ")")
            tests.shotPopup(setupGuide, "clicks-setup-theme")
            tests.check(tests.findName("setupNext").text === "Finish", "the last page finishes") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("setupNext"), "Finish") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(!setupGuide.visible && Workspace.setting("setup_done") === "1", "Finish closes it and marks harlo set up")
            win.maybeShowSetup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!setupGuide.visible, "not again on the next start")
            win.openDraft(-1)
            chat.draftView.harness = "claude" } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(chat.draftView.model === tests.setupModel, "a new session starts with that model (" + chat.draftView.model + ")")
            // A harlo set up before the guide (it has projects): not shown, marked set up.
            Workspace.setSetting("setup_done", "")
            Workspace.addProject(win.argValue("--cwd="))
            win.maybeShowSetup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!setupGuide.visible && Workspace.setting("setup_done") === "1", "a harlo with projects doesn't get it")
            settingsDialog.openPage("git") } }
        PauseAnimation { duration: 1500 }
        // (The services checked: on a busy computer that takes a while.)
        ScriptAction { script: tests.waitUntil(setupSteps, () => tests.gitHostsChecked(settingsDialog.contentItem), 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("gitHost-gitlab", settingsDialog.contentItem) !== null, "Settings → Git has the services")
            const over = ["github", "gitlab", "forgejo"].map(v => tests.findName("gitHost-" + v, settingsDialog.contentItem)).filter(c => c).map(c => tests.spills(c)).reduce((a, b) => a.concat(b), [])
            tests.check(over.length === 0, "there too, their buttons stay inside the cards (" + over.join(", ") + ")")
            tests.shotPopup(settingsDialog, "clicks-setup-settings-git") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: settingsDialog.openPage("general") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("openSetupGuide"), "Settings → Open the setup guide") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(setupGuide.visible && !settingsDialog.visible && setupGuide.step === 0, "Settings opens it again, from the start")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!setupGuide.visible, "Escape closes it")
            tests.summary()
            Qt.quit() } }
    }
    property string setupModel: ""

    // Undo a turn: a message in a git folder takes a snapshot first; the
    // folder changes after it (README.md edited, as an agent would); the
    // message offers Undo, which lists README.md and puts it back (your
    // untracked notes.md, there before, stays).
    property string undoKey: ""
    // Review comments on a diff: a click on a new line's number opens the
    // comment box (the line's code in it); one there and one on a removed
    // line; marks on both lines and "2 comments on 1 file" at the bottom;
    // Send puts them in the session as one message (file, line, code, the
    // comment), and they're gone from the diff.
    property string reviewKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksreview"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("diff_layout", "split")
            Workspace.addProject(win.argValue("--cwd="))
            tests.reviewKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: tests.clickText("app.rs", changes) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.reviewRow = dv.rows.findIndex(r => r[5] === 1)
            tests.clickAt(tests.findName("gutter-r-" + tests.reviewRow, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = chat.diffView.commentBox
            tests.check(box !== null && box.visible && box.side === "new" && box.code !== "", "the comment box, with the line's code (" + (box ? box.code : "") + ")")
            tests.clickItem(tests.findName("commentText", box.contentItem), "the comment")
            TestInput.typeText("Use a clearer name here") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("commentSave", chat.diffView.commentBox.contentItem), "Comment") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(dv.comments.length === 1 && dv.comments[0].path === "src/app.rs" && dv.comments[0].text === "Use a clearer name here", "the comment is kept (" + JSON.stringify(dv.comments) + ")")
            const old = dv.rows.findIndex(r => r[0] === 1)
            tests.clickAt(tests.findName("gutter-l-" + old, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("commentText", chat.diffView.commentBox.contentItem), "the comment")
            TestInput.typeText("Keep this check")
            TestInput.key(Qt.Key_Return, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const dv = chat.diffView, bar = tests.findName("reviewBar", dv)
            tests.check(dv.comments.length === 2 && dv.comments.some(c => c.side === "old"), "Ctrl+Enter keeps the second (on a removed line)")
            tests.check(bar !== null && bar.visible && dv.canSend, "the bar: 2 comments, ready to send")
            const summary = tests.findName("reviewSummary", bar), who = tests.findName("reviewFor", bar)
            // (Narrow, who they're for is cut first: it may have no room.)
            tests.check(summary !== null && (summary.text === "2 comments on 1 file" || (chat.diffView.width < 460 && summary.text === "2 comments")) && (who === null || who.text.indexOf("for ") === 0), "it says how many, on how many files, for which session (" + (summary ? summary.text + " " + (who ? who.text : "-") : "") + ")")
            // Each saved comment shows under its line.
            tests.check(tests.findName("diffNote-0", dv) !== null && tests.findName("diffNote-1", dv) !== null && dv.rows.filter(r => r[0] === 4).length === 6, "the comments show under their lines (" + dv.rows.filter(r => r[0] === 4).length + " rows kept for them)")
            tests.shot("clicks-review") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickItem(tests.findName("reviewSend", chat.diffView), "Send to the agent") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const text = tests.chatText()
            tests.check(Workspace.activeKey === tests.reviewKey, "back in the session (" + Workspace.activeKey + ")")
            tests.check(text.indexOf("Review comments") >= 0 && text.indexOf("src/app.rs:") >= 0 && text.indexOf("Use a clearer name here") >= 0 && text.indexOf("Keep this check") >= 0 && text.indexOf("a line you removed") >= 0,
                      "the agent gets them as one message (" + text.slice(0, 600) + ")")
            tests.check(Object.keys(chat.reviewStore.all).length === 0, "and they're gone from the diff")
            // Off in Settings → Git: a diff takes no comments.
            Workspace.setSetting("review_comments", "0")
            tests.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(chat.diffView.visible && chat.diffView.review === null, "comments off: the diff takes none")
            tests.summary()
            Qt.quit() } }
    }
    property int reviewRow: -1
    /// The diff's "hide unchanged lines": its button, or (a tight toolbar)
    /// the More menu's item.
    function diffCollapseClick() {
        const b = tests.findName("diffCollapse")
        if (b) { tests.clickItem(b, "Hide unchanged lines"); return }
        tests.clickItem(tests.findName("diffMore"), "More")
        diffMenuTimer.restart()
    }
    Timer { id: diffMenuTimer; interval: 350; onTriggered: tests.clickItem(tests.findName("diffMenuCollapse"), "Hide unchanged lines (menu)") }
    /// A click in the middle of an item, without scrolling to it (the
    /// diff's line numbers follow its text: scrolling them alone would
    /// part them).
    function clickAt(item) {
        if (!item) { check(false, "find a line number"); return }
        const p = item.mapToItem(null, item.width / 2, item.height / 2)
        TestInput.click(p.x, p.y)
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksundo"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            tests.undoKey = Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            chat.composerItem.text = "Rewrite the README"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const undo = JSON.parse(chat.session.undoJson || "{}")
            tests.check(Object.keys(undo).length === 1, "the message has a snapshot to undo to (" + chat.session.undoJson + ")")
            Workspace.openFileTab(win.argValue("--cwd=") + "/README.md") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { chat.fileEditor.typeText("The agent was here.\n"); chat.fileEditor.save() } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.open(tests.undoKey); windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(git.files.some(f => f.startsWith("README.md\t")), "README.md is changed (" + git.files + ")")
            const key = Object.keys(JSON.parse(chat.session.undoJson || "{}"))[0]
            const i = chat.session.indexOfKey(key)
            tests.shot("clicks-undo-button")
            const latest = tests.findName("undoLatest")
            tests.check(i >= 0 && chat.transcript.latestUndo === key && latest !== null && latest.visible && latest.text === "Undo these changes (1 file)",
                      "under the reply: Undo these changes (1 file) (" + (latest ? latest.text : "none") + ")")
            chat.undoDialog.show(key) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson || "{}")
            tests.check(plan.state === "ready" && plan.restore.indexOf("README.md") >= 0 && plan.remove.length === 0, "it would bring README.md back (" + chat.session.undoPlanJson + ")")
            tests.check(chat.undoDialog.title === "Undo this turn", "the last turn: Undo this turn")
            tests.shotPopup(chat.undoDialog, "clicks-undo-dialog") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("undoConfirm", chat.undoDialog.footer), "Undo") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson || "{}")
            tests.check(plan.state === "done" && plan.message.indexOf("Undone") === 0, "undone (" + plan.message + ")")
            tests.check(!git.files.some(f => f.startsWith("README.md\t")) && git.files.some(f => f.startsWith("notes.md\t")), "README.md is back; your notes.md stays (" + git.files + ")")
            tests.shotPopup(chat.undoDialog, "clicks-undo-done") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(chat.transcript.latestUndo === "", "undone: nothing left to undo, the button goes")
            tests.shot("clicks-undo-after") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            chat.undoDialog.close()
            // A second turn: the first one's end (where your next message starts) offers it.
            chat.composerItem.text = "And now the tests"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 6000 }
        // (It shows while you point at the turn's start.)
        ScriptAction { script: {
            const first = Object.keys(JSON.parse(chat.session.undoJson || "{}")).find(k => JSON.parse(chat.session.undoJson)[k].next !== "")
            const second = chat.session.indexOfKey(JSON.parse(chat.session.undoJson)[first || ""] ? JSON.parse(chat.session.undoJson)[first].next : "")
            chat.transcript.positionAt(second)
            const p = chat.transcript.mapToItem(null, chat.transcript.width / 2, chat.transcript.height / 2)
            TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const first = Object.keys(JSON.parse(chat.session.undoJson || "{}")).find(k => JSON.parse(chat.session.undoJson)[k].next !== "")
            const second = chat.session.indexOfKey(JSON.parse(chat.session.undoJson)[first || ""] ? JSON.parse(chat.session.undoJson)[first].next : "")
            const earlier = tests.findName("undoTurn-" + second)
            tests.check(first !== undefined && earlier !== null && earlier.visible && earlier.turn === first, "the earlier turn ends with Undo from here (" + chat.session.undoJson + ")")
            // Off in Settings → Git: no Undo, old snapshots neither.
            Workspace.setSetting("undo_snapshots", "0")
            chat.session.loadUndo() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(chat.session.undoJson === "", "Undo off: no message offers it (" + chat.session.undoJson + ")")
            tests.summary()
            Qt.quit() } }
    }

    // A pull request from the Changes panel: a pushed branch on a GitHub
    // remote (a stand-in gh): none yet, so Create, with the title and
    // description from its two commits and main to go into; created, the
    // panel opens it.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksrequest"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo")
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            tests.check(req.state === "ready" && req.service === "github" && req.repo === "octo/app" && req.head === "fix/sidebar-scroll" && req.base === "main", "the branch can have a pull request on GitHub (" + git.requestJson + ")")
            tests.check(req.url === "" && req.canCreate, "none yet; gh can create it")
            tests.check(req.title === "Sidebar scroll" && req.body === "- Keep the sidebar's place\n- Test it with 200 sessions", "title and description from its commits (" + req.title + " / " + req.body + ")")
            tests.clickItem(tests.findName("requestButton"), "Create pull request") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = changes.requestDialog
            tests.check(d.visible && tests.findName("requestTitle", d.contentItem).text === "Sidebar scroll" && tests.findName("requestBase", d.contentItem).text === "main", "the form, filled in")
            tests.shotPopup(d, "clicks-request-form") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("requestCreate", changes.requestDialog.footer), "Create") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            tests.check(!changes.requestDialog.visible, "created: the form closes")
            tests.check(req.url === "https://github.com/octo/app/pull/7" && req.number === 7, "the panel has it (" + git.requestJson + ")")
            tests.check(git.message === "Opened the pull request #7.", "and says so (" + git.message + ")")
            const button = tests.findName("requestButton")
            tests.check(button !== null && button.text === "Open", "its button opens it now")
            const checks = req.checks || {}
            tests.check(checks.state === "pending" && tests.findName("checksSummary").text === "Checks running · 1 of 2 done", "its checks show, still running (" + JSON.stringify(checks) + ")")
            tests.shot("clicks-request-panel") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The lint check fails; harlo (watching) says so.
            TestInput.writeFile(win.argValue("--cwd=") + "/../gh-checks.json", JSON.stringify([
                { name: "build", bucket: "pass", link: "https://github.com/octo/app/actions/runs/41/job/1" },
                { name: "lint", bucket: "fail", link: "https://github.com/octo/app/actions/runs/42/job/2" }]))
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const n = tests.lastNote()
            tests.check(n.kind === "ci" && n.title === "Checks failed: Pull request #7" && n.body.startsWith("1 of 2 checks failed: lint"), "harlo notifies when the checks finish (" + Workspace.lastNotification + ")")
            git.loadRequest()
            // (Looked up again, as when the panel opens: the row stays.)
            const row = tests.findName("requestRow")
            tests.check(JSON.parse(git.requestJson).state === "loading" && row !== null && row.visible, "while it's looked up again the request row stays (" + JSON.parse(git.requestJson).state + ")") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findName("checksSummary").text === "Failed: lint", "the panel shows what failed")
            tests.shot("clicks-request-failed") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("ciSend"), "Send to the agent") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const prompts = JSON.parse(chat.session.userPrompts())
            const last = prompts.length ? prompts[prompts.length - 1].text : ""
            tests.check(last.startsWith("The CI checks on pull request #7 (https://github.com/octo/app/pull/7) failed: lint.") && tests.chatText().indexOf("error: lint failed") > 0,
                      "Send to the agent sends what failed, with its log (" + last + ")")
            tests.summary(); Qt.quit() } }
    }

    // Your avatar: the classic penguin by default, at the bottom of the
    // left rail and on your turns; the rail's avatar opens the picker, and
    // a pick is saved and shows on both.
    property bool narrowChat: false
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksavatar"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            chat.composerItem.text = "hello penguin"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const turn = tests.findName("turnAvatar")
            // (A narrow chat drops the turns' avatars.)
            tests.narrowChat = turn === null
            tests.check(Theme.avatar === "classic" && (tests.narrowChat || String(turn.source).endsWith("penguin-classic.png")), "the classic penguin by default, on your turn (" + (turn ? turn.source : "narrow: no turn avatar") + ")")
            tests.clickItem(tests.findName("avatarButton"), "your avatar") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findName("avatar-glasses") !== null && tests.findName("avatar-scarf") !== null, "the picker shows the penguins")
            tests.shotWindow("clicks-avatar-picker")
            tests.clickItem(tests.findName("avatar-glasses"), "the glasses penguin") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const turn = tests.findName("turnAvatar"), rail = tests.findName("avatarButton")
            tests.check(Theme.avatar === "glasses" && JSON.parse(Workspace.look).avatar === "glasses", "the pick is saved (" + Workspace.look + ")")
            tests.check(tests.narrowChat || String(turn.source).endsWith("penguin-glasses.png"), "and shows on your turn")
            tests.check(tests.findName("avatar-glasses") === null || !tests.findName("avatar-glasses").visible, "the picker closes")
            tests.shotWindow("clicks-avatar")
            tests.summary()
            Qt.quit() } }
    }

    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksappearance"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            settingsDialog.openPage("appearance") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            tests.check(Workspace.themeFamily === "harlo", "harlo's own theme by default (" + Workspace.themeFamily + ")")
            tests.check(tests.findName("scheme-system") !== null && tests.findName("theme-Dracula") !== null, "the scheme and theme cards show")
            tests.shotWindow("clicks-appearance-top")
            tests.clickItem(tests.findName("scheme-light"), "Light") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Workspace.themeMode === "light" && Theme.isLight, "Light switches to light (" + Workspace.themeMode + ")")
            tests.clickItem(tests.findName("theme-Dracula"), "Dracula") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Workspace.themeFamily === "Dracula", "a theme's row picks the theme (" + Workspace.themeFamily + ")")
            const row = tests.findName("theme-Dracula")
            tests.check(row.current && !tests.findName("theme-Midnight").current, "and only it is chosen")
            tests.check(row.cols[0] === row.modelData.light[0], "the themes show in light, as chosen (" + row.cols[0] + ")")
            tests.scrollIntoView(row)
            tests.shotWindow("clicks-appearance-dracula-light")
            tests.clickItem(tests.findName("scheme-dark"), "Dark") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("theme-Midnight"), "Midnight") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Workspace.themeFamily === "Midnight" && String(Theme.accent) === "#b986fd", "Midnight: navy and purple (" + Workspace.themeFamily + ", " + Theme.accent + ")")
            const mid = tests.findName("theme-Midnight")
            tests.check(mid.cols[0] === mid.modelData.dark[0], "and in dark once Dark is (" + mid.cols[0] + ")")
            tests.scrollIntoView(mid)
            tests.shotWindow("clicks-appearance-themes-dark")
            settingsDialog.close()
            tests.shotWindow("clicks-appearance-midnight") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            Workspace.chooseTheme("harlo")
            settingsDialog.openPage("appearance") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const before = String(Theme.hairline)
            Workspace.chooseLook("contrast", "160")
            tests.check(Math.abs(Theme.contrast - 1.6) < 0.001 && String(Theme.hairline) !== before, "contrast spreads the colours (" + Theme.contrast + ")") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("chatWidth-wide"), "Wide") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.chatMaxWidth === 1180, "Wide widens the chat (" + Theme.chatMaxWidth + ")")
            tests.clickItem(tests.findName("diff-blue-orange"), "Blue & orange") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.diffBlueOrange && Qt.colorEqual(Theme.diffAdd, "#5b9bf0"), "diffs in blue and orange (" + Theme.diffAdd + ")")
            tests.scrollIntoView(tests.findName("contrastSlider"))
            tests.shotWindow("clicks-appearance-interface")
            tests.findName("fontSize-mono").activated(6) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.mpx(13) === 16, "the code font's size (" + Theme.mpx(13) + " px)")
            tests.findName("fontSize-ui").activated(4) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.px(14) === 16 && Theme.mpx(13) === 16, "the interface's size, the code's kept (" + Theme.px(14) + ", " + Theme.mpx(13) + ")")
            tests.clickItem(tests.findName("wrapSwitch"), "Word wrap") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Theme.wrapCode, "word wrap on")
            Workspace.chooseLook("motion", "0")
            tests.check(Theme.motion === 0, "animations off")
            tests.scrollIntoView(tests.findName("wrapSwitch"))
            tests.shotWindow("clicks-appearance-typography") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            for (const k of ["contrast", "chatWidth", "diff", "monoScale", "wrap", "motion"]) Workspace.chooseLook(k, "null")
            Workspace.chooseTextScale(1)
            tests.check(Workspace.look === "{}" && Theme.contrast === 1, "back to the defaults (" + Workspace.look + ")")
            settingsDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // Accounts (Settings → Agents): signing Claude in from harlo, against
    // a stand-in (scripts/fake-signin.py: its link and prompt, the test's
    // code only), and out again.
    SequentialAnimation {
        id: accountsTest
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksaccounts"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickItem(tests.findName("provider-claude"), "the Claude row") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const none = tests.findName("accountsNone-claude")
            const claude = JSON.parse(Workspace.providersJson()).find(p => p.id === "claude")
            tests.check(none !== null, "Claude: not signed in (" + Accounts.status("claude") + ")")
            tests.check(claude.modelCount === 0 && claude.error.indexOf("Not signed in") === 0, "so no Claude models are offered (" + claude.modelCount + ", " + claude.error + ")")
            tests.clickItem(tests.findName("signIn-claude"), "Sign in…") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(signInDialog.visible && tests.findText("Claude account") !== null && tests.findText("Anthropic Console") !== null, "the dialog offers a Claude account or the Console")
            tests.clickItem(tests.findName("signInMethod-claudeai-claudeai"), "Sign in (Claude account)") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const f = signInDialog.flow
            const link = f && f.steps.find(s => s.kind === "link")
            tests.check(link && link.url === "https://claude.com/cai/oauth/authorize?code=true&test=1" && link.open === false, "Claude Code's own page, as a link (Claude Code opens it) (" + (link ? link.url : "none") + ")")
            tests.check(f && f.ask && f.ask.kind === "code" && f.ask.optional === true, "a box for the page's code, if it shows one")
            tests.shotWindow("clicks-accounts-claude")
            tests.clickItem(tests.findName("signInAnswer"), "the code box")
            TestInput.typeText("good-code")
            tests.clickItem(tests.findName("signInSend"), "Continue") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const f = signInDialog.flow, st = JSON.parse(Accounts.status("claude"))
            tests.check(f && f.state === "done", "signed in (" + (f ? f.state + " " + f.message : "") + ")")
            tests.check(st.accounts.length === 1 && st.accounts[0].label === "me@example.com" && st.accounts[0].detail === "Claude Max", "the account shows: me@example.com · Claude Max (" + JSON.stringify(st.accounts) + ")")
            tests.clickItem(tests.findName("signInClose"), "Done") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const claude = JSON.parse(Workspace.providersJson()).find(p => p.id === "claude")
            tests.check(claude.modelCount === 17 && claude.error === "", "signed in: its 17 models are offered (" + claude.modelCount + ")")
            tests.clickItem(tests.findName("signOut-claude-claude"), "Sign out") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(JSON.parse(Accounts.status("claude")).accounts.length === 0 && tests.findName("accountsNone-claude") !== null, "signed out")
            settingsDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // iPhone notifications (Settings → Phone): through harlo's relay, and
    // the test notification goes to the phone that allowed them (written to
    // a file instead of sent: HARLO_TEST_PUSH_FILE).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickspush"
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000))
            Phone.setAccess(true)
            settingsDialog.openPage("devices") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("iPhone notifications", d) !== null && tests.findName("pushTest", d).enabled, "Settings → Phone has iPhone notifications")
            tests.check(tests.findText("Your own APNs key", d) === null && tests.findName("relayAddress", d) === null, "no key or relay address to set")
            tests.check(JSON.parse(Phone.devicesJson())[0].push === true, "the phone that allowed them is known")
            tests.check(JSON.parse(Phone.pushJson()).relay === "https://relay.harlo.run", "they go through harlo's relay (" + JSON.parse(Phone.pushJson()).relay + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("pushTest", settingsDialog.contentItem), "Send a test notification") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(Phone.pushNote === "Sent to 1 phone.", "the test notification goes to the phone (" + Phone.pushNote + ")")
            tests.shotWindow("clicks-push")
            tests.summary(); Qt.quit() } }
    }

    // After piqt's data moved over (not in the click suite): what harlo sees.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "migratecheck"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const projects = JSON.parse(Workspace.projectsJson())
            tests.log("projects:", projects.length, projects.map(p => p.name).join(", "), "| sessions in sidebar:", tests.sidebarCount(), "| theme:", Workspace.themeFamily, Workspace.themeMode)
            Qt.quit() } }
    }
    // Project icons and colours (Settings → Projects):
    // the sidebar shows a grey folder (Other sessions too) and the expand
    // arrow when hovered; from the project's menu, pick an icon and a
    // colour, and rename; the sidebar follows.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksprojects"
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            // A session outside any project, so Other sessions shows.
            Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const icon = tests.findName("projectIcon-repo"), other = tests.findName("projectIcon-Other sessions")
            tests.check(icon !== null && icon.icon === "" && other !== null, "a new project, and Other sessions, show the folder")
            TestInput.moveTo(icon.mapToItem(null, 4, 4).x, icon.mapToItem(null, 4, 4).y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("projectIcon-repo") === null && tests.findText("▾") !== null, "hovered, the arrow takes the icon's place")
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Project settings…") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(settingsDialog.page === "projects" && tests.findName("projectName", d).text === "repo", "Project settings opens on the project")
            tests.clickItem(tests.findName("projectIconPick-rocket", d), "the rocket") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findName("projectIcon-repo").icon === "rocket", "the sidebar shows the rocket")
            tests.clickItem(tests.findName("projectColor-keyword", settingsDialog.contentItem), "a colour") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findName("projectIcon-repo").color === "keyword", "and its colour")
            tests.clickItem(tests.findName("projectName", settingsDialog.contentItem), "the name")
            TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("Rocket app"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findText("Rocket app") !== null && tests.findName("projectIcon-Rocket app") !== null, "renaming shows in the sidebar")
            tests.shotWindow("clicks-projects")
            tests.clickItem(tests.findName("projectIconPick-none", settingsDialog.contentItem), "the folder") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findName("projectIcon-Rocket app").icon === "", "the folder again")
            settingsDialog.close()
            // A second project (the test's own folder): it goes first.
            Workspace.addProject(win.selftestDir) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const names = () => JSON.parse(Workspace.projectsJson()).map(p => p.name)
            const second = win.selftestDir.split("/").filter(s => s).pop()
            tests.check(names()[0] === second && names()[1] === "Rocket app", "a new project goes first (" + names().join(", ") + ")")
            // A session in the lower project doesn't move it up.
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[1].id, "demo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const second = win.selftestDir.split("/").filter(s => s).pop()
            const top = tests.findName("projectTitle-" + second), low = tests.findName("projectTitle-Rocket app")
            tests.check(top !== null && low !== null && top.mapToItem(null, 0, 0).y < low.mapToItem(null, 0, 0).y, "a new session in the lower project leaves it where it is")
            // Dragged by its header above the first: it goes first.
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            TestInput.moveTo(from.x, from.y); TestInput.press(from.x, from.y, true)
            for (let i = 1; i <= 4; i++) TestInput.moveTo(from.x, from.y - 3 * i) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Lifted but not past another project's header: no line.
            const line = tests.findName("projectDropLine"), ghost = tests.findName("projectDragGhost")
            tests.check(ghost !== null && ghost.visible && (line === null || !line.visible), "lifted where it would stay, no line shows")
            const low = tests.findName("projectDragGhost"), top = tests.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop())
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            tests.dragPointerY = to.y
            for (let i = 1; i <= 12; i++) TestInput.moveTo(from.x, from.y + (to.y - from.y) * i / 12) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const line = tests.findName("projectDropLine"), ghost = tests.findName("projectDragGhost")
            const first = tests.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop())
            const ly = line ? line.mapToItem(null, 0, line.height / 2).y : -1, fy = first.mapToItem(null, 0, 0).y
            tests.check(line !== null && line.visible && ly < fy + 6 && ly > fy - 20, "past the first project's header, the line shows just above it (" + Math.round(ly) + " over " + Math.round(fy) + ")")
            tests.check(line !== null && line.y >= 0 && line.y + line.height <= line.parent.height, "and inside the list, at its very top too (" + (line ? line.y.toFixed(1) : "") + ")")
            tests.check(ghost !== null && ghost.visible && tests.findText("Rocket app", ghost) !== null, "and a copy of its header follows the pointer")
            const below = first.mapToItem(null, 0, first.height).y, gy = ghost ? ghost.mapToItem(null, 0, 0).y : -1
            tests.check(line !== null && ghost !== null && line.z > ghost.z && gy >= below && gy - tests.dragPointerY <= 36, "the copy hangs just under the name it lands above, close to the pointer, the line drawn over it (" + Math.round(gy) + " under " + Math.round(below) + ", pointer " + Math.round(tests.dragPointerY) + ")")
            // Nothing under the pointer lights up as if it took the drop.
            const lit = []
            const walk = item => { for (const c of item.children) { if (c.hovered === true && c.objectName !== "projectDragGhost") lit.push(c.objectName || String(c)); walk(c) } }
            walk(sidebar)
            tests.check(lit.length === 0, "rows don't light up under the dragged project (" + lit.join(", ") + ")")
            const rows = tests.findAllNames("sessionRow", sidebar).filter(r => r.visible)
            tests.check(rows.length > 0 && rows.every(r => r.opacity < 0.6), "the sessions fade while it's dragged: only the headers take it (" + rows.map(r => r.opacity).join(", ") + ")")
            // (findName skips what's faded out: its title isn't to be found.)
            tests.check(tests.findName("projectTitle-Rocket app") === null && tests.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop()) !== null, "its place is left empty")
            tests.shotWindow("clicks-projects-drag")
            // A session starting mid-drag (the rows made again) doesn't drop it.
            tests.sessionsBefore = tests.sidebarCount()
            Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const ghost = tests.findName("projectDragGhost")
            tests.check(ghost !== null && ghost.visible && tests.sidebarCount() === tests.sessionsBefore, "a session starting mid-drag leaves the rows as they are, still lifted")
            const top = tests.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop()), to = top.mapToItem(null, 10, -6)
            TestInput.press(to.x, to.y, false) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.sidebarCount() === tests.sessionsBefore + 1, "dropped, the rows catch up (" + tests.sidebarCount() + ")")
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            tests.check(names[0] === "Rocket app", "dropped above, it's first (" + names.join(", ") + ")")
            const line = tests.findName("projectDropLine"), ghost = tests.findName("projectDragGhost")
            tests.check((line === null || !line.visible) && (ghost === null || !ghost.visible), "and the line and the copy go")
            // Its menu: Move down, and Move up is off at the top.
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.place = Workspace.projectPlace(p.id)
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = sidebar.projectMenu.contentItem, up = tests.findName("projectMoveUp", m), down = tests.findName("projectMoveDown", m)
            tests.check(up !== null && !up.enabled && down !== null && down.enabled, "at the top, Move up is off")
            tests.clickText("Move down") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            tests.check(names[1] === "Rocket app", "Move down moves it a place (" + names.join(", ") + ")")
            // Esc calls a drag off: it stays where it was.
            const low = tests.findName("projectTitle-Rocket app"), top = tests.findName("projectTitle-" + names[0])
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            TestInput.moveTo(from.x, from.y); TestInput.press(from.x, from.y, true)
            for (let i = 1; i <= 12; i++) TestInput.moveTo(from.x, from.y + (to.y - from.y) * i / 12) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const line = tests.findName("projectDropLine")
            tests.check(line !== null && line.visible, "dragged again, the line shows")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const ghost = tests.findName("projectDragGhost"), line = tests.findName("projectDropLine")
            tests.check((ghost === null || !ghost.visible) && (line === null || !line.visible), "Esc calls the drag off")
            const top = tests.findName("projectTitle-" + JSON.parse(Workspace.projectsJson())[0].name), to = top.mapToItem(null, 10, -6)
            TestInput.press(to.x, to.y, false) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            tests.check(names[1] === "Rocket app", "and letting go then leaves it where it was (" + names.join(", ") + ")")
            // From the keyboard: its header focused, Alt+Shift+↑ moves it up.
            let h = tests.findName("projectTitle-Rocket app")
            while (h && h.projectId === undefined) h = h.parent
            tests.check(h !== null && h.focusPolicy === Qt.TabFocus && h.Accessible.name.startsWith("Rocket app, project"), "a project's header takes the keys from Tab, named for a screen reader (" + (h ? h.Accessible.name : "") + ")")
            h.forceActiveFocus(Qt.TabFocusReason)
            TestInput.key(Qt.Key_Up, Qt.AltModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            const f = win.activeFocusItem
            tests.check(names[0] === "Rocket app" && f && f.projectId !== undefined && f.title === "Rocket app" && f.background.border.width === 2, "Alt+Shift+↑ moves it up, and its header keeps the keys, ringed (" + names.join(", ") + "; " + (f ? f.title : "none") + ")")
            tests.shotWindow("clicks-projects-keys")
            TestInput.key(Qt.Key_F10, Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(sidebar.projectMenu.opened && sidebar.projectMenu.name === "Rocket app", "Shift+F10 opens its menu")
            sidebar.projectMenu.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The move's notice offers Undo, and Ctrl+Z (not typing) takes it.
            const undo = tests.findName("noticeUndo")
            tests.check(undo !== null && Workspace.notice.startsWith("Rocket app moved above "), "a move's notice offers Undo (" + Workspace.notice + ")")
            tests.shotWindow("clicks-projects-undo")
            const f = win.activeFocusItem
            tests.check(f && f.cursorPosition === undefined, "the keys aren't in a text field (" + f + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            tests.check(names[1] === "Rocket app" && Workspace.notice === "" && tests.findName("noticeUndo") === null, "Ctrl+Z puts it back, and the notice goes (" + names.join(", ") + ")")
            tests.summary(); Qt.quit() } }
    }

    // Pair a device sized to its content (its first screen once spilled out
    // of a too-short dialog), and in a very short window: it fits, and its
    // content scrolls.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksphoneshort"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.width = Number(win.argValue("--winwidth=") || 900); win.height = 700; settingsDialog.openPage("devices") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("pairDeviceSettings", settingsDialog.contentItem), "Pair a device…") }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const d = pairDeviceDialog, scroller = tests.findName("pairScroller")
            tests.check(d.visible && scroller !== null && scroller.height >= scroller.contentHeight - 1 && scroller.contentHeight > 150,
                      "the dialog is as tall as its content (" + (scroller ? Math.round(scroller.height) + " for " + Math.round(scroller.contentHeight) : "") + ")")
            tests.shotPopup(d, "clicks-phone-sized") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.height = 260 }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = pairDeviceDialog, scroller = tests.findName("pairScroller")
            const top = d.background.parent.mapToItem(null, 0, 0).y
            tests.check(top >= 0 && top + d.height <= win.height + 1, "in a short window it fits (" + Math.round(top) + "+" + Math.round(d.height) + " of " + win.height + ")")
            tests.check(scroller.contentHeight > scroller.height + 1, "and its content scrolls (" + Math.round(scroller.contentHeight) + " in " + Math.round(scroller.height) + ")")
            tests.shotPopup(d, "clicks-phone-short") } }
        PauseAnimation { duration: 400 }
        // A port of its own (the running harlo may have 7676).
        ScriptAction { script: { Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000)); tests.clickItem(tests.findName("phoneTurnOn"), "Turn on (scrolled into view)") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.check(Phone.enabled, "a button below the fold is scrolled to and clicked")
            pairDeviceDialog.close(); settingsDialog.close(); win.height = 700 } }
        // Opened again with phone access already on: the code is there at
        // once, for this network (it once needed another address picked
        // and back).
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickItem(tests.findName("pairDeviceButton"), "phone button") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const qr = tests.findName("pairQr"), link = tests.findName("pairLink")
            tests.check(qr !== null && String(qr.source).startsWith("data:image/svg+xml") && link !== null && /^http:\/\/[0-9.]+:[0-9]+\/#p=/.test(link.text),
                      "reopened: the QR code shows straight away (" + (link ? link.text.substring(0, 32) : "none") + ")")
            tests.summary(); Qt.quit() } }
    }
    // Runs of tool calls sit under a summary line that folds them (a saved session that
    // scripts/click-tests.sh writes); session rows are one line.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksruns"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            const row = tests.findText("Fix the login redirect loop", sidebar)
            let d = row
            while (d && d.rowKey === undefined) d = d.parent
            tests.check(d && d.height <= 46 && d.ToolTip.text === "", "the session row is one line, with no hover tip (" + (d ? d.height : "none") + ")")
            // Never opened in harlo: its icon is still its model's, from its file.
            const logo = d ? tests.findAllNamed("sessionMaker", d, [])[0] : null
            const tag = d ? tests.findAllNamed("sessionAgentTag", d, [])[0] : null
            tests.check(logo && logo.visible && logo.provider === "openai" && tag && tag.visible, "a session not opened yet shows its model's logo, Harlo as its badge (" + (logo ? logo.provider : "none") + ")")
            tests.clickItem(row, "the session") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const t = chat.transcript
            const run = tests.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t)
            tests.check(run !== null, "the second run's summary")
            tests.check(tests.findContaining("npm test -- auth", t) !== null && tests.findAllNames("runBox", t).length > 0, "a finished run starts open, in its box")
            const whats = tests.findAllNames("toolWhat", t)
            const npm = whats.find(l => l.plain === "npm test -- auth")
            let card = npm
            while (card && card.echoed === undefined) card = card.parent
            tests.check(card && card.echoed && !card.outInWell, "one line of output that the line already says ('48 passed') isn't shown again")
            const guard = whats.find(l => l.plain === "src/auth/guard.ts")
            tests.check(guard && guard.font.family === Theme.mono && npm.font.family === Theme.mono, "paths and commands in code type")
            tests.shotWindow("clicks-runs-open")
            tests.clickItem(run, "the summary") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = chat.transcript
            tests.check(tests.findContaining("npm test -- auth", t) === null, "clicking it folds the run")
            tests.check(t.debugFoldedRuns() === 1, "only that run")
            tests.shotWindow("clicks-runs-folded")
            tests.clickItem(tests.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t), "the summary again") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findContaining("npm test -- auth", chat.transcript) !== null, "and again opens it")
            chat.transcript.positionAt(0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = chat.transcript
            const read = tests.findAllNames("codeView", t).find(v => v.lines.some(l => l.h.indexOf("guard") >= 0))
            tests.check(read && read.lines[0].n === "1" && read.lines[0].p.endsWith("/src/auth/guard.ts") && tests.findAllNames("codeNumber", read).length === 1,
                      "Harlo's plain read numbered from its first line, a link to the file (" + (read ? JSON.stringify(read.lines[0].n) : "none") + ")")
            tests.summary(); Qt.quit() } }
    }
    // Glass menus and dialogs over a busy chat, for looking at (not in the
    // click suite): a context menu, a dialog, Settings with a font list
    // open, and the menu again with glass off.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "glassshots"
        ScriptAction { script: {
            Workspace.newSession(-1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--glass-opacity=")) Workspace.chooseGlass(true, Number(win.argValue("--glass-opacity=")))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.session.loadDemoHistory(40) }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { sidebar.sessionMenu.sessionTitle = "A session"; sidebar.sessionMenu.live = false; sidebar.sessionMenu.popup(win.contentItem, 520, 260) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("glass:", Theme.glass, "backdrop:", Theme.backdrop !== null); tests.shotWindow("glass-menu") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { sidebar.sessionMenu.close(); pairDeviceDialog.open() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.shotWindow("glass-dialog") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { pairDeviceDialog.close(); settingsDialog.openPage("appearance") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: tests.shotWindow("glass-settings") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("fontPick-Interface font", settingsDialog.contentItem), "font list") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.shotWindow("glass-settings-combo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { settingsDialog.close(); Workspace.chooseGlass(false, Workspace.glassOpacity) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: sidebar.sessionMenu.popup(win.contentItem, 520, 260) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shotWindow("glass-off-menu") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }
    // Phone host for scripts/phone-e2e.sh (not in the click suite): phone
    // access on (--phone-port=), a project (--project=), a demo session, and a pairing link for
    // 127.0.0.1 (or through the relay at --relay=) written to pair-url.txt;
    // runs until --stay= seconds pass.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "phonehost"
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            if (win.argValue("--project=")) Workspace.addProject(win.argValue("--project="))
            Phone.setAccessPort(Number(win.argValue("--phone-port=") || 7676)); Phone.setAccess(true)
            // (Without one: --nodemo=1, as the iOS app's recording starts.)
            if (win.argValue("--nodemo=") !== "1") Workspace.newSession(-1, "demo")
            if (win.argValue("--fake-claude=")) Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            if (win.argValue("--relay=")) Phone.useRelay(true) } }
        PauseAnimation { duration: 1500 }
        // A long session (--history=N exchanges before it), as a phone may open.
        ScriptAction { script: if (win.argValue("--history=")) chat.session.loadDemoHistory(Number(win.argValue("--history="))) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.log("phone running:", Phone.running, Phone.error, "relay:", Phone.relayState, Phone.relayError)
            testFile.path = win.selftestDir + "/pair-url.txt"
            testFile.save(JSON.parse(Phone.pairingJson(win.argValue("--relay=") ? "relay" : "127.0.0.1")).url) } }
        PauseAnimation { duration: Number(win.argValue("--stay=") || 60) * 1000 }
        ScriptAction { script: { tests.log("devices:", Phone.devicesJson()); Qt.quit() } }
    }
    // A phone paired before, a firewall (a stand-in ufw) that blocks every
    // address: phone access on shows a notice per address in the corner,
    // and its Allow opens that one.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksphonefirewall"
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000))
            Phone.setAccess(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const addresses = JSON.parse(Phone.addressesJson())
            tests.blockedCount = addresses.length
            tests.check(Phone.running && addresses.length > 0, "phone access runs (" + addresses.map(a => a.label).join(", ") + ")")
            tests.check(addresses.every(a => tests.findName("phoneFirewall-" + a.address) !== null), "a notice for each blocked address")
            const tail = addresses.find(a => String(a.label).startsWith("Tailscale"))
            if (tail) tests.check(tests.findText("Your phone can't reach Harlo over Tailscale") !== null, "the Tailscale one says so")
            // (The notices themselves: they sit above the page.)
            tests.grabItem(tests.findName("phoneFirewallToast"), "clicks-phone-firewall-toast")
            tests.clickItem(tests.findName("phoneFirewallAllow"), "Allow") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const left = JSON.parse(Phone.blockedJson()).length
            tests.check(left === tests.blockedCount - 1 && Phone.firewallError === "", "Allow opens that address (" + left + " left, " + Phone.firewallError + ")")
            tests.summary(); Qt.quit() } }
    }
    // (phonehost, --workflow-trigger=<file>: a workflow run starts in the
    // project once the file is there, for the phone's recording.)
    TextFile { id: workflowTrigger }
    Timer {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "phonehost" && win.argValue("--workflow-trigger=") !== "" && !tests.workflowTriggered
        interval: 500
        repeat: true
        onTriggered: {
            if (workflowTrigger.state === "text") {
                tests.workflowTriggered = true
                const p = JSON.parse(Workspace.projectsJson())[0]
                const why = Workspace.startWorkflow(p.id, workflowTrigger.text)
                tests.log("workflow started:", why === "" ? "yes" : why)
                return
            }
            workflowTrigger.load(win.argValue("--workflow-trigger="))
        }
    }
    property bool workflowTriggered: false
    property int blockedCount: 0
    property bool otherAddress: false
    // The real thing, read-only (not in the click suite): plan limits via harlo.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "quotareal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: Workspace.refreshQuota() }
        PauseAnimation { duration: 25000 }
        ScriptAction { script: { const d = JSON.parse(Workspace.quotaJson); tests.log("accounts:", d.accounts.length, "error:", d.error, "first:", d.accounts.length ? d.accounts[0].provider + " " + d.accounts[0].plan + " " + d.accounts[0].windows.map(w => w.label + " " + Math.round(w.usedFraction * 100) + "%").join(", ") : "-"); Qt.quit() } }
    }

    // Committed but not pushed, said in the commit box with Push beside it;
    // and Commit and push. `--cwd`: a repo whose main is one commit ahead
    // of its (local, bare) origin, with an untracked notes.md.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickscommitpush"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const line = tests.findName("unpushed")
            tests.check(git.ahead === 1 && git.upstream !== "", "one commit ahead of its upstream (" + git.ahead + ", " + git.upstream + ")")
            tests.check(line !== null && tests.findText("1 commit not pushed", line) !== null, "the commit box says it isn't pushed")
            tests.check(tests.findName("commitPush") !== null && tests.findText("Commit & push") !== null, "Commit and push beside Commit all")
            tests.shotWindow("clicks-commit-unpushed")
            tests.clickItem(tests.findName("unpushedPush"), "Push") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.check(git.ahead === 0 && tests.findName("unpushed") === null, "Push sends it: the line goes (" + git.ahead + ", " + git.message + ")")
            tests.clickItem(tests.findName("commitMessage"), "commit message")
            TestInput.typeText("add notes") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("commitPush"), "Commit and push") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(changes.changedCount === 0 && changes.commitText === "", "Commit and push commits (" + changes.changedCount + ", " + git.message + ")")
            tests.check(git.ahead === 0 && tests.findName("unpushed") === null, "…and pushes: nothing left to push (" + git.ahead + ")")
            tests.shotWindow("clicks-commit-pushed")
            tests.summary(); Qt.quit() } }
    }

    // Branch switching by real clicks and typing (steps C–G of the manual
    // test). `--cwd`: a repo on main with an untracked notes.md and a
    // branch checked out in another worktree (`wt-branch`).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "branchclicks"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        // A: create feature-x from the picker
        ScriptAction { script: { tests.check(git.branch === "main" && changes.changedCount === 1, "starts on main with notes.md untracked"); tests.clickItem(tests.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const p = changes.branchPicker, x = p.background.parent.mapToItem(null, 0, 0).x
            tests.check(p.opened, "chip click opens the picker")
            tests.check(x >= 0 && x + p.width <= win.width, "picker inside the window (" + Math.round(x) + "–" + Math.round(x + p.width) + " of " + win.width + ")")
            tests.check(p.items.some(i => i.name === "wt-branch"), "picker lists the worktree branch")
            TestInput.typeText("feature-x") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.check(changes.branchPicker.items[0].kind === "create", "typing offers Create branch"); tests.shotPopup(changes.branchPicker, "clicks-create"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.check(git.branch === "feature-x" && changes.changedCount === 1, "Enter creates feature-x and keeps notes.md (" + git.branch + ", " + changes.changedCount + ")") }
        // B: commit notes.md on feature-x
        ScriptAction { script: { tests.clickItem(tests.findName("commitMessage"), "commit message"); TestInput.typeText("add notes") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Commit all") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: tests.check(changes.changedCount === 0, "commit leaves nothing to commit (" + git.message + ")") }
        // C: clean switches, no dialog
        ScriptAction { script: tests.clickItem(tests.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("main", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.check(git.branch === "main" && !changes.switchDialog.visible, "clean switch to main, no dialog"); tests.check(Workspace.fileStamp(win.argValue("--cwd=") + "/notes.md") === "", "notes.md not on main") } }
        ScriptAction { script: tests.clickItem(tests.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("feature-x", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.check(git.branch === "feature-x", "back on feature-x"); tests.check(Workspace.fileStamp(win.argValue("--cwd=") + "/notes.md") !== "", "notes.md back") ; testFile.load(win.argValue("--cwd=") + "/README.md") } }
        PauseAnimation { duration: 500 }
        // D: change README, switch bringing it along
        ScriptAction { script: { testFile.save(testFile.text + "world\n"); git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.check(changes.changedCount === 1, "README.md modified"); tests.clickItem(tests.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("main", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.check(changes.switchDialog.visible && changes.switchDialog.title === "Switch to main?", "dirty switch asks first"); tests.shotPopup(changes.switchDialog, "clicks-switch-dialog"); } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Bring changes along") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.check(git.branch === "main" && changes.changedCount === 1 && !changes.switchDialog.visible, "Bring changes along: on main, README still modified") }
        // E: stash and switch, then pop
        ScriptAction { script: tests.clickItem(tests.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("feature-x", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickText("Stash and switch") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { tests.check(git.branch === "feature-x" && changes.changedCount === 0 && git.stashCount === 1, "Stash and switch: on feature-x, clean, 1 stash (" + git.stashCount + ")"); tests.clickItem(tests.findName("stashButton"), "stash button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { const e = changes.stashesDialog.entries; tests.check(changes.stashesDialog.visible && e.length === 1 && e[0].message === "harlo: before switching to feature-x" && e[0].branch === "main", "stash list shows it (" + JSON.stringify(e.map(x => x.message + " on " + x.branch)) + ")"); tests.shotPopup(changes.stashesDialog, "clicks-stashes"); } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickText("Pop") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: tests.check(git.stashCount === 0 && changes.changedCount === 1 && changes.stashesDialog.entries.length === 0, "Pop restores README and drops the stash") }
        // F: stash by hand with a message, then drop
        ScriptAction { script: { tests.clickItem(tests.findName("stashMessage"), "stash message"); TestInput.typeText("try later") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Stash 1 file") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { const e = changes.stashesDialog.entries; tests.check(git.stashCount === 1 && e.length === 1 && e[0].message === "try later" && changes.changedCount === 0, "manual stash 'try later'"); tests.clickText("Drop") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickText("Discard") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { tests.check(git.stashCount === 0 && changes.stashesDialog.entries.length === 0, "Drop + Discard removes it"); tests.clickText("Close") } }
        PauseAnimation { duration: 500 }
        // G: a branch git refuses (checked out in another worktree)
        ScriptAction { script: { tests.check(!changes.stashesDialog.visible, "Close closes the stashes dialog"); tests.clickItem(tests.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("wt-branch", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.check(git.branch === "feature-x" && git.error && git.message !== "", "git's refusal shows as an error, branch unchanged (" + git.message.split("\n")[0] + ")"); tests.shot("clicks-error") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("checks:", tests.checksPassed, "passed,", tests.checksFailed, "failed"); Qt.quit() } }
    }

    // Branch switching and stashes against a scratch repo (`--cwd`, dirty, with a `feature` branch).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "gitbranches"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("start: branch", git.branch, "changed", changes.changedCount, "stashes", git.stashCount); changes.branchPicker.open() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { const x = changes.branchPicker.background.parent.mapToItem(null, 0, 0).x; tests.log("picker: left", Math.round(x), "right", Math.round(x + changes.branchPicker.width), "window", win.width) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { const p = changes.branchPicker; tests.log("picker items:", p.items.map(i => i.kind + ":" + i.name + (i.current ? "*" : "")).join(" | ")); tests.shotPopup(p, "branch-picker") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: changes.branchPicker.query = "fea" }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.log("filtered:", changes.branchPicker.items.map(i => i.kind + ":" + i.name).join(" | ")); tests.shotPopup(changes.branchPicker, "branch-picker-search") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.branchPicker.pick(changes.branchPicker.items.findIndex(i => i.name === "feature")) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("dialog open:", changes.switchDialog.visible, "dirty:", changes.switchDialog.dirty); tests.shotPopup(changes.switchDialog, "branch-switch-dirty") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.switchDialog.accept() }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after stash+switch: branch", git.branch, "changed", changes.changedCount, "stashes", git.stashCount, "msg:", git.message); tests.shot("branch-switched") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.stashesDialog.open() }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("stash entries:", JSON.stringify(changes.stashesDialog.entries)); tests.shotPopup(changes.stashesDialog, "stashes") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: git.stashApply(0, true) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after pop: changed", changes.changedCount, "stashes", git.stashCount, "entries", changes.stashesDialog.entries.length, "msg:", git.message); tests.shotPopup(changes.stashesDialog, "stashes-popped") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: git.stashPush("selftest stash") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after push: changed", changes.changedCount, "stashes", git.stashCount, "entries", JSON.stringify(changes.stashesDialog.entries)); changes.stashesDialog.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.requestSwitch("topic/new-idea", false, true) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after create: branch", git.branch, "msg:", git.message); changes.branchPicker.open() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("picker after:", changes.branchPicker.items.map(i => i.name + (i.current ? "*" : "")).join(" | ")); changes.branchPicker.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Worktrees against a scratch repo (`--cwd`): create with setup, merge, remove.
    property string wtKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "worktree"
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.setProjectSetup(1, "# commit something, as an agent would\necho feature > feature.txt\ngit add feature.txt && git -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -qm 'feature work'")
                Workspace.newWorktreeSession(1, "harlo/test-branch", "main", "demo")
            }
        }
        PauseAnimation { duration: 4000 }
        ScriptAction {
            script: {
                tests.wtKey = Workspace.activeKey
                tests.log("notice:", Workspace.notice, "| error:", Workspace.noticeError, "| active:", tests.wtKey, "| cwd:", chat.session.cwd)
                tests.log("info:", Workspace.worktreeInfo(tests.wtKey))
                let rows = []
                for (let i = 0; Workspace.keyAt(i) !== ""; i++) rows.push(Workspace.keyAt(i))
                tests.log("sidebar keys:", rows.join(" , "))
                tests.shot("worktree-created")
                Workspace.mergeWorktree(tests.wtKey)
            }
        }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { tests.log("after merge:", Workspace.notice, "| error:", Workspace.noticeError); Workspace.removeWorktree(tests.wtKey, true, false) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: tests.log("after remove:", Workspace.notice, "| error:", Workspace.noticeError, "| active:", Workspace.activeKey) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Live resize with the changes panel open: shrink, then grow.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "resize"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("start", win.width, "side", sidebar.width, "chat", chat.width, "changes", changes.width, "x", changes.x); win.width = 853 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("853:", "side", sidebar.width, "chat", chat.width, "changes", changes.width, "right edge", changes.x + changes.width); tests.shot("resize-853") } }
        ScriptAction { script: win.width = 1100 }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("1100:", "side", sidebar.width, "chat", chat.width, "changes", changes.width, "right edge", changes.x + changes.width); tests.shot("resize-1100") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Clipboard round trip through the transcript's copy helper.
    TextEdit { id: pasteProbe; visible: false }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clipboard"
        PauseAnimation { duration: 500 }
        ScriptAction { script: chat.transcript.copyToClipboard("fn main() {\n    println!(\"copied\");\n}") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { pasteProbe.paste(); tests.log("pasted:", JSON.stringify(pasteProbe.text)) } }
        ScriptAction { script: Qt.quit() }
    }

    // Scrolling up a long transcript must stay put (scrollbar-style scroll,
    // which bypasses flick/drag/wheel events).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "scrollup"
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.session.loadDemoHistory(300) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("loaded, follow:", chat.transcript.follow, "atEnd:", chat.transcript.atEnd); chat.transcript.scrollBy(-3000) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.log("after 1st scroll up: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd); chat.transcript.scrollBy(-20000) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.log("after 2nd scroll up: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd); tests.shot("scrolled-up") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.log("1.5 s later: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd) }
        ScriptAction { script: Qt.quit() }
    }

    // Terminal tabs: several shells per folder, exit closes one, per-folder sets.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "termtabs"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.toggleTerminal() }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("opened: tabs", terminal.tabs.length, "running", terminal.current.running); win.newTerminalTab() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.log("after Ctrl+Shift+T: tabs", terminal.tabs.length, "active", terminal.entry.active, "titles", terminal.tabs.map(t => terminal.tabTitle(t)).join(" | ")); terminal.current.session.paste("cd /tmp"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { win.newTerminalTab() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { tests.log("three tabs:", terminal.tabs.length, "active", terminal.entry.active); tests.shot("termtabs-three") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { terminal.current.session.paste("exit"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("after exit: tabs", terminal.tabs.length, "active", terminal.entry.active); terminal.activate(0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.log("activated 0: active", terminal.entry.active, "focused", terminal.terminalFocused); win.closeTerminalTab() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("after Ctrl+Shift+W: tabs", terminal.tabs.length, "title", terminal.tabTitle(terminal.current)); tests.shot("termtabs-closed") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("other folder:", terminal.folder === Workspace.homeDir(), "tabs", terminal.tabs.length, "folders", Object.keys(terminal.folders).length); terminal.focusTerminal() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { terminal.current.session.paste("exit"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("last tab exit: tabs", terminal.tabs.length, "running", terminal.current.running, "exitText", terminal.current.session.exitText); tests.shot("termtabs-last-exit") } }
        ScriptAction { script: Qt.quit() }
    }

    // Tool approvals on their tool row (demo harness), approve then deny.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "approval"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("fast: please approve the listing", "") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { const s = chat.session; tests.log("request:", s.requestMethod, "approval:", s.requestIsApproval, "row:", s.approvalRow, "index:", s.indexOfKey(s.approvalRow)); tests.shot("approval-inline") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Approve") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { const s = chat.session; tests.log("after approve: hasRequest", s.hasRequest, "row:", JSON.stringify(s.approvalRow), "status", s.status); tests.shot("approval-approved") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.submit("fast: approve again", "") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollBy(-3000); tests.log("second row:", chat.session.approvalRow) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shot("approval-scrolled") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Deny") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.log("after deny: hasRequest", chat.session.hasRequest, "status", chat.session.status) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shot("approval-denied") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // The prompt rail: ticks, the hover list, jumping and stepping.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "promptrail"
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.session.loadDemoHistory(60) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { const t = chat.transcript; tests.log("prompts:", t.prompts.length, "active:", t.activePrompt, "rail visible:", t.rail.visible, "start:", t.rail.start, "first:", JSON.stringify(t.prompts[0])); tests.shot("rail-idle") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.openPromptRail() }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("keyboard:", chat.transcript.rail.keyboard, "highlighted:", chat.transcript.rail.highlighted); tests.shot("rail-keyboard") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.moveTo(12) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shot("rail-scrubbed") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.select(12) }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { const t = chat.transcript; tests.log("after jump to 12: active", t.activePrompt, "follow", t.follow, "keyboard", t.rail.keyboard, "start", t.rail.start); tests.shot("rail-jumped") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.stepPrompt(-1) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("Alt+Up: active", chat.transcript.activePrompt); chat.transcript.stepPrompt(1) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("Alt+Down: active", chat.transcript.activePrompt); chat.transcript.scrollBy(300) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.log("partway in: active", chat.transcript.activePrompt); chat.transcript.stepPrompt(-1) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("Alt+Up from partway: active", chat.transcript.activePrompt); chat.transcript.rail.moveTo(59) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("end of list start:", chat.transcript.rail.start); tests.shot("rail-end") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { chat.transcript.rail.close(); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.moveTo(30) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shot("rail-light") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.chooseThemeMode("dark"); win.width = 700 } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("narrow: rail width", chat.transcript.rail.width, "gutter", chat.transcript.gutter); tests.shot("rail-narrow") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Opens a saved session (`--open=<file>`) at its top, for a look.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "opensaved"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("rows:", chat.transcript.count); chat.transcript.positionAt(Number(win.argValue("--row=") || "0")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: tests.shot("opensaved") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: Qt.quit() }
    }

    // The memory benchmark (scripts/bench/memory-long.sh samples the process
    // at each MEMMARK): a long saved session (`--open=`) opened, `--turns=`
    // real-sized turns by the stand-in claude (`--fake-claude=`), the
    // pictures in `--cwd=`/pics opened and closed `--cycles=` times, and the
    // sessions in `--more=` (|-separated) opened beside it and closed.
    // The Changes panel follows the agent's edits as each tool call ends,
    // not at the next poll or the run's end.
    property real liveWritten: 0
    SequentialAnimation {
        id: changesLiveSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlochanges"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(changesLiveSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(!git.files.some(f => f.startsWith("live-change.txt\t")), "the file isn't there yet (" + git.files + ")")
            win.gitPollOff = true
            tests.sendByKeys("write and wait") } }
        // (Its write asks first.)
        ScriptAction { script: tests.waitUntil(changesLiveSteps, () => chat.session.hasRequest, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.clickText("Approve", chat.transcript); tests.liveWritten = Date.now(); tests.log("window in front:", win.active) } }
        ScriptAction { script: tests.waitUntil(changesLiveSteps, () => git.files.some(f => f.startsWith("live-change.txt\t")), 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const ms = Date.now() - tests.liveWritten
            tests.check(ms < 2000 && chat.session.status === "busy", "the Changes panel lists the agent's new file " + ms + " ms after its write was approved, while it still works (" + chat.session.status + ")")
            tests.shotWindow("clicks-changes-live") } }
        // (Then its slow step, asked too.)
        ScriptAction { script: tests.waitUntil(changesLiveSteps, () => chat.session.hasRequest, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Approve", chat.transcript) }
        ScriptAction { script: tests.waitUntil(changesLiveSteps, () => chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }
    property int memTurn: 0
    property int memRowsBefore: 0
    property var memMore: (win.argValue("--more=") || "").split("|").filter(f => f !== "")
    function memMark(name) { tests.log("MEMMARK " + name) }
    function memShowing(rows) { return chat.session.key !== "" && chat.transcript.listView.count >= rows && chat.session.status !== "running" }
    SequentialAnimation {
        id: memLongSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "memlong"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.memMark("start") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.memShowing(1000), 120000) }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { tests.memMark("opened"); tests.log("rows:", chat.transcript.listView.count); if (win.argValue("--warm=")) Qt.quit(); else Workspace.setSetting("claude_path", win.argValue("--fake-claude=")) } }
        PauseAnimation { duration: 500 }
        SequentialAnimation {
            loops: Number(win.argValue("--turns=") || 30)
            ScriptAction { script: { tests.memTurn++; tests.memRowsBefore = chat.transcript.listView.count; tests.sendByKeys("heavy turn " + tests.memTurn) } }
            PauseAnimation { duration: 300 }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => chat.session.status === "idle" && chat.transcript.listView.count >= tests.memRowsBefore + 6, 120000) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: if (tests.memTurn % 25 === 0) tests.memMark("turn-" + tests.memTurn) }
        }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.memMark("after-turns") }
        PauseAnimation { duration: 1500 }
        SequentialAnimation {
            loops: Number(win.argValue("--cycles=") || 5)
            ScriptAction { script: tests.imgOpen("img1.png") }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.imgReady("img1.png"), 30000) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.imgOpen("img9.png") }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.imgReady("img9.png"), 30000) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.imgOpen("img17.png") }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.imgReady("img17.png"), 30000) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.imgBig = TestInput.readFile(tests.imgDir + "/big.png") !== "" ? "big.png" : "big.jpg" }
            ScriptAction { script: tests.imgOpen(tests.imgBig) }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.imgReady(tests.imgBig), 30000) }
            PauseAnimation { duration: 500 }
            ScriptAction { script: tests.memMark("pictures-open") }
            PauseAnimation { duration: 1500 }
            ScriptAction { script: {
                Workspace.tabs.map(t => t.split("\t")[0]).filter(k => k.startsWith("file:" + tests.imgDir + "/")).forEach(k => Workspace.closeTab(k))
                Workspace.open(win.argValue("--open=")) } }
            PauseAnimation { duration: 3000 }
            ScriptAction { script: tests.memMark("pictures-closed") }
            PauseAnimation { duration: 1500 }
        }
        SequentialAnimation {
            loops: 3
            ScriptAction { script: if (tests.memMore.length > 0) Workspace.open(tests.memMore[0]) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.memShowing(100), 60000) }
            PauseAnimation { duration: 1000 }
            ScriptAction { script: if (tests.memMore.length > 1) Workspace.open(tests.memMore[1]) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.memShowing(100), 60000) }
            PauseAnimation { duration: 1000 }
            ScriptAction { script: if (tests.memMore.length > 2) Workspace.open(tests.memMore[2]) }
            PauseAnimation { duration: 200 }
            ScriptAction { script: tests.waitUntil(memLongSteps, () => tests.memShowing(100), 60000) }
            PauseAnimation { duration: 1000 }
            ScriptAction { script: Workspace.open(win.argValue("--open=")) }
            PauseAnimation { duration: 3000 }
            ScriptAction { script: tests.memMark("sessions-open") }
            PauseAnimation { duration: 1500 }
        }
        ScriptAction { script: {
            const keep = Workspace.activeKey
            Workspace.tabs.map(t => t.split("\t")[0]).filter(k => k !== keep && !k.startsWith("view:")).forEach(k => Workspace.closeTab(k)) } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: tests.memMark("sessions-closed") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { tests.memMark("end"); tests.log("turns:", tests.memTurn, "rows:", chat.transcript.listView.count) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Qt.quit() }
    }

    // A subagent transcript file (`--open=`) in the subagent dialog.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "subagentfile"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: chat.subagentDialog.show({ id: "ListRepoFiles", agent: "scout", status: "completed", description: "List every repository file with its byte size", task: "Complete assignment thoroughly:\n\n# Target", sessionFile: win.argValue("--open=") }, false) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: chat.subagentDialog.background.parent.grabToImage(r => r.saveToFile(win.selftestDir + "/subagent-file.png")) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: Qt.quit() }
    }

    // First scroll after opening a saved session, in wheel-sized steps.
    property int wheelSteps: 0
    Timer {
        id: wheelSim
        interval: 16
        repeat: true
        onTriggered: {
            chat.transcript.scrollBy(-40)
            if (++tests.wheelSteps >= 15) stop()
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "scrollsaved"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("opened rows:", chat.transcript.count, "follow:", chat.transcript.follow, "atEnd:", chat.transcript.atEnd); wheelSim.start() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: tests.log("after first wheel scroll: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd) }
        ScriptAction { script: Qt.quit() }
    }

    // Scroll jitter: scroll up in 40 px steps and check that the row under
    // the viewport's middle moves exactly 40 px per step.
    property var jitter: ({ steps: 0, bad: 0, worst: 0, last: null })
    Timer {
        id: jitterSim
        interval: 32
        repeat: true
        onTriggered: {
            const j = tests.jitter
            const now = chat.transcript.probe()
            if (j.last && j.last[0] === now[0] && now[0] >= 0) {
                // `last` was taken right after the previous scroll, so any
                // movement since then is a jump nobody asked for.
                const moved = now[1] - j.last[1]
                const extra = Math.abs(moved)
                if (extra > 0.5) { j.bad++; j.worst = Math.max(j.worst, extra) }
                if (extra > 0.5 && j.bad < 10) tests.log("jump at step", j.steps, "row", now[0], "moved", moved.toFixed(2), chat.transcript.debugContentY())
            }
            chat.transcript.scrollBy(-40)
            j.last = chat.transcript.probe()
            if (++j.steps >= 400) { stop(); tests.log("jitter: steps", j.steps, "jumps", j.bad, "worst", j.worst.toFixed(2), "px"); Qt.quit() }
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "jitter"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { tests.log("rows:", chat.transcript.count, "overscroll after open:", chat.transcript.overscroll().toFixed(1), "px"); jitterSim.start() } }
    }

    // Tabs and palette over real saved sessions (nothing is started or sent).
    function tabState() {
        const keys = Workspace.tabs.map(t => t.split("\t")[0])
        return "tabs " + keys.length + " active #" + keys.indexOf(Workspace.activeKey)
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "tabs"
        PauseAnimation { duration: 2500 }
        ScriptAction {
            script: {
                const recent = Workspace.search("", 3).map(l => l.split("\t")[1])
                for (const k of recent) Workspace.open(k)
                tests.log("opened 3:", tests.tabState(), "| titles:", Workspace.tabs.map(t => t.split("\t")[1].substring(0, 24)).join(" / "))
            }
        }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.nextTab(1); tests.log("next (wraps):", tests.tabState()); Workspace.activateTab(1); tests.log("tab 2:", tests.tabState()) } }
        ScriptAction { script: { Workspace.closeTab(Workspace.activeKey); tests.log("closed active:", tests.tabState()) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.shot("tabs"); commandPalette.openWith(""); commandPalette.items.length; } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.log("palette empty query:", commandPalette.items.length, "items, first:", commandPalette.items[0] ? commandPalette.items[0].kind + " " + commandPalette.items[0].title.substring(0, 30) : "-") } }
        ScriptAction { script: { commandPalette.openWith("rock"); tests.log("palette 'rock':", commandPalette.items.length, "first:", commandPalette.items[0] ? commandPalette.items[0].kind + " " + commandPalette.items[0].title.substring(0, 40) : "-") } }
        ScriptAction { script: { commandPalette.openWith(">"); tests.log("palette '>':", commandPalette.items.length, "actions:", commandPalette.items.slice(0, 4).map(i => i.title).join(" | ")) } }
        ScriptAction { script: commandPalette.openWith("@main") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("palette '@main':", commandPalette.items.length, "first:", commandPalette.items[0] ? commandPalette.items[0].title : "-"); commandPalette.openWith("houdini") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.log("palette 'houdini':", commandPalette.items.length, "visible:", commandPalette.visible); commandPalette.grab(win.selftestDir + "/palette.png") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { commandPalette.close(); shortcutsDialog.open(); tests.log("shortcuts visible:", shortcutsDialog.visible, "rows:", shortcutsDialog.rowCount) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: shortcutsDialog.grab(win.selftestDir + "/shortcuts.png") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Context panel on a saved session, then the Files panel in a scratch repo.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "panels"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { Workspace.open(win.argValue("--open=")); windowSettings.rightMode = "context" } }
        PauseAnimation { duration: 3000 }
        ScriptAction {
            script: {
                const r = chat.session.contextJson ? JSON.parse(chat.session.contextJson) : null
                tests.log("context:", r ? ("messages " + r.messages + ", user " + r.user + ", assistant " + r.assistant + ", tokens " + r.contextTokens + ", cost " + r.cost.toFixed(2) + ", rows " + r.rows.length + ", model " + r.model) : "none")
                tests.shot("panel-context")
                const more = tests.findName("contextShowMore")
                tests.log("show more:", more ? more.visible + " " + more.contentItem.text + ", " + more.remaining + " hidden" : "none")
                // The whole list, the row under it included (it's below the fold).
                if (more) more.parent.grabToImage(r => r.saveToFile(win.selftestDir + "/panel-context-list.png"))
            }
        }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const more = tests.findName("contextShowMore"); if (more && more.visible) tests.clickItem(more, "show more") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { const more = tests.findName("contextShowMore"); tests.log("after show more:", more ? more.visible + " " + more.contentItem.text + ", " + more.remaining + " hidden" : "none"); tests.shot("panel-context-more") } }
        PauseAnimation { duration: 600 }
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.newSession(1, "demo")
                windowSettings.rightMode = "files"
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("files: folder", files.browser.folder, "rows", files.browser.rowCount(files.browser.index(-1, -1))); files.browser.toggle(0); Workspace.openFileTab(files.browser.folder + "/src/app.rs") } }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                tests.log("file tab:", Workspace.activeKey.split("/").pop(), "state", chat.fileEditor.fileState, "language", chat.fileEditor.language, "tabs", Workspace.tabs.length)
                chat.fileEditor.typeText("// edited in harlo\n")
                tests.log("dirty:", chat.fileDrafts[chat.filePath] !== undefined)
                tests.shot("file-tab")
            }
        }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { chat.fileEditor.save(); tests.log("after save, dirty:", chat.fileDrafts[chat.filePath] !== undefined) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: Qt.quit() }
    }

    // Rapidly switch between file tabs (highlighter re-use crashed once).
    property int switchCount: 0
    property var switchFiles: []
    Timer {
        id: switcher
        interval: 60
        repeat: true
        onTriggered: {
            Workspace.openFileTab(tests.switchFiles[tests.switchCount % tests.switchFiles.length])
            if (tests.switchCount % 7 === 3) chat.fileEditor.typeText("x")
            if (++tests.switchCount >= 300) { stop(); tests.log("switched", tests.switchCount, "times without crashing"); Qt.quit() }
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "fileswitch"
        PauseAnimation { duration: 800 }
        ScriptAction {
            script: {
                const dir = win.argValue("--cwd=")
                tests.switchFiles = ["a.rs", "b.py", "c.md", "d.json", "e.sh"].map(f => dir + "/" + f)
                switcher.start()
            }
        }
    }

    // One file tab, unsaved-edits prompt, image viewer.
    function fileTabs() { return Workspace.tabs.filter(t => t.startsWith("file:")).length }
    function activeName() { return Workspace.activeKey.split("/").pop() }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "singletab"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.requestOpenFile(win.argValue("--cwd=") + "/a.rs") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { chat.requestOpenFile(win.argValue("--cwd=") + "/b.py"); tests.log("after 2 opens: file tabs", tests.fileTabs(), "active", tests.activeName()) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { chat.fileEditor.typeText("# edit\n"); chat.requestOpenFile(win.argValue("--cwd=") + "/a.rs"); tests.log("edited + open a.rs: prompt", chat.unsavedDialog.visible, "active", tests.activeName()) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.unsavedDialog.discarded() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.log("after Discard: active", tests.activeName(), "file tabs", tests.fileTabs()); chat.fileEditor.typeText("// saved edit\n"); chat.requestOpenFile(win.argValue("--cwd=") + "/pic.png") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.log("edited + open png: prompt", chat.unsavedDialog.visible); chat.unsavedDialog.accept() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("after Save: active", tests.activeName(), "image", chat.fileEditor.imageInfo, "file tabs", tests.fileTabs()); tests.shot("image-tab") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // After restoring saved tabs, report what the file editor shows.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "restored"
        PauseAnimation { duration: 4000 }
        ScriptAction {
            script: {
                const ed = chat.fileEditor.editor
                tests.log("active", Workspace.activeKey, "state", chat.fileEditor.fileState, "chars", ed ? ed.text.length : -1, "visible", chat.fileEditor.visible)
                tests.shot("restored")
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Open one file (--open-file) in the editor and report what it shows.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "openfile"
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.requestOpenFile(win.argValue("--open-file=")) }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                const ed = chat.fileEditor.editor
                tests.log("state", chat.fileEditor.fileState, "language", chat.fileEditor.language, "editor", !!ed, "chars", ed ? ed.text.length : -1, "lines", ed ? ed.lineCount : -1, "visible", chat.fileEditor.visible)
                tests.shot("openfile")
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Switch among text, binary and image files at random speeds; every few
    // switches settle and check the editor shows what the file is.
    property var mix: ({ n: 0, bad: 0, checks: 0, files: [] })
    Timer {
        id: mixer
        repeat: false
        onTriggered: {
            const m = tests.mix
            if (m.n > 0 && m.n % 6 === 0 && !mixer.settled) {
                // Settled: verify.
                const path = Workspace.activeKey.substring(5)
                const kind = /\.hip$/.test(path) ? "binary" : /\.png$/.test(path) ? "image" : "text"
                const fe = chat.fileEditor
                let ok = true
                if (kind === "text") ok = fe.state === "text" && fe.editor && fe.editor.text.length > 0 && fe.shownMessage === ""
                else if (kind === "binary") ok = fe.state === "binary" && fe.shownMessage.indexOf(path) >= 0
                else ok = fe.shownMessage === ""
                m.checks++
                if (!ok) { m.bad++; if (m.bad < 6) tests.log("MISMATCH on", path.split("/").pop(), "state", fe.state, "message:", fe.shownMessage.substring(0, 90)) }
                mixer.settled = true
                mixer.interval = 10
                mixer.start()
                return
            }
            mixer.settled = false
            const f = m.files[Math.floor(Math.random() * m.files.length)]
            chat.requestOpenFile(f)
            m.n++
            if (m.n >= 240) { tests.log("mixswitch:", m.n, "switches,", m.checks, "checks,", m.bad, "mismatches"); Qt.quit(); return }
            mixer.interval = (m.n % 6 === 0) ? 900 : Math.floor(Math.random() * 120)
            mixer.start()
        }
        property bool settled: false
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "mixswitch"
        PauseAnimation { duration: 800 }
        ScriptAction {
            script: {
                const d = win.argValue("--cwd=")
                tests.mix.files = ["notes.md", "README.md", "a.rs", "scene.hip", "pic.png", "b.py"].map(f => d + "/" + f)
                mixer.interval = 10
                mixer.start()
            }
        }
    }

    // The open file changing on disk: silent reload, then a bar when there
    // are unsaved edits, "keep mine", and deletion.
    TextFile { id: diskWriter }
    function writeDisk(text) { diskWriter.load(chat.filePath); diskWriterText = text; diskWriteTimer.start() }
    property string diskWriterText: ""
    Timer { id: diskWriteTimer; interval: 300; onTriggered: tests.log("  (wrote disk:", diskWriter.save(tests.diskWriterText), ")") }
    function editorText() { return chat.fileEditor.editor ? chat.fileEditor.editor.text : "<none>" }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "diskchange"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: chat.requestOpenFile(win.argValue("--cwd=") + "/a.rs") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: tests.writeDisk("// agent edit 1\nfn main() {}\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { tests.log("no edits -> reloaded:", tests.editorText().startsWith("// agent edit 1"), "bar:", chat.fileEditor.diskBarVisible); chat.fileEditor.typeText("// mine\n") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.writeDisk("// agent edit 2\nfn main() {}\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { tests.log("with edits -> bar:", chat.fileEditor.diskBarVisible, "kept mine:", tests.editorText().startsWith("// mine")); tests.shot("disk-bar") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: chat.fileEditor.keepMine() }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { tests.log("after keep mine -> bar:", chat.fileEditor.diskBarVisible); chat.fileEditor.save() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.writeDisk("// agent edit 3\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { tests.log("after save, next agent edit reloads:", tests.editorText().startsWith("// agent edit 3"), "bar:", chat.fileEditor.diskBarVisible); Qt.quit() } }
    }

    // The file editor's keys: Enter keeps the indentation, brackets,
    // Tab and Shift+Tab, Backspace by a level, Ctrl+/ comments, find and
    // replace (one undo), go to line, the bracket pair; per file type.
    function edText() { return chat.fileEditor.editor ? chat.fileEditor.editor.text : "" }
    function edLines() { return tests.edText().split("\n") }
    function edLine() { const ed = chat.fileEditor.editor; return ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1 }
    function edCol() { const ed = chat.fileEditor.editor; return ed ? ed.cursorPosition - (ed.text.lastIndexOf("\n", ed.cursorPosition - 1) + 1) : -1 }
    function edCount(what) { return tests.edText().split(what).length - 1 }
    property string edBefore: ""
    SequentialAnimation {
        id: editorSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickseditor"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/editor.rs") } }
        ScriptAction { script: tests.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/editor.rs"), 8000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            tests.clickItem(ed, "the editor")
            // Enter after an opening brace: a level deeper.
            ed.cursorPosition = ed.text.indexOf("fn main() {") + 11
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edLines()[1] === "    " && tests.edCol() === 4, "Enter after {: the new line a level in (" + JSON.stringify(tests.edLines()[1]) + ", col " + tests.edCol() + ")")
            TestInput.typeText("let x = 1;")
            tests.check(tests.edLines()[1] === "    let x = 1;", "…and you type there (" + JSON.stringify(tests.edLines()[1]) + ")")
            // Between a pair: the closing one on a line of its own.
            ed.cursorPosition = ed.length
            TestInput.typeText("fn f() {}")
            TestInput.key(Qt.Key_Left, 0)
            TestInput.key(Qt.Key_Return, 0)
            const l = tests.edLines(), n = tests.edLine()
            tests.check(l[n - 2] === "fn f() {" && l[n - 1] === "    " && l[n] === "}" && tests.edCol() === 4, "Enter between {}: the } on its own line, the cursor between (" + JSON.stringify(l.slice(n - 2, n + 1)) + ")")
            // A } typed on a line of spaces: lined up with its {.
            TestInput.typeText("if x {")
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edLines()[tests.edLine() - 1] === "        ", "a nested block: two levels")
            TestInput.typeText("}")
            tests.check(tests.edLines()[tests.edLine() - 1] === "    }", "} lines up with its { (" + JSON.stringify(tests.edLines()[tests.edLine() - 1]) + ")")
            // Backspace in the indentation: a level at once.
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edLines()[tests.edLine() - 1] === "    ", "Enter after }: the same indentation")
            TestInput.key(Qt.Key_Backspace, 0)
            tests.check(tests.edLines()[tests.edLine() - 1] === "" && tests.edCol() === 0, "Backspace: back a level, not a space (" + JSON.stringify(tests.edLines()[tests.edLine() - 1]) + ")")
            // Tab on selected lines: each a level in, one undo.
            tests.edBefore = ed.text
            const s = ed.text.indexOf("fn add("), e = ed.text.indexOf("}\n", s) + 2
            ed.select(s, e)
            TestInput.key(Qt.Key_Tab, 0)
            tests.check(ed.text.indexOf("    fn add(a: i32, b: i32) -> i32 {\n        a + b\n    }\n") >= 0, "Tab: the selected lines a level in")
            tests.check(ed.selectionStart === s && ed.selectionEnd === ed.text.indexOf("    }\n", s) + 5, "…still selected (" + [ed.selectionStart, ed.selectionEnd] + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            tests.check(ed.text === tests.edBefore, "Ctrl+Z: undone in one step")
            ed.select(s, e)
            TestInput.key(Qt.Key_Backtab, Qt.ShiftModifier)
            tests.check(ed.text.indexOf("fn add(a: i32, b: i32) -> i32 {\na + b\n}") >= 0, "Shift+Tab: a level out (" + JSON.stringify(ed.text.slice(s, s + 44)) + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            // Ctrl+/: comments the lines out at their indentation, then back.
            ed.select(s, e)
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            tests.check(ed.text.indexOf("// fn add(a: i32, b: i32) -> i32 {\n//     a + b\n// }\n") >= 0, "Ctrl+/: commented out (" + JSON.stringify(ed.text.slice(s, s + 50)) + ")")
            tests.check(!shortcutsDialog.visible, "…not the shortcuts sheet")
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            tests.check(ed.text === tests.edBefore, "Ctrl+/ again: back in")
            ed.cursorPosition = 0 } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Find: Ctrl+F in a file finds in the file.
            TestInput.key(Qt.Key_F, Qt.ControlModifier)
            const fe = chat.fileEditor
            tests.check(fe.findOpen && tests.findName("editorFindField", fe).activeFocus && !chat.findBar.open, "Ctrl+F: the file's find, not the session's")
            TestInput.typeText("total") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            const first = ed.text.indexOf("total"), second = ed.text.indexOf("total", first + 1)
            tests.check(fe.matches.length === 2 && fe.currentMatch === 0 && ed.selectionStart === first && ed.selectionEnd === first + 5, "typed: both found, the first selected (" + fe.matches.length + ")")
            tests.check(tests.findName("editorFindCount", fe).text === "1 of 2", "1 of 2 (" + tests.findName("editorFindCount", fe).text + ")")
            tests.check(ed.visibleMatches() === 2, "both marked in the text (" + ed.visibleMatches() + ")")
            tests.shot("editor-find")
            TestInput.key(Qt.Key_Return, 0)
            tests.check(fe.currentMatch === 1 && ed.selectionStart === second, "Enter: the next")
            TestInput.key(Qt.Key_Return, Qt.ShiftModifier)
            tests.check(fe.currentMatch === 0 && ed.selectionStart === first, "Shift+Enter: back")
            tests.clickItem(tests.findName("editorMatchCase", fe), "Aa")
            tests.findName("editorFindField", fe).forceActiveFocus()
            tests.findName("editorFindField", fe).selectAll()
            TestInput.typeText("TOTAL") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor
            tests.check(fe.matchCase && fe.matches.length === 0 && tests.findName("editorFindCount", fe).text === "None", "Aa: TOTAL isn't total (" + tests.findName("editorFindCount", fe).text + ")")
            tests.clickItem(tests.findName("editorMatchCase", fe), "Aa")
            tests.findName("editorFindField", fe).forceActiveFocus() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor
            tests.check(!fe.matchCase && fe.matches.length === 2, "Aa off: found again")
            // Replace: Ctrl+H, then All.
            TestInput.key(Qt.Key_H, Qt.ControlModifier)
            tests.check(fe.replaceOpen && tests.findName("editorReplaceField", fe).activeFocus, "Ctrl+H: the replace line, its field focused")
            TestInput.typeText("sum") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            tests.shot("editor-replace")
            tests.edBefore = ed.text
            tests.clickItem(tests.findName("editorReplaceAll", fe), "All")
            tests.check(tests.edCount("total") === 0 && tests.edCount("sum") === 2, "All: both replaced (" + tests.edCount("sum") + ")")
            tests.check(tests.findName("editorFindCount", fe).text === "Replaced 2", "…and says so (" + tests.findName("editorFindCount", fe).text + ")")
            ed.forceActiveFocus()
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            tests.check(ed.text === tests.edBefore, "Ctrl+Z: every replacement undone at once") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            tests.check(fe.matches.length === 2, "the matches found again after the undo (" + fe.matches.length + ")")
            tests.clickItem(tests.findName("editorFindNext", fe), "next")
            tests.clickItem(tests.findName("editorReplaceField", fe), "the replace field")
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edCount("total") === 1 && tests.edCount("sum") === 1 && fe.matches.length === 1, "Enter in Replace: just the one shown (" + tests.edCount("sum") + ")")
            TestInput.key(Qt.Key_Escape, 0)
            tests.check(!fe.findOpen && !tests.findName("editorFind", fe) && ed.activeFocus, "Esc: find closed, back in the text")
            tests.check(ed.visibleMatches() === 0, "…its marks gone") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            // Go to line: Ctrl+G, or the line count in the header.
            TestInput.key(Qt.Key_G, Qt.ControlModifier)
            tests.check(fe.lineOpen && tests.findName("editorLineField", fe).activeFocus, "Ctrl+G: go to line")
            tests.shot("editor-goto")
            TestInput.typeText("7")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            tests.check(!fe.lineOpen && tests.edLine() === 7 && ed.activeFocus, "…line 7 (" + tests.edLine() + ")")
            tests.clickItem(tests.findName("editorLineLabel", fe), "Ln")
            tests.check(fe.lineOpen, "the header's Ln opens it too")
            TestInput.key(Qt.Key_Escape, 0)
            tests.check(!fe.lineOpen, "Esc closes it")
            // The bracket by the cursor and its partner.
            ed.forceActiveFocus()
            const open = ed.text.indexOf("fn add(") + 6, close = ed.text.indexOf(")", open)
            ed.cursorPosition = open } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            const open = ed.text.indexOf("fn add(") + 6, close = ed.text.indexOf(")", open)
            const marks = JSON.parse(ed.bracketPair())
            tests.check(marks.length === 2 && marks[0] === open && marks[1] === close, "( and its ) marked (" + marks + " vs " + [open, close] + ")")
            tests.shot("editor-brackets")
            ed.cursorPosition = ed.text.indexOf("let x")
            TestInput.key(Qt.Key_S, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(chat.fileEditor.editor.bracketPair() === "[]", "no bracket by the cursor: no marks")
            tests.check(TestInput.readFile(win.argValue("--cwd=") + "/src/editor.rs") === tests.edText() && !chat.fileEditor.modified, "Ctrl+S saves it")
            tests.check(win.composerKeys.some(k => k.group === "File editor" && k.seq === "Ctrl+/"), "the shortcuts sheet lists the editor's keys")
            // A file indented by two spaces: Enter adds two.
            Workspace.openFileTab(win.argValue("--cwd=") + "/web/app.js") } }
        ScriptAction { script: tests.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/app.js"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            ed.forceActiveFocus()
            ed.cursorPosition = ed.text.indexOf("function show() {") + 17
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edLines()[1] === "  " && tests.edCol() === 2, "two-space JavaScript: Enter adds two (" + JSON.stringify(tests.edLines()[1]) + ")")
            TestInput.key(Qt.Key_Tab, 0)
            tests.check(tests.edLines()[1] === "    ", "…and Tab two more (" + JSON.stringify(tests.edLines()[1]) + ")")
            ed.cursorPosition = 0
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            tests.check(tests.edLines()[0] === "// function show() {", "// for JavaScript (" + tests.edLines()[0] + ")")
            Workspace.openFileTab(win.argValue("--cwd=") + "/tool.py") } }
        ScriptAction { script: tests.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/tool.py"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            ed.forceActiveFocus()
            ed.cursorPosition = ed.text.indexOf("def main():") + 11
            TestInput.key(Qt.Key_Return, 0)
            tests.check(tests.edLines()[1] === "    ", "Python: a level in after a colon (" + JSON.stringify(tests.edLines()[1]) + ")")
            ed.cursorPosition = ed.text.indexOf("print")
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            tests.check(tests.edText().indexOf("    # print(\"hi\")") >= 0, "# for Python, at the line's indentation")
            // Back to app.js (left with unsaved edits): they're still there,
            // and so is their undo.
            Workspace.openFileTab(win.argValue("--cwd=") + "/web/app.js") } }
        ScriptAction { script: tests.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/app.js"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            tests.check(fe.modified && tests.edLines()[0] === "// function show() {" && tests.edLines()[1] === "    ", "back to a file with unsaved edits: they're kept (" + JSON.stringify(tests.edLines()[0]) + ")")
            tests.check(chat.fileDrafts[chat.filePath] === true && chat.fileDrafts[win.argValue("--cwd=") + "/tool.py"] === true, "…and both files are marked unsaved")
            ed.forceActiveFocus()
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            tests.check(tests.edLines()[0] === "function show() {", "…and so is their undo (" + JSON.stringify(tests.edLines()[0]) + ")")
            tests.summary(); Qt.quit() } }
    }

    // A big file (8000 lines): its diff from the Changes panel, then the
    // file itself, scrolled; then 100,000 lines, typed at the end. The code
    // views (drawn in Rust) show at once and colour in the background.
    property real perfT0: 0
    property real perfLast: 0
    property var perfMarks: ({})
    property var perfStalls: []
    function perfStart() { tests.perfT0 = Date.now(); tests.perfLast = tests.perfT0; tests.perfMarks = {}; tests.perfStalls = []; perfPoll.start() }
    function perfLongest() { return tests.perfStalls.reduce((m, s) => Math.max(m, s.ms), 0) }
    Timer {
        id: perfPoll
        interval: 5
        repeat: true
        onTriggered: {
            const now = Date.now(), t = now - tests.perfT0, m = tests.perfMarks
            if (now - tests.perfLast > 40) tests.perfStalls.push({ at: Math.round(t - (now - tests.perfLast)), ms: now - tests.perfLast })
            tests.perfLast = now
            const dv = chat.diffView, fe = chat.fileEditor
            if (m.diffReady === undefined && dv.visible && dv.leftPane.ready && dv.rightPane.ready && dv.rightPane.text !== "") m.diffReady = t
            if (m.jumped === undefined && dv.visible && dv.jumped) m.jumped = t
            if (m.fileShown === undefined && fe.visible && fe.editor && fe.editor.length > 0) m.fileShown = t
            if (m.fileReady === undefined && fe.visible && fe.loaded) m.fileReady = t
        }
    }
    SequentialAnimation {
        id: bigFileSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksbigfile"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("diff_layout", "split")
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.perfStart(); tests.clickText("big.rs", changes) } }
        ScriptAction { script: tests.waitUntil(bigFileSteps, () => tests.perfMarks.jumped !== undefined, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop()
            const dv = chat.diffView, m = tests.perfMarks
            console.log("PERF big diff: marks", JSON.stringify(m), "stalls", JSON.stringify(tests.perfStalls))
            tests.check(m.jumped !== undefined, "the diff opened at its first change (" + m.jumped + " ms)")
            tests.check(m.jumped < 300 && tests.perfLongest() < 200, "the window kept answering: longest hold " + tests.perfLongest() + " ms (it was 5100; then 1900)")
            tests.check(dv.rightPane.editor.text === dv.rightPane.text && dv.leftPane.editor.text === dv.leftPane.text && dv.rightPane.editor.lineCount >= 8000, "both sides all in (" + dv.rightPane.editor.lineCount + " lines)")
            const f = dv.rightPane.flick, y = dv.blocks[0].start * dv.lineHeight
            tests.check(y >= f.contentY && y <= f.contentY + f.height, "the first change in view")
            tests.shot("bigfile-diff")
            tests.perfStart()
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/big.rs") } }
        ScriptAction { script: tests.waitUntil(bigFileSteps, () => tests.perfMarks.fileReady !== undefined, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop()
            const fe = chat.fileEditor, m = tests.perfMarks
            console.log("PERF big file: marks", JSON.stringify(m), "stalls", JSON.stringify(tests.perfStalls))
            tests.check(m.fileReady !== undefined && m.fileShown < 150 && tests.perfLongest() < 100, "the file shown in " + m.fileShown + " ms, the window answering: longest hold " + tests.perfLongest() + " ms")
            tests.check(fe.editor.text === TestInput.readFile(win.argValue("--cwd=") + "/src/big.rs") && !fe.modified, "all of it in, and not an edit")
            tests.check(fe.language === "Rust" && !fe.editor.readOnly, "coloured, and editable once in (" + fe.language + ")")
            tests.shot("bigfile-editor")
            // Scrolling through it: each frame drawn quickly, the window
            // never held.
            tests.cvFrames = []
            cvScroll.n = 0
            tests.perfStart()
            cvScroll.start() } }
        ScriptAction { script: tests.waitUntil(bigFileSteps, () => !cvScroll.running, 10000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            perfPoll.stop()
            const f = tests.cvFrames.slice(2), ed = chat.fileEditor.editor
            const avg = f.reduce((a, b) => a + b, 0) / f.length, max = Math.max.apply(null, f)
            console.log("PERF big file scroll: frames", f.length, "avg", avg.toFixed(2), "ms max", max.toFixed(2), "ms; stalls", JSON.stringify(tests.perfStalls))
            tests.check(ed.contentY > 5000 && max < 8, "scrolling: every frame drawn in under 8 ms (max " + max.toFixed(2) + " ms)")
            tests.check(tests.perfLongest() < 50, "…and the window never held (longest " + tests.perfLongest() + " ms)")
            // 100,000 lines.
            tests.perfStart()
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/huge.rs") } }
        ScriptAction { script: tests.waitUntil(bigFileSteps, () => tests.perfMarks.fileReady !== undefined && chat.fileEditor.editor && chat.fileEditor.editor.path.endsWith("/huge.rs"), 15000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            perfPoll.stop()
            const fe = chat.fileEditor, ed = fe.editor, m = tests.perfMarks
            console.log("PERF huge file: lines", ed.lineCount, "marks", JSON.stringify(m), "stalls", JSON.stringify(tests.perfStalls))
            tests.check(ed.lineCount >= 100000, "100,000 lines in (" + ed.lineCount + ")")
            tests.check(m.fileShown < 250, "…shown in " + m.fileShown + " ms")
            tests.check(tests.perfLongest() < 150, "…the window held at most " + tests.perfLongest() + " ms")
            // Typing at the very end.
            ed.forceActiveFocus()
            ed.cursorPosition = ed.length
            const t0 = Date.now()
            TestInput.typeText("// the end.")
            const ms = (Date.now() - t0) / 11
            console.log("PERF huge file typing:", ms.toFixed(2), "ms a key")
            tests.check(ed.text.endsWith("// the end.") && fe.modified, "typing at the end of 100,000 lines")
            tests.check(ms < 16, "…" + ms.toFixed(1) + " ms a key")
            tests.check(ed.colouring, "(colouring in order hasn't got this far)") } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            tests.check(ed.lineColoured(ed.lineCount - 3) && ed.lineColoured(ed.lineCount - 30), "…yet the lines on screen are coloured at once")
            tests.shot("bigfile-huge-end")
            tests.summary(); Qt.quit() } }
    }

    // The terminal under a flood of output: 20,000 coloured lines, then 400
    // full screens redrawn in place. The window keeps answering and the
    // screen keeps up.
    property int termFrames: 0
    property real termT0: 0
    Connections {
        target: win
        enabled: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminalspeed"
        function onFrameSwapped() { tests.termFrames++ }
    }
    function termRun(command) {
        const t = terminal.current.session
        tests.termFrames = 0
        tests.termT0 = Date.now()
        tests.perfStart()
        tests.termTicks = 0
        tests.termPaintMax = 0
        tests.termPaints = []
        termTick.start()
        t.paste(command)
        t.key(Qt.Key_Return, 0, "\r")
    }
    // Ticks every 5 ms while the window is free: how much of the time it was.
    property int termTicks: 0
    property real termPaintMax: 0
    property real termPaintLast: 0
    property var termPaints: []
    Timer {
        id: termTick
        interval: 5
        repeat: true
        onTriggered: {
            tests.termTicks++
            const ms = terminal.current.session.paintMs
            if (ms !== tests.termPaintLast) { tests.termPaints.push(Math.round(ms * 10) / 10); tests.termPaintLast = ms }
            tests.termPaintMax = Math.max(tests.termPaintMax, ms)
        }
    }
    /// The terminal's rows run together, without spaces (wrapped lines whole).
    function termScreen() {
        const t = terminal.current.session
        let text = ""
        for (let r = 0; r < 300; r++) text += t.rowText(r)
        return text.replace(/ /g, "")
    }
    function termReport(what, limit, file) {
        perfPoll.stop()
        termTick.stop()
        const ms = Number(TestInput.readFile(win.argValue("--cwd=") + "/" + file).trim()) - tests.termT0
        const free = Math.round(100 * tests.termTicks * 5 / (Date.now() - tests.termT0))
        console.log("PERF terminal " + what + ":", ms, "ms,", tests.termFrames, "frames, slowest drawn in", tests.termPaintMax.toFixed(2), "ms, window free", free + "%, longest hold", tests.perfLongest(), "ms; stalls", JSON.stringify(tests.perfStalls), "frames drawn in", JSON.stringify(tests.termPaints))
        tests.check(tests.perfLongest() < limit, what + ": the window held at most " + tests.perfLongest() + " ms (" + ms + " ms in all, " + tests.termFrames + " frames; it held 120–360 ms a frame when rows were rich text)")
        tests.check(tests.termPaintMax < 30, what + ": every frame drawn in under 30 ms (slowest " + tests.termPaintMax.toFixed(1) + ")")
    }
    SequentialAnimation {
        id: termSpeedSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminalspeed"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { windowSettings.terminalHeight = win.height - 120; tests.clickItem(tests.findName("composerInput"), "composer"); TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        ScriptAction { script: tests.waitUntil(termSpeedSteps, () => terminal.visible && terminal.current !== null && terminal.current.running, 5000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.termRun("awk 'BEGIN{for(i=1;i<=20000;i++) printf \"\\033[3%dm%06d\\033[0m the quick brown fox jumps over the lazy dog \\033[1m%d\\033[0m\\n\", i%8, i, i*i}'; date +%s%3N > stream.done") }
        ScriptAction { script: tests.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/stream.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.termReport("20,000 lines", 100, "stream.done")
            const screen = tests.termScreen()
            tests.check(screen.indexOf("020000thequickbrownfoxjumpsoverthelazydog400000000") >= 0, "the last line drawn whole (" + screen.slice(-120) + ")")
            tests.shot("terminal-stream") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.termRun("clear; awk -v R=$LINES -v C=$COLUMNS 'BEGIN{for(f=0;f<400;f++){printf \"\\033[H\"; for(r=0;r<R-1;r++){for(c=0;c<int(C/10);c++) printf \"\\033[3%dm%-9d\\033[0m \", (r+c+f)%8, f*r+c; printf \"\\n\"}}}'; date +%s%3N > redraw.done") }
        ScriptAction { script: tests.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/redraw.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.termReport("400 full screens", 100, "redraw.done"); tests.shot("terminal-redraw") } }
        PauseAnimation { duration: 300 }
        // Every cell its own colours (a busy full-screen program).
        ScriptAction { script: tests.termRun("clear; awk -v R=$LINES -v C=$COLUMNS 'BEGIN{for(f=0;f<200;f++){printf \"\\033[H\"; for(r=0;r<R-1;r++){for(c=0;c<C;c++) printf \"\\033[38;5;%d;48;5;%dm%c\", (r+c+f)%256, (r*3+c+f*2)%256, 33+(r+c+f)%90; printf \"\\033[0m\\n\"}}}'; date +%s%3N > cells.done") }
        ScriptAction { script: tests.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/cells.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.termReport("200 screens of coloured cells", 100, "cells.done"); tests.shot("terminal-cells") } }
        PauseAnimation { duration: 300 }
        // Boxes, blocks, styles, wide characters.
        ScriptAction { script: tests.termRun("clear; printf '╭──┬──╮ ┏━━┓ ╔══╗\\n│ab│cd│ ┃  ┃ ║  ║\\n├──┼──┤ ┗━━┛ ╚══╝\\n╰──┴──╯ ▀▄█▌▐░▒▓ ▖▗▘▝▙▟\\n\\033[1mbold\\033[0m \\033[3mitalic\\033[0m \\033[4munderline\\033[0m \\033[9mstrike\\033[0m \\033[7minverse\\033[0m \\033[2mdim\\033[0m\\n漢字 テスト ✓ → é ü\\n'; date +%s%3N > glyphs.done") }
        ScriptAction { script: tests.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/glyphs.done") !== "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop(); termTick.stop()
            const screen = tests.termScreen()
            tests.check(screen.indexOf("╭──┬──╮┏━━┓╔══╗") === 0 && screen.indexOf("漢字テスト✓→éü") > 0, "boxes and wide characters in their cells (" + screen + ")")
            tests.shot("terminal-glyphs"); tests.summary(); Qt.quit() } }
    }

    // The image viewer: a big image (a copy at the screen's size first, then
    // whole), wheel zoom about the pointer, actual size, fit, turning,
    // stepping to an image decoded ahead, an animation, the folder's grid
    // (thumbnails made, cached on disk, read back) and filling the window.
    property string imgDir: win.argValue("--cwd=") + "/pics"
    property string imgBig: "big.png"
    property real imgT0: 0
    property var imgBefore: null
    property real imgZoom: 0
    // The colours just above the image's view and in the bar below it, with
    // the image fitted: a zoomed image mustn't be drawn over them.
    property var imgEdges: []
    function imgEdgeColours(view) {
        const v = view || tests.imgView()
        const top = v.mapToItem(null, v.width / 2, -2), below = v.mapToItem(null, v.width / 2, v.height + 12)
        return [TestInput.colourAt(top.x, top.y), TestInput.colourAt(below.x, below.y)]
    }
    property int imgFrames: 0
    property int imgLongAt: 0
    property int imgStepMs: 0
    property var imgStats: null
    function imgView() { return chat.fileEditor.imageViewer.view }
    function imgOpen(name) { tests.imgT0 = Date.now(); Workspace.openFileTab(tests.imgDir + "/" + name) }
    function imgReady(name) { const v = tests.imgView(); return v.path === tests.imgDir + "/" + name && v.status === "ready" }
    function imgDpr() { return win.screen ? win.screen.devicePixelRatio : 1 }
    /// The grid's thumbnails in view.
    function imgThumbs() {
        const g = chat.fileEditor.imageViewer.grid
        const out = []
        for (const d of g.contentItem.children)
            for (const c of d.children)
                if (c.objectName === "imageThumb" && d.y + d.height > g.contentY && d.y < g.contentY + g.height) out.push(c)
        return out
    }
    function imgThumbsReady() { const t = tests.imgThumbs(); return t.length > 0 && t.every(c => c.status === "ready") }
    Connections {
        target: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksimages" ? chat.fileEditor.imageViewer.view : null
        function onFrameChanged() { tests.imgFrames++ }
    }
    SequentialAnimation {
        id: imgSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksimages"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        // A big one: more than a texture's side; a JPEG shows a copy at the
        // screen's size first.
        ScriptAction { script: tests.imgBig = TestInput.readFile(tests.imgDir + "/big.png") !== "" ? "big.png" : "big.jpg" }
        ScriptAction { script: tests.imgOpen(tests.imgBig) }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady(tests.imgBig), 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            console.log("PERF image 9000x6000 " + tests.imgBig + ": first on screen", v.firstMs.toFixed(1), "ms, whole", v.loadMs.toFixed(1), "ms")
            tests.check(v.imageWidth === 9000 && v.imageHeight === 6000, "the big image whole: " + v.imageWidth + "×" + v.imageHeight + " (" + v.status + ")")
            tests.check(v.fit && v.zoom < 0.2, "fitted to the view (" + (v.zoom * 100).toFixed(1) + "%)")
            const st = JSON.parse(Images.stats())
            tests.check(!v.whole && st.fullMB < 64, "much bigger than the screen, it's kept at the screen's size (" + st.fullMB + " MB, not 206)")
            v.actualSize() } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgView().whole && tests.imgView().status === "ready", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(v.whole && v.status === "ready" && Math.abs(v.zoom - 1) < 1e-9, "zoomed in past the copy, its own pixels (" + v.status + ", " + v.zoom + ")")
            v.fitView() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(!v.whole && v.status === "ready" && v.fit, "fitted again, back to the copy (" + v.status + ")")
            tests.imgOpen("img1.png") } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            const viewer = chat.fileEditor.imageViewer
            console.log("PERF image 1200x800:", v.loadMs.toFixed(1), "ms")
            tests.check(v.loadMs < 1000, "a 1200×800 image on screen in " + v.loadMs.toFixed(0) + " ms")
            tests.check(v.activeFocus, "the image has the keys")
            const dpr = tests.imgDpr()
            const fitZoom = Math.min(1, v.width * dpr / 1200, v.height * dpr / 800)
            tests.check(v.fit && Math.abs(v.zoom - fitZoom) < 1e-6, "fitted: " + v.zoom.toFixed(4) + " (" + fitZoom.toFixed(4) + ")")
            tests.shotWindow("clicks-images-tab")
            const pathLabel = tests.findName("editorPath", chat.fileEditor)
            const outside = tests.findName("editorOpenOutside", chat.fileEditor)
            tests.check(outside !== null && outside.iconName === "external-link", "opening it outside is a drawn icon")
            tests.check(pathLabel && pathLabel.text === "pics/img1.png" && pathLabel.whole.endsWith("/pics/img1.png"), "the tab's path is the file's in its project, the whole on hover (" + (pathLabel ? pathLabel.text : "none") + ")")
            const tight = viewer.width < 640
            const more = tests.findName("imageMore", viewer), turn = tests.findName("imageTurnLeft", viewer)
            tests.check(tight ? (more !== null && turn === null) : (more === null && turn !== null), (tight ? "narrow, turning and the grid go into the ⋯ menu" : "wide, the bar shows turning, no ⋯ menu") + " (more " + (more !== null) + ", turn " + (turn !== null) + ")")
            const zoom = tests.findName("imageZoom", viewer)
            const barRight = zoom ? zoom.parent.mapToItem(viewer, zoom.parent.width, 0).x : -1
            tests.check(barRight > 0 && barRight <= viewer.width + 0.5, "the bar fits its view (" + barRight.toFixed(0) + " of " + viewer.width.toFixed(0) + ")")
            if (tight) viewer.moreMenu.popup(more, 0, 0)
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            if (viewer.width < 640)
                tests.clickItem(tests.findName("imageKeys", viewer.moreMenu.contentItem), "Keyboard shortcuts")
        } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer, v = tests.imgView()
            if (viewer.width < 640) {
                tests.check(shortcutsDialog.visible, "⋯ → Keyboard shortcuts opens the sheet")
                shortcutsDialog.close()
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.imgView(), viewer = chat.fileEditor.imageViewer
            viewer.focusView()
            tests.check(viewer.images.length === 34 && viewer.images[2] === tests.imgDir + "/img1.png" && viewer.images[3] === tests.imgDir + "/img2.png" && viewer.images[11] === tests.imgDir + "/img10.png",
                      "the folder's 34 images in natural order (" + viewer.images.slice(0, 5).map(p => p.split("/").pop()).join(" ") + " …), not the text file")
            // Four wheel notches at a point left of the middle.
            const x = v.width * 0.35, y = v.height * 0.5
            tests.imgBefore = JSON.parse(v.imagePointAt(x, y))
            tests.imgZoom = v.zoom
            tests.imgEdges = tests.imgEdgeColours()
            const p = v.mapToItem(null, x, y)
            for (let i = 0; i < 4; i++) TestInput.wheel(p.x, p.y, 120) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = tests.imgView()
            const after = JSON.parse(v.imagePointAt(v.width * 0.35, v.height * 0.5))
            const moved = Math.hypot(after[0] - tests.imgBefore[0], after[1] - tests.imgBefore[1])
            tests.check(Math.abs(v.zoom / tests.imgZoom - Math.pow(1.25, 4)) < 0.01, "four wheel notches zoom ×2.44 (" + (v.zoom / tests.imgZoom).toFixed(3) + ")")
            tests.check(moved < 1, "the point under the pointer stays under it (moved " + moved.toFixed(2) + " image px)")
            tests.check(!v.fit, "zoomed, it no longer follows the view's size")
            TestInput.key(Qt.Key_1, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(Math.abs(v.zoom - 1) < 1e-9 && v.pixelated === false, "1: actual size (" + v.zoom + ")")
            const tl = JSON.parse(v.imagePointAt(0, 0))
            tests.check(Math.abs(tl[0] - Math.round(tl[0])) < 1e-6 && Math.abs(tl[1] - Math.round(tl[1])) < 1e-6, "at actual size the pixels sit on whole screen pixels (" + tl + ")")
            TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(Math.abs(v.zoom - 1.5) < 1e-6, "+ zooms a step, to 150% (" + v.zoom + ")")
            TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(Math.abs(v.zoom - 2) < 1e-6 && v.pixelated, "the next step is 200%, pixels drawn as squares (" + v.zoom + ", " + v.pixelated + ")")
            tests.shotWindow("clicks-images-zoomed")
            const edges = tests.imgEdgeColours()
            tests.check(edges[0] !== "" && edges[0] === tests.imgEdges[0] && edges[1] === tests.imgEdges[1],
                      "bigger than its view, the image is drawn only inside it: not over the bar below or what is above (" + edges + ", fitted " + tests.imgEdges + ")")
            v.pan(-100000, -100000)
            const br = JSON.parse(v.imagePointAt(v.width - 0.01, v.height - 0.01))
            tests.check(Math.abs(br[0] - 1200) < 0.1 && Math.abs(br[1] - 800) < 0.1, "dragged far, it stops at the image's corner (" + br + ")")
            TestInput.key(Qt.Key_0, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.imgView()
            const dpr = tests.imgDpr()
            tests.check(v.fit && Math.abs(v.zoom - Math.min(1, v.width * dpr / 1200, v.height * dpr / 800)) < 1e-6, "0 fits it again (" + v.zoom.toFixed(4) + ")")
            TestInput.key(Qt.Key_R, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            const dpr = tests.imgDpr()
            tests.check(v.turn === 90 && Math.abs(v.zoom - Math.min(1, v.width * dpr / 800, v.height * dpr / 1200)) < 1e-6, "R turns it right and fits it turned (" + v.turn + "°, " + v.zoom.toFixed(4) + ")")
            const c = JSON.parse(v.imagePointAt(v.width / 2, v.height / 2))
            tests.check(Math.abs(c[0] - 600) < 0.6 && Math.abs(c[1] - 400) < 0.6, "turned about its middle (" + c + ")")
            TestInput.key(Qt.Key_R, Qt.ShiftModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.imgView().turn === 0, "Shift+R turns it back")
            // (img2 was decoded ahead while this one was looked at.)
            tests.imgT0 = Date.now()
            TestInput.key(Qt.Key_Right, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("img2.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.imgView()
            console.log("PERF image next (decoded ahead):", v.loadMs.toFixed(2), "ms")
            tests.check(v.loadMs < 30, "→ steps to img2, decoded ahead: on screen in " + v.loadMs.toFixed(1) + " ms")
            tests.check(Workspace.activeKey === "file:" + tests.imgDir + "/img2.png" && v.fit && v.turn === 0, "the file tab now shows img2, fitted")
            TestInput.key(Qt.Key_K, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.imgView().loadMs < 30, "K steps back, from memory (" + tests.imgView().loadMs.toFixed(1) + " ms)")
            tests.imgOpen("anim.gif") } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("anim.gif"), 10000) }
        ScriptAction { script: tests.imgFrames = 0 }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(v.frameCount === 3 && v.playing && tests.imgFrames >= 4, "a GIF plays: " + v.frameCount + " frames, " + tests.imgFrames + " shown in 0.9 s")
            TestInput.key(Qt.Key_P, 0)
            tests.imgFrames = 0 } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(!v.playing && tests.imgFrames === 0, "P pauses it (" + tests.imgFrames + " frames since)")
            TestInput.key(Qt.Key_Period, 0) } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(tests.imgFrames === 1, ". shows the next frame (" + tests.imgView().frame + ")")
            tests.imgStats = JSON.parse(Images.stats())
            tests.imgOpen("long.gif") } }
        // Too long to keep decoded (200 frames, 160 MB): played as it's
        // decoded, a few frames ahead.
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("long.gif"), 20000) }
        ScriptAction { script: tests.imgFrames = 0 }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const v = tests.imgView(), st = JSON.parse(Images.stats())
            tests.check(v.frameCount === 200 && v.playing && tests.imgFrames >= 15, "a long GIF plays: " + v.frameCount + " frames, " + tests.imgFrames + " shown in 1.5 s (at frame " + v.frame + ")")
            const kept = st.fullMB - tests.imgStats.fullMB
            tests.check(kept <= 2, "its frames aren't all kept (" + kept + " MB more decoded, not 160)")
            TestInput.key(Qt.Key_P, 0)
            tests.imgLongAt = v.frame } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(!v.playing && v.frame === tests.imgLongAt, "P pauses it at frame " + v.frame)
            // Stepping back decodes it again from the start, off the
            // window's thread: the keys come back at once.
            tests.imgT0 = Date.now()
            for (let k = 0; k < 4; k++)
                TestInput.key(Qt.Key_Comma, 0)
            tests.imgStepMs = Date.now() - tests.imgT0 } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgView().frame === (tests.imgLongAt + 196) % 200, 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.imgView()
            console.log("PERF four steps back on a long GIF:", tests.imgStepMs, "ms for the keys,", Date.now() - tests.imgT0, "ms to the frame")
            tests.check(tests.imgStepMs < 40, ", ×4 never waits for the frames (" + tests.imgStepMs + " ms for the keys)")
            tests.check(v.frame === (tests.imgLongAt + 196) % 200, ", ×4 steps back four frames, decoded again (" + v.frame + ", was " + tests.imgLongAt + ")")
            TestInput.key(Qt.Key_Period, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgView().frame === (tests.imgLongAt + 197) % 200, 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(v.frame === (tests.imgLongAt + 197) % 200, ". steps on again (" + v.frame + ")")
            tests.imgOpen("logo.png") } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("logo.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.imgView()
            const dpr = tests.imgDpr()
            tests.check(v.fit && Math.abs(v.zoom - Math.min(1, v.width * dpr / 400, v.height * dpr / 400)) < 1e-6, "a small image isn't enlarged to fit (" + v.zoom + ")")
            tests.imgT0 = Date.now()
            tests.imgStats = JSON.parse(Images.stats())
            TestInput.key(Qt.Key_G, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgThumbsReady(), 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            const shown = tests.imgThumbs().length
            const st = JSON.parse(Images.stats())
            console.log("PERF image grid:", shown, "thumbnails made in", Date.now() - tests.imgT0, "ms;", JSON.stringify(st))
            tests.check(viewer.gridMode && viewer.grid.count === 34 && viewer.grid.activeFocus, "G shows the folder's 34 images as a grid, with the keys")
            tests.check(tests.imgThumbsReady(), "every thumbnail in view is drawn (" + shown + ")")
            tests.check(st.diskThumbs >= shown - 1 && st.thumbsMade - tests.imgStats.thumbsMade >= shown - 1, "they're kept on disk (" + st.diskThumbs + " files)")
            tests.check(viewer.grid.currentIndex === 32, "the grid starts at the image shown (" + viewer.grid.currentIndex + ")")
            tests.check(Math.abs(viewer.gridShape - 800 / 1200) < 0.01 && viewer.grid.cellHeight < viewer.grid.cellWidth, "its cells are shaped like most of its pictures, 3:2 (" + viewer.gridShape.toFixed(3) + ", " + viewer.grid.cellWidth + "×" + viewer.grid.cellHeight + ")")
            tests.shotWindow("clicks-images-grid")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            tests.check(!viewer.gridMode && tests.imgView().activeFocus, "Esc goes back to the image")
            // Again, with only the disk cache.
            Images.forgetMemory()
            tests.imgStats = JSON.parse(Images.stats())
            tests.imgT0 = Date.now()
            TestInput.key(Qt.Key_G, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgThumbsReady(), 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const st = JSON.parse(Images.stats())
            const ms = Date.now() - tests.imgT0
            console.log("PERF image grid from the disk cache:", ms, "ms;", JSON.stringify(st))
            const read = st.thumbsFromDisk - tests.imgStats.thumbsFromDisk
            tests.check(read >= tests.imgThumbs().length - 1, "opened again, the thumbnails come from disk (" + read + " read, " + (st.thumbsMade - tests.imgStats.thumbsMade) + " made: those just past the view the first time)")
            const g = chat.fileEditor.imageViewer.grid
            g.currentIndex = 0
            TestInput.key(Qt.Key_L, 0)
            TestInput.key(Qt.Key_L, 0)
            TestInput.key(Qt.Key_Return, 0) } }
        ScriptAction { script: tests.waitUntil(imgSteps, () => tests.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(!chat.fileEditor.imageViewer.gridMode && Workspace.activeKey === "file:" + tests.imgDir + "/img1.png", "L L Enter opens the third image (" + Workspace.activeKey.split("/").pop() + ")")
            TestInput.key(Qt.Key_F, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(chat.fileEditor.imageViewer.filled && v.width === win.width, "F fills the window (" + v.width + " of " + win.width + ")")
            const back = tests.findName("imageFill")
            tests.check(back !== null && back.iconName === "fullscreen-exit", "and the way back shows in its bar, narrow too (" + (back ? back.iconName : "none") + ")")
            tests.shotWindow("clicks-images-filled")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.imgView()
            tests.check(!chat.fileEditor.imageViewer.filled && v.width < win.width && v.activeFocus, "Esc puts it back in the tab")
            windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            // File-type icons: glyphs of the bundled font, in their type's colour.
            const icons = tests.findAllNames("fileIcon", win.contentItem).filter(i => i.visible && i.text !== "")
            tests.check(icons.length >= 3, "file icons in the tabs and the files panel (" + icons.length + ")")
            tests.check(icons.every(i => i.fontInfo.family === "Harlo File Icons" && i.contentWidth > 6 && i.contentHeight >= 12),
                      "drawn from the bundled icon font (" + icons.map(i => i.fontInfo.family + " " + i.contentWidth.toFixed(0) + "×" + i.contentHeight.toFixed(0)).slice(0, 3).join(", ") + ")")
            const rs = Workspace.fileIcon("src/main.rs"), toml = Workspace.fileIcon("Cargo.toml"), png = Workspace.fileIcon("a.png")
            tests.check(rs.startsWith("#ff7043") && rs !== toml && toml !== png && png.length >= 8, "each type its own glyph and colour (" + [rs, toml, png].map(t => t.substring(0, 7)).join(" ") + ")")
            const pale = Theme.fileTint("#ffd54f")
            const contrast = (Math.max(Theme.relativeLuminance(pale), Theme.relativeLuminance(Theme.background)) + 0.05) / (Math.min(Theme.relativeLuminance(pale), Theme.relativeLuminance(Theme.background)) + 0.05)
            tests.check(contrast >= 2.9, "a pale colour still stands out from the background (" + contrast.toFixed(2) + ":1)")
            const st = JSON.parse(Images.stats())
            tests.check(st.full >= 1, "while it's open, decoded pictures are kept (" + st.full + ", " + st.fullMB + " MB)")
            // Every picture's tab closed: what was decoded goes.
            Workspace.tabs.map(t => t.split("\t")[0]).filter(k => k.startsWith("file:" + tests.imgDir + "/")).forEach(k => Workspace.closeTab(k)) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const st = JSON.parse(Images.stats())
            tests.check(st.full === 0 && st.fullMB === 0, "once no viewer shows one, no decoded picture is kept (" + st.full + ", " + st.fullMB + " MB)")
            tests.summary(); Qt.quit() } }
    }

    // Pictures in the chat: thumbnails for one you sent, one a tool
    // returned, a file a command wrote, a reply's Markdown images (one gone
    // from disk) and one a model made; a click opens the viewer over the
    // chat, stepping through them all; Open in tab; the composer's.
    function toolCardOf(it) { for (let q = it.parent; q; q = q.parent) if (q.objectName === "toolCard") return q; return null }
    function picTiles() { return tests.findAllNames("pictureTile", chat.transcript).filter(t => tests.shown(t)) }
    function picViewer() { return chat.transcript.chatPictures.viewer }
    property var picAll: []
    property real dragPointerY: 0
    SequentialAnimation {
        id: picSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickspictures"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            // (Tall: the whole conversation at once.)
            win.height = 2000
            tests.clickItem(tests.findText("Make me a sun picture", sidebar), "the session") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); tests.waitUntil(picSteps, () => tests.picTiles().length >= 8 && tests.picTiles().every(t => t.thumb.status === "ready" || t.thumb.status === "error"), 10000) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const tiles = tests.picTiles()
            const names = tiles.map(t => t.modelData.split("/").pop())
            tests.log("tiles:", tiles.map(t => t.modelData.split("/").pop() + "=" + t.thumb.status).join(" "), "all:", tests.findAllNames("pictureTile", chat.transcript).length, "pictures:", chat.session.sessionPictures())
            tests.check(tiles.length === 8, "8 thumbnails: sent, returned, written, two Markdown images, a named file, made, pasted (" + names.join(", ") + ")")
            const gone = tiles.filter(t => t.thumb.status === "error")
            tests.check(gone.length === 1 && gone[0].modelData.endsWith("/art/gone.png") && tests.findText("No longer on disk", gone[0]) !== null, "a picture gone from disk says so")
            tests.check(tiles.filter(t => t.thumb.status === "ready").length === 7, "the rest are drawn")
            {
                // A tool's pictures sit under its name, clear of the run's line.
                const inTool = tiles.filter(t => tests.toolCardOf(t) !== null)
                const bad = inTool.filter(t => { const card = tests.toolCardOf(t), mark = card ? tests.findName("toolMark", card) : null; return !mark || t.mapToItem(null, 0, 0).x < mark.mapToItem(null, mark.width, 0).x + 4 })
                tests.check(inTool.length > 0 && bad.length === 0, "a tool's thumbnails start past its mark (" + inTool.length + " in tools, " + bad.length + " over the line)")
                // Its tip keeps its words as the pointer leaves.
                const t0 = tiles[0]
                tests.check(t0.ToolTip.text !== "" && t0.ToolTip.text === chat.transcript.pictureLabel(t0.modelData), "a thumbnail's tip has its words whether or not it's hovered (" + t0.ToolTip.text + ")")
            }
            tests.shotWindow("clicks-pictures-thumbs")
            tests.check(tests.findText("paste-1.png", chat.transcript) === null, "a pasted picture shows as itself, not its file's name")
            tests.check(tests.findContaining("a yellow sun", chat.transcript) !== null, "a Markdown image's words stay in the reply")
            const all = JSON.parse(chat.session.sessionPictures())
            tests.picAll = all
            tests.clickItem(tiles.find(t => t.modelData === all[1]), "the returned picture") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(picSteps, () => tests.picViewer().view.status === "ready", 5000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.picViewer()
            tests.check(chat.transcript.chatPictures.opened && v.view.imageWidth === 800 && v.view.activeFocus, "a click opens it over the chat (" + v.view.imageWidth + "×" + v.view.imageHeight + ", keys " + v.view.activeFocus + ")")
            tests.check(v.images.length === 7 && v.index === 1, "it steps through the chat's 7 pictures, this the 2nd (" + (v.index + 1) + " / " + v.images.length + ")")
            tests.check(v.view.width >= chat.transcript.width - 1, "over the whole conversation")
            tests.shotWindow("clicks-pictures-viewer")
            const origin = tests.findName("imageOrigin", v), more = tests.findName("imageMore", v), close = tests.findName("imageClose", v)
            if (v.width < 640) {
                tests.check(origin === null && more !== null && close !== null && close.mapToItem(v, 0, 0).x > more.mapToItem(v, 0, 0).x, "narrow, the bar has ⋯ and close stays last, after it")
                v.moreMenu.popup(more, 0, 0)
            } else {
                tests.check(origin !== null && origin.text.startsWith("From "), "the bar says where it came from (" + (origin ? origin.text : "none") + ")")
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.picViewer()
            if (v.width < 640) {
                const item = tests.findName("imageOriginItem", v.moreMenu.contentItem)
                tests.check(item !== null && item.text.startsWith("From "), "and where it came from heads the ⋯ menu (" + (item ? item.text : "none") + ")")
                tests.shotWindow("clicks-pictures-more")
                tests.clickItem(tests.findText("All the chat's pictures", v.moreMenu.contentItem), "All the chat's pictures")
            }
        } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const v = tests.picViewer()
            if (v.width < 640) {
                tests.check(v.gridMode && v.grid.activeFocus, "the grid chosen from ⋯ opens with the keys")
                TestInput.key(Qt.Key_Escape, 0)
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = tests.picViewer()
            v.focusView()
            tests.check(v.view.activeFocus, "the image has the keys again")
            tests.imgEdges = tests.imgEdgeColours(v.view)
            for (let i = 0; i < 8; i++) TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const v = tests.picViewer()
            const edges = tests.imgEdgeColours(v.view)
            tests.check(v.view.zoom * 600 / tests.imgDpr() > v.view.height * 2 && edges[0] !== "" && edges[0] === tests.imgEdges[0] && edges[1] === tests.imgEdges[1],
                      "zoomed past its view, the picture is drawn only inside it: not over the bar below or the tabs above (" + edges + ", fitted " + tests.imgEdges + ", zoom " + v.view.zoom + ")")
            tests.shotWindow("clicks-pictures-zoomed")
            TestInput.key(Qt.Key_0, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.picViewer()
            tests.check(v.view.fit, "0 fits it again")
            TestInput.key(Qt.Key_Right, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(picSteps, () => tests.picViewer().view.status === "ready", 5000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = tests.picViewer()
            tests.check(v.path.endsWith("/art/sun.png") && v.view.imageWidth === 640 && chat.transcript.chatPictures.opened, "→ steps to the next picture, the file the command wrote")
            TestInput.key(Qt.Key_G, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = tests.picViewer()
            tests.check(v.gridMode && v.grid.count === 7, "G: all the chat's pictures as a grid (" + v.grid.count + ")")
            tests.shotWindow("clicks-pictures-grid")
            const names = tests.findAllNames("imageThumbName", v.grid).map(l => l.text)
            tests.check(names.length === 7 && names.indexOf("You sent · Make me a sun picture") >= 0 && names.indexOf("You sent · And this one?") >= 0 && names.indexOf("art/sun.png") >= 0 && names.every(n => !/[0-9a-f]{12}/.test(n)),
                      "the grid names each by where it came from, not its stored file's name (" + names.join(", ") + ")")
            const goneTile = tests.findAllNames("imageThumb", v.grid).find(t => t.path.endsWith("/art/gone.png"))
            tests.check(goneTile && goneTile.status === "error" && tests.findText("No longer on disk", goneTile.parent) !== null, "in the grid too, a picture gone from disk says so")
            const sun = tests.findAllNames("imageThumb", v.grid).find(t => t.path.endsWith("/art/sun.png"))
            tests.check(sun && sun.bottomAligned, "the grid's pictures sit on their names")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(!tests.picViewer().gridMode && chat.transcript.chatPictures.opened, "Esc leaves the grid first")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!chat.transcript.chatPictures.opened, "Esc again closes the viewer")
            // The header's Pictures: the grid without opening one first.
            const button = tests.findName("picturesButton", chat)
            const all = JSON.parse(chat.session.sessionPictures())
            tests.check(button && tests.shown(button) && chat.session.pictureCount === all.length, "the header has Pictures (" + chat.session.pictureCount + " of " + all.length + ")")
            tests.shotWindow("clicks-pictures-header")
            tests.clickItem(button, "Pictures") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = tests.picViewer()
            const all = JSON.parse(chat.session.sessionPictures())
            tests.check(chat.transcript.chatPictures.opened && v.gridMode && v.grid.currentIndex === all.length - 1 && v.grid.activeFocus,
                        "Pictures opens the grid, the latest marked, keys on it (" + v.grid.currentIndex + " of " + all.length + ")")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!chat.transcript.chatPictures.opened, "Esc twice closes it")
            tests.clickItem(tests.picTiles().find(t => t.modelData === tests.picAll[0]), "the sent picture") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = tests.picViewer()
            tests.check(v.index === 0 && v.view.imageWidth === 300, "the picture you sent opens (" + v.view.imageWidth + ")")
            TestInput.key(Qt.Key_T, 0) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(!chat.transcript.chatPictures.opened && Workspace.activeKey.startsWith("file:") && chat.fileEditor.imageViewer.view.imageWidth === 300, "T opens it in the file tab (" + Workspace.activeKey.split("/").pop() + ")")
            Workspace.open(Workspace.tabs.map(t => t.split("\t")[0]).find(k => !k.startsWith("file:")) || "")
            chat.session.addAttachment(win.argValue("--cwd=") + "/out/sun 2.png") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const thumbs = tests.findAllNames("attachmentThumb", chat).filter(t => tests.shown(t))
            tests.check(thumbs.length === 1 && thumbs[0].status === "ready", "a picture to send shows as a thumbnail (" + thumbs.map(t => t.status) + ")")
            tests.summary(); Qt.quit() } }
    }

    // Scrolls the file editor a frame at a time (clicksbigfile), noting how
    // long each frame took to draw.
    property var cvFrames: []
    Timer {
        id: cvScroll
        interval: 16
        repeat: true
        property int n: 0
        onTriggered: {
            const v = chat.fileEditor.editor
            tests.cvFrames.push(v.paintMs)
            v.contentY = v.contentY + 53
            if (++n >= 150) stop()
        }
    }

    // Multirun (demo harness): two lanes in worktrees, overview, keep one.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "multirun"
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                const id = Workspace.startRun(JSON.stringify({ projectId: 1, prompt: "Add a cache to the parser", starter: "", attachments: [],
                    worktrees: true, base: "main", lanes: [{ harness: "demo", model: "fast/small" }, { harness: "demo", model: "slow/large" }] }))
                tests.log("run:", id, "active:", Workspace.activeKey)
            }
        }
        PauseAnimation { duration: 3500 }
        ScriptAction {
            script: {
                const r = JSON.parse(Workspace.runJson(Workspace.activeKey.substring(9)))
                tests.log("lanes:", r.lanes.map(l => l.letter + " " + l.model + " " + l.status + " " + l.branch + " changes=" + l.changes).join(" | "))
                tests.shot("multirun")
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Workspace.keepRunLane(Workspace.activeKey.substring(9), 0) }
        PauseAnimation { duration: 3000 }
        ScriptAction {
            script: {
                const r = JSON.parse(Workspace.runJson(Workspace.activeKey.substring(9)))
                tests.log("after keep:", "kept", r.kept, r.lanes.map(l => l.letter + " gone=" + l.worktreeGone).join(" | "), "notice:", Workspace.notice)
                tests.shot("multirun-kept")
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Drop-down window and commands from later launches (--ipc-test).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "dropdown"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: chat.submit("stream a demo reply fast", "") }
        PauseAnimation { duration: 7000 }
        ScriptAction {
            script: {
                tests.log("dropdown:", win.dropDown ? "created " + (win.dropDown.toString().indexOf("Layer") >= 0 ? "layer" : "plain") + " visible " + win.dropDown.visible : "not created")
                if (win.dropDown) win.dropDown.contentItem.children[0].grabToImage(r => r.saveToFile(win.selftestDir + "/dropdown.png"))
            }
        }
        PauseAnimation { duration: 600 }
        ScriptAction { script: Qt.quit() }
    }
    Connections {
        target: Workspace
        function onRemoteCommand(command) { if (win.selftestDir !== "") tests.log("remote command:", command) }
    }

    // Session branches: header button, dialog, preview, continue in a new session.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "branches"
        PauseAnimation { duration: 3000 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("branches:", chat.branchCount, "rows:", chat.transcript.count); tests.shot("branches-header"); chat.branchDialog.show(win.argValue("--open="), "harlo", "/tmp") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("dialog:", chat.branchDialog.branches.map(b => (b.current ? "*" : "") + b.lastUser + "@" + b.divergesAt).join(" | "), "selected", chat.branchDialog.selected); tests.shotPopup(chat.branchDialog, "branches-dialog") } }
        PauseAnimation { duration: 500 }
        ScriptAction {
            script: {
                const d = chat.branchDialog
                const key = Workspace.continueBranch(d.file, d.current.leaf)
                d.close()
                tests.log("continued:", key.split("/").pop(), "active:", Workspace.activeKey === key)
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("new session rows:", chat.transcript.count, "branches:", chat.branchCount); tests.shot("branches-continued") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Search while the indexer is slow (each file 400 ms): typing never
    // waits for it, and the hits grow as it commits, without typing again.
    Timer {
        id: slowSearchTick
        interval: 16
        repeat: true
        property real last: 0
        property real worst: 0
        onTriggered: { const n = Date.now(); if (last > 0) worst = Math.max(worst, n - last); last = n }
    }
    property int slowSearchFirst: -1
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickssearchslow"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            slowSearchTick.start()
            searchDialog.openSearch()
            TestInput.typeText("walrus") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            tests.slowSearchFirst = searchDialog.hits.length
            tests.check(searchDialog.settled && tests.slowSearchFirst < 16, "typing answered at once, before the indexer finished (" + tests.slowSearchFirst + " of 16)")
            tests.check(slowSearchTick.worst < 300, "the window kept drawing while it searched (longest gap " + slowSearchTick.worst + " ms)")
            TestInput.typeText(" tusk") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(searchDialog.settled && searchDialog.query === "walrus tusk", "more typing answered too (" + searchDialog.hits.length + ")")
            TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0) } }
        PauseAnimation { duration: 8000 }
        ScriptAction { script: {
            tests.check(searchDialog.hits.length === 16 && tests.slowSearchFirst < 16, "the rest arrived late, as the indexer committed them (" + searchDialog.hits.length + ")")
            tests.check(slowSearchTick.worst < 300, "and the window never stalled (longest gap " + slowSearchTick.worst + " ms)")
            tests.shotWindow("search-slow-index")
            tests.summary(); Qt.quit() } }
    }

    // Full-text search over sessions: index, query, open at the message.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "search"
        PauseAnimation { duration: 4000 }
        ScriptAction {
            script: {
                searchDialog.openSearch()
                searchDialog.query = win.argValue("--query=")
                searchDialog.run()
            }
        }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.log("hits:", searchDialog.hits.length, "first:", searchDialog.hits.length ? searchDialog.hits[0].title + " #" + searchDialog.hits[0].index + " " + searchDialog.hits[0].snippet.slice(0, 80) : "-") }
        ScriptAction { script: tests.shotPopup(searchDialog, "search") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: searchDialog.pick(0) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("opened:", Workspace.activeKey.split("/").pop(), "rows:", chat.transcript.count, "pending:", chat.pendingReveal); tests.shot("search-opened") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Screenshots of the shortcuts sheet, the command palette and usage, for a
    // design review (not a test: SCENARIOS=kbshots KEEP=1 scripts/click-tests.sh 1280).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "kbshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            win.height = 900
            Workspace.addProject(win.argValue("--cwd=").replace(/\/repo$/, "/webapp")) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.clickText("Fix the login redirect loop", sidebar) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: shortcutsDialog.open() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("kb-shortcuts")
            const groups = win.shortcutList.concat(win.composerKeys).filter(s => !s.hidden).map(s => s.group)
            tests.check(groups.every(g => /^[A-Z][a-z ]*$/.test(g)), "shortcut groups are in sentence case")
            tests.check(win.shortcutList.concat(win.composerKeys).every(s => s.hidden || (s.icon && !s.glyph)), "every shortcut row has an icon, no glyph")
            const go = win.shortcutList.filter(s => s.label === "Go to a session or action")
            tests.check(go.length === 2 && go[0].group === go[1].group, "Ctrl+K and Ctrl+P are one row")
            tests.check(win.composerKeys.some(s => s.group === "Runs in a session"), "the run keys are listed")
            tests.check(win.composerKeys.filter(s => s.group === "Image viewer").length >= 12 && win.composerKeys.some(s => s.group === "Sidebar" && s.seq === "Esc"), "the image viewer's keys and Esc for a drag are listed")
            tests.check(!!tests.findName("shortcutTips", shortcutsDialog.contentItem), "the footer explains > and @")
            shortcutsDialog.close(); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: shortcutsDialog.open() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("kb-shortcuts-light")
            tests.check(!!tests.findName("shortcutsMore", shortcutsDialog.contentItem), "a fade says more is below")
            { const cap = tests.findText("Up", shortcutsDialog.contentItem); tests.check(!!cap && cap.children.some(c => c.text === "↑"), "arrow keys are shown as arrows") }
            tests.clickItem(tests.findText("Search all sessions", shortcutsDialog.contentItem), "the Search all sessions row") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(!shortcutsDialog.visible && searchDialog.visible, "clicking a row runs it")
            searchDialog.close(); commandPalette.openWith("") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.shotWindow("pal-empty-light"); commandPalette.close(); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: commandPalette.openWith("") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("pal-empty")
            const it = commandPalette.items
            tests.check(it[0].kind === "header" && it[0].title === "Recent sessions", "empty palette starts with Recent sessions")
            tests.check(it.some(i => i.kind === "header" && i.title === "Actions"), "then Actions")
            tests.check(it.filter(i => i.title === "New session in webapp").length === 1, "one New session in webapp row (" + it.filter(i => i.title === "New session in webapp").length + ")")
            tests.check(!it.some(i => i.title === "New demo session"), "no demo action")
            tests.check(commandPalette.current === 1, "the first session is chosen, not the heading")
            const firstAction = it.findIndex(i => i.kind === "action")
            for (let i = 1; i < firstAction - 2; i++) commandPalette.move(1)
            tests.check(commandPalette.current === firstAction - 2, "Down walks the sessions")
            commandPalette.move(1)
            tests.check(commandPalette.current === firstAction, "Down skips the Actions heading (" + commandPalette.current + " vs " + firstAction + ")")
            commandPalette.openWith("login") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.shotWindow("pal-query"); commandPalette.openWith("set") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("pal-query-action")
            tests.check(commandPalette.marked("Settings", "set") === "<b><font color='" + Theme.rgb(Theme.accent) + "'>Set</font></b>tings", "matched letters are marked")
            tests.check(commandPalette.marked("Close tab", "set").split("<b>").length === 3, "letters in order when no word matches")
            tests.check(commandPalette.marked("Add CSV export", "set").split("<b>").length === 2, "a loose match marks only where a word starts (" + commandPalette.marked("Add CSV export", "set") + ")")
            tests.check(commandPalette.choices[0].title === "Settings" && commandPalette.choices[2].title === "Close tab", "whole-word matches come first (" + commandPalette.choices.slice(0, 3).map(c => c.title).join(", ") + ")")
            const firstLoose = commandPalette.choices.findIndex(c => c.kind === "session" && commandPalette.strength(c.title, "set") === 0)
            tests.check(firstLoose < 0 || commandPalette.choices.slice(firstLoose).filter(c => c.kind === "session").every(c => commandPalette.strength(c.title, "set") === 0), "strong sessions before loose ones")
            tests.check(commandPalette.choices.filter(i => i.kind === "session").every(i => i.subtitle === ""), "one project: not said on every row")
            commandPalette.openWith(">") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("pal-actions")
            tests.check(commandPalette.items.length > 0 && !commandPalette.items.some(i => i.kind === "header"), "> lists actions without headings")
            commandPalette.openWith("zzqx") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.shotWindow("pal-none")
            tests.check(commandPalette.items.length === 0 && !!tests.findName("paletteNone", commandPalette.contentItem), "no matches says so")
            commandPalette.openWith("@auth") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.shotWindow("pal-files")
            tests.check(commandPalette.choices.length === 2 && commandPalette.choices[0].kind === "file", "@auth finds two files (" + commandPalette.choices.length + ")")
            tests.check(tests.findText("Insert", commandPalette.contentItem) === null, "file rows don't each say Insert")
            commandPalette.close(); Workspace.open("view:stats") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            tests.shotWindow("stats-top")
            const sv = chat.statsView, inner = tests.findName("statsInner", sv)
            const right = inner.mapToItem(sv, inner.width, 0).x
            tests.check(right <= sv.width - 8, "Usage fits its tab (" + Math.round(right) + " of " + sv.width + ")")
            tests.check(tests.findName("statsLeadValue", sv).text === "$11.69" || /^\$/.test(tests.findName("statsLeadValue", sv).text), "the lead is the cost")
            tests.check(/over 11 sessions/.test(tests.findName("statsLeadText", sv).text), "and says what it covers")
            tests.check(!/Helpers|helper/.test(tests.findName("statsFigures", sv).text), "no helper count when there are none")
            tests.check(!Qt.colorEqual(sv.cInput, sv.cCacheWrite) && !Qt.colorEqual(sv.cCacheRead, sv.cCacheWrite) && !Qt.colorEqual(sv.cInput, sv.cOutput), "the four token kinds have their own colours")
            tests.check(!tests.findText("unknown", sv), "no unknown model row")
            tests.check(!!tests.findName("statsRefresh", sv), "refresh is an icon button")
            tests.check(!Qt.colorEqual(sv.cCount, Theme.info) && !Qt.colorEqual(sv.cCount, Theme.primary), "counts aren't drawn in the input blue")
            tests.check(!!tests.findName("statsModelLegend", sv), "the models' colours have a legend")
            tests.check(tests.findText("Output", sv) !== null && tests.findText("Output (with reasoning)", sv) === null, "Output is called one thing")
            if (sv.width > 500) { const pr = tests.findName("statsProject", sv); tests.check(pr.y === 0, "the project picker sits beside the periods (y " + pr.y + ")") }
            chat.statsView.scrollToEnd() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.shotWindow("stats-bottom"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.shotWindow("stats-bottom-light"); Workspace.chooseThemeMode("dark"); settingsDialog.openPage("usage") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            tests.shotWindow("usage-settings")
            const sd = settingsDialog.contentItem
            tests.check(!!tests.findName("limitsRefresh", sd) && !!tests.findName("statsRefresh", sd) && tests.findText("Refresh", sd) === null, "one refresh style on the page")
            tests.check(!!tests.findText("Every session over the period you pick, read from their files; the spending limits above count today only.", sd), "Activity says it isn't today's spending")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }

    // Screenshots of the diff viewer and the session search, for a design
    // review (not a test: SCENARIOS=diffshots KEEP=1 scripts/click-tests.sh 1280).
    function diffShot(name) { tests.shotWindow("diff-" + name) }
    function diffSearch(q) { searchDialog.query = q; searchDialog.run() }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "diffshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            win.height = 900
            Workspace.addProject(win.argValue("--cwd=").replace(/\/repo$/, "/webapp")) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.clickText("Fix the login redirect loop", sidebar) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.diffShot("changes"); tests.clickText("session.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView
            // Beside the sidebar and Changes, the sides don't fit: inline.
            tests.check(dv.unified === !dv.splitFits && (win.width > 1500 || dv.unified), "inline when the sides don't fit (" + dv.width + " px, inline " + dv.unified + ")")
            tests.check(tests.findName("hunk-stage-0", dv) !== null && tests.findName("hunk-discard-0", dv) !== null, "each change has Stage and Discard buttons")
            tests.diffShot("file")
            tests.clickItem(tests.findName("diffNext"), "next change") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.diffShot("next"); tests.diffCollapseClick() } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.diffShot("collapsed")
            const dv = chat.diffView, u = dv.shownRows
            tests.reviewRow = u.findIndex(r => r[5] === 1 && r[14] !== 1)
            tests.clickAt(tests.findName((dv.unified ? "gutter-u-" : "gutter-r-") + tests.reviewRow, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.clickItem(tests.findName("commentText", chat.diffView.commentBox.contentItem), "the comment")
            TestInput.typeText("Should this also check the cookie's secure flag? It only matters behind a proxy that ends TLS.") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.diffShot("comment-box"); tests.clickItem(tests.findName("commentSave", chat.diffView.commentBox.contentItem), "Comment") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(tests.findName("diffNote-0", dv) !== null, "the saved comment shows under its line")
            tests.diffShot("review-bar")
            // Wide (no sidebar, no panel): side by side.
            windowSettings.sidebarOpen = false
            windowSettings.rightMode = "" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const dv = chat.diffView
            tests.check(dv.unified === !dv.splitFits, "side by side when it fits (" + dv.width + " px, inline " + dv.unified + ")")
            tests.diffShot("wide")
            tests.clickItem(tests.findName("diffMore"), "More") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.diffShot("more-menu"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { windowSettings.sidebarOpen = true; windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.diffShot("rail"); tests.clickText("return-to.test.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.diffShot("new-file"); tests.clickText("guard.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.diffShot("small"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.diffShot("small-light"); tests.clickText("session.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.diffShot("collapsed-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 900 }
        // The sidebar's filter, and from it the messages.
        ScriptAction { script: { tests.clickItem(tests.findName("sessionFilter"), "Filter sessions"); TestInput.typeText("cookie") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const row = tests.findName("filterSearchMessages")
            tests.check(row !== null && row.visible, "filtering offers the messages search")
            tests.shotWindow("search-filter")
            tests.clickItem(row, "Search messages for") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.check(searchDialog.visible && searchDialog.query === "cookie" && searchDialog.hits.length > 0, "it opens the search with those words (" + searchDialog.hits.length + " hits)")
            const f = tests.findName("sessionFilter"); f.text = ""
            tests.diffSearch("") } }
        PauseAnimation { duration: 600 }
        // Search: empty (recent sessions), results, none, then a hit opened.
        ScriptAction { script: {
            tests.check(searchDialog.recents.length === 4 && tests.findName("searchHint") !== null, "empty, it says what it searches and offers recent sessions (" + searchDialog.recents.length + ")")
            tests.shotWindow("search-empty")
            tests.diffSearch("returnTo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const h = searchDialog.hits
            tests.check(h.length > 0 && h.every(x => x.snippet.indexOf("`") < 0), "snippets without markdown")
            tests.shotWindow("search-results")
            searchDialog.current = Math.min(2, h.length - 1) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.shotWindow("search-chosen"); tests.diffSearch("kubernetes") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.shotWindow("search-none"); Workspace.chooseThemeMode("light"); tests.diffSearch("returnTo") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.shotWindow("search-results-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const i = searchDialog.hits.findIndex(h => (h.title || "").indexOf("security review") >= 0 && h.snippet.indexOf("evil") >= 0)
            searchDialog.pick(Math.max(0, i)) } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            tests.check(chat.transcript.wordMarks.length > 0, "the matched words are marked where it lands (" + chat.transcript.wordMarks.length + ")")
            tests.shotWindow("search-opened")
            chat.openFind()
            TestInput.typeText("cookie") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const fb = chat.findBar
            tests.check(fb.visible && fb.hits.length === 3 && fb.current >= 0 && chat.transcript.wordMarks.length > 0, "find in the session: every message with the word, the one shown marked (" + fb.hits.length + ")")
            tests.shotWindow("find")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.shotWindow("find-next")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(!chat.findBar.visible && chat.transcript.wordMarks.length === 0, "Esc closes it and clears the marks")
            tests.summary(); Qt.quit() } }
    }

    // Todos and queue editing (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "extras"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("make a plan with todos", "") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { const d = chat.todoDock; tests.log("todos:", d.done + "/" + d.tasks.length, "current:", d.current ? d.current.content : "-"); d.expanded = true } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.shot("todos"); chat.submit("queue demo", "") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("queue:", chat.session.queued.join(" | ")); chat.session.promoteQueued(1) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { tests.log("after promote:", chat.session.queued.join(" | ")); chat.session.removeQueued(0) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.log("after remove:", chat.session.queued.join(" | ")) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Subagents (demo harness): dock, transcript dialog, steer, cancel.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "subagents"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("start some subagents", "") }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { tests.log("dock:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" "), "running", chat.subagentDock.running); tests.shot("subagents-running") } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { tests.log("later:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" ")); chat.subagentDialog.show(chat.subagentDock.agents[1], true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("dialog rows:", chat.subagentDialog.contentItem ? "open" : "-", "running", chat.subagentDialog.running); chat.subagentDialog.background.parent.grabToImage(r => r.saveToFile(win.selftestDir + "/subagent-dialog.png")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { chat.session.steerSubagent("BuildOtter", "Only run the unit tests"); chat.session.cancelSubagent("BuildOtter") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { tests.log("after cancel:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" "), "msg", chat.session.subagentMessage); chat.subagentDialog.close() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.shot("subagents-done") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // An open image reloads when the file changes, and notes a deletion.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "imagereload"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.openFileTab(win.argValue("--image=")) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.log("before:", chat.fileEditor.imageInfo) }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: { tests.log("after change:", chat.fileEditor.imageInfo, "version", chat.fileEditor.imageVersion); tests.shot("image-changed") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: { tests.log("after delete: gone", chat.fileEditor.imageGone); tests.shot("image-gone") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Terminal panel: a real shell in the session's folder.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "terminal"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); windowSettings.terminalOpen = false } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.toggleTerminal() }
        PauseAnimation { duration: 2500 }
        ScriptAction {
            script: {
                const t = terminal.current
                tests.log("terminal: running", t.running, "cols", t.cols, "rows", t.rows, "cwd", t.session.cwd.split("/").pop(), "focus", t.activeFocus)
                t.session.paste("clear; printf '\\033[31mred\\033[0m \\033[1;32mbold green\\033[0m \\033[44m on blue \\033[0m\\n'; ls -la --color=always | head -5\n")
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                const s = terminal.current.session
                for (const ch of "echo typed") s.key(ch.toUpperCase().charCodeAt(0), 0, ch)
                s.key(Qt.Key_Return, 0, "\r")
            }
        }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: tests.shot("terminal") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Workspace.chooseThemeMode("light") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: tests.shot("terminal-light") }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                Workspace.chooseThemeMode("dark")
                const s = terminal.current.session
                for (const ch of "exit") s.key(ch.toUpperCase().charCodeAt(0), 0, ch)
                s.key(Qt.Key_Return, 0, "\r")
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("after exit: running", terminal.current.running, "text", terminal.current.session.exitText); tests.shot("terminal-exited") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // Commit message written by a (stand-in) Harlo: --fake-harlo=<script>.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "commitmsg"
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.newSession(1, "demo")
                windowSettings.rightMode = "changes"
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("changes:", git.unstagedCount, "harness:", changes.harness); git.generateMessage(win.argValue("--fake-harlo="), "", false); tests.log("generating:", git.generating) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.shot("commitmsg-writing") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("message:", JSON.stringify(changes.commitText), "generating:", git.generating, "error:", git.error, git.message); tests.shot("commitmsg-done") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // New-session page: empty state, project/branch, starter, send, worktree.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "draft"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { tests.log("empty: draft visible", chat.draftView.visible, "tabs", Workspace.tabs.length); tests.shot("draft-empty") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); win.openDraft(1) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { const d = chat.draftView; tests.log("project:", d.project ? d.project.name : "-", "branch:", d.currentBranch, "default:", d.atDefaultBranch, "tab:", Workspace.activeKey); tests.shot("draft-project") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const d = chat.draftView; d.starter = "explore"; d.text = "Focus on the build setup"; d.harness = "demo" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shot("draft-starter") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.draftView.send() }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("sent: active", Workspace.activeKey, "tabs", Workspace.tabs.map(t => t.split("\t")[1]).join(" | "), "cwd", chat.session.cwd.split("/").pop()); tests.shot("draft-sent") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.openDraft(1); const d = chat.draftView; d.harness = "demo"; d.useWorktree = true; d.worktreeName = "harlo/draft-test"; d.text = "hello from a worktree" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.shot("draft-worktree"); chat.draftView.send() } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { tests.log("worktree: active", Workspace.activeKey, "cwd", chat.session.cwd, "notice", Workspace.notice); tests.shot("draft-worktree-sent") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // `ask` questions and a `select` with descriptions (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "ask"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.submit("please ask me something", "") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("request:", chat.session.requestMethod, "questions:", JSON.parse(chat.session.requestJson).questions.length, "complete:", chat.askPanel.complete); tests.shot("ask-open") } }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                const p = chat.askPanel
                p.toggle(0, "Postgres")
                p.previewText = "DATABASE_URL=postgres://localhost/harlo"
                p.toggle(1, "Search"); p.toggle(1, "Export")
                p.setCustom(1, "Offline mode")
                tests.log("answers:", JSON.stringify(p.answers), "complete:", p.complete)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shot("ask-answered") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.askPanel.submit() }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("after send: hasRequest", chat.session.hasRequest); chat.submit("now choose one", "") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("select request:", chat.session.requestMethod, chat.session.requestOptions.join("|")); tests.shot("ask-select") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Merge") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("after select: hasRequest", chat.session.hasRequest); tests.shot("ask-done") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Fonts: the desktop's by default; a chosen interface and code font.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "fonts"
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                tests.log("system ui:", Workspace.systemFont(false), "| system mono:", Workspace.systemFont(true),
                        "| families:", Workspace.fontFamilies(false).length, "fixed:", Workspace.fontFamilies(true).length,
                        "| Theme:", Theme.uiFont, "/", Theme.mono, "| app font:", Qt.application.font.family)
                Workspace.newSession(-1, "demo")
            }
        }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("stream a demo reply fast", "") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.shot("fonts-system") }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                Workspace.chooseFont("ui", win.argValue("--ui-font="))
                Workspace.chooseFont("mono", win.argValue("--mono-font="))
                tests.log("chosen:", Theme.uiFont, "/", Theme.mono, "| app font:", Qt.application.font.family)
            }
        }
        PauseAnimation { duration: 800 }
        ScriptAction { script: tests.shot("fonts-chosen") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.openPage("appearance") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/fonts-settings.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Settings window: every page, a theme switch, light mode, search.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "settings"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { Workspace.newSession(-1, "demo"); settingsDialog.open() } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("settings open:", settingsDialog.visible, "page:", settingsDialog.page, "theme:", Workspace.themeFamily, Workspace.themeMode, Workspace.textScale); settingsDialog.grab(win.selftestDir + "/settings-general.png") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "appearance" }
        PauseAnimation { duration: 600 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-appearance.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.chooseTheme("GitHub"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-github-light.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.chooseThemeMode("dark"); Workspace.chooseTheme("Ember") } }
        ScriptAction { script: settingsDialog.page = "notifications" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-notifications.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "shortcuts" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-shortcuts.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "usage" }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-usage.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "worktrees" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-worktrees.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "git" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-git.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.page = "harnesses" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: settingsDialog.grab(win.selftestDir + "/settings-harnesses.png") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.query = "font" }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.log("search 'font':", settingsDialog.shownPages.map(p => p.id).join(","), "page:", settingsDialog.page); settingsDialog.grab(win.selftestDir + "/settings-search.png") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { settingsDialog.query = ""; settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.log("closed:", !settingsDialog.visible, "tabs:", tests.tabState()); Qt.quit() } }
    }

    // Uncommitted-change pills on session rows.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "dirty"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 2500 }
        ScriptAction {
            script: {
                const rows = []
                for (let i = 0; i < 6; i++) { const k = Workspace.keyAt(i); if (k !== "") rows.push(k) }
                tests.log("rows:", rows.join(" | "))
                tests.shot("dirty")
            }
        }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                const repo = Workspace.sessionRepo("live:1")
                tests.log("repo of live:1:", repo, "ignored:", Workspace.isRepoIgnored(repo))
                Workspace.setRepoIgnored(repo, true)
            }
        }
        PauseAnimation { duration: 1200 }
        ScriptAction {
            script: {
                tests.shot("dirty-ignored")
                const m = sidebar.sessionMenu
                m.key = "live:1"; m.sessionTitle = "New session"; m.live = true; m.branch = ""
                m.repo = Workspace.sessionRepo("live:1"); m.repoIgnored = Workspace.isRepoIgnored(m.repo)
                m.popup(sidebar, 120, 140)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.shotPopup(sidebar.sessionMenu, "dirty-menu"); sidebar.sessionMenu.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { if (win.argValue("--unignore=") === "1") Workspace.setRepoIgnored(Workspace.sessionRepo("live:1"), false) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: tests.shot("dirty-after") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Attachments: any file; chips in the composer and in the sent message.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "attach"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                const dir = win.argValue("--cwd=")
                const results = ["notes.md", "photo.png", "blob.bin", "src/main.rs", "src", "missing.txt"].map(f => f + "=" + chat.session.addAttachment("file://" + dir + "/" + f))
                tests.log("added:", results.join(" "), "| attachments:", chat.session.attachments.length)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shot("attach-composer") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.submit("Please review these files.", "") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("after send: attachments", chat.session.attachments.length); tests.shot("attach-sent") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Themed menus and dialogs.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "menus"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { sidebar.sessionMenu.sessionTitle = "A session"; sidebar.sessionMenu.live = false; sidebar.sessionMenu.popup(sidebar, 120, 140) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.sessionMenu, "menu-session") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.sessionMenu.close(); sidebar.projectMenu.harness = "harlo"; sidebar.projectMenu.popup(sidebar, 120, 110) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.projectMenu, "menu-project") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.projectMenu.close(); sidebar.renameDialog.ask("x", "create a file hello.py that prints hello") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.renameDialog, "dialog-rename") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.renameDialog.close(); sidebar.trashDialog.label = "create a file hello.py that prints hello"; sidebar.trashDialog.open() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.trashDialog, "dialog-trash") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.trashDialog.close(); sidebar.openNewWorktree(-1, "houdini-mcp", "harlo") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.worktreeDialogs.newDialog, "dialog-worktree") }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                const d = sidebar.worktreeDialogs
                d.newDialog.close()
                d.finishDialog.info = { branch: "harlo-fuzzy-harbor", path: "/home/me/.local/share/harlo/worktrees/repo/harlo-fuzzy-harbor", exists: true, ahead: 2, behind: 0, dirty: 1, base: "main", mainBranch: "main" }
                d.finishDialog.open()
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.worktreeDialogs.finishDialog, "dialog-finish") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.worktreeDialogs.finishDialog.close(); sidebar.worktreeDialogs.openSetup(-1, "houdini-mcp") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.shotPopup(sidebar.worktreeDialogs.setupDialog, "dialog-setup") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // File-type icons in the Files tree, a file tab and the Changes panel.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "fileicons"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { files.browser.toggle(1); Workspace.openFileTab(files.browser.folder + "/Cargo.toml") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: tests.shot("fileicons-files") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: windowSettings.rightMode = "changes" }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.shot("fileicons-changes") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Bottom-left buttons: usage stats, settings placeholder, shortcuts dialog.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "stats"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.open("view:stats") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { tests.log("stats tab:", tests.tabState(), "covered:", chat.covered); tests.shot("stats") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.statsView.scrollToEnd() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.shot("stats-bottom") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.width = 760; win.height = 900 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { tests.log("narrow: win", win.width, "sidebar", sidebar.width, "chat", chat.width, "panel", rightPanel.visible ? rightPanel.width : 0); tests.shot("stats-narrow") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.width = 1400; win.height = 900 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { shortcutsDialog.open(); tests.log("shortcuts rows:", shortcutsDialog.rowCount) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { shortcutsDialog.grab(win.selftestDir + "/shortcuts.png"); tests.shot("shortcuts-window") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: Qt.quit() }
    }

    // Real sessions: sidebar from disk, then open the newest session read-only
    // (no harness process is started; nothing is sent).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "sidebar"
        PauseAnimation { duration: 2500 }
        ScriptAction { script: tests.log("sidebar rows:", tests.sidebarCount(), "sessions:", Workspace.sessionCount, "projects:", Workspace.projectCount) }
        ScriptAction { script: tests.shot("sidebar") }
        ScriptAction {
            script: {
                const explicit = win.argValue("--open=")
                if (explicit !== "") { tests.log("opening", explicit); Workspace.open(explicit); return }
                for (let i = 0; i < 500; i++) {
                    const k = Workspace.keyAt(i)
                    if (k === "") break
                    if (k.startsWith("/")) { tests.log("opening", k); Workspace.open(k); break }
                }
            }
        }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: tests.log("opened:", chat.session.key, "status:", chat.session.status, "title:", chat.session.title, "rows:", chat.transcript.count) }
        ScriptAction { script: tests.shot("saved-session") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // ---- Harlo with a scripted model (scripts/fake-harlo-model.py, set up
    // by click-tests.sh: Harlo's providers.json in the test's config) ----
    /// The first shown item under `root` whose text contains `part`.
    function findContaining(part, root) {
        for (const r of searchRoots(root)) {
            const f = findContainingIn(part, r)
            if (f) return f
        }
        return null
    }
    function findContainingIn(part, root) {
        for (const c of root.children) {
            if (!c.visible || c.opacity <= 0) continue
            if (typeof c.text === "string" && c.text.indexOf(part) >= 0 && shown(c)) return c
            const f = findContainingIn(part, c)
            if (f) return f
        }
        return null
    }
    /// The scripted model's requests so far (its log, one JSON per line).
    function modelRequests() {
        return (TestInput.readFile(win.selftestDir + "/model-log.jsonl") || "").split("\n").filter(l => l !== "").map(l => JSON.parse(l).body)
    }
    property string harloKey: ""
    property string approvedHelper: ""

    // Harlo's question card, a side question (/btw), a background job whose
    // end starts a run by itself, and a destructive command asking in yolo.
    SequentialAnimation {
        id: harloSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlo"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            // (Background commands run without asking in yolo.)
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.check(chat.session.harness === "harlo" && chat.session.modelName === "Fake Coder", "a Harlo session on the scripted model (" + chat.session.modelName + ")")
            tests.sendByKeys("please ask me something") } }
        PauseAnimation { duration: 300 }
        // The ask card.
        ScriptAction { script: tests.waitUntil(harloSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.requestMethod === "ask" && tests.findText("Which colour should the button be?", chat.askPanel) !== null, "Harlo's question shows in the question card")
            tests.shotWindow("clicks-harlo-ask")
            tests.clickText("Blue", chat.askPanel) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickText("Send answers", chat.askPanel) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSteps, () => tests.chatText().indexOf("Done:") >= 0, 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const asked = tests.modelRequests().filter(b => (b.messages || []).some(m => m.role === "tool" && JSON.stringify(m.content).indexOf("Blue") >= 0))
            tests.check(!chat.session.hasRequest && asked.length > 0, "the answer goes back to the model (" + tests.chatText().slice(-120) + ")")
            const askRes = tests.findAllNames("toolResult", chat.transcript).map(l => l.text)
            tests.check(askRes.indexOf("Blue") >= 0, "the question's line shows what was answered (" + askRes.join(" | ") + ")")
            tests.sendByKeys("/btw which colour did I pick?") } }
        PauseAnimation { duration: 300 }
        // The side question: a card above the composer, not in the conversation.
        ScriptAction { script: tests.waitUntil(harloSteps, () => chat.session.asideJson !== "" && JSON.parse(chat.session.asideJson).done, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const card = tests.findName("asideCard")
            const a = chat.session.asideJson !== "" ? JSON.parse(chat.session.asideJson) : {}
            tests.check(card !== null && (a.text || "").indexOf("Aside: which colour did I pick?") === 0, "/btw answers in the card above the composer (" + JSON.stringify(a).slice(0, 160) + ")")
            tests.check(tests.chatText().indexOf("Aside:") < 0, "and not in the conversation")
            const foot = tests.findText("A side question: not part of the conversation, and not kept.")
            tests.check(foot && !foot.truncated && foot.mapToItem(null, foot.contentWidth, 0).x <= tests.findName("asideCard").mapToItem(null, tests.findName("asideCard").width, 0).x + 1, "the aside's footnote stays inside its card")
            tests.shotWindow("clicks-harlo-btw")
            tests.clickItem(tests.findName("asideDismiss"), "dismiss the aside") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findName("asideCard") === null, "dismissing it hides the card")
            tests.sendByKeys("quote 2>&1; and <b>bold</b> here") } }
        PauseAnimation { duration: 300 }
        // A reply quoting a shell redirect: its Context row shows it as
        // text (unescaped, "&1;" read as markup and Qt warned).
        ScriptAction { script: tests.waitUntil(harloSteps, () => tests.chatText().indexOf("ok: quote") >= 0, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.harloRightWas = windowSettings.rightMode; windowSettings.rightMode = "context" } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const rows = tests.findAllNames("contextRowLabel", win.contentItem).map(l => l.text)
            tests.check(rows.some(t => t.indexOf("ok: quote 2>&amp;1; and &lt;b>bold&lt;/b> here") >= 0), "a reply's Context row shows its 2>&1; and <b> as text (" + rows.slice(-2).join(" | ").slice(0, 200) + ")")
            windowSettings.rightMode = tests.harloRightWas
            tests.sendByKeys("run the build in the background") } }
        PauseAnimation { duration: 300 }
        // A background job: the turn ends at once; its end starts a run.
        ScriptAction { script: tests.waitUntil(harloSteps, () => tests.chatText().indexOf("The background job ended") >= 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("Started in the background as j1") >= 0, "the command starts as a job (" + t.slice(-300) + ")")
            tests.check(t.indexOf("The background job ended: built-ok") >= 0, "its end comes as a notice, with its output, and starts a run by itself")
            const bg = tests.findAllNames("backgroundNotice", chat.transcript).filter(n => n.visible)
            const head = bg.length ? tests.findName("backgroundHeadline", bg[0]) : null
            const cmd = bg.length ? tests.findName("backgroundCommand", bg[0]) : null
            tests.check(bg.length === 1 && head && head.text === "Background command finished" && cmd && cmd.text === "$ sleep 1; echo built-ok" && tests.findContaining("read or grep it", chat.transcript) === null, "the notice shows how it went and its command, apart (" + (head ? head.text : "none") + " | " + (cmd ? cmd.text : "none") + ")")
            tests.shotWindow("clicks-harlo-job")
            // Its one line, cut short to the well: a click shows all of it.
            const w = tests.findAllNames("toolWhat", chat.transcript).find(l => l.plain.indexOf("echo built-ok") >= 0)
            let card = w
            while (card && card.cutLines === undefined) card = card.parent
            tests.harloJobCard = card
            tests.check(card && card.cutLines > 0 && card.canOpen, "a line cut short to its well opens the call (" + (card ? card.cutLines : "none") + ")")
            if (w) tests.clickItem(w, "the job's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const card = tests.harloJobCard
            tests.check(card && card.open && tests.findAllNames("toolTail", card).length === 0 && tests.findContaining("j1.log", card) !== null, "opened: all of it, the log's path too")
            tests.shotWindow("clicks-harlo-job-open")
            tests.sendByKeys("now reset hard") } }
        PauseAnimation { duration: 300 }
        // A destructive command asks, even in yolo, without Always allow.
        ScriptAction { script: tests.waitUntil(harloSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.approvalMode(tests.harloKey) === "yolo" && chat.session.requestIsApproval, "git reset --hard asks in yolo")
            tests.check(tests.findContaining("can't be undone", chat.transcript) !== null, "and says why")
            tests.check(tests.findAllNames("approveInline", chat.transcript).some(b => b.text === "Run anyway"), "its confirm names what it does, not Approve")
            tests.check(tests.findName("alwaysAllowInline", chat.transcript) === null && tests.findText("Always allow") === null, "with no Always allow")
            const q = tests.findAllNames("approvalQuestion", chat.transcript)[0]
            const grid = q ? q.parent : null
            tests.check(q && !q.truncated && (win.width < 700 ? grid.columns === 1 && q.lineCount === 1 : grid.columns === 2), "the question beside its answers, or over them when narrow (" + (grid ? grid.columns : "none") + ")")
            tests.shotWindow("clicks-harlo-destructive")
            tests.clickText("Deny", chat.transcript) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSteps, () => !chat.session.hasRequest && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("toolMark", chat.transcript).some(m => m.name === "close"), "Deny: it isn't run (" + tests.chatText().slice(-160) + ")")
            // A file too big to read whole: its outline, each row its line.
            const big = ["// The ledger.", "", "pub struct Ledger {", "    id: u32,", "}", "", "impl Ledger {", "    pub fn post(&self) {}", "}"]
            for (let i = big.length; i < 2099; i++) big.push("// filler " + i)
            big.push("fn last() {}")
            TestInput.writeFile(win.argValue("--cwd=") + "/big.rs", big.join("\n") + "\n")
            tests.sendByKeys("read the big file") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSteps, () => tests.chatText().indexOf("Its outline follows") >= 0 && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.shotWindow("clicks-harlo-outline")
            const views = tests.findAllNames("codeView", chat.transcript)
            const code = views.length ? views[views.length - 1].lines : []
            tests.check(JSON.stringify(code.map(l => l.n)) === JSON.stringify(["3", "7", "8", "2100"]) && code[1].h.indexOf("Ledger") >= 0, "a big file's read: its outline, each definition at its own line (" + JSON.stringify(code.map(l => l.n)) + ")")
            tests.check(tests.findAllNames("toolResult", chat.transcript).some(l => l.text === "2100 lines, outline"), "its card says it's the outline of 2100 lines")
            tests.summary(); Qt.quit() } }
    }

    // The wiki with Harlo and Claude Code: with Wiki on for a project that
    // has one (.okf/), both are told of it through harlo's tools, with the
    // pages whose code changed since; neither gets a wiki tool. With Wiki
    // off they aren't told. An update with nothing owed starts no session;
    // one with work runs a session of its own, which alone gets wiki_check.
    property int wikiProject: -1
    property int wikiDone: 0
    property int wikiRequests: 0
    function doneCount() { return tests.chatText().split("Done:").length - 1 }
    function sendWiki(text) { tests.wikiDone = tests.doneCount(); tests.sendByKeys(text) }
    function wikiAnswered() { return tests.doneCount() > tests.wikiDone && chat.session.status === "idle" }
    // What the last tool said, as the model got it.
    function lastToolResult() {
        const reqs = tests.modelRequests()
        const msgs = (reqs[reqs.length - 1] || {}).messages || []
        const tools = msgs.filter(m => m.role === "tool")
        const last = tools[tools.length - 1]
        return last ? (typeof last.content === "string" ? last.content : JSON.stringify(last.content)) : ""
    }
    // The system lines of the model's request `i`, and its wiki tools.
    /// Wiki update sessions so far: their first requests to the model.
    function wikiUpdates() { return tests.modelRequests().filter(r => { const m = (r.messages || []).filter(x => x.role === "user"); return m.length === 1 && !(r.messages || []).some(x => x.role === "assistant") && JSON.stringify(m[0]).indexOf("Bring this repository's wiki") >= 0 }).length }
    /// The wiki of the project being tested is current, with no session writing it.
    function wikiSettled() { const st = JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)); return !st.running && !!st.work && st.work.current }
    function wikiSaid(i) { return JSON.stringify(((tests.modelRequests()[i] || {}).messages || []).filter(m => m.role === "system")) }
    function wikiTools(i) { return ((tests.modelRequests()[i] || {}).tools || []).map(t => t.function.name).filter(n => n.indexOf("wiki") >= 0) }
    /// Opens a new Harlo session in the wiki's project, its requests counted from here.
    function wikiHello() {
        tests.wikiRequests = tests.modelRequests().length
        tests.harloKey = Workspace.newSession(tests.wikiProject, "harlo")
        Workspace.setApprovalMode(tests.harloKey, "yolo")
    }
    // Code lookup: off as it comes, neither Harlo nor Claude Code gets the
    // lookup tool; switched on for the project (Settings → Projects), both
    // do, and it answers from harlo's code index.
    property int lookupProject: -1
    function lookupHello() {
        tests.wikiRequests = tests.modelRequests().length
        tests.harloKey = Workspace.newSession(tests.lookupProject, "harlo")
        Workspace.setApprovalMode(tests.harloKey, "yolo")
    }
    function lookupTools(i) { return ((tests.modelRequests()[i] || {}).tools || []).map(t => t.function.name).filter(n => n.indexOf("lookup") >= 0) }
    SequentialAnimation {
        id: harloLookupSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlolookup"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.lookupProject = JSON.parse(Workspace.projectsJson())[0].id
            tests.check(Workspace.setting("lookup_" + tests.lookupProject) === "", "Code lookup not set (off as it comes)")
            tests.lookupHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(tests.lookupTools(tests.wikiRequests).length === 0, "off: no lookup tool reaches Harlo (" + tests.lookupTools(tests.wikiRequests).join(", ") + ")")
            tests.claudeKey = Workspace.newSession(tests.lookupProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("lookup note") }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => tests.chatText().indexOf("Lookup tools:") >= 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Lookup tools: none") >= 0, "nor Claude Code (" + tests.chatText().slice(-120) + ")")
            // On, from Settings → Projects.
            settingsDialog.projectId = tests.lookupProject
            settingsDialog.openPage("projects") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const sw = tests.findName("projectLookup", settingsDialog.contentItem)
            const f = tests.findName("settingsPage", settingsDialog.contentItem)
            if (sw && f) f.contentY = Math.max(0, Math.min(sw.mapToItem(f.contentItem, 0, 0).y - 160, f.contentHeight - f.height))
            tests.check(sw !== null && tests.shown(sw) && !sw.checked, "Settings → Projects has Code lookup, off (" + (sw ? sw.checked : "no switch") + ")")
            if (sw) tests.clickItem(sw, "Code lookup") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.setting("lookup_" + tests.lookupProject) === "1", "clicked, it's on for the project (" + Workspace.setting("lookup_" + tests.lookupProject) + ")")
            tests.shotWindow("clicks-harlo-lookup-setting")
            settingsDialog.close()
            tests.lookupHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { tests.wikiDone = tests.doneCount(); tests.sendByKeys("look up retry") } }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => tests.wikiAnswered(), 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.lookupTools(tests.wikiRequests).length === 1 && tests.lookupTools(tests.wikiRequests)[0] === "lookup", "on: Harlo gets the lookup tool (" + tests.lookupTools(tests.wikiRequests).join(", ") + ")")
            const said = tests.lastToolResult()
            tests.check(said.indexOf("fn retry  src/jobs.rs:1-3") >= 0 && said.indexOf("pub fn retry()") >= 0, "and its card comes from the code index (" + said.slice(0, 160) + ")")
            tests.shotWindow("clicks-harlo-lookup")
            tests.claudeKey = Workspace.newSession(tests.lookupProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("lookup note") }
        ScriptAction { script: tests.waitUntil(harloLookupSteps, () => tests.chatText().indexOf("Lookup tools:") >= 0, 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(/Lookup tools: \S*lookup; retry: fn retry  src\/jobs\.rs:1-3/.test(t), "Claude Code gets it through harlo's tools, the same card (" + t.slice(t.indexOf("Lookup tools:"), t.indexOf("Lookup tools:") + 160) + ")")
            tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: harloWikiSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowiki"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            // (Wiki left as it comes: on, the project having a wiki.)
            tests.check(Workspace.setting("wiki_" + tests.wikiProject) === "", "the Wiki setting not set")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const said = tests.wikiSaid(tests.wikiRequests)
            tests.check(tests.wikiTools(tests.wikiRequests).length === 0, "Wiki on (as it comes): no wiki tool reaches Harlo (" + tests.wikiTools(tests.wikiRequests).join(", ") + ")")
            tests.check(said.indexOf("This project has a wiki in .okf/") >= 0 && said.indexOf("check that code before relying on it") >= 0, "but a few lines on the wiki and when to read it")
            tests.check(said.indexOf("Out of date") < 0, "with no page out of date yet")
            const mapAt = said.indexOf("Wiki map (.okf/)")
            tests.check(mapAt >= 0 && said.indexOf("src/jobs.rs", mapAt) >= 0, "and the wiki's map: its pages and the code they cite (" + said.slice(mapAt, mapAt + 160).replace(/\\n/g, " | ") + ")")
            // Nothing owed: Update records that and starts no session.
            tests.wikiKeyBefore = Workspace.activeKey
            tests.check(Workspace.startWikiRun("project:" + tests.wikiProject, "update") === "", "Update starts") } }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)).recorded, 20000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === tests.wikiKeyBefore, "with nothing owed, no session starts")
            tests.check((TestInput.readFile(win.argValue("--cwd=") + "/.okf/.harlo/ledger.json") || "").indexOf("src/jobs.rs") >= 0, "the check records the footnotes' code")
            // The code a footnote cites changes: sessions are told the page is out of date.
            TestInput.writeFile(win.argValue("--cwd=") + "/src/jobs.rs", "pub fn retry() {\n    four();\n}\n")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const said = tests.wikiSaid(tests.wikiRequests)
            tests.check(said.indexOf("so the code wins): .okf/architecture/jobs.md.") >= 0, "a page whose code changed is named out of date (" + said.slice(said.indexOf("Out of date"), said.indexOf("Out of date") + 120) + ")")
            // Reading that page, Harlo is told which of its footnotes to check.
            tests.wikiDone = tests.doneCount()
            tests.sendByKeys("read the wiki at architecture/jobs.md") } }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.wikiAnswered(), 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const read = tests.lastToolResult()
            const note = read.indexOf("[Wiki: this page is out of date.")
            tests.check(read.indexOf("retried three times") >= 0 && note >= 0 && read.indexOf("[^retry] src/jobs.rs, fn retry (lines 1–3) changed since the page was written", note) >= 0,
                        "reading it, Harlo is told which footnotes' code changed (" + read.slice(Math.max(note, 0), Math.max(note, 0) + 200) + ")")
            const counts = tests.findAllNames("toolResult", chat.transcript).map(l => l.text)
            tests.check(counts.indexOf("15 lines") >= 0, "its card counts the page's 15 lines, not the note's (" + counts.join(" | ") + ")")
            tests.shotWindow("clicks-harlo-wiki")
            // Claude Code, through the same tools.
            tests.claudeKey = Workspace.newSession(tests.wikiProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("wiki note") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.chatText().indexOf("Wiki tools:") >= 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("Wiki tools: none; wiki told; out of date: .okf/architecture/jobs.md") >= 0, "Claude Code is told the same, with no wiki tool (" + t.slice(t.indexOf("Wiki tools:"), t.indexOf("Wiki tools:") + 200) + ")")
            // The map off in Settings → Wiki: the note alone.
            settingsDialog.openPage("wiki") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const sw = tests.findName("wikiMapDefault", settingsDialog.contentItem)
            const f = tests.findName("settingsPage", settingsDialog.contentItem)
            if (sw && f) f.contentY = Math.max(0, Math.min(sw.mapToItem(f.contentItem, 0, 0).y - 160, f.contentHeight - f.height))
            tests.check(sw !== null && tests.shown(sw) && sw.checked, "Settings → Wiki gives sessions the map, on as it comes (" + (sw ? sw.checked : "no switch") + ")")
            if (sw) tests.clickItem(sw, "the wiki's map") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(Workspace.setting("wiki_map") === "0", "clicked, it's off (" + Workspace.setting("wiki_map") + ")")
            tests.shotWindow("clicks-harlo-wiki-map-setting")
            settingsDialog.close()
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const said = tests.wikiSaid(tests.wikiRequests)
            tests.check(said.indexOf("This project has a wiki") >= 0 && said.indexOf("Wiki map (.okf/)") < 0, "map off: a new session gets the note without the map")
            // This project on again, in Settings → Projects.
            settingsDialog.projectId = tests.wikiProject
            settingsDialog.openPage("projects") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const c = tests.findName("projectWikiMap", settingsDialog.contentItem)
            const f = tests.findName("settingsPage", settingsDialog.contentItem)
            if (c && f) f.contentY = Math.max(0, Math.min(c.mapToItem(f.contentItem, 0, 0).y - 160, f.contentHeight - f.height))
            tests.check(c !== null && tests.shown(c) && c.currentIndex === 0 && c.model[0] === "As in Settings (off)", "the project's Wiki map follows Settings, which says off (" + (c ? c.model[0] : "no choice") + ")")
            if (c) c.activated(1) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const c = tests.findName("projectWikiMap", settingsDialog.contentItem)
            tests.check(Workspace.setting("wiki_map_" + tests.wikiProject) === "1" && c && c.currentIndex === 1, "picked On, the project has it on (" + Workspace.setting("wiki_map_" + tests.wikiProject) + ")")
            tests.shotWindow("clicks-harlo-wiki-map-project")
            settingsDialog.close()
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiSaid(tests.wikiRequests).indexOf("Wiki map (.okf/)") >= 0, "on for the project: a new session gets the map again")
            // Wiki off: a new session isn't told.
            Workspace.setSetting("wiki_" + tests.wikiProject, "0")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("hello") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiSaid(tests.wikiRequests).indexOf("This project has a wiki") < 0, "Wiki off: a new session isn't told of it")
            tests.claudeKey = Workspace.newSession(tests.wikiProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("wiki note") }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.chatText().indexOf("Wiki tools:") >= 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Wiki tools: none; wiki not told") >= 0, "nor is Claude Code (" + tests.chatText().slice(-120) + ")")
            // Updating it: a session of its own with harlo's check, which
            // only it gets.
            Workspace.setSetting("default_harness", "harlo")
            // (Written with the project's model for Harlo.)
            Workspace.setSetting("project_model_" + tests.wikiProject + "_harlo", "fake/fake-helper")
            tests.wikiRequests = tests.modelRequests().length
            tests.wikiKeyBefore = Workspace.activeKey
            tests.check(Workspace.startWikiRun("project:" + tests.wikiProject, "update") === "", "Update starts") } }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => Workspace.activeKey !== tests.wikiKeyBefore && chat.session.key === Workspace.activeKey, 20000) }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => /Wiki: /.test(tests.chatText()) && chat.session.status === "idle", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = tests.modelRequests().slice(tests.wikiRequests)
            const first = reqs[0] || {}
            const names = (first.tools || []).map(t => t.function.name).filter(n => n.indexOf("wiki") >= 0)
            const asked = JSON.stringify(first.messages || [])
            tests.check(names.length === 1 && names[0].endsWith("wiki_check"), "the wiki session gets wiki_check, and no other wiki tool (" + names.join(", ") + ")")
            tests.check(asked.indexOf("Bring this repository's wiki (.okf/) in line") >= 0 && asked.indexOf("[^retry] src/jobs.rs, fn retry (lines 1–3) changed since the page was written") >= 0, "its prompt says what's owed: the stale footnote")
            tests.check(chat.session.title === "Wiki: update", "titled Wiki: update (" + chat.session.title + ")")
            tests.check(reqs.length > 0 && reqs.every(r => r.model === "fake-helper"), "with the project's model (" + reqs.map(r => r.model).filter((m, i, a) => a.indexOf(m) === i).join(", ") + ")")
            tests.check(Workspace.approvalMode(Workspace.activeKey) === "write", "its writes go ahead (the write mode); commands still ask (" + Workspace.approvalMode(Workspace.activeKey) + ")")
            const said = tests.chatText()
            tests.check(JSON.parse(chat.session.userPrompts()).length === 0 && tests.findText("Wiki: update. The agent brings the wiki in line with the code again.", chat.transcript) !== null,
                      "it opens with harlo's line, not the prompt as your message")
            tests.check(tests.findText("Check the wiki", chat.transcript) !== null && tests.findText("1 footnote confirmed", chat.transcript) !== null,
                      "its check's card says what it did (1 footnote confirmed)")
            tests.check(said.indexOf("Wiki: complete") >= 0, "the update finishes (" + said.slice(said.lastIndexOf("Wiki:"), said.lastIndexOf("Wiki:") + 300) + ")")
            const cwd = win.argValue("--cwd=")
            tests.check((TestInput.readFile(cwd + "/.okf/log.md") || "").indexOf("(/architecture/jobs.md)") >= 0, "the log records the stamped pages (" + (TestInput.readFile(cwd + "/.okf/log.md") || "").slice(0, 200) + ")")
            tests.check(TestInput.readFile(cwd + "/AGENTS.md") === "" && TestInput.listDir(cwd + "/.github") === "", "and nothing outside .okf/ is written")
            tests.shotWindow("clicks-harlo-wiki-update")
            // Done, and left: on the project's Wiki shelf, folded.
            Workspace.open(tests.wikiKeyBefore) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const shelf = tests.findName("shelfRow-wikishelf:" + tests.wikiProject)
            tests.check(shelf !== null && shelf.collapsed && shelf.title === "Wiki" && shelf.count === 1, "the finished run goes on the Wiki shelf (" + (shelf ? shelf.title + " · " + shelf.count : "none") + ")")
            tests.check(tests.findName("shelfRow-shelf:" + tests.wikiProject) === null, "not on Settled")
            if (shelf) tests.clickItem(shelf, "the Wiki shelf") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const rows = tests.findAllNames("sessionRow", win.contentItem)
            tests.check(rows.some(r => r.title === "Wiki: update" && r.settled !== ""), "opened, it shows the run (" + rows.map(r => r.title + (r.settled ? " [" + r.settled + "]" : "")).join(", ") + ")")
            tests.check(tests.wikiUpdates() === 1, "with Keep it current off (as it comes), no update started on its own (" + tests.wikiUpdates() + ")")
            // Keep it current as you work: the code a footnote cites
            // changes, a turn ends, and once it's quiet an update starts
            // in the background.
            Workspace.setSetting("wiki_" + tests.wikiProject, "1")
            Workspace.setSetting("wiki_keep_" + tests.wikiProject, "1")
            TestInput.writeFile(win.argValue("--cwd=") + "/src/jobs.rs", "pub fn retry() {\n    five();\n}\n")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { tests.wikiKeyBefore = Workspace.activeKey; tests.sendByKeys("hello") } }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => tests.wikiUpdates() === 2 && tests.wikiSettled(), 30000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const reqs = tests.modelRequests(), asked = JSON.stringify(reqs.filter(r => JSON.stringify(r.messages || []).indexOf("Bring this repository's wiki") >= 0).slice(-1)[0] || {})
            tests.check(tests.wikiUpdates() === 2 && asked.indexOf("[^retry] src/jobs.rs, fn retry") >= 0, "Keep it current on: a session updates the wiki on its own (" + tests.wikiUpdates() + ")")
            tests.check(Workspace.activeKey === tests.wikiKeyBefore, "in the background: the session you're in stays open")
            const st = JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject))
            tests.check(!st.running && st.work && st.work.current, "and the wiki is current again (" + (st.work ? st.work.text : "") + ")")
            // A turn that changes nothing the wiki cites: no update.
            tests.sendByKeys("hello") } }
        ScriptAction { script: tests.waitUntil(harloWikiSteps, () => chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            tests.check(tests.wikiUpdates() === 2, "and a turn after that, with nothing owed, starts none (" + tests.wikiUpdates() + ")")
            tests.summary(); Qt.quit() } }
    }
    property string wikiKeyBefore: ""

    // The Wiki panel: a project's pages by folder and by type, each read
    // with its diagram, code, footnotes and the pages linking to it; links
    // between pages followed (and Back), a footnote opening its code, a
    // search, the graph whose boxes open pages; Mark reviewed, a footnote
    // whose code changed, new code no page mentions and Check this page;
    // and a project with none, whose Create runs a wiki session and then
    // shows its pages, and whose Update with nothing owed starts none.
    property int wikiProject2: -1
    /// Clicks the words of a link in a page's text.
    function clickLink(edit, words) {
        if (!edit) { tests.check(false, "find the text with " + words); return }
        tests.scrollIntoView(edit)
        const i = edit.getText(0, edit.length).indexOf(words)
        const r = edit.positionToRectangle(i + 2)
        const p = edit.mapToItem(null, r.x + 2, r.y + r.height / 2)
        TestInput.click(p.x, p.y)
    }
    function wikiProse(words) { return tests.findAllNames("wikiProse", wikiPanel).find(t => t.getText(0, t.length).indexOf(words) >= 0) || null }
    function wikiRows() { return tests.findAllNames("wikiPageRow", wikiPanel).map(r => r.modelData.title) }
    /// What the open page owes the code, as it lists it.
    function wikiOwes() { return (wikiPanel.pageView && wikiPanel.pageView.items) || [] }
    SequentialAnimation {
        id: wikiPanelSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowikipanel"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.setSetting("default_harness", "harlo")
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.addProject(win.argValue("--cwd=").replace(/\/repo$/, "/repo2")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const ps = JSON.parse(Workspace.projectsJson())
            tests.wikiProject = ps.find(p => p.folder === "repo").id
            tests.wikiProject2 = ps.find(p => p.folder === "repo2").id
            tests.harloKey = Workspace.newSession(tests.wikiProject, "harlo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.clickItem(tests.findName("rail-wiki"), "the rail's Wiki") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiRows().length === 3, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(windowSettings.rightMode === "wiki", "the rail opens the Wiki panel")
            TestInput.key(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(windowSettings.rightMode === "", "Ctrl+Shift+K closes it")
            TestInput.key(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiRows().length === 3, 10000) }
        // (The pointer off the rail button clicked before: its tip covers
        // the filter's second row in a narrow panel.)
        ScriptAction { script: { const at = wikiPanel.mapToItem(null, 12, 12); TestInput.moveTo(at.x, at.y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(windowSettings.rightMode === "wiki" && tests.findName("wikiSearch", wikiPanel).activeFocus, "and opens it again, its search ready to type in")
            tests.check(win.shortcutList.some(x => x.id === "wikiPanel" && x.seq === "Ctrl+Shift+K" && x.group === "Panels"), "listed with the other panels' shortcuts")
            tests.check(JSON.stringify(tests.wikiRows()) === JSON.stringify(["Overview", "Jobs", "Releasing"]), "its pages, the overview first (" + JSON.stringify(tests.wikiRows()) + ")")
            tests.check(tests.findText("Architecture", wikiPanel) !== null && tests.findText("Workflows", wikiPanel) !== null, "grouped by folder")
            const marks = tests.findAllNames("wikiPageMark", wikiPanel).map(m => m.text)
            tests.check(marks.length === 3 && marks.every(m => m === "Edited, not checked yet"), "pages harlo hasn't checked say so (" + marks.join(", ") + ")")
            tests.check(tests.wikiFresh() === "Up to date", "nothing owed to the code (" + tests.wikiFresh() + ")")
            const run = tests.findName("wikiRun", wikiPanel)
            tests.check(run && run.text === "Update" && run.variant === "outline", "so Update is quiet")
            tests.check(tests.findName("wikiSwitch", wikiPanel).checked && tests.findText("Agents are pointed to it, in sessions started from now on.", wikiPanel) !== null, "and agents are pointed to it (on unless switched off)")
            tests.shotWindow("clicks-wiki-pages")
            tests.clickItem(tests.findName("wikiType-Workflow", wikiPanel), "the Workflow filter") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(JSON.stringify(tests.wikiRows()) === JSON.stringify(["Releasing"]), "a type shows only its pages (" + JSON.stringify(tests.wikiRows()) + ")")
            tests.clickItem(tests.findName("wikiType-all", wikiPanel), "All") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiRows().length === 3, "All shows them all again")
            // (Off the rail first: its button's tooltip can sit over the row.)
            TestInput.moveTo(win.width / 2, win.height / 2) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findAllNames("wikiPageRow", wikiPanel)[0], "the Overview row") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "overview.md" && tests.wikiProse("job runner") !== null, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const meta = tests.findName("wikiPageMeta", wikiPanel)
            tests.check(meta && meta.text === "Overview · not checked yet", "a page says what kind it is, and that it's not checked yet (" + (meta ? meta.text : "") + ")")
            const title = tests.findName("wikiPageTitle", wikiPanel), lede = tests.findName("wikiPageLede", wikiPanel)
            tests.check(title && title.text === "Overview" && lede && lede.text === "Where to start." && title.y < lede.y && lede.y < meta.y
                      && tests.findAllNames("wikiProse", wikiPanel).every(t => !t.visible || t.text.indexOf(">Overview<") < 0),
                      "its title first, then its summary and kind, the title not again in its text")
            tests.clickLink(tests.wikiProse("job runner"), "job runner") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md", 10000) }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.findAllNames("wikiDiagram", wikiPanel).some(d => d.ready), 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(true, "a link to another page opens it in the panel")
            const back = tests.findName("wikiBack", wikiPanel)
            tests.check(back && back.text === "← Overview", "with Back to the page before (" + (back ? back.text : "") + ")")
            tests.check(tests.findAllNames("wikiDiagram", wikiPanel).some(d => d.ready), "its diagram drawn as a picture")
            const code = tests.findAllNames("wikiCode", wikiPanel)
            tests.check(code.length === 1 && code[0].children[0].getText(0, code[0].children[0].length).indexOf("run-jobs --retries 3") >= 0, "its code in a code well")
            const codeText = code.length === 1 ? code[0].children[0] : null
            tests.check(codeText && codeText.contentWidth <= codeText.width + 1 && codeText.lineCount > 1,
                      "its long line wrapped in the well, not cut (" + (codeText ? Math.round(codeText.contentWidth) + " in " + Math.round(codeText.width) + ", " + codeText.lineCount + " lines" : "") + ")")
            const retry = tests.wikiProse("retried three times")
            tests.check(retry && retry.text.indexOf("okf-fn:retry") >= 0 && retry.getText(0, retry.length).indexOf("[^retry]") < 0, "a footnote's mark is a link, not its syntax")
            const notes = tests.findAllNames("wikiFootnote", wikiPanel).map(f => f.getText(0, f.length))
            tests.check(notes.length === 1 && notes[0].indexOf("src/jobs.rs, fn retry") >= 0 && notes[0].indexOf("lines 1–3") >= 0 && notes[0].indexOf("changed") < 0,
                      "the footnote names its code and lines (" + notes.join(" | ") + ")")
            const links = tests.findAllNames("wikiBacklink", wikiPanel).map(b => b.text)
            tests.check(JSON.stringify(links) === JSON.stringify(["Overview", "Releasing"]), "and the pages linking here (" + JSON.stringify(links) + ")")
            tests.shotWindow("clicks-wiki-reader")
            tests.clickLink(tests.findAllNames("wikiFootnote", wikiPanel)[0], "src/jobs.rs") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => Workspace.activeKey.endsWith("/repo/src/jobs.rs"), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === "file:" + win.argValue("--cwd=") + "/src/jobs.rs", "a footnote opens its code (" + Workspace.activeKey + ")")
            Workspace.open(tests.harloKey) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickLink(tests.wikiProse("starts in"), "README.md") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => Workspace.activeKey.endsWith("/repo/README.md"), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === "file:" + win.argValue("--cwd=") + "/README.md", "a link to a file opens it (" + Workspace.activeKey + ")")
            Workspace.open(tests.harloKey) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(wikiPanel.openPage === "architecture/jobs.md", "and back in the session the page is still open")
            tests.clickItem(tests.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "overview.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.findName("wikiBack", wikiPanel).text === "← Pages", "Back returns to it, then to the pages")
            tests.clickItem(tests.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiRows().length === 3, "the pages again")
            tests.clickItem(tests.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.typeText("failed job retried")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => !wikiPanel.searching && tests.findAllNames("wikiHit", wikiPanel).length > 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const hits = tests.findAllNames("wikiHit", wikiPanel)
            tests.check(hits[0].modelData.page === "architecture/jobs.md" && hits[0].modelData.section === "retry-control",
                      "the search finds the section (" + JSON.stringify(hits[0].modelData) + ")")
            tests.check(tests.findText("Retry control", wikiPanel) !== null, "shown by its heading")
            const snip = tests.findName("wikiHitSnippet", hits[0])
            tests.check(snip && snip.visible && /<b><font[^>]*>(failed|job|retried)<\/font><\/b>/i.test(snip.text)
                      && hits.every(h => !/-->|\| *-{3}|\[\^/.test(h.modelData.snippet)),
                      "its text says why it matched, the query's words marked, no diagram, table or footnote syntax (" + (snip ? snip.text : "-") + ")")
            const fill = wikiPanel.marked("How a job is retried", "how is a job retried")
            tests.check(fill.split("<b>").length === 3 && fill.indexOf(">job<") > 0 && fill.indexOf(">retried<") > 0 && wikiPanel.marked("How it works", "how").split("<b>").length === 2,
                      "filler words left unmarked, unless the query is only those (" + fill + ")")
            tests.shotWindow("clicks-wiki-search")
            tests.clickItem(hits[0], "the first result") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("wikiBack", wikiPanel).text === "← Results", "a result opens its page, with Back to the results")
            tests.clickItem(tests.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("wikiHit", wikiPanel).length > 0, "the results again")
            tests.clickItem(tests.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiRows().length === 3 && tests.findName("wikiSearch", wikiPanel).text === "", "Escape clears the search")
            // A name: its declaration in the code, under the sections.
            tests.clickItem(tests.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.typeText("retry")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => !wikiPanel.searching && tests.findAllNames("wikiCodeHit", wikiPanel).length > 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const code = tests.findAllNames("wikiCodeHit", wikiPanel)
            tests.check(code.length === 1 && code[0].hit.path === "src/jobs.rs" && code[0].hit.name === "retry" && code[0].hit.kind === "fn",
                      "a name's search lists its declaration in the code (" + code.map(c => JSON.stringify(c.hit)).join(" | ") + ")")
            tests.check(tests.findText("In the code", wikiPanel) !== null, "under In the code")
            tests.clickItem(code[0], "the declaration") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => { const c = tests.findName("wikiCodeCard", wikiPanel); return c && c.visible && c.text.indexOf("pub fn retry()") === 0 }, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const card = tests.findName("wikiCodeCard", wikiPanel)
            tests.check(card.text === "pub fn retry()\ncalled by: nothing the index finds",
                      "clicked, its card from the code index, without the row's name and place again (" + card.text.replace(/\n/g, " / ") + ")")
            const open = tests.findName("wikiCodeOpen", wikiPanel)
            tests.check(open && open.visible && open.text === "Open jobs.rs", "with Open for its file (" + (open ? open.text : "-") + ")")
            tests.shotWindow("clicks-wiki-code")
            tests.clickItem(tests.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiRows().length === 3, "Escape clears it")
            tests.clickItem(tests.findName("wikiViewGraph", wikiPanel), "Graph") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.findAllNames("wikiGraphBox", wikiPanel).length === 3, 20000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("wikiGraphBox", wikiPanel).length === 3, "the graph: a box per page, to click")
            tests.check(wikiPanel.graphSource.indexOf("p0 --> p1") >= 0 && wikiPanel.graphSource.indexOf("p2 --> p1") >= 0, "and an arrow per link (" + wikiPanel.graphSource.replace(/\n/g, " ") + ")")
            tests.shotWindow("clicks-wiki-graph")
            const box = tests.findAllNames("wikiGraphBox", wikiPanel).find(b => b.page.path === "workflows/release.md")
            tests.clickItem(box, "the Releasing box") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "workflows/release.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const meta = tests.findName("wikiPageMeta", wikiPanel)
            tests.check(meta.text.indexOf("Workflow · reviewed by sam ") === 0 && meta.text.endsWith(" · edited since"), "a box opens its page, which says who reviewed it (" + meta.text + ")")
            // A link to a section of another page: opened at it.
            tests.clickLink(tests.wikiProse("the jobs"), "the jobs") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("wikiBack", wikiPanel).text === "← Releasing", "a link to a section opens its page")
            // Read against its code: Mark reviewed.
            tests.clickItem(tests.findName("wikiMarkReviewed", wikiPanel), "Mark reviewed") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => /reviewed by you just now$/.test(tests.findName("wikiPageMeta", wikiPanel).text), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(true, "Mark reviewed stamps it as reviewed by you (" + tests.findName("wikiPageMeta", wikiPanel).text + ")")
            // The code its footnote cites changes.
            TestInput.writeFile(win.argValue("--cwd=") + "/src/jobs.rs", "pub fn retry() {\n    four();\n}\n")
            tests.clickItem(tests.findName("wikiRefresh", wikiPanel), "Refresh") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiFresh() === "1 footnote's code changed" && tests.wikiOwes().length === 1, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiOwes()[0] === "Footnote 1: its code changed since the page was written (src/jobs.rs, fn retry)", "the page says which footnote's code changed (" + tests.wikiOwes().join(" | ") + ")")
            const note = tests.findAllNames("wikiFootnote", wikiPanel)[0]
            tests.check(note && note.getText(0, note.length).indexOf("its code changed since the page was written") >= 0, "and so does the footnote")
            tests.check(tests.findName("wikiRun", wikiPanel).variant === "primary" && tests.findName("wikiCheckPage", wikiPanel).variant === "primary", "Update and Check this page, the things to do")
            tests.shotWindow("clicks-wiki-stale")
            tests.clickItem(tests.findName("wikiMarkReviewed", wikiPanel), "Mark reviewed") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiFresh() === "Up to date" && tests.wikiOwes().length === 0, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(true, "reviewed again, the footnote counts as read: up to date")
            // New code no page mentions.
            TestInput.writeFile(win.argValue("--cwd=") + "/src/jobs.rs", "pub fn retry() {\n    four();\n}\n\npub fn cancel() {}\n")
            tests.clickItem(tests.findName("wikiRefresh", wikiPanel), "Refresh") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiFresh() === "1 new declaration no page mentions" && tests.wikiOwes().length === 1, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.wikiOwes()[0] === "New function cancel (in src/jobs.rs, line 5) that no page mentions", "the page that covers it names it (" + tests.wikiOwes().join(" | ") + ")")
            tests.wikiKeyBefore = Workspace.activeKey
            tests.wikiRequests = tests.modelRequests().length
            tests.clickItem(tests.findName("wikiCheckPage", wikiPanel), "Check this page") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => Workspace.activeKey !== tests.wikiKeyBefore && chat.session.key === Workspace.activeKey, 20000) }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => /Wiki: /.test(tests.chatText()) && chat.session.status === "idle", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const asked = JSON.stringify((tests.modelRequests()[tests.wikiRequests] || {}).messages || [])
            tests.check(chat.session.title === "Wiki: check a page" && asked.indexOf("Check the wiki page .okf/architecture/jobs.md") >= 0 && asked.indexOf("new fn cancel in src/jobs.rs:5") >= 0,
                      "Check this page starts a session on that page and what it owes (" + chat.session.title + ")")
            tests.check(tests.findText("1 path left out", chat.transcript) !== null && tests.chatText().indexOf("Wiki: complete") >= 0, "which leaves the new function out, and the wiki is current (" + tests.chatText().slice(-200) + ")")
            Workspace.open(tests.harloKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => /^Up to date · written /.test(tests.wikiFresh()), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(true, "back in the panel: up to date, and when it was written (" + tests.wikiFresh() + ")")
            // A project without a wiki: what it is, and Create.
            tests.harloKey = Workspace.newSession(tests.wikiProject2, "harlo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.findName("wikiCreate", wikiPanel) !== null && tests.findName("wikiCreate", wikiPanel).visible, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findText("No wiki yet", wikiPanel) !== null && tests.findName("wikiCreate", wikiPanel).enabled, "a project without one: what a wiki is, and Create")
            tests.check(wikiPanel.openPage === "" && tests.findName("wikiSearch", wikiPanel) === null, "another repository starts afresh")
            tests.shotWindow("clicks-wiki-empty")
            tests.wikiKeyBefore = Workspace.activeKey
            tests.clickItem(tests.findName("wikiCreate", wikiPanel), "Create wiki") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => Workspace.activeKey !== tests.wikiKeyBefore && chat.session.key === Workspace.activeKey, 20000) }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => /Wiki: /.test(tests.chatText()) && chat.session.status === "idle", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => tests.wikiRows().length === 1, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.title === "Wiki: create" && tests.chatText().indexOf("Wiki: complete") >= 0, "Create runs a wiki session (" + chat.session.title + ", " + tests.chatText().slice(-160) + ")")
            tests.check(Workspace.setting("wiki_" + tests.wikiProject2) === "1", "and turns the wiki on for the project's agents")
            tests.check(JSON.stringify(tests.wikiRows()) === JSON.stringify(["Overview"]), "its page shows in the panel (" + JSON.stringify(tests.wikiRows()) + ")")
            tests.check(tests.wikiFresh() === "Up to date · written just now", "up to date (" + tests.wikiFresh() + ")")
            tests.check(tests.findName("wikiRun", wikiPanel).variant === "outline", "Update is quiet then")
            tests.check(tests.findText("Agents are pointed to it, in sessions started from now on.", wikiPanel) !== null, "and agents are pointed to it")
            tests.shotWindow("clicks-wiki-created")
            // A commit no page needs to answer for (click-tests.sh makes it).
            tests.log("wiki-quiet-now") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => TestInput.readFile(win.argValue("--cwd=").replace(/\/repo$/, "/repo2") + "/README.md").indexOf("and more") >= 0, 15000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.wikiKeyBefore = Workspace.activeKey
            tests.wikiRequests = tests.modelRequests().length
            tests.clickItem(tests.findName("wikiRun", wikiPanel), "Update") } }
        ScriptAction { script: tests.waitUntil(wikiPanelSteps, () => !!wikiPanel.st.recorded, 20000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === tests.wikiKeyBefore && tests.modelRequests().length === tests.wikiRequests, "nothing owed: Update records that, and no session or model runs")
            tests.check(tests.wikiFresh() === "Up to date · written just now", "still up to date (" + tests.wikiFresh() + ")")
            tests.shotWindow("clicks-wiki-quiet")
            tests.clickItem(tests.findName("wikiSwitch", wikiPanel), "the panel's Wiki switch") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.setting("wiki_" + tests.wikiProject2) === "0" && tests.findText("Agents aren't told of it: switched off for this project.", wikiPanel) !== null, "switched off, it says so")
            tests.shotWindow("clicks-wiki-off")
            settingsDialog.projectId = tests.wikiProject2
            settingsDialog.openPage("projects") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            // (Settings → Projects: its Wiki row, scrolled to.)
            const sw = tests.findName("projectWiki", settingsDialog.contentItem)
            const f = tests.findName("settingsPage", settingsDialog.contentItem)
            if (sw && f) f.contentY = Math.max(0, Math.min(sw.mapToItem(f.contentItem, 0, 0).y - 160, f.contentHeight - f.height))
            tests.check(sw !== null && !sw.checked, "Settings shows it off too")
            tests.check(!tests.shown(tests.findName("projectWikiKeep", settingsDialog.contentItem)), "with no Keep it current while it's off")
            tests.clickItem(sw, "the project's Wiki switch") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const keep = tests.findName("projectWikiKeep", settingsDialog.contentItem)
            tests.check(Workspace.setting("wiki_" + tests.wikiProject2) === "1" && tests.shown(keep) && !keep.checked, "on again: Keep it current as you work shows, off unless switched on")
            tests.shotWindow("clicks-wiki-settings")
            tests.clickItem(keep, "Keep it current as you work") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.setting("wiki_keep_" + tests.wikiProject2) === "1", "and switches on")
            settingsDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // Goes through `items` one at a time: start(item), then once ready(item)
    // (or `ms` pass), finish(item, ready); `walker.done` after the last.
    Timer {
        id: walker
        interval: 150
        repeat: true
        property var items: []
        property int at: -1
        property var startFn: null
        property var readyFn: null
        property var finishFn: null
        property double since: 0
        property int ms: 20000
        property bool done: true
        onTriggered: {
            if (at >= 0 && at < items.length) {
                const ok = !!readyFn(items[at])
                if (!ok && Date.now() - since < ms) return
                finishFn(items[at], ok)
            }
            at++
            if (at >= items.length) { stop(); done = true; return }
            since = Date.now()
            startFn(items[at])
        }
    }
    function walk(items, start, ready, finish, ms) {
        walker.items = items; walker.at = -1; walker.startFn = start; walker.readyFn = ready; walker.finishFn = finish
        walker.ms = ms || 20000; walker.done = false; walker.start()
    }

    // A wiki checked whole (scripts/wiki-check: the demo project, with the
    // hand-written wiki or one an agent wrote): the panel lists its pages,
    // the overview first, and what it owes the code; every page opens;
    // every link to a page goes to one, every footnote to code that's
    // there, every diagram draws; the graph has a box per page; the search
    // finds the pages that answer each of wiki-queries.json; and a Harlo
    // session is told of it (and of its out-of-date pages) and reads one.
    property var demoPages: []
    property var demoProblems: ({ opened: [], links: [], code: [], diagrams: [], search: [] })
    property int demoLinks: 0
    property int demoCode: 0
    property int demoDiagrams: 0
    property var demoQueries: []
    property string demoBest: ""
    function demoFile(path) { return TestInput.readFile(win.argValue("--cwd=") + "/" + path) }
    SequentialAnimation {
        id: wikiDemoSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowikidemo"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("wiki_" + tests.wikiProject, "1")
            tests.harloKey = Workspace.newSession(tests.wikiProject, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo")
            windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => wikiPanel.pages.length > 0 && !wikiPanel.st.checking && !wikiPanel.st.loading, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pages = wikiPanel.pages
            tests.demoPages = pages.map(p => p.path)
            tests.check(pages.length >= 3, "the wiki has pages (" + pages.length + ": " + tests.demoPages.join(", ") + ")")
            tests.check(pages[0].path === "overview.md" && pages[0].type === "Overview", "the overview first (" + pages[0].path + ")")
            tests.check(pages.every(p => p.title !== "" && p.type !== ""), "every page has a title and a kind")
            const stale = win.argValue("--wiki-stale=") !== ""
            const fresh = tests.wikiFresh()
            // (A wiki with a footnote broken says so rather than up to date.)
            tests.check(stale ? /^\d+ footnotes?'s? code changed/.test(fresh) : /^Up to date|^1 footnote finds no code$/.test(fresh), "what it owes the code (" + fresh + ")")
            tests.demoQueries = JSON.parse(TestInput.readFile(win.argValue("--wiki-queries=")) || "[]")
            tests.shotWindow("wiki-demo-pages")
            // Every page, opened in the panel and checked.
            tests.walk(tests.demoPages, p => wikiPanel.openWiki(p, "", false),
                     p => {
                         if (wikiPanel.openPage !== p || wikiPanel.loadingPage !== "" || !wikiPanel.pageView) return false
                         // (Its diagrams drawn, or failed.)
                         return wikiPanel.pageView.error || (wikiPanel.pageView.blocks || []).filter(b => b.kind === "mermaid").every(b => {
                             const d = JSON.parse(Diagrams.diagram(b.code, Theme.isLight, "{}"))
                             return d.state !== "drawing"
                         })
                     },
                     (p, ok) => {
                         const v = wikiPanel.pageView
                         if (!ok || !v || v.error || (v.blocks || []).length === 0) { tests.demoProblems.opened.push(p + ": " + (v ? v.error || "no text" : "didn't open")); return }
                         const dir = p.split("/").slice(0, -1)
                         for (const b of v.blocks) {
                             if (b.kind === "mermaid") {
                                 tests.demoDiagrams++
                                 const d = JSON.parse(Diagrams.diagram(b.code, Theme.isLight, "{}"))
                                 if (d.state !== "ready") tests.demoProblems.diagrams.push(p + ": " + (d.error || d.state))
                             }
                             // (No matchAll in Qt's JavaScript.)
                             const hrefs = /href="([^"]+)"/g
                             for (let m = hrefs.exec(String(b.html || "")); m !== null; m = hrefs.exec(String(b.html || ""))) {
                                 const href = m[1].replace(/&amp;/g, "&")
                                 if (/\.md(#|$)/.test(href) && !/^[a-z-]+:/.test(href)) {
                                     tests.demoLinks++
                                     const parts = href.startsWith("/") ? [] : dir.slice()
                                     for (const s of href.split("#")[0].split("/")) { if (s === "..") parts.pop(); else if (s !== "." && s !== "") parts.push(s) }
                                     const to = parts.join("/").replace(/^\.okf\//, "")
                                     if (tests.demoPages.indexOf(to) < 0) tests.demoProblems.links.push(p + " → " + href)
                                 }
                             }
                         }
                         // (Its footnotes: each names code that's there, at lines the file has.)
                         const notes = v.footnotes || []
                         for (const f of notes) {
                             tests.demoCode++
                             const text = tests.demoFile(f.path)
                             if (text === "" || f.state === "gone" || f.state === "missing" || (f.end > 0 && text.split("\n").length < f.end))
                                 tests.demoProblems.code.push(p + " [^" + f.label + "] " + f.path + (f.locator ? ", " + f.locator : "") + " (" + (f.state || "lines " + f.end) + ")")
                         }
                         // (Shown under the page, a line each.)
                         if (tests.findAllNames("wikiFootnote", wikiPanel).length !== notes.length)
                             tests.demoProblems.code.push(p + ": " + tests.findAllNames("wikiFootnote", wikiPanel).length + " of " + notes.length + " footnotes shown")
                         if (!tests.demoBest || v.blocks.length > 6) tests.demoBest = p
                     }, 30000) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => walker.done, 600000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const pr = tests.demoProblems
            tests.check(pr.opened.length === 0, "every page opens (" + tests.demoPages.length + " read" + (pr.opened.length ? "; " + pr.opened.join("; ") : "") + ")")
            tests.check(pr.links.length === 0, "every link to a page goes to one (" + tests.demoLinks + " links" + (pr.links.length ? "; broken: " + pr.links.join("; ") : "") + ")")
            tests.check(tests.demoCode > 0 && pr.code.length === 0, "every footnote names code that's there (" + tests.demoCode + " footnotes" + (pr.code.length ? "; broken: " + pr.code.join("; ") : "") + ")")
            tests.check(pr.diagrams.length === 0, "every diagram draws (" + tests.demoDiagrams + (pr.diagrams.length ? "; failed: " + pr.diagrams.join("; ") : "") + ")")
            wikiPanel.openWiki(tests.demoBest, "", false) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => wikiPanel.openPage === tests.demoBest && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            tests.shotWindow("wiki-demo-reader")
            // (A flowchart drawn across shows down when that fits larger.)
            const drawn = tests.findAllNames("wikiDiagram", wikiPanel).filter(d => d.ready)
            tests.check(drawn.every(d => !d.down || d.down.state !== "ready" || d.fit + 0.001 >= Math.max(0.7, Math.min(1, d.width / Math.max(1, d.across.width)))),
                      "its diagrams shown the way they fit larger (" + drawn.map(d => d.downward ? "down" : "across").join(", ") + ")")
            // (Its end: its footnotes and the pages linking to it.)
            const reader = tests.findName("wikiReader", wikiPanel)
            if (reader) reader.contentY = Math.max(0, reader.contentHeight - reader.height) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.shotWindow("wiki-demo-footnotes")
            const edge = tests.findName("wikiBackEdge", wikiPanel)
            tests.check(edge && Qt.colorEqual(edge.color, Theme.hairline), "scrolled, the page has an edge under Back")
            wikiPanel.closePage()
            wikiPanel.mode = "graph" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => tests.findAllNames("wikiGraphBox", wikiPanel).length > 0 || (wikiPanel.graphPic && wikiPanel.graphPic.state === "failed"), 30000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const boxes = tests.findAllNames("wikiGraphBox", wikiPanel).length
            tests.check(boxes === wikiPanel.graphPages.length, "the graph: a box per page (" + boxes + " of " + wikiPanel.graphPages.length + (wikiPanel.graphPic && wikiPanel.graphPic.error ? "; " + wikiPanel.graphPic.error : "") + ")")
            const cap = tests.findName("wikiGraphCaption", wikiPanel)
            tests.check(!cap || !cap.visible || cap.columns === (wikiPanel.width < 360 ? 1 : 2), "its zoom beside the caption, or under it in a narrow panel")
            const gbox = tests.findName("wikiGraph", wikiPanel), gpic = tests.findName("wikiGraphPicture", wikiPanel), zoom = tests.findName("wikiGraphZoom", wikiPanel)
            tests.check(gpic && zoom && zoom.text === Math.round(gpic.fit * 100) + "%", "its zoom says the size it's drawn at (" + (zoom ? zoom.text : "-") + ", fit " + (gpic ? gpic.fit.toFixed(2) : "-") + ")")
            const capText = cap ? cap.children[0].text : ""
            tests.check(!!gbox && gbox.wider === /scroll sideways/.test(capText) && !!tests.findName("wikiGraphFull", wikiPanel),
                      "wider than the panel, its caption says to scroll or open it full size, with a button for that (" + (gbox ? gbox.wider : "-") + ")")
            const last = wikiPanel.graphPages.findIndex(p => p.path === wikiPanel.lastRead)
            const qbox = tests.findAllNames("wikiGraphBox", wikiPanel).find(b => b.index === last)
            const qx = qbox ? qbox.mapToItem(gbox, 0, 0).x : -1
            tests.check(last >= 0 && qbox && qbox.lastRead && tests.findAllNames("wikiGraphBox", wikiPanel).filter(b => b.lastRead).length === 1,
                      "the page read last ringed in it (" + wikiPanel.lastRead + ")")
            tests.check(qbox && qx >= 0 && qx + qbox.width <= gbox.width + 1, "it opens with the page read last in view (" + Math.round(qx) + "+" + (qbox ? Math.round(qbox.width) : "-") + " in " + (gbox ? Math.round(gbox.width) + ", scrolled " + Math.round(gbox.contentX) + " of " + Math.round(gbox.contentWidth) : "-") + ")")
            tests.shotWindow("wiki-demo-graph")
            wikiPanel.mode = "pages"
            // Each search, as the panel asks it.
            tests.walk(tests.demoQueries, q => wikiPanel.search(q.q), q => !wikiPanel.searching && wikiPanel.query === q.q,
                     (q, ok) => {
                         const hits = ok && wikiPanel.results ? wikiPanel.results.hits.map(h => h.page.replace(/^\.okf\//, "")) : []
                         const good = hits.length > 0 && (q.pages.length === 0 || q.pages.some(p => hits.slice(0, 3).indexOf(p) >= 0))
                         if (!good) tests.demoProblems.search.push("“" + q.q + "” → " + (hits.slice(0, 3).join(", ") || "nothing"))
                         // (No hit reads as a flattened code block, or shows a footnote's raw form.)
                         const code = ok && wikiPanel.results ? wikiPanel.results.hits.find(h => /(^|\s)(python|rust|sh|bash|js|ts)\s+\S*[(@{=]/.test(h.snippet) || /\[\^\w+\]/.test(h.snippet)) : null
                         if (code) tests.demoProblems.search.push("“" + q.q + "” shows markup: " + code.snippet.slice(0, 60))
                     }, 30000) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => walker.done, 300000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(tests.demoProblems.search.length === 0, "the search finds the page for each question (" + tests.demoQueries.length + (tests.demoProblems.search.length ? "; missed: " + tests.demoProblems.search.join("; ") : "") + ")")
            tests.shotWindow("wiki-demo-search")
            wikiPanel.search("") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendWiki("read the wiki at " + tests.demoBest) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiDemoSteps, () => tests.wikiAnswered(), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            // (Told of the wiki in its instructions, with the pages whose
            // code changed since; no wiki tools of its own.)
            const req = tests.modelRequests()[0] || {}
            const said = JSON.stringify(req.messages || [])
            // (A broken footnote makes its page out of date too.)
            const stale = win.argValue("--wiki-stale=") !== "", broken = win.argValue("--wiki-break=") !== ""
            const note = (/This project has a wiki in \.okf\/[^"]{0,200}/.exec(said) || [""])[0]
            tests.check(note !== "" && (stale ? said.indexOf(".okf/architecture/orders.md") >= 0 : broken ? said.indexOf(".okf/overview.md") >= 0 : said.indexOf("Out of date") < 0),
                      "a Harlo session is told of the wiki" + (stale ? ", and that orders.md is out of date" : broken ? ", and that overview.md is out of date" : "") + " (" + note + ")")
            tests.check(!(req.tools || []).some(t => /wiki/.test((t.function || t).name || "")), "with no wiki tools (" + (req.tools || []).map(t => (t.function || t).name).filter(n => /wiki/.test(n)).join(", ") + ")")
            const t = tests.lastToolResult()
            tests.check(t.length > 80 && /^---|type:|# /.test(t.trim().slice(0, 200)) && !/error|not found/i.test(t.slice(0, 120)), "and reads a page as a file (" + tests.demoBest + ": " + t.slice(0, 120) + ")")
        } }
        // (The reply's rows fade in as they come: the shot once they have.)
        PauseAnimation { duration: Theme.paneMotion + 200 }
        ScriptAction { script: {
            tests.shotWindow("wiki-demo-session")
            tests.summary(); Qt.quit() } }
    }

    // Merged work the wiki owes something (a cited function changed, a new
    // one): an offer in the corner to update it; Not now holds until main
    // moves again; Update starts the update's session.
    property int wikiSessionsBefore: 0
    property string wikiPanelWriter: ""
    function wikiFresh() {
        const f = tests.findName("wikiFreshness", wikiPanel)
        return f ? f.text.replace(/\u00a0/g, " ") : ""
    }
    SequentialAnimation {
        id: wikiOfferSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowikioffer"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.setSetting("default_harness", "harlo")
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("project_model_" + tests.wikiProject + "_harlo", "fake/fake-helper")
            // (Its footnote recorded: an update with nothing owed, no session.)
            tests.wikiSessionsBefore = Workspace.sessionCount
            Workspace.startWikiRun("project:" + tests.wikiProject, "update") } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => !!JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)).recorded, 10000) }
        ScriptAction { script: {
            tests.check(Workspace.sessionCount === tests.wikiSessionsBefore, "a current wiki: Update starts no session")
            Workspace.checkWikiOffers() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findName("wikiOffer-" + tests.wikiProject) === null && Workspace.setting("wiki_seen_" + tests.wikiProject) !== "", "the first look notes main, and offers nothing")
            tests.log("wiki-merge-now") } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/src/b.rs") !== "", 10000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Workspace.checkWikiOffers() }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => tests.findName("wikiOffer-" + tests.wikiProject) !== null, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const toast = tests.findName("wikiOffer-" + tests.wikiProject)
            const text = toast ? tests.findName("wikiOfferText", toast).text : ""
            tests.check(/^3 commits on main added b, which no page mentions\. An update deals with the 2 things it owes in a session you can watch\.$/.test(text),
                      "an offer: main's commits, the declaration they add and what the wiki owes (" + text + ")")
            tests.check(tests.findText("Merged work isn't in repo's wiki yet", toast) !== null, "naming the project")
            tests.shotWindow("clicks-harlo-wiki-offer")
            tests.clickItem(tests.findName("wikiOfferLater", toast), "Not now") } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => tests.findName("wikiOffer-" + tests.wikiProject) === null, 5000) }
        ScriptAction { script: {
            tests.check(tests.findName("wikiOffer-" + tests.wikiProject) === null, "Not now: it goes")
            Workspace.checkWikiOffers() } }
        PauseAnimation { duration: 1500 }
        // A commit made outside harlo while the Wiki panel is open: the
        // panel counts what it adds.
        ScriptAction { script: {
            tests.check(tests.findName("wikiOffer-" + tests.wikiProject) === null, "and doesn't come back while main stays where it is")
            win.openDraft(tests.wikiProject)
            windowSettings.rightMode = "wiki" } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => tests.wikiFresh() === "1 footnote's code changed · 1 new declaration no page mentions", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.log("wiki-commit-now") }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => /2 new declarations/.test(tests.wikiFresh()), 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(true, "a commit made elsewhere shows in the open panel (" + tests.wikiFresh() + ")")
            windowSettings.rightMode = ""
            // Main moved: the offer comes again.
            Workspace.checkWikiOffers() } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => tests.findName("wikiOffer-" + tests.wikiProject) !== null, 10000) }
        PauseAnimation { duration: 400 }
        // Settings → Wiki: Harlo with Fake Coder, thinking low, writes
        // every wiki (the project's own model is Fake Helper).
        ScriptAction { script: settingsDialog.openPage("wiki") }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => { const c = tests.findName("wikiWriterAgent", settingsDialog.contentItem); return c && c.model.indexOf("Harlo") > 0 }, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const agent = tests.findName("wikiWriterAgent", settingsDialog.contentItem)
            tests.check(agent.currentText === "Same as the project", "unset, each project's own agent writes its wiki (" + agent.currentText + ")")
            agent.activated(agent.model.indexOf("Harlo")) } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => { const c = tests.findName("wikiWriterModel", settingsDialog.contentItem); return c && c.visible && c.model.length > 1 }, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const model = tests.findName("wikiWriterModel", settingsDialog.contentItem)
            model.activated(model.ids.indexOf("fake/fake-coder") + 1)
            const thinking = tests.findName("wikiWriterThinking", settingsDialog.contentItem)
            thinking.activated(1)
            const v = JSON.parse(Workspace.setting("wiki_writer") || "{}")
            tests.check(JSON.stringify(v) === JSON.stringify({ update: { agent: "harlo", model: "fake/fake-coder", thinking: "low" } }), "Settings → Wiki keeps Update's agent, model and thinking, Create's unset (" + JSON.stringify(v) + ")")
            const create = tests.findName("wikiCreateAgent", settingsDialog.contentItem)
            tests.check(create && create.currentText === "Same as the project", "Create has its own (" + (create ? create.currentText : "") + ")")
            // Create's set after Update's, on the same page: both kept.
            create.activated(create.model.indexOf("Harlo")) } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => { const c = tests.findName("wikiCreateModel", settingsDialog.contentItem); return c && c.visible && c.model.length > 1 }, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const cm = tests.findName("wikiCreateModel", settingsDialog.contentItem)
            cm.activated(cm.ids.indexOf("fake/fake-helper") + 1)
            const both = JSON.parse(Workspace.setting("wiki_writer") || "{}")
            tests.check(both.update && both.update.model === "fake/fake-coder" && both.create && both.create.model === "fake/fake-helper",
                      "Create's and Update's writers both kept (" + JSON.stringify(both) + ")")
            const um = tests.findName("wikiWriterModel", settingsDialog.contentItem)
            tests.check(um.currentText.indexOf("Fake Coder") === 0 || um.currentText.indexOf("fake-coder") >= 0, "and Update's still shows its model (" + um.currentText + ")")
            // (Back to the project's for Create, as the rest expects.)
            const ca = tests.findName("wikiCreateAgent", settingsDialog.contentItem)
            ca.activated(0)
            const left = JSON.parse(Workspace.setting("wiki_writer") || "{}")
            tests.check(!left.create && left.update && left.update.model === "fake/fake-coder", "unsetting Create leaves Update's (" + JSON.stringify(left) + ")")
            // (The stand-in models are served on this computer: local.)
            tests.check(um.model.slice(1).every(m => / · local$/.test(m)), "models from a server here say local (" + JSON.stringify(um.model) + ")")
            tests.shotWindow("clicks-harlo-wiki-writer")
            settingsDialog.close()
            // (As it reads when shown.)
            wikiPanel.load()
            tests.wikiPanelWriter = wikiPanel.updaterText } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.wikiKeyBefore = Workspace.activeKey
            tests.clickItem(tests.findName("wikiOfferUpdate", tests.findName("wikiOffer-" + tests.wikiProject)), "Update the wiki") } }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => Workspace.activeKey !== tests.wikiKeyBefore && chat.session.key === Workspace.activeKey && chat.session.title === "Wiki: update", 20000) }
        ScriptAction { script: tests.waitUntil(wikiOfferSteps, () => JSON.stringify(tests.modelRequests().map(r => r.messages || [])).indexOf("Bring this repository's wiki") >= 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.session.title === "Wiki: update", "Update starts the update's session (" + chat.session.title + ")")
            tests.check(tests.findName("wikiOffer-" + tests.wikiProject) === null, "and the offer goes")
            // (Its thinking is asked for too; the stand-in model has none.)
            tests.check(chat.session.modelId.endsWith("fake-coder"), "written with the wiki's model, not the project's (" + chat.session.modelId + ")")
            const w = JSON.parse(Workspace.wikiWriterJson("project:" + tests.wikiProject))
            tests.check(w.update.model === "fake/fake-coder" && w.create.model === "fake/fake-helper", "Update's writer, Create's the project's (" + JSON.stringify(w) + ")")
            // (Its prompt: where the check found change.)
            const asked = JSON.stringify(tests.modelRequests().map(r => r.messages || []))
            const at = asked.indexOf("harlo's check found")
            tests.check(at >= 0 && asked.indexOf("new fn c in src/c.rs:1", at) >= 0 && asked.indexOf("[^a] src/a.rs, fn a", at) >= 0, "the update starts with what the check found owed (" + (at >= 0 ? asked.slice(at, at + 300) : "not said") + ")")
            tests.check(tests.wikiPanelWriter === "Written by Harlo with " + Theme.modelLabel("fake/fake-coder") + " (Settings → Wiki)", "and the panel says who updates it (" + tests.wikiPanelWriter + ")")
            tests.summary(); Qt.quit() } }
    }

    // A wiki written and used on a real model (scripts/wiki-check real;
    // it needs a sign-in and spends tokens): --wiki-mode=create or update
    // runs that with the project's model (--model=) until the run is done
    // (its session's commands go ahead: nobody is there to approve them);
    // lookup gives a session of --agent= the question in --prompt-file=.
    // What happened goes to <selftest>/wiki-real.json.
    property double realQuiet: 0
    property bool realStarted: false
    property int realText: 0
    property double realBegin: 0
    SequentialAnimation {
        id: wikiRealSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "wikireal"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--agent=") !== "claude") Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("wiki_" + tests.wikiProject, "1")
            Workspace.setSetting("default_harness", win.argValue("--agent=") || "harlo")
            if (win.argValue("--model="))
                Workspace.setSetting("project_model_" + tests.wikiProject + "_" + (win.argValue("--agent=") || "harlo"), win.argValue("--model="))
            const mode = win.argValue("--wiki-mode=")
            if (mode === "lookup") {
                tests.harloKey = Workspace.newSession(tests.wikiProject, win.argValue("--agent=") || "harlo")
                Workspace.setApprovalMode(tests.harloKey, "yolo")
            } else {
                tests.wikiKeyBefore = Workspace.activeKey
                const err = Workspace.startWikiRun("project:" + tests.wikiProject, mode)
                if (err !== "") { tests.log("not started:", err); Qt.quit() }
            } } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.waitUntil(wikiRealSteps, () => tests.harloKey !== "" ? chat.session.modelName !== "" || chat.session.harness === "claude"
                                                                         : Workspace.activeKey !== tests.wikiKeyBefore && Workspace.activeKey !== "" || !!JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)).recorded, 120000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            // (An update with nothing stale: recorded by harlo, no session.)
            const quiet = JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject))
            if (tests.harloKey === "" && quiet.recorded) {
                TestInput.writeFile(win.selftestDir + "/wiki-real.json", JSON.stringify({ mode: win.argValue("--wiki-mode="), recorded: quiet.recorded, wiki: quiet }, null, 1))
                tests.log("done quiet", quiet.recorded)
                Qt.quit()
            } else {
                tests.log("session", chat.session.key, chat.session.harness, chat.session.modelName, chat.session.title)
                if (tests.harloKey === "") {
                    tests.harloKey = Workspace.activeKey
                    Workspace.setApprovalMode(tests.harloKey, "yolo")
                } else {
                    chat.session.send(TestInput.readFile(win.argValue("--prompt-file=")).trim(), "normal")
                }
                tests.realStarted = false; tests.realQuiet = 0; tests.realText = tests.chatText().length; tests.realBegin = Date.now()
            } } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.waitUntil(wikiRealSteps, () => {
            if (chat.session.key !== tests.harloKey) Workspace.open(tests.harloKey)
            const st = JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject))
            const busy = chat.session.status !== "idle" || st.running
            // (Started: busy, or a reply already there, or idle for 20 s
            // since: done before it was looked at.)
            if (busy || tests.chatText().length > tests.realText + 40 || Date.now() - tests.realBegin > 20000) tests.realStarted = true
            if (busy || !tests.realStarted || chat.session.hasRequest) { tests.realQuiet = 0; return false }
            if (!tests.realQuiet) tests.realQuiet = Date.now()
            return Date.now() - tests.realQuiet > 15000
        }, 3 * 3600 * 1000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const st = JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject))
            TestInput.writeFile(win.selftestDir + "/wiki-real.json", JSON.stringify({
                mode: win.argValue("--wiki-mode="), agent: chat.session.harness, model: chat.session.modelName, title: chat.session.title,
                status: chat.session.status, request: chat.session.hasRequest, wiki: st, chat: tests.chatText().slice(-6000) }, null, 1))
            tests.log("done", chat.session.status, st.pages.length, "pages")
            Qt.quit() } }
    }

    // The wiki's demo project to look at by hand (scripts/wiki-check
    // demo): the project added with Wiki on and the Wiki panel open; it
    // stays open until closed.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "wikidemoshow"
        PauseAnimation { duration: 600 }
        ScriptAction { script: Workspace.addProject(win.argValue("--cwd=")) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson())[0]
            if (p) Workspace.setSetting("wiki_" + p.id, "1")
            windowSettings.rightMode = "wiki" } }
    }

    // A wiki on another machine: the panel reads it there (pages, a page
    // with its footnotes, search) through harlo's agent on it; a quiet
    // Update records it there; a session there is told a page is out of
    // date; Mark reviewed stamps it there; Create writes one for a second
    // repository there, its check run there too.
    function remoteWikiFile(rel) { return TestInput.readFile(win.argValue("--cwd=") + rel) || "" }
    SequentialAnimation {
        id: wikiRemoteSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowikiremote"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.setProviderEnabled("claude", false)
            tests.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => JSON.parse(Remotes.hostsJson())[0].state === "connected", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: Workspace.addProject("ssh://box" + win.argValue("--cwd=")) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo")
            tests.wikiProject = p ? Number(p[0]) : -1
            tests.check(Workspace.instance === "box" && tests.wikiProject >= 0 && p[2] === "harlo", "the project on box, with Harlo there")
            tests.harloKey = Workspace.newSession(tests.wikiProject, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo")
            windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 600 }
        // (The first read may put harlo's agent on box.)
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.wikiRows().length === 2 && !wikiPanel.st.loading, 150000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(wikiPanel.st.host === "box" && wikiPanel.st.root === "ssh://box" + win.argValue("--cwd=") && !wikiPanel.st.error, "the panel reads the wiki on box (" + wikiPanel.st.root + (wikiPanel.st.error ? ", " + wikiPanel.st.error : "") + ")")
            tests.check(JSON.stringify(tests.wikiRows()) === JSON.stringify(["Overview", "Jobs"]), "its pages, the overview first (" + JSON.stringify(tests.wikiRows()) + ")")
            tests.check(tests.wikiFresh() === "Up to date", "nothing owed to box's code (" + tests.wikiFresh() + ")")
            tests.check(tests.findText("repo on box", wikiPanel) !== null, "its head names the host")
            tests.shotWindow("wiki-remote-pages")
            wikiPanel.openWiki("architecture/jobs.md", "", false) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => wikiPanel.openPage === "architecture/jobs.md" && wikiPanel.loadingPage === "" && !!wikiPanel.pageView, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = wikiPanel.pageView, notes = v.footnotes || []
            const html = (v.blocks || []).map(b => b.html || "").join("")
            tests.check(!v.error && v.title === "Jobs" && html.indexOf("okf-fn:retry") >= 0, "a page read from box, its footnote marked in the text (" + (v.error || v.title) + ")")
            tests.check(notes.length === 1 && notes[0].path === "src/jobs.rs" && notes[0].start === 1 && notes[0].state === "current", "its footnote found in box's code (" + JSON.stringify(notes) + ")")
            tests.check((v.backlinks || []).length === 1 && v.backlinks[0].path === "overview.md", "and the page linking to it")
            tests.shotWindow("wiki-remote-reader")
            wikiPanel.closePage()
            wikiPanel.search("retry backoff") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => !wikiPanel.searching && wikiPanel.query === "retry backoff" && !!wikiPanel.results, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = wikiPanel.results || {}, hits = (r.hits || []).map(h => h.page + "#" + h.section)
            tests.check(hits[0] === "architecture/jobs.md#retry-control", "the search on box finds the section (" + hits.slice(0, 3).join(", ") + (r.error ? " " + r.error : "") + ")")
            // A name: box's code index lists it, and its card comes from there.
            wikiPanel.search("retry") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => !wikiPanel.searching && wikiPanel.query === "retry" && !!wikiPanel.results, 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const code = (wikiPanel.results || {}).code || []
            tests.check(code.length === 1 && code[0].path === "src/jobs.rs" && code[0].name === "retry", "a name's search lists its declaration in box's code (" + JSON.stringify(code) + ")")
            if (code[0]) wikiPanel.toggleCard(code[0]) } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => wikiPanel.cards["src/jobs.rs#retry"] !== undefined, 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const card = wikiPanel.cards["src/jobs.rs#retry"]
            tests.check(card === "pub fn retry()\ncalled by: nothing the index finds", "and its card, from the code index on box (" + JSON.stringify(card) + ")")
            wikiPanel.search("")
            // Nothing owed: Update records that there, and starts no session.
            tests.wikiKeyBefore = Workspace.activeKey
            tests.check(Workspace.startWikiRun("project:" + tests.wikiProject, "update") === "", "Update starts on box") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => !!JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)).recorded, 60000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === tests.wikiKeyBefore, "with nothing owed, no session starts")
            tests.check(tests.remoteWikiFile("/.okf/.harlo/ledger.json").indexOf("src/jobs.rs") >= 0, "the check records the footnotes' code on box")
            // The code its footnote cites changes on box.
            TestInput.writeFile(win.argValue("--cwd=") + "/src/jobs.rs", "pub fn retry() {\n    four();\n}\n")
            Workspace.refreshWiki() } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.wikiFresh().indexOf("1 footnote's code changed") >= 0, 40000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(tests.wikiFresh().indexOf("1 footnote's code changed") >= 0, "the panel says box's code changed (" + tests.wikiFresh() + ")")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => chat.session.modelName !== "", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "a Harlo session on box (" + chat.session.cwd + ")")
            tests.sendByKeys("hello") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.modelRequests().length > tests.wikiRequests && chat.session.status === "idle", 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const said = tests.wikiSaid(tests.wikiRequests)
            tests.check(tests.wikiTools(tests.wikiRequests).length === 0, "no wiki tool reaches it (" + tests.wikiTools(tests.wikiRequests).join(", ") + ")")
            tests.check(said.indexOf("so the code wins): .okf/architecture/jobs.md.") >= 0, "and it's told the page is out of date, as box's code says (" + said.slice(said.indexOf("This project has a wiki"), said.indexOf("This project has a wiki") + 60) + ")")
            tests.check(tests.lookupTools(tests.wikiRequests).length === 0, "Code lookup off: no lookup tool (" + tests.lookupTools(tests.wikiRequests).join(", ") + ")")
            tests.shotWindow("wiki-remote-session")
            // Code lookup on: a new session on box gets it, answered there.
            Workspace.setSetting("lookup_" + tests.wikiProject, "1")
            tests.wikiHello() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => chat.session.modelName !== "", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { tests.wikiDone = tests.doneCount(); tests.sendByKeys("look up retry") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.wikiAnswered(), 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const said = tests.lastToolResult()
            tests.check(tests.lookupTools(tests.wikiRequests).join() === "lookup" && said.indexOf("fn retry  src/jobs.rs:1-3") >= 0,
                      "on: the session on box gets lookup, its card from box's code (" + tests.lookupTools(tests.wikiRequests).join() + ": " + said.slice(0, 120) + ")")
            Workspace.markWikiReviewed("project:" + tests.wikiProject, "architecture/jobs.md") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.remoteWikiFile("/.okf/architecture/jobs.md").indexOf("verified:") >= 0, 30000) }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => tests.wikiFresh() === "Up to date", 40000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const page = tests.remoteWikiFile("/.okf/architecture/jobs.md")
            tests.check(/by: "?human:/.test(page), "Mark reviewed stamps the page on box (" + page.slice(0, 200).replace(/\n/g, " | ") + ")")
            tests.check(tests.wikiFresh() === "Up to date", "and its changed footnote counts as read (" + tests.wikiFresh() + ")")
            // Create, for another repository on box: a wiki session there,
            // its check run there.
            Workspace.addProject("ssh://box" + win.argValue("--cwd=") + "2") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo2")
            tests.wikiProject = p ? Number(p[0]) : -1
            tests.check(tests.wikiProject >= 0, "a second project on box")
            Workspace.setSetting("default_harness", "harlo")
            Workspace.setSetting("project_model_" + tests.wikiProject + "_harlo", "fake/fake-helper")
            tests.wikiKeyBefore = Workspace.activeKey
            tests.check(Workspace.startWikiRun("project:" + tests.wikiProject, "create") === "", "Create starts on box") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => Workspace.activeKey !== tests.wikiKeyBefore && chat.session.key === Workspace.activeKey, 60000) }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => /Wiki: /.test(tests.chatText()) && chat.session.status === "idle", 90000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const said = tests.chatText(), cwd = win.argValue("--cwd=") + "2"
            tests.check(chat.session.title === "Wiki: create" && chat.session.cwd === "ssh://box" + cwd, "a wiki session on box (" + chat.session.title + ", " + chat.session.cwd + ")")
            tests.check(said.indexOf("Wiki: complete") >= 0, "the run finishes there (" + said.slice(said.lastIndexOf("Wiki:"), said.lastIndexOf("Wiki:") + 300) + ")")
            const page = TestInput.readFile(cwd + "/.okf/overview.md") || ""
            tests.check(page.indexOf("generated:") >= 0 && page.indexOf("harlo/fake-helper") >= 0, "the page is written and stamped on box (" + page.slice(0, 160).replace(/\n/g, " | ") + ")")
            tests.check((TestInput.readFile(cwd + "/.okf/log.md") || "").indexOf("(/overview.md)") >= 0 && (TestInput.readFile(cwd + "/.okf/index.md") || "").indexOf("okf_version") >= 0, "with its index and log there")
            tests.check(TestInput.readFile(cwd + "/AGENTS.md") === "", "and nothing outside .okf/ written")
            tests.shotWindow("wiki-remote-create")
            // A commit there to the README, which no page answers for:
            // Update checks on box, and needs no model.
            tests.log("wiki-quiet-now") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => (TestInput.readFile(win.argValue("--cwd=") + "2/.git/logs/HEAD") || "").indexOf("commit: more") >= 0, 30000) }
        // (git writes its log a moment before HEAD moves.)
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.wikiKeyBefore = Workspace.activeKey
            tests.wikiRequests = tests.modelRequests().length
            tests.check(Workspace.startWikiRun("project:" + tests.wikiProject, "update") === "", "Update starts on box") } }
        ScriptAction { script: tests.waitUntil(wikiRemoteSteps, () => !!JSON.parse(Workspace.wikiJson("project:" + tests.wikiProject)).recorded, 60000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(Workspace.activeKey === tests.wikiKeyBefore && tests.modelRequests().length === tests.wikiRequests, "nothing owed: recorded on box with no model, and no session starts")
            tests.summary(); Qt.quit() } }
    }

    // A diagram that doesn't parse: harlo tells the agent what's wrong
    // when its turn ends and it sends the diagram again, fixed, drawn as a
    // picture; one sent broken again isn't asked about a second time.
    function harloNotes() {
        const reqs = tests.modelRequests()
        const msgs = (reqs[reqs.length - 1] || {}).messages || []
        return msgs.filter(m => m.role === "user" && JSON.stringify(m.content).indexOf("In your last reply, a diagram doesn't parse") >= 0)
    }
    property string diagramKey2: ""
    SequentialAnimation {
        id: diagramSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlodiagram"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(diagramSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("draw a broken diagram") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(diagramSteps, () => tests.chatText().indexOf("Again:") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const notes = tests.harloNotes()
            const said = notes.length ? JSON.stringify(notes[0].content) : ""
            tests.check(notes.length === 1 && /line \d+: /.test(said), "harlo tells the agent what's wrong in its diagram (" + said.slice(0, 260) + ")")
            const pics = tests.findAllNames("diagram", chat.transcript)
            tests.check(pics.length >= 1, "the diagram sent again is drawn (" + pics.length + " pictures)")
            tests.check(tests.modelRequests().length === 2, "and harlo asks no more (" + tests.modelRequests().length + " requests)")
            tests.shotWindow("clicks-harlo-diagram")
            tests.clickItem(pics[pics.length - 1], "the diagram") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            tests.check(v && v.opened && v.pictures.length === 1 && v.title === "Diagram" && v.fit > 1 && tests.findName("viewerZoom") !== null,
                      "clicking the diagram opens it full size in harlo, a small one enlarged (" + (v ? v.fit.toFixed(2) : "") + ")")
            tests.shotWindow("clicks-harlo-diagram-viewer")
            v.close()
            tests.diagramKey2 = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(diagramSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("draw a stubborn diagram") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(diagramSteps, () => tests.chatText().indexOf("Again:") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const reqs = tests.modelRequests()
            tests.check(reqs.length === 4, "broken again: harlo asks only once (" + reqs.length + " requests)")
            Workspace.open(tests.harloKey) } }
        // A diagram shown as its code stays so when its row is made again
        // (another session in between).
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickText("Code", chat.transcript) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("diagram", chat.transcript).length === 0 && tests.findText("Code", chat.transcript) === null, "Code shows the diagram's code")
            Workspace.open(tests.diagramKey2) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: Workspace.open(tests.harloKey) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("diagram", chat.transcript).length === 0 && tests.findText("Code", chat.transcript) === null,
                      "back in its session, the diagram still shows its code (" + tests.findAllNames("diagram", chat.transcript).length + " pictures)")
            tests.summary(); Qt.quit() } }
    }

    // Options and mockups in a reply: a comparison drawn as cards (its code
    // a click away), mockups the agent wrote shown as pictures with the one
    // before them, taken again when it edits the file, and a mockup of a
    // file that isn't there sent back to the agent.
    property string mockUrl: ""
    SequentialAnimation {
        id: visualSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlovisuals"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const said = JSON.stringify(((tests.modelRequests()[0] || {}).messages || []).filter(m => m.role === "system"))
            tests.sendByKeys("compare the approaches") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => tests.chatText().indexOf("I'd match by key") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const said = JSON.stringify(((tests.modelRequests()[0] || {}).messages || []).filter(m => m.role === "system"))
            tests.check(said.indexOf("```options") >= 0 && said.indexOf("```mockup") >= 0, "the agent is told about options and mockups")
            const cards = tests.findAllNames("optionCard", chat.transcript)
            const marked = tests.findAllNames("optionRecommended", chat.transcript)
            tests.check(cards.length === 2 && marked.length === 1, "the options are drawn as cards, the recommended one marked (" + cards.length + ", " + marked.length + ")")
            tests.check(tests.findText("nothing to restore", chat.transcript) !== null && tests.findText("the list still redraws", chat.transcript) !== null, "with their pros and cons")
            const about = tests.findAllNames("optionAbout", chat.transcript).find(l => l.text.indexOf("key") >= 0)
            tests.check(about && about.textFormat === Text.RichText && about.text.indexOf("<code>key</code>") >= 0 && about.text.indexOf("`") < 0,
                      "a card's Markdown is drawn: `key` as code, no backticks (" + (about ? about.text.replace(/^<style>.*<\/style>/, "") : "none") + ")")
            const plainOne = tests.findAllNames("optionAbout", chat.transcript).find(l => l.text === "Smaller change.")
            tests.check(plainOne && plainOne.textFormat === Text.PlainText, "a line with no marks stays plain text")
            tests.shotWindow("clicks-harlo-options")
            tests.clickText("Code", chat.transcript) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("optionCard", chat.transcript).length === 0 && tests.findText("Cards", chat.transcript) !== null, "Code shows the block's text, Cards the cards again")
            tests.clickText("Cards", chat.transcript) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // Waiting for your answer: a Choose on each card; one sends its name.
            const chooses = tests.findAllNames("optionChoose", chat.transcript).filter(b => b.visible)
            tests.check(chooses.length === 2 && chooses.some(b => b.variant === "primary"), "a Choose button on each card, the recommended one filled (" + chooses.length + ")")
            const [p0, p1] = chooses.map(b => b.mapToItem(null, 0, 0))
            tests.check(chooses.length === 2 && (Math.abs(p0.x - p1.x) < 1 || Math.abs(p0.y - p1.y) < 1),
                      "side by side, the buttons line up (" + (p0 ? p0.y.toFixed(0) : "") + ", " + (p1 ? p1.y.toFixed(0) : "") + ")")
            tests.clickItem(chooses.find(b => b.Accessible.name === "Choose Match rows by key"), "Choose on the recommended card") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => tests.chatText().indexOf("ok: Match rows by key") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const sent = ((tests.modelRequests().slice(-1)[0] || {}).messages || []).filter(m => m.role === "user").slice(-1)[0]
            const said = sent ? (typeof sent.content === "string" ? sent.content : JSON.stringify(sent.content)) : ""
            tests.check(said.indexOf("Match rows by key") >= 0, "the choice's name is sent as your answer (" + said.substring(0, 60) + ")")
            const chosen = tests.findAllNames("optionChosen", chat.transcript).filter(l => l.visible)
            const cards = tests.findAllNames("optionCard", chat.transcript)
            tests.check(chosen.length === 1 && tests.findAllNames("optionChoose", chat.transcript).every(b => !b.visible), "the card is marked Chosen, the buttons gone")
            tests.check(cards.filter(c => c.opacity < 0.9).length === 1, "the other card steps back")
            tests.sendByKeys("mock up the settings") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => tests.chatText().indexOf("That's the change") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => { const p = tests.findName("mockupPicture"); return p !== null && p.visible }, 15000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pic = tests.findName("mockupPicture")
            tests.check(pic && pic.visible && pic.width <= 642 && Math.abs((pic.width - 2) / (pic.height - 2) - 1.6) < 0.02, "the mockup is shown as its picture, in its window's proportions, fitted to the column (" + (pic ? pic.width + "x" + pic.height : "none") + ")")
            const cap = tests.findName("mockupCaption")
            const ba = tests.findName("mockupBeforeAfter")
            tests.check(cap && cap.text === "Settings with Sync now" && ba && ba.visible && ba.current === 1, "with its title, After chosen of Before | After (" + (cap ? cap.text : "") + ")")
            tests.mockUrl = pic.children[0].source.toString()
            tests.shotWindow("clicks-harlo-mockup") } }
        // (Both pictures taken: a real browser takes a moment.)
        ScriptAction { script: tests.waitUntil(visualSteps, () => { const m = tests.findName("mockupView"); return m && m.mock && m.mock.before && m.mock.before.state === "ready" && m.mock.after.state === "ready" }, 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.clickItem(tests.findName("mockupPicture"), "the mockup's picture") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            const frame = tests.findName("viewerPictureFrame")
            tests.check(v && v.opened && v.pictures.length === 2 && v.pic.label === "After" && frame && frame.width > 0,
                      "clicking it opens it full size in harlo, After shown, Before beside it (" + (v ? v.pictures.length : "none") + "; " + (() => { const m = tests.findName("mockupView"); return m && m.mock ? (m.mock.before ? m.mock.before.state : "-") + "/" + m.mock.after.state : "" })() + ")")
            tests.shotWindow("clicks-harlo-mockup-viewer")
            // (Shown whole at its own size, there's nothing to switch.)
            if (v.fit < 1) tests.clickItem(tests.findName("viewerZoom"), "100%")
            else tests.check(tests.findName("viewerZoom") === null, "no 100% when it's shown at its own size") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            tests.check(Math.abs(v.scaleNow - 1) < 0.001 && Math.abs(tests.findName("viewerPictureFrame").width - v.pic.width - 2) <= 1, "100% shows it at its own size (" + v.scaleNow + ", " + tests.findName("viewerPictureFrame").width + " for " + v.pic.width + ")")
            tests.clickItem(tests.findName("viewerPictureBefore"), "Before in the viewer") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            tests.check(v.pic.label === "Before" && v.pic.url !== tests.mockUrl, "Before | After switches the picture there too")
            v.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("mockupBefore"), "Before") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => { const p = tests.findName("mockupPicture"); return p !== null && p.visible }, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pic = tests.findName("mockupPicture")
            tests.check(tests.findName("mockupBeforeAfter").current === 0 && pic.children[0].source.toString() !== tests.mockUrl, "Before shows how it is now")
            tests.clickItem(tests.findName("mockupAfter"), "After")
            tests.sendByKeys("change the mockup") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => { const p = tests.findName("mockupPicture"); return p !== null && p.visible && p.children[0].source.toString() !== tests.mockUrl && tests.findName("mockupBeforeAfter").current === 1 }, 15000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(true, "the agent's edit of the file shows in the picture")
            tests.sendByKeys("mock up a missing file") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(visualSteps, () => tests.chatText().split("That's the change").length >= 3 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const notes = tests.modelRequests().reduce((all, b) => all.concat(b.messages || []), []).filter(m => m.role === "user" && JSON.stringify(m.content).indexOf("a mockup names `mockups/none.html`, which isn't there") >= 0)
            tests.check(notes.length >= 1, "a mockup of a file that isn't there goes back to the agent, which shows the right one")
            tests.summary(); Qt.quit() } }
    }

    // A split: Harlo offers a helper in a worktree, Start helpers starts it,
    // it writes a file and reports, and Harlo brings its work over
    // (bring_over) to the project as an uncommitted change.
    SequentialAnimation {
        id: splitSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlosplit"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            // (The helper works in its parent's mode: writes without asking.)
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(splitSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("split it please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(splitSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findText("Split into helpers?") !== null || tests.findContaining("Split into helpers", chat.requestDock) !== null, "Harlo offers the split")
            tests.shotWindow("clicks-split-offer")
            tests.clickText("Start helpers", chat.requestDock) } }
        PauseAnimation { duration: 300 }
        // The helper starts in its worktree, writes, reports; the parent
        // gets its report and brings the work over.
        ScriptAction { script: tests.waitUntil(splitSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/helper.txt") !== "", 40000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            tests.check(kids.length === 1, "one helper started (" + JSON.stringify(kids) + ")")
            const t = tests.chatText()
            tests.check(t.indexOf("finished and reported: succeeded") >= 0, "its report comes back to Harlo")
            tests.check(TestInput.readFile(win.argValue("--cwd=") + "/helper.txt") === "from the helper\n", "bring_over brings helper.txt to the project")
            tests.check(git.unstagedCount + git.stagedCount > 0 || t.indexOf("helper.txt") >= 0, "as a change, nothing committed (" + t.slice(-200) + ")")
            // The helper isn't offered splits of its own part; its parent is.
            const reqs = tests.modelRequests()
            const firstUser = r => { const m = (r.messages || []).find(m => m.role === "user"); return m ? JSON.stringify(m.content) : "" }
            const offers = r => (r.tools || []).some(x => x["function"].name.endsWith("propose_helpers"))
            const said = r => JSON.stringify((r.messages || []).filter(m => m.role === "system"))
            const helperReqs = reqs.filter(r => firstUser(r).indexOf("You're a helper") >= 0)
            const parentReqs = reqs.filter(r => firstUser(r) === JSON.stringify("split it please"))
            tests.check(helperReqs.length > 0 && !helperReqs.some(offers) && !helperReqs.some(r => said(r).indexOf("propose_helpers") >= 0),
                      "the helper isn't offered propose_helpers, nor told of it (" + helperReqs.length + " requests)")
            tests.check(parentReqs.length > 0 && parentReqs.every(offers), "its parent is")
            tests.shotWindow("clicks-split-brought") } }
        PauseAnimation { duration: 200 }
        // The report shows as harlo's, not yours: not on the prompt rail, no
        // Edit & resend; Open helper shows the helper.
        ScriptAction { script: {
            // (Among harlo's rows: the report.)
            const rows = []
            const walk = it => { if (!it) return; if (it.objectName === "harloMessage" && it.visible) rows.push(it); for (const c of it.children || []) walk(c); if (it.contentItem && it.contentItem !== it) walk(it.contentItem) }
            walk(chat)
            const msg = rows.find(r => / finished/.test(tests.findName("harloMessageText", r).text)) || null
            const text = msg ? tests.findName("harloMessageText", msg) : null
            tests.check(msg !== null && msg.objectName === "harloMessage" && msg.visible && text.text.indexOf("[harlo]") < 0,
                      "the helper's report shows as harlo's (" + (text ? text.text.slice(0, 120) : "none") + ")")
            tests.check(text !== null && !/live:|report_done|\(local:|session "/.test(text.text), "it says what happened plainly, no session keys or tool names (" + (text ? text.text : "") + ")")
            const toggle = msg ? tests.findName("harloSentToggle", msg) : null
            tests.check(toggle !== null && toggle.visible, "what harlo sent is a click away")
            if (toggle) toggle.clicked()
            const sent = msg ? tests.findName("harloSent", msg) : null
            tests.check(sent !== null && sent.visible && sent.text.indexOf("finished and reported") >= 0, "a click shows it")
            if (toggle) toggle.clicked()
            const prompts = JSON.parse(chat.session.userPrompts())
            tests.check(prompts.length === 1 && prompts[0].text === "split it please", "the prompt rail has only yours (" + JSON.stringify(prompts) + ")")
            tests.check(msg !== null && tests.findText("Edit & resend", msg.parent) === null, "no Edit & resend on it")
            const open = msg ? tests.findName("openHelper", msg) : null
            tests.check(open !== null && open.visible, "it offers Open helper")
            tests.clickItem(open, "Open helper") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            const helper = kids.length ? kids[0].openKey : ""
            tests.check(helper !== "" && Workspace.activeKey === helper, "Open helper shows the helper (" + Workspace.activeKey + ", " + JSON.stringify(kids) + ")")
            tests.shotWindow("clicks-split-helper") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { tests.summary(); Qt.quit() } }
    }

    // A split on a real model (run by hand, not in the click tests: it
    // needs a sign-in): Harlo main offers helpers for two independent
    // modules, they're started, report back, and the work comes over.
    // --model=provider/model picks the model.
    SequentialAnimation {
        id: liveSplitSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "liveharlosplit"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(liveSplitSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.log("model", chat.session.modelName)
            tests.sendByKeys("Create two independent Python modules with tests: bmi.py (bmi(weight_kg, height_m) rounded to one decimal, and category(value) giving underweight, normal, overweight or obese by the WHO cut-offs) and roman.py (to_roman(n) and from_roman(text) for 1 to 3999, ValueError on anything else). Each gets its own pytest file." + (win.argValue("--split=") ? " Split it into two helpers, one per module." : "")) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: tests.waitUntil(liveSplitSteps, () => chat.session.hasRequest || chat.session.status === "idle", 600000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // (Asked for helpers, it starts them itself; otherwise it offers.)
            const offered = chat.session.hasRequest && tests.findContaining("Split into helpers", chat.requestDock) !== null
            const started = JSON.parse(Workspace.orchestratedJson(tests.harloKey)).length > 0
            tests.check(offered || started, "Harlo offers the split, or starts the helpers asked for")
            tests.shotWindow("live-split-offer")
            if (offered) tests.clickText("Start helpers", chat.requestDock)
            if (offered || started) return
            tests.log("chat", tests.chatText().slice(-3000))
            tests.summary(); Qt.quit() } }
        PauseAnimation { duration: 5000 }
        // Both helpers report, and the main session is done with them.
        ScriptAction { script: tests.waitUntil(liveSplitSteps, () => {
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            const reports = tests.chatText().split("finished and reported").length - 1
            return kids.length > 0 && reports >= kids.length && chat.session.status === "idle" && !chat.session.hasRequest
        }, 1500000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            tests.log("helpers", JSON.stringify(kids))
            tests.check(kids.length >= 2, "two helpers (" + kids.length + ")")
            tests.check(TestInput.readFile(cwd + "/bmi.py") !== "" && TestInput.readFile(cwd + "/roman.py") !== "", "both modules are in the project")
            tests.log("chat", tests.chatText().slice(-3000))
            tests.shotWindow("live-split-done")
            tests.summary(); Qt.quit() } }
    }

    // A benchmark task in harlo (scripts/bench's `app` agent, on a real
    // model): a Harlo session in the task's repository is given the task
    // (--prompt-file), a split it offers is accepted, and once it and its
    // helpers have all been done for a while, what its helpers did goes to
    // <selftest>/bench.json for the task's check.
    property double benchQuiet: 0
    SequentialAnimation {
        id: benchSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "benchtask"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            // (--harness=claude: a Claude Code session, with harlo's tools.)
            Workspace.setProviderEnabled("claude", win.argValue("--harness=") === "claude")
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const project = JSON.parse(Workspace.projectsJson())[0].id
            // (--wiki=on|off: the project's wiki, its agents told of it or not;
            // --lookup=on|off: the lookup tool.)
            if (win.argValue("--wiki=") !== "") Workspace.setSetting("wiki_" + project, win.argValue("--wiki=") === "on" ? "1" : "0")
            if (win.argValue("--lookup=") !== "") Workspace.setSetting("lookup_" + project, win.argValue("--lookup=") === "on" ? "1" : "0")
            tests.harloKey = Workspace.newSession(project, win.argValue("--harness=") || "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(benchSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            if (win.argValue("--thinking=")) chat.session.setThinkingLevel(win.argValue("--thinking=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const task = TestInput.readFile(win.argValue("--prompt-file=")).trim()
            tests.log("model", chat.session.modelName)
            chat.session.send(task, "normal") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.waitUntil(benchSteps, () => {
            // (A split it offers: accepted, as the task asks for one.)
            if (chat.session.hasRequest && tests.findContaining("Split into helpers", chat.requestDock) !== null) {
                tests.clickText("Start helpers", chat.requestDock)
                tests.benchQuiet = 0
                return false
            }
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            const done = chat.session.status === "idle" && !chat.session.hasRequest && kids.every(k => ["completed", "failed", "ended"].includes(k.status))
            if (!done) { tests.benchQuiet = 0; return false }
            if (!tests.benchQuiet) tests.benchQuiet = Date.now()
            return Date.now() - tests.benchQuiet > 15000
        }, 3 * 3600 * 1000) }
        // (A pause between: a wait followed at once by another action
        // doesn't hold the animation.)
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            TestInput.writeFile(win.selftestDir + "/bench.json", JSON.stringify({ helpers: kids, status: chat.session.status, chat: tests.chatText().slice(-4000) }, null, 1))
            tests.log("helpers", kids.length)
            Qt.quit() } }
    }

    // A plan run with helpers on a real model (run by hand: it needs a
    // sign-in): the session makes a two-step plan, the run hands each step
    // to a helper and has each reviewed before the next.
    SequentialAnimation {
        id: livePlanSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "liveharloplan"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(livePlanSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("Make the shared plan (plan_set) for this, with exactly two steps, and stop there: don't do the steps yourself. Step 1: bmi.py (bmi(weight_kg, height_m) rounded to one decimal, and category(value) giving underweight, normal, overweight or obese by the WHO cut-offs) with test_bmi.py. Step 2: roman.py (to_roman(n) and from_roman(text) for 1 to 3999, ValueError on anything else) with test_roman.py. Both use pytest.") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: tests.waitUntil(livePlanSteps, () => (JSON.parse(Workspace.planJson(tests.harloKey)).steps || []).length >= 2 && chat.session.status === "idle", 300000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.log("plan", Workspace.planJson(tests.harloKey))
            tests.clickItem(tests.findName("planRun"), "Run with helpers") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("planRunTeam"), "Helpers: as this session") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: tests.waitUntil(livePlanSteps, () => { const p = JSON.parse(Workspace.planJson(tests.harloKey)); return !p.run || (p.run.paused || "") !== "" }, 1800000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.planJson(tests.harloKey))
            tests.log("plan", JSON.stringify(p))
            tests.log("helpers", Workspace.orchestratedJson(tests.harloKey))
            tests.check(!p.run && (p.steps || []).every(s => s.status === "done"), "the plan ran to the end (" + (p.run ? p.run.paused : "") + ")")
            tests.check((p.steps || []).every(s => (s.summary || "").indexOf("Reviewed:") >= 0), "each step was reviewed")
            tests.shotWindow("live-plan-done")
            tests.summary(); Qt.quit() } }
    }

    // Implement and verify (a workflow): Harlo implements in a worktree,
    // harlo runs the tests, the stand-in Claude reviews. greet.txt says
    // "hi" (the tests fail: back to the same session), then "hello" (the
    // review asks for "hello!": back again, tested again), then "hello!"
    // (approved). scripts/fake-harlo-model.py and fake-claude.py play them.
    readonly property string workflowTests: "grep -Ex 'hello!?' greet.txt || { echo \"expected hello, got: $(cat greet.txt)\"; exit 1; }"
    function workflowRun() { const runs = JSON.parse(Workspace.workflowsJson()); return runs.length ? runs[0] : null }
    function workflowLogHas(text) { const r = workflowRun(); return !!r && r.log.some(e => e.text.indexOf(text) >= 0) }
    /// The stand-in claude reviews (never the installed one): false, and
    /// the scenario ends, when it isn't given.
    function useFakeClaude() {
        const fake = win.argValue("--fake-claude=")
        if (fake === "") { tests.check(false, "the stand-in claude is given (--fake-claude)"); tests.summary(); Qt.quit(); return false }
        Workspace.setSetting("claude_path", fake)
        return true
    }
    function startWorkflowDirect() {
        const p = JSON.parse(Workspace.projectsJson())[0]
        return Workspace.startWorkflow(p.id, JSON.stringify({ task: "Write greet.txt greeting the user", criteria: "- greet.txt says hello!", testCommand: workflowTests, implementer: "harlo", reviewer: "claude", approval: "write" }))
    }
    SequentialAnimation {
        id: workflowSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflow"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findText("Run a workflow…"), "the project menu's Run a workflow…") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            tests.check(d.visible && d.title.indexOf("Run a workflow in") === 0, "the project menu opens Run a workflow (" + d.title + ")")
            tests.check(d.flowBox.currentText === "Implement and verify" && tests.findName("workflowPickStages").text.indexOf("Implement  →  ") === 0, "it offers Implement and verify first, its stages under it (" + d.flowBox.currentText + ", " + tests.findName("workflowPickStages").text + ")")
            tests.check(d.implementerBox.currentText === "Harlo" && d.reviewerBox.currentText === "Claude", "Harlo implements and Claude reviews by default (" + d.implementerBox.currentText + ", " + d.reviewerBox.currentText + ")")
            tests.check(!tests.findName("workflowStart").enabled, "Start waits for a task")
            tests.check(d.repoState === "ready" && !tests.shown(tests.findName("workflowNoGit")) && !tests.shown(tests.findName("workflowNoCommits")), "a repository with commits needs nothing more (" + d.repoState + ")")
            d.taskArea.text = "Write greet.txt greeting the user"
            d.criteriaArea.text = "- greet.txt says hello!"
            d.testField.text = "" } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(tests.findName("workflowStart").enabled, "with no test command it can still start")
            tests.check(tests.findName("workflowPickStages").text === "Implement  →  Review    (Tests left out: no test command)", "and the stages say its tests are left out (" + tests.findName("workflowPickStages").text + ")")
            sidebar.workflowDialog.testField.text = tests.workflowTests
            tests.check(tests.findName("workflowPickStages").text === "Implement  →  Tests  →  Review", "given one, they're back (" + tests.findName("workflowPickStages").text + ")")
            tests.check(sidebar.workflowDialog.testField.cursorPosition === 0, "a long test command shows where it starts")
            tests.check(tests.findName("workflowStart").accent, "Start is the dialog's main button")
            const d = sidebar.workflowDialog
            tests.check(d.implementerModelBox.currentIndex === 0 && d.implementerModelBox.currentText.startsWith("Default") && d.reviewerModelBox.currentIndex === 0 && d.reviewerModelBox.currentText.startsWith("Default"), "each agent starts on its default model (" + d.implementerModelBox.currentText + ", " + d.reviewerModelBox.currentText + ")") } }
        // Each role's model: its agent's default, or one of its models
        // (read when the dialog opens).
        ScriptAction { script: tests.waitUntil(workflowSteps, () => sidebar.workflowDialog.implementerModels.length > 0 && sidebar.workflowDialog.reviewerModels.length > 0, 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            const helper = d.implementerModels.findIndex(m => m.name === "Fake Helper")
            tests.check(helper >= 0 && d.reviewerModels.length > 0, "the model menus list the agents' models (" + d.implementerModels.map(m => m.name) + " | " + d.reviewerModels.map(m => m.name) + ")")
            d.implementerModelBox.currentIndex = helper + 1
            d.implementerModelBox.activated(helper + 1)
            d.reviewerModelBox.currentIndex = 1
            d.reviewerModelBox.activated(1)
            tests.workflowModels = { implementer: d.implementerModel, reviewer: d.reviewerModel }
            tests.shotWindow("clicks-workflow-dialog") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findName("workflowStart"), "Start") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(workflowSteps, () => !!tests.workflowRun() && tests.workflowRun().implementation !== "" && tests.shown(tests.findName("workflowCard", chat)), 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(!sidebar.workflowDialog.visible && !!r, "Start closes the dialog and starts the run")
            tests.check(r && r.branch.indexOf("workflow/write-greet-txt") === 0, "it works in a new worktree on its own branch (" + (r ? r.branch : "") + ")")
            tests.check(Workspace.workflowTestCommand(JSON.parse(Workspace.projectsJson())[0].id) === tests.workflowTests, "the project's test command is remembered")
            tests.harloKey = chat.session.key
            tests.check(Workspace.workflowFor(chat.session.key) !== "", "the implementation session is open, with the run's card")
            tests.check(r && r.implementerModel === tests.workflowModels.implementer && r.reviewerModel === tests.workflowModels.reviewer && r.implementerModel.indexOf("fake-helper") >= 0,
                        "the run keeps the models picked (" + (r ? r.implementerModel + ", " + r.reviewerModel : "") + ")")
            tests.check(chat.session.modelId.indexOf("fake-helper") >= 0, "and the implementation session runs on the implementer's (" + chat.session.modelId + ")")
            tests.check(Workspace.setting("workflow_implementer_" + JSON.parse(Workspace.projectsJson())[0].id) === "harlo\t" + tests.workflowModels.implementer, "remembered for the project's next run") } }
        // The tests fail: back to the same session, as attempt 2.
        ScriptAction { script: tests.waitUntil(workflowSteps, () => tests.workflowLogHas("Tests failed") && tests.workflowRun().attempt === 2, 30000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.attempt === 2 && r.stages[0].status === "active", "a failed test goes back to the implementation, as attempt 2 (" + r.attempt + ", " + JSON.stringify(r.stages) + ")")
            tests.check(chat.session.key === tests.harloKey, "the same session, still open")
            tests.clickItem(tests.findName("workflowToggle", chat), "open the run's card") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.findContaining("Tests failed", card) !== null && tests.findContaining("back to the implementation session", card) !== null, "opened, the card's log says the tests failed and it went back")
            tests.check(tests.shown(tests.findName("workflowTest", card)) && tests.findContaining("expected hello, got: hi", card) !== null, "and shows the failed tests' output")
            const dots = tests.findAllNames("workflowLogDot", card)
            tests.check(dots.length > 0 && dots.filter(d => d.color.a > 0).length === 1, "a dot only on what went wrong (" + dots.filter(d => d.color.a > 0).length + " of " + dots.length + ")")
            const details = tests.findName("workflowDetails", card), lastDot = dots[dots.length - 1]
            tests.check(details && lastDot && details.mapToItem(card, 0, 0).y > lastDot.mapToItem(card, 0, 0).y, "the run's details come after its log")
            tests.check(details && !/-[0-9a-f]{6,}( |$)/.test(details.text) && details.text.indexOf("…") > 0 && details.text.indexOf("`") < 0 && details.text.indexOf("cat greet.txt") < 0 && details.ToolTip.text.indexOf("cat greet.txt") > 0,
                        "the details name the branch without its id and cut the long command, whole on hover (" + (details ? details.text : "none") + ")")
            tests.check(details && details.text.indexOf("Implement: Harlo · Fake Helper") >= 0 && details.text.indexOf("Review: Claude · " + Theme.helperModel("claude", tests.workflowModels.reviewer)) >= 0,
                        "and each agent's model (" + (details ? details.text : "none") + ")")
            tests.shotWindow("clicks-workflow-test-failed") } }
        // The review asks for changes: back to the same session again.
        ScriptAction { script: tests.waitUntil(workflowSteps, () => tests.workflowLogHas("Review asked for changes"), 40000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.attempt === 3 && chat.session.key === tests.harloKey, "a review asking for changes goes back to the same session, as attempt 3 (" + r.attempt + ")")
            tests.check(tests.findContaining("hello! (with the exclamation mark)", tests.findName("workflowCard", chat)) !== null, "the card says what the reviewer asked for")
            const notes = tests.findAllNames("workflowNote", chat).filter(n => n.visible).map(n => n.text)
            tests.check(notes.indexOf("Sent back for attempt 3: review of attempt 2 asked for changes") >= 0 && notes.every(n => /^(Sent the task|Sent back for attempt \d: (tests failed after attempt \d|review of attempt \d asked for changes))$/.test(n)),
                        "the workflow's messages to the agent are one quiet line each (" + notes.join(" | ") + ")")
            const shownTexts = tests.findAllNames("userText", chat).filter(t => t.visible && t.text.indexOf("report_done") >= 0)
            tests.check(shownTexts.length === 0, "their instructions stay folded (" + shownTexts.length + " shown)")
            const checkTitle = tests.findName("workflowCheckTitle", tests.findName("workflowCard", chat))
            tests.check(checkTitle && checkTitle.text === "Attempt 2 · Tests passed in 0 s", "attempt 2's check names its attempt while attempt 3 is under way (" + (checkTitle ? checkTitle.text : "none") + ")")
            tests.shotWindow("clicks-workflow-review-changes") } }
        ScriptAction { script: tests.waitUntil(workflowSteps, () => !!tests.workflowRun() && tests.workflowRun().state === "done", 40000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            const texts = r.log.map(e => "#" + e.attempt + " " + e.text)
            tests.log("workflow log:\n" + texts.join("\n"))
            tests.check(r.state === "done" && r.attempt === 3, "the run is done on attempt 3 (" + r.state + ")")
            // In order: tests (1 failed, 2 passed), review 2 (changes), the
            // fix tested before review 3 (approved).
            const at = (attempt, text) => r.log.findIndex(e => e.attempt === attempt && e.text.indexOf(text) === 0)
            const order = [at(1, "Tests failed"), at(2, "Tests passed"), at(2, "Review asked for changes"), at(3, "Tests passed"), at(3, "Review approved")]
            tests.check(order.every((i, n) => i >= 0 && (n === 0 || i > order[n - 1])), "each fix is tested before it's reviewed again (" + order + ")")
            tests.check(tests.workflowLogHas("Refused a report for attempt 2"), "a report for an earlier attempt is refused, and can't move the run")
            const tree = JSON.parse(Workspace.worktreeInfo(tests.harloKey)).path
            tests.check(TestInput.readFile(tree + "/greet.txt") === "hello!\n", "the worktree has the approved change")
            tests.check(TestInput.readFile(win.argValue("--cwd=") + "/greet.txt") === "", "the project's own checkout is untouched")
            // One implementation conversation, all three attempts in it.
            const reqs = tests.modelRequests()
            const body = r => JSON.stringify(r.messages || [])
            tests.check(reqs.some(q => body(q).indexOf("attempt 1 · implement]") >= 0 && body(q).indexOf("attempt 3 · implement]") >= 0), "every attempt went to the same implementation conversation")
            tests.check(reqs.some(q => body(q).indexOf("Tests failed after attempt 1") >= 0 && body(q).indexOf("expected hello, got: hi") >= 0), "the failed tests' output went back to it")
            const note = JSON.parse(Workspace.lastNotification || "{}")
            tests.check(note.title === "Workflow done", "you're told it's done (" + note.title + ")")
            const card = tests.findName("workflowCard", chat), composer = tests.findName("composerInput")
            tests.check(tests.findText("Done: Tests passed · Review approved", card) !== null, "the card says so")
            const reports = tests.findAllNames("toolName", chat).filter(n => n.visible).map(n => n.text).filter(t => /^(Reported )?[Aa]ttempt/.test(t))
            tests.check(reports.indexOf("Reported attempt 3") >= 0 || reports.indexOf("Attempt 3") >= 0, "a report reads as one (" + reports.join(", ") + ")")
            {
                const unused = tests.findContaining("not used", chat)
                tests.check(unused !== null && unused.objectName === "toolResult", "and a report for an attempt that's over says it wasn't used, not that it failed")
                // (Narrow too: its summary keeps room beside it.)
                const what = unused ? Array.from(unused.parent.children).find(c => c.objectName === "toolWhat") : null
                tests.check(what && what.width >= Math.min(60, what.implicitWidth) && unused.mapToItem(null, unused.width, 0).x <= chat.mapToItem(null, chat.width, 0).x,
                            "its summary still shows beside it (" + (what ? Math.round(what.width) + " px of " + Math.round(what.implicitWidth) : "none") + ", " + (unused ? unused.text.replace(/<[^>]*>/g, "") : "") + ")")
            }
            {
                const right = card.mapToItem(null, card.width, 0).x
                const inside = it => it.mapToItem(null, it.width, 0).x <= right + 1
                const parts = ["workflowStages", "workflowState", "workflowToggle"].map(n => tests.findName(n, card))
                tests.check(right <= chat.mapToItem(null, chat.width, 0).x + 1 && parts.every(p => p && inside(p)), "the card and its header fit the chat at this width (" + Math.round(card.width) + " px)")
                const title = tests.findName("workflowTitle", card), state = tests.findName("workflowState", card)
                tests.check(title && !title.truncated && title.text.indexOf("attempt 3") >= 0 || title && title.text === "Attempt 3", "its title shows the attempt whole (" + (title ? title.text + (title.truncated ? ", cut" : "") : "none") + ")")
                tests.check(state && !state.truncated, "and the state isn't cut")
            }
            tests.shotWindow("clicks-workflow-done")
            // (The card folded, for the reports above it.)
            tests.findName("workflowCard", chat).expanded = false } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { chat.transcript.listView.positionViewAtEnd(); tests.shotWindow("clicks-workflow-reports") } }
        // A reviewer's session shows the run's card too.
        ScriptAction { script: {
            const rows = tests.findAllNames("sessionTitle", sidebar).filter(t => t.visible && (t.text || "").indexOf("Review: ") === 0)
            tests.check(rows.length >= 1, "the reviewers are in the sidebar (" + rows.length + ")")
            if (rows.length) tests.clickItem(rows[0], "a reviewer's session") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.shown(card) && chat.session.key !== tests.harloKey, "its card shows in a reviewer's session")
            tests.summary(); Qt.quit() } }
    }

    // Closed while the review is pending (the stand-in reviewer holds), then
    // opened again: the run reconnects to the same reviewer session.
    SequentialAnimation {
        id: workflowRestart
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowrestart"
        PauseAnimation { duration: 900 }
        ScriptAction { script: if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd=")) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.check(tests.startWorkflowDirect() === "", "the run starts") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(workflowRestart, () => { const r = tests.workflowRun(); return !!r && r.stages[2].status === "active" && r.session !== "" }, 60000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.stages[2].status === "active" && r.session !== "" && r.attempt === 2, "the review of attempt 2 is under way, its session known (" + r.session + ")")
            tests.log("closing harlo while the review is pending")
            tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: workflowRestartAgain
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowrestart-reopened"
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(workflowRestartAgain, () => tests.workflowLogHas("reconnected to its session"), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(tests.workflowLogHas("harlo was opened again during the review of attempt 2: reconnected to its session (from its file)"), "reopened, the run reconnects to the reviewer's session")
            tests.check(r.state === "running" && r.session !== "", "it waits on that session again (" + r.session + ")")
            Workspace.open(r.implementation) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.check(tests.shown(tests.findName("workflowCard", chat)), "the implementation session shows the run")
            const rows = tests.findAllNames("sessionTitle", sidebar).filter(t => t.visible).map(t => t.text)
            tests.check(rows.some(t => t.indexOf("Review: Write greet.txt") === 0) && !rows.some(t => t.indexOf("[harlo workflow") === 0), "the reconnected reviewer keeps its name (" + rows + ")")
            tests.shotWindow("clicks-workflow-reopened") } }
        ScriptAction { script: tests.waitUntil(workflowRestartAgain, () => tests.workflowRun().state === "done", 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.state === "done" && r.attempt === 3, "it finishes: the review's changes, tested, then approved (" + r.state + ", " + r.attempt + ")")
            tests.summary(); Qt.quit() } }
    }

    // On a project on a host (the throwaway SSH server, as "box"): the
    // worktree made there, Harlo implementing there, the checks run over
    // SSH with their output coming in, the stand-in claude reviewing there.
    // harlo is closed while the review of attempt 2 is pending; opened
    // again, it reconnects to that session on box and the run finishes.
    readonly property string workflowRemoteTests: "echo checking on the host; sleep 2; " + workflowTests
    property bool workflowLiveSeen: false
    property string workflowOpenTried: ""
    function workflowRemoteHost() { return JSON.parse(Remotes.hostsJson()).find(h => h.name === "box") }
    SequentialAnimation {
        id: workflowRemote
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowremote"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (!tests.useFakeClaude()) return
            tests.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        ScriptAction { script: tests.waitUntil(workflowRemote, () => (tests.workflowRemoteHost() || {}).state === "connected", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: Workspace.addProject("ssh://box" + win.argValue("--cwd=")) }
        PauseAnimation { duration: 800 }
        // The start dialog's model menus list what the agents offer on box.
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.workflowDialog.show(p.id, p.name, p.harness)
            tests.check(sidebar.workflowDialog.host === "box", "the dialog knows the project is on box (" + sidebar.workflowDialog.host + ")") } }
        ScriptAction { script: tests.waitUntil(workflowRemote, () => sidebar.workflowDialog.implementerModels.length > 0, 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            tests.check(d.implementerModels.some(m => m.name === "Fake Coder") && d.implementerModelBox.currentIndex === 0 && d.implementerModelBox.currentText.startsWith("Default"), "on box, the implementer's models are Harlo's there (" + d.implementerModels.map(m => m.name) + ")")
            d.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson())[0]
            tests.check(p && p.path === "box:" + win.argValue("--cwd="), "the project is on box (" + (p ? p.path : "none") + ")")
            const why = Workspace.startWorkflow(p.id, JSON.stringify({ task: "Write greet.txt greeting the user", criteria: "- greet.txt says hello!", testCommand: tests.workflowRemoteTests, implementer: "harlo", reviewer: "claude", approval: "write" }))
            tests.check(why === "", "the run starts (" + why + ")") } }
        // A check's output, coming in over SSH as it runs.
        ScriptAction { script: tests.waitUntil(workflowRemote, () => { const r = tests.workflowRun(); if (r && r.liveCheck && r.liveCheck.output.indexOf("checking on the host") >= 0) tests.workflowLiveSeen = true; return tests.workflowLiveSeen }, 90000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(tests.workflowLiveSeen && r.liveCheck.name === "Tests", "the check's output comes in over SSH as it runs (" + JSON.stringify(r.liveCheck) + ")")
            tests.check(r.host === "box" && r.worktree.indexOf(win.argValue("--cwd=")) < 0 && r.worktree !== "", "the run is on box, in a worktree of its own (" + r.host + ", " + r.worktree + ")")
            Workspace.open(r.implementation) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            tests.check(tests.shown(tests.findName("workflowCard", chat)), "the implementation session on box opens, with the run's card")
            tests.shotWindow("clicks-workflow-remote-check") } }
        ScriptAction { script: tests.waitUntil(workflowRemote, () => { const r = tests.workflowRun(); return !!r && r.stages[2].status === "active" && r.session !== "" && r.attempt === 2 }, 120000) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.stages[2].status === "active" && r.session !== "", "the review of attempt 2 is under way on box (" + r.session + ")")
            tests.check(tests.workflowLogHas("Tests failed: exited with 1") && tests.workflowLogHas("Attempt 2: back to the implementation session"), "attempt 1's check failed over SSH and went back")
            tests.log("closing harlo while the review is pending")
            tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: workflowRemoteAgain
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowremote-reopened"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(!!r && r.host === "box", "the run is still there, on box")
            Remotes.connectHost("box") } }
        ScriptAction { script: tests.waitUntil(workflowRemoteAgain, () => tests.workflowLogHas("reconnected to its session"), 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(tests.workflowLogHas("harlo was opened again during the review of attempt 2: reconnected to its session"), "reopened, the run reconnects to the reviewer's session on box")
            tests.check(r.state === "running" && r.session !== "", "it waits on that session again (" + r.session + ")") } }
        ScriptAction { script: tests.waitUntil(workflowRemoteAgain, () => tests.workflowRun().state === "done", 120000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.state === "done" && r.attempt === 3, "it finishes on box: the review's changes, tested, then approved (" + r.state + ", " + r.attempt + ")")
            tests.check(TestInput.readFile(r.worktree.replace(/^ssh:\/\/box/, "") + "/greet.txt") === "hello!\n", "box's worktree has the approved change")
            tests.check(TestInput.readFile(win.argValue("--cwd=") + "/greet.txt") === "", "the project's own checkout on box is untouched") } }
        // (Opened again, the window shows this computer: opening box's
        // session goes there once harlo has box's sessions.)
        ScriptAction { script: tests.waitUntil(workflowRemoteAgain, () => {
            const card = tests.findName("workflowCard", chat)
            if (tests.shown(card)) return true
            tests.workflowOpenTried = Workspace.open(tests.workflowRun().implementation)
            return false }, 20000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            tests.log("open said:", tests.workflowOpenTried, "instance:", Workspace.instance)
            tests.check(tests.shown(tests.findName("workflowCard", chat)) && tests.findText("Done: Tests passed · Review approved", tests.findName("workflowCard", chat)) !== null, "its card says it's done")
            tests.shotWindow("clicks-workflow-remote-done")
            // What git needs, read on box: its repository is ready; a folder
            // there with a file and no git isn't.
            const proj = JSON.parse(Workspace.projectsJson()).find(x => x.path === "box:" + win.argValue("--cwd="))
            const ready = proj ? JSON.parse(Workspace.projectRepoState(proj.id)) : {}
            tests.check(ready.state === "ready", "box's repository is ready for a run (" + JSON.stringify(ready) + ")")
            Workspace.addProject("ssh://box" + win.argValue("--cwd=").replace(/\/[^\/]+$/, "/plain"))
            const plain = JSON.parse(Workspace.projectsJson()).find(x => x.path === "box:" + win.argValue("--cwd=").replace(/\/[^\/]+$/, "/plain"))
            const state = plain ? JSON.parse(Workspace.projectRepoState(plain.id)) : {}
            tests.check(state.state === "no-git" && state.files === true, "a folder on box with a file and no git is seen as one (" + JSON.stringify(state) + ")")
            tests.summary(); Qt.quit() } }
    }

    // The workflow lab (scripts/workflow-lab.sh): one case of its matrix,
    // read from --lab-case=<file.json> ({ name, tests, host, quitWhen,
    // stopWhen, resumes, timeout }). It starts the run (on this computer or
    // box), watches it, resumes it when it pauses (up to `resumes` times),
    // and writes what happened to lab-<phase>.json in the test's folder; a
    // case with a crash point or a quit is opened again (…-reopened) and
    // watched to its end.
    property var labState: ({})
    Timer {
        running: win.selftestDir !== "" && win.argValue("--scenario=").indexOf("clicksharloworkflowlab") === 0
        interval: 250
        repeat: true
        onTriggered: tests.labTick()
    }
    function labEnd(outcome) {
        const s = tests.labState, r = tests.workflowRun(), reopened = win.argValue("--scenario=").endsWith("-reopened")
        s.step = "end"
        const result = { name: s.case.name, phase: reopened ? "reopened" : "first", outcome: outcome, state: r ? r.state : "", attempt: r ? r.attempt : 0,
                         why: r ? r.why : "", liveSeen: s.liveSeen, seconds: Math.round((Date.now() - s.t0) / 100) / 10, resumes: s.resumes,
                         events: s.events, log: r ? r.log.map(e => "#" + e.attempt + " " + e.text) : [], lastCheck: r ? r.lastCheck : null }
        testFile.path = win.selftestDir + "/lab-" + result.phase + ".json"
        testFile.save(JSON.stringify(result, null, 1))
        tests.log("lab " + result.phase + ": " + outcome + " (" + result.state + ", attempt " + result.attempt + ", " + result.seconds + " s)")
        tests.check(true, "the lab case ran (" + outcome + ")")
        // The run's implementation session in sight (opened again, harlo
        // shows no session at first).
        if (r && r.implementation && Workspace.activeKey === "")
            Workspace.open(r.implementation)
        labShot.phase = result.phase
        labShot.start()
    }
    Timer {
        id: labShot
        property string phase
        interval: 800
        onTriggered: { tests.shotWindow("lab-" + phase); tests.summary(); Qt.quit() }
    }
    function labTick() {
        const s = tests.labState
        if (s.step === "end") return
        const reopened = win.argValue("--scenario=").endsWith("-reopened"), cwd = win.argValue("--cwd=")
        if (!s.step) {
            s.step = "setup"; s.t0 = Date.now(); s.resumes = 0; s.events = []; s.liveSeen = false; s.seen = ""
            s.case = JSON.parse(TestInput.readFile(win.argValue("--lab-case=")) || "{}")
            return
        }
        const c = s.case, at = () => ((Date.now() - s.t0) / 1000).toFixed(1) + " s"
        if (Date.now() - s.t0 > (c.timeout || 150) * 1000) return tests.labEnd("timed out at " + s.step)
        if (s.step === "setup") {
            if (!reopened && !tests.useFakeClaude()) return
            if (c.host) {
                if (!reopened) Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh="))
                Remotes.connectHost("box"); s.lastConnect = Date.now(); s.step = "connecting"
            } else {
                if (!reopened) Workspace.addProject(cwd)
                s.step = reopened ? "watching" : "start"; s.startAt = Date.now() + 600
            }
            return
        }
        if (s.step === "connecting") {
            const h = tests.workflowRemoteHost()
            if (h && h.state === "connected") {
                s.events.push(at() + " box connected")
                if (!reopened) Workspace.addProject("ssh://box" + cwd)
                s.step = reopened ? "watching" : "start"; s.startAt = Date.now() + 800
            } else if (Date.now() - s.lastConnect > 3000) {
                Remotes.connectHost("box"); s.lastConnect = Date.now()
            }
            return
        }
        if (s.step === "start") {
            if (Date.now() < s.startAt) return
            const p = JSON.parse(Workspace.projectsJson())[0]
            const why = p ? Workspace.startWorkflow(p.id, JSON.stringify({ task: "Write greet.txt greeting the user", criteria: "- greet.txt says hello!", testCommand: c.tests || tests.workflowTests, implementer: "harlo", reviewer: "claude", approval: "write" })) : "no project"
            if (why !== "") return tests.labEnd("not started: " + why)
            s.events.push(at() + " started"); s.step = "watching"
            return
        }
        const r = tests.workflowRun()
        if (!r) return
        if (r.liveCheck && r.liveCheck.output) s.liveSeen = true
        const shape = r.state + " · attempt " + r.attempt + " · " + r.stages.map(x => x.label + " " + x.status).join(", ")
        if (shape !== s.seen) { s.seen = shape; s.events.push(at() + " " + shape) }
        const reviewing = r.stages.some(x => x.kind === "review" && x.status === "active") && r.session !== ""
        if (!reopened && c.quitWhen === "review" && reviewing) {
            if (!s.quitAt) s.quitAt = Date.now() + 2000
            if (Date.now() > s.quitAt) return tests.labEnd("quit during the review")
            return
        }
        if (c.stopWhen === "review" && reviewing && !s.stopped) {
            if (!s.stopAt) s.stopAt = Date.now() + 3000
            if (Date.now() > s.stopAt) { s.stopped = true; s.events.push(at() + " Stop (the reviewer hangs)"); Workspace.workflowAction(r.id, "stop") }
            return
        }
        if (r.state === "done" || r.state === "stopped") return tests.labEnd(r.state)
        if (r.state === "paused") {
            if (!s.pausedAt) { s.pausedAt = Date.now(); s.events.push(at() + " paused: " + r.why); return }
            // (Waiting for box: it goes on by itself once box answers.)
            if (r.why.indexOf("couldn't list the sessions") >= 0) {
                if (Date.now() - (s.lastConnect || 0) > 3000) { Remotes.connectHost("box"); s.lastConnect = Date.now() }
                return
            }
            if (Date.now() - s.pausedAt < 1200) return
            if (s.resumes >= (c.resumes === undefined ? 3 : c.resumes)) return tests.labEnd("paused")
            s.resumes++; s.pausedAt = 0; s.events.push(at() + " Resume")
            Workspace.workflowAction(r.id, "resume")
            return
        }
        s.pausedAt = 0
    }

    // Stopped dead just after the reviewer's dispatch was saved, before it
    // started: reopened, harlo can't tell whether it began, so the run
    // pauses and says why; Resume starts the review.
    SequentialAnimation {
        id: workflowCrash
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowcrash"
        PauseAnimation { duration: 900 }
        ScriptAction { script: if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd=")) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.startWorkflowDirect() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(workflowCrash, () => false, 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: { tests.check(false, "harlo stopped at the crash point"); tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: workflowCrashAgain
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowcrash-reopened"
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(workflowCrashAgain, () => !!tests.workflowRun() && tests.workflowRun().state === "paused", 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.state === "paused" && r.why.indexOf("can't tell whether it began") >= 0 && r.why.indexOf("Resume starts the review again") >= 0, "the run pauses and says why (" + r.why + ")")
            tests.check(r.stages[2].status === "paused" && r.attempt === 2, "at attempt 2's review")
            tests.check(JSON.parse(Workspace.lastNotification || "{}").title === "Workflow paused", "you're told")
            Workspace.open(r.implementation) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.shown(card) && tests.shown(tests.findName("workflowWhy", card)) && tests.findName("workflowPause", card).text === "Resume", "its card says why, with Resume")
            tests.shotWindow("clicks-workflow-crash-paused")
            tests.clickItem(tests.findName("workflowPause", card), "Resume") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(workflowCrashAgain, () => tests.workflowRun().state === "done", 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.state === "done" && tests.workflowLogHas("Resumed."), "resumed, a reviewer starts and the run finishes (" + r.state + ")")
            tests.summary(); Qt.quit() } }
    }

    // Stopped dead once the reviewer had started but before harlo kept its
    // session: reopened, its session is found by its first message and the
    // run reconnects to it.
    SequentialAnimation {
        id: workflowFound
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowfound"
        PauseAnimation { duration: 900 }
        ScriptAction { script: if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd=")) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.startWorkflowDirect() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(workflowFound, () => false, 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: { tests.check(false, "harlo stopped at the crash point"); tests.summary(); Qt.quit() } }
    }
    SequentialAnimation {
        id: workflowFoundAgain
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowfound-reopened"
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(workflowFoundAgain, () => tests.workflowLogHas("reconnected to its session"), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: tests.check(tests.workflowLogHas("harlo was opened again during the review of attempt 2: reconnected to its session (found by its first message)"), "reopened, the reviewer's session is found by its first message") }
        ScriptAction { script: tests.waitUntil(workflowFoundAgain, () => tests.workflowRun().state === "done", 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.state === "done", "and the run finishes (" + r.state + ")")
            tests.summary(); Qt.quit() } }
    }

    // Runs go on without the window: Quit asks (keep running / quit), the
    // window closed keeps harlo going (it says so once), and it quits by
    // itself once the run is over. Then (the -reopened phase) Quit mid-run,
    // and a start at login (--background) picks the run up, windowless.
    SequentialAnimation {
        id: workflowBackgroundSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowbackground"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(Workspace.workflowsUnderWay() === 0, "nothing under way yet")
            tests.check(tests.startWorkflowDirect() === "", "a run starts") } }
        ScriptAction { script: tests.waitUntil(workflowBackgroundSteps, () => !!tests.workflowRun() && tests.workflowRun().implementation !== "", 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            tests.check(Workspace.workflowsUnderWay() === 1, "one run under way")
            win.requestQuit() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findText("A workflow run is under way") !== null && win.visible, "Quit asks first while a run is under way")
            tests.shotWindow("clicks-workflow-quit-ask")
            tests.clickItem(tests.findName("quitRunsKeep"), "Keep it running") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const note = JSON.parse(Workspace.lastNotification || "{}")
            tests.check(!win.visible && win.keptForRuns, "kept running: the window goes, harlo stays")
            tests.check(note.title === "Harlo keeps running" && (note.body || "").indexOf("A workflow run is under way") === 0, "and says so (" + note.title + ": " + note.body + ")")
            win.showMainWindow() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(win.visible && !win.keptForRuns, "shown again (as a second launch does)")
            win.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.check(!win.visible && win.keptForRuns, "closing the window while a run is under way keeps harlo going") }
        ScriptAction { script: tests.waitUntil(workflowBackgroundSteps, () => !!tests.workflowRun() && tests.workflowRun().state === "done", 90000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(win.runsOverQuit.running, "the run over, harlo is about to quit by itself")
            win.runsOverQuit.stop()
            win.keptForRuns = false
            // Another run, left under way by Quit (it picks up next time).
            win.showMainWindow()
            tests.check(tests.startWorkflowDirect() === "", "a second run starts") } }
        ScriptAction { script: tests.waitUntil(workflowBackgroundSteps, () => { const r = tests.workflowRun(); return !!r && r.state === "running" && r.attempt === 2 }, 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.requestQuit()
            tests.summary() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("quitRunsQuit"), "Quit and pause") }
    }
    SequentialAnimation {
        id: workflowBackgroundAgain
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowbackground-reopened"
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.check(!win.visible, "started at login with a run under way: no window") }
        PauseAnimation { duration: 3400 }
        ScriptAction { script: tests.check(!win.visible && win.keptForRuns, "and none comes: harlo goes on with the run") }
        ScriptAction { script: tests.waitUntil(workflowBackgroundAgain, () => { const r = tests.workflowRun(); return !!r && r.state === "done" }, 90000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.log.some(e => e.text.indexOf("harlo was opened again during") === 0), "the run picked up where it was")
            tests.check(r.state === "done" && win.runsOverQuit.running, "it finished, and harlo quits by itself")
            win.runsOverQuit.stop()
            tests.summary(); Qt.quit() } }
    }

    // Settings → Workflows: a workflow of your own (two checks before the
    // review), made in the editor, then run from the project menu with the
    // stand-ins: each fix goes through both checks before the review.
    SequentialAnimation {
        id: workflowCustomSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflowcustom"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (tests.useFakeClaude()) Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("workflows") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            tests.check(tests.findText("Built in", settingsDialog.contentItem) !== null && tests.findText("Implement and verify", settingsDialog.contentItem) !== null, "Settings → Workflows lists the built-in one")
            tests.check(!tests.shown(tests.findName("workflowEdit", settingsDialog.contentItem)) && tests.shown(tests.findName("workflowDuplicate", settingsDialog.contentItem)), "the built-in one can be duplicated, not edited")
            tests.shotPopup(settingsDialog, "clicks-workflows-settings")
            tests.clickItem(tests.findName("newWorkflow", settingsDialog.contentItem), "New workflow") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const ed = tests.findName("workflowsPage", settingsDialog.contentItem).editor
            tests.check(ed && ed.visible && ed.stageModel.count === 3, "the editor opens with an implementation, the tests and a review (" + (ed ? ed.stageModel.count : -1) + ")")
            tests.clickItem(tests.findName("addCheck"), "Add check") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = tests.findName("workflowsPage", settingsDialog.contentItem).editor
            const row = tests.findName("workflowStage3")
            tests.check(ed.stageModel.count === 4 && !!row, "Add check adds a stage at the end")
            const name = tests.findName("stageName", row), command = tests.findName("stageCommand", row)
            name.text = "Short"; name.textEdited()
            command.text = "echo counting words; sleep 2; test $(wc -c < greet.txt) -lt 100"; command.textEdited()
            tests.clickItem(tests.findName("stageUp", row), "move it up, before the review") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = tests.findName("workflowsPage", settingsDialog.contentItem).editor
            tests.check(ed.stageModel.get(2).name === "Short" && ed.stageModel.get(3).name === "Review", "it moves up, before the review")
            tests.clickItem(tests.findName("workflowSave"), "Save without a name") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = tests.findName("workflowsPage", settingsDialog.contentItem).editor
            const err = tests.findName("workflowEditorError")
            tests.check(ed.visible && tests.shown(err) && err.text === "Give the workflow a name.", "a workflow without a name isn't saved, and the editor says why (" + err.text + ")")
            ed.nameField.text = "Careful"
            tests.check(!tests.shown(tests.findName("workflowEditorError")), "the error goes once there's a name")
            // The implementation gets its own agent and model.
            const page = tests.findName("workflowsPage", settingsDialog.contentItem)
            const agentBox = tests.findName("stageAgent", tests.findName("workflowStage0"))
            const i = page.providers.findIndex(p => p.id === "harlo") + 1
            agentBox.currentIndex = i
            agentBox.activated(i) } }
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => tests.findName("workflowsPage", settingsDialog.contentItem).modelsOf("harlo").length > 0, 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const ed = tests.findName("workflowsPage", settingsDialog.contentItem).editor
            const modelBox = tests.findName("stageModel", tests.findName("workflowStage0"))
            const i = modelBox ? modelBox.models.findIndex(m => m.name === "Fake Helper") + 1 : 0
            tests.check(i > 0, "the implementation's model menu lists its agent's models")
            modelBox.currentIndex = i
            modelBox.activated(i)
            tests.check(ed.stageModel.get(0).harness === "harlo" && ed.stageModel.get(0).agentModel.indexOf("fake-helper") >= 0, "the implementation names its agent and model (" + ed.stageModel.get(0).harness + ", " + ed.stageModel.get(0).agentModel + ")")
            {
                const l0 = tests.findName("stageAgentLabel", tests.findName("workflowStage0")), m0 = tests.findName("stageAgent", tests.findName("workflowStage0"))
                tests.check(tests.shown(l0) && l0.text === "Agent" && m0.mapToItem(null, 0, 0).x > l0.mapToItem(null, l0.width, 0).x, "the implementation's agent menu has a label (" + (l0 ? l0.text : "none") + ")")
            }
            tests.shotPopup(ed, "clicks-workflows-editor")
            tests.clickItem(tests.findName("workflowSave"), "Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const flows = JSON.parse(Workspace.workflowDefinitionsJson())
            const mine = flows.find(f => f.name === "Careful")
            tests.check(!tests.findName("workflowsPage", settingsDialog.contentItem).editor.visible && !!mine && mine.stages.map(s => s.name).join(",") === "Implement,Tests,Short,Review", "saved, in order (" + (mine ? mine.stages.map(s => s.name) : "none") + ")")
            tests.check(mine && mine.needsTestCommand && mine.needsReviewer && mine.stages[2].command === "echo counting words; sleep 2; test $(wc -c < greet.txt) -lt 100", "its tests and review are the ones picked at the start; its own check keeps its command")
            tests.check(mine && mine.stages[0].agent.harness === "harlo" && mine.stages[0].agent.model.indexOf("fake-helper") >= 0, "and its implementation keeps its agent and model")
            tests.check(tests.findText("Careful", settingsDialog.contentItem) !== null, "and listed")
            settingsDialog.close()
            // Started from a new session's message: Run as workflow.
            win.openDraft(JSON.parse(Workspace.projectsJson())[0].id)
            chat.draftView.harness = "harlo"
            chat.draftView.input.text = "Write greet.txt greeting the user" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const b = tests.draftWorkflow()
            tests.check(tests.shown(b) && b.enabled, "a new session with a task and a project offers Run as workflow")
            tests.check(win.width >= 900 ? b.objectName === "draftWorkflowButton" && b.text === "Workflow" : true, "labelled \"Workflow\" when there's room (" + b.objectName + ")")
            const hint = tests.findName("draftStartHint")
            tests.check(!hint || hint.text === "" || hint.contentWidth <= hint.width + 1, "the start hint fits beside the buttons, or isn't there (" + (hint ? hint.text + " " + hint.contentWidth + "/" + hint.width : "") + ")")
            tests.shotWindow("clicks-workflow-draft") } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: tests.clickItem(tests.draftWorkflow(), "Run as workflow") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            tests.check(d.visible && d.fromDraft && d.taskArea.text === "Write greet.txt greeting the user" && d.title === "Run a workflow in " + JSON.parse(Workspace.projectsJson())[0].name,
                        "it opens the start dialog with the message as the task, in the session's project (" + d.title + ", " + d.taskArea.text + ")")
            d.flowBox.currentIndex = d.flows.findIndex(f => f.name === "Careful")
            d.flowBox.activated(d.flowBox.currentIndex)
            d.criteriaArea.text = "- greet.txt says hello!"
            d.testField.text = tests.workflowTests } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            tests.check(tests.findText("Implement  →  Tests  →  Short  →  Review", d.contentItem) !== null, "picked, the dialog shows its stages")
            tests.check(tests.shown(d.testField) && tests.shown(d.reviewerBox), "and asks for the tests and the reviewer it needs")
            tests.check(d.implementerAgent === "harlo" && d.implementerModel.indexOf("fake-helper") >= 0, "and starts with the workflow's own implementer and model (" + d.implementerAgent + ", " + d.implementerModel + ")")
            tests.shotWindow("clicks-workflow-dialog-custom")
            tests.clickItem(tests.findName("workflowStart"), "Start") } }
        PauseAnimation { duration: 300 }
        // The Short check takes a moment: its output shows while it runs.
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => { const r = tests.workflowRun(); return !!r && !!r.liveCheck && r.liveCheck.output.indexOf("counting words") >= 0 && r.liveCheck.seconds >= 1 }, 60000) }
        PauseAnimation { duration: 150 }
        ScriptAction { script: {
            const card = tests.findName("workflowCard", chat)
            const out = tests.findName("workflowLiveOutput", card)
            tests.check(tests.shown(out) && out.text.indexOf("counting words") >= 0 && tests.findContaining("Short running", card) !== null, "a running check's output shows on the card as it comes")
            tests.check(chat.draftView.input.text === "" && !Workspace.tabs.some(t => t.startsWith("view:new\t")), "the new session's message went into the run: the draft is cleared and its tab closed")
            tests.shotWindow("clicks-workflow-live-check") } }
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => !!tests.workflowRun() && tests.workflowRun().state === "done", 120000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.state === "done" && r.attempt === 3 && r.workflow === "Careful", "the run of Careful is done on attempt 3 (" + r.state + ", " + r.attempt + ")")
            const at = (attempt, text) => r.log.findIndex(e => e.attempt === attempt && e.text.indexOf(text) === 0)
            const order = [at(1, "Tests failed"), at(2, "Tests passed"), at(2, "Short passed"), at(2, "Review asked for changes"), at(3, "Tests passed"), at(3, "Short passed"), at(3, "Review approved")]
            tests.check(order.every((i, n) => i >= 0 && (n === 0 || i > order[n - 1])), "each fix goes through both checks before the review (" + order + ")")
            tests.check(at(1, "Short") < 0, "a failed check stops the attempt: the next check doesn't run")
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.findText("Done: Tests passed · Short passed · Review approved", card) !== null, "the card says what it went through")
            // (Narrow, the title is the attempt; the name is in the details.)
            const named = c => c.narrow ? tests.findText("Attempt 3", c) !== null && c.run.workflow === "Careful" : tests.findText("Careful · attempt 3", c) !== null
            tests.workflowNamed = named
            tests.check(named(card), "and names its workflow")
            tests.check(JSON.stringify(r.stages.map(s => s.label)) === JSON.stringify(["Implement", "Tests", "Short", "Review"]), "its stages are the workflow's (" + r.stages.map(s => s.label) + ")")
            tests.check(!tests.shown(tests.findName("workflowLive", card)) && r.liveCheck === null, "ended, it's gone from the card")
            tests.shotWindow("clicks-workflow-custom-done")
            const mine = JSON.parse(Workspace.workflowDefinitionsJson()).find(f => f.name === "Careful")
            Workspace.deleteWorkflowDefinition(mine.id) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(!JSON.parse(Workspace.workflowDefinitionsJson()).some(f => f.name === "Careful"), "the workflow is deleted")
            tests.check(tests.workflowRun().stages.length === 4 && tests.workflowNamed(tests.findName("workflowCard", chat)), "its run keeps its own copy")
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.shown(tests.findName("workflowMerge", card)) && tests.shown(tests.findName("workflowPublish", card)) && tests.shown(tests.findName("workflowRemove", card)), "done, its card offers Merge, a pull request and Remove worktree")
            tests.check(tests.findName("workflowMerge", card).text === "Merge into " + JSON.parse(Workspace.worktreeInfo(chat.session.key)).base, "Merge names the branch it goes into (" + tests.findName("workflowMerge", card).text + ")")
            tests.check(tests.shown(tests.findName("undoLatest", chat)), "before it's merged, the last turn's changes can be undone")
            tests.clickItem(tests.findName("workflowPublish", card), "Push for a pull request") } }
        // (The test repository has no remote: the push fails, and says so.)
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => tests.workflowLogHas("Pushing failed"), 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(!tests.workflowRun().published && Workspace.noticeError, "a push that fails is logged and shown, and the run isn't marked published (" + Workspace.notice + ")")
            tests.clickItem(tests.findName("workflowMerge", tests.findName("workflowCard", chat)), "Merge") } }
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => tests.workflowRun().merged, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = tests.workflowRun(), card = tests.findName("workflowCard", chat)
            tests.check(TestInput.readFile(win.argValue("--cwd=") + "/greet.txt") === "hello!\n", "merged: the project's checkout has the approved change")
            tests.check(r.log.some(e => e.text.indexOf("Merged workflow/") === 0), "the run's log says it was merged")
            tests.check(!tests.shown(tests.findName("workflowMerge", card)) && tests.findText("Merged into " + r.base, card) !== null, "the card says it's merged, and offers no second merge")
            tests.check(!tests.shown(tests.findName("undoLatest", chat)), "merged, its session no longer offers to undo the last turn's changes")
            tests.shotWindow("clicks-workflow-merged")
            tests.clickItem(tests.findName("workflowRemove", card), "Remove worktree") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.check(tests.findText("Remove the run's worktree?") !== null, "Remove asks first")
            tests.clickText("Discard") } }
        ScriptAction { script: tests.waitUntil(workflowCustomSteps, () => tests.workflowRun().removed, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(r.log.some(e => e.text.indexOf("Removed the worktree and the merged branch") === 0), "its worktree and merged branch are removed (" + r.log[r.log.length - 1].text + ")")
            tests.check(r.worktree !== "" && TestInput.readFile(r.worktree + "/greet.txt") === "", "its folder is gone (" + r.worktree + ")")
            tests.check(r.removed && tests.findText("Merged, worktree removed") !== null || !tests.shown(tests.findName("workflowCard", chat)), "the card, where it still shows, says so")
            tests.summary(); Qt.quit() } }
    }

    // A new project from a workflow: Add a project → a new folder (a git
    // repository with nothing in it) → its new session's message run as a
    // workflow with no tests yet: an empty first commit is made for the
    // run's branch, Harlo writes greet.txt, the stand-in claude reviews.
    // Then a folder with files and no git: the dialog offers git init and
    // says the run would start without its files.
    property string workflowNewFolder: ""
    SequentialAnimation {
        id: workflowNewSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloworkflownew"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (tests.useFakeClaude()) sidebar.openAddProject() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(sidebar.addProjectDialog.visible, "Add a project opens")
            tests.clickItem(tests.findName("addProjectNewFolder"), "Or start a new folder…") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = sidebar.addProjectDialog
            tests.check(d.creating && d.newParent !== "" && tests.shown(tests.findName("addProjectNewName")), "it asks for the new folder's name, and where (" + d.newParent + ")")
            tests.check(!tests.findName("addProjectAdd").enabled, "Add waits for a name")
            d.newFolderName.text = "greeter"
            const agent = tests.findName("addProjectAgent-harlo")
            if (agent) tests.clickItem(agent, "Harlo")
            tests.workflowNewFolder = d.newParent + "/greeter" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const add = tests.findName("addProjectAdd")
            tests.check(add.enabled && add.text === "Create and add", "Add reads Create and add (" + add.text + ")")
            tests.shotPopup(sidebar.addProjectDialog, "clicks-new-project-folder")
            tests.clickItem(add, "Create and add") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(x => x.path === tests.workflowNewFolder)
            tests.check(!sidebar.addProjectDialog.visible && !!p, "the folder is made and added (" + tests.workflowNewFolder + ")")
            tests.check(TestInput.listDir(tests.workflowNewFolder) === ".git", "a git repository with nothing in it (" + TestInput.listDir(tests.workflowNewFolder) + ")")
            const repo = p ? JSON.parse(Workspace.projectRepoState(p.id)) : {}
            tests.check(repo.state === "no-commits" && !repo.files, "nothing committed yet, and no files (" + JSON.stringify(repo) + ")")
            tests.check(chat.draftView.visible && chat.draftView.project && chat.draftView.project.id === p.id, "its new session opens")
            chat.draftView.harness = "harlo"
            chat.draftView.input.text = "Write greet.txt greeting the user" } }
        PauseAnimation { duration: 400 }
        // (Ctrl+Shift+Enter in the message box: Run as workflow.)
        ScriptAction { script: { chat.draftView.input.forceActiveFocus(); TestInput.key(Qt.Key_Return, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            tests.check(d.visible && d.fromDraft && d.taskArea.text === "Write greet.txt greeting the user", "the message opens the start dialog as its task")
            tests.check(d.repoState === "no-commits" && tests.shown(tests.findName("workflowNoCommits")) && !tests.shown(tests.findName("workflowNoGit")), "it says an empty first commit will be made")
            tests.check(d.testField.text === "", "an empty folder has no test command to guess (" + d.testField.text + ")")
            tests.check(tests.findName("workflowPickStages").text.indexOf("Implement  →  Review") === 0, "so its stages leave the tests out (" + tests.findName("workflowPickStages").text + ")")
            tests.check(tests.findName("workflowStart").enabled, "and it can start without one")
            d.criteriaArea.text = "- greet.txt says hello!"
            tests.shotWindow("clicks-workflow-new-project") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findName("workflowStart"), "Start") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.check(!!r && !sidebar.workflowDialog.visible, "the run starts (" + (r ? r.branch : "none") + ")")
            tests.check(r && JSON.stringify(r.stages.map(s => s.label)) === JSON.stringify(["Implement", "Review"]), "with no tests, it goes straight from the implementation to the review (" + (r ? r.stages.map(s => s.label) : "") + ")")
            const p = JSON.parse(Workspace.projectsJson()).find(x => x.path === tests.workflowNewFolder)
            tests.check(JSON.parse(Workspace.projectRepoState(p.id)).state === "ready", "the folder has its first commit now")
            tests.check(chat.draftView.input.text === "", "the draft's message went into the run") } }
        ScriptAction { script: tests.waitUntil(workflowNewSteps, () => !!tests.workflowRun() && tests.workflowRun().state === "done", 60000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.log("workflow log:\n" + r.log.map(e => "#" + e.attempt + " " + e.text).join("\n"))
            tests.check(r.state === "done" && r.attempt === 2, "the review's changes made, it's done on attempt 2 (" + r.state + ", " + r.attempt + ")")
            tests.check(TestInput.readFile(r.worktree + "/greet.txt") === "hello!\n", "the worktree has greet.txt")
            tests.check(TestInput.listDir(tests.workflowNewFolder) === ".git", "the project's own folder is untouched until it's merged")
            if (!tests.shown(tests.findName("workflowCard", chat))) Workspace.open(r.implementation) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const card = tests.findName("workflowCard", chat)
            tests.check(tests.findText("Done: Review approved", card) !== null, "the card says it's done")
            tests.shotWindow("clicks-workflow-new-project-done")
            // A folder with a file, not a repository.
            Workspace.addProject(win.argValue("--cwd=").replace(/\/[^\/]+$/, "/plain"))
            const p = JSON.parse(Workspace.projectsJson()).find(x => x.name === "plain")
            tests.check(!!p, "a folder without git is added")
            if (p) sidebar.workflowDialog.show(p.id, p.name, p.harness)
            sidebar.workflowDialog.taskArea.text = "Tidy the notes" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            const note = tests.findName("workflowNoGit")
            tests.check(d.repoState === "no-git" && d.folderFiles && tests.shown(note), "the dialog says it isn't a git repository")
            tests.check(tests.findContaining("won't be in the run unless you commit", d.contentItem) !== null, "and that the run would start without its files")
            tests.check(d.criteriaArea.text === "", "the last project's criteria don't carry over")
            tests.check(!tests.findName("workflowStart").enabled, "Start waits until you say to make it one")
            tests.clickItem(tests.findName("workflowMakeRepo"), "Make it a git repository") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("workflowStart").enabled, "then Start can start")
            tests.check(TestInput.listDir(win.argValue("--cwd=").replace(/\/[^\/]+$/, "/plain")) === "notes.md", "nothing is done to the folder before Start")
            tests.shotWindow("clicks-workflow-no-git")
            sidebar.workflowDialog.close()
            tests.summary(); Qt.quit() } }
    }

    // Workflows run for real (scripts/workflow-check/real.sh; real agents,
    // it spends tokens): --case-file= says what (kind "new": Add a project's
    // new folder in --parent=, "existing": the repository at --cwd=), the
    // task typed in the project's new session, Run as workflow with the
    // case's criteria and test command, Harlo on --model= with "Run anything
    // without asking", Start. It watches the run (a status file every 10 s;
    // a session waiting on you for 90 s, a paused run, 25 min with nothing
    // new, or --limit= minutes end it), merges a done run from its card and
    // writes <selftest>/workflow-real.json: a verdict, the run, problems.
    property var realCase: ({})
    property var realProblems: []
    property int realLogSeen: 0
    property double realLogAt: 0
    property double realTick: 0
    property double realAsking: 0
    property int realShotAttempt: 0
    property double realUntil: 0
    /// "asks": why the card didn't say the run needs you ("" it did), or
    /// null before anything asked.
    property var realAsked: null
    function realProblem(text) { tests.log("problem:", text); tests.realProblems = tests.realProblems.concat([text]) }
    function realFinish(verdict) {
        const r = tests.workflowRun()
        TestInput.writeFile(win.selftestDir + "/workflow-real.json", JSON.stringify({ verdict: verdict, case: tests.realCase, problems: tests.realProblems, run: r }, null, 1))
        tests.log("verdict:", verdict)
        Qt.quit()
    }
    function realProject() { return JSON.parse(Workspace.projectsJson()).find(p => p.path === win.argValue("--cwd=")) }
    /// The run's end, or a reason it won't: true stops the wait.
    function realWatch() {
        const r = tests.workflowRun()
        if (!r) return Date.now() > tests.realUntil
        const now = Date.now()
        if (r.log.length !== tests.realLogSeen) { tests.realLogSeen = r.log.length; tests.realLogAt = now }
        if (r.state === "done" || r.state === "stopped" || r.state === "failed") return true
        if (r.state === "paused") { tests.realProblem("paused: " + (r.why || "no reason given")); return true }
        if (now - tests.realTick < 10000) return false
        tests.realTick = now
        TestInput.writeFile(win.selftestDir + "/workflow-status.json", JSON.stringify({ at: new Date().toISOString(), run: r, open: chat.session.key, status: chat.session.status, asking: chat.session.hasRequest }, null, 1))
        if (r.attempt !== tests.realShotAttempt) { tests.realShotAttempt = r.attempt; tests.shotWindow("4-attempt-" + r.attempt) }
        // (What it waits on is open, as you'd have it.)
        const waited = r.session || r.implementation
        if (waited && Workspace.activeKey !== waited) Workspace.open(waited)
        if (chat.session.hasRequest) {
            if (!tests.realAsking) tests.realAsking = now
            // ("asks": the case wants a session asking, its card saying so.)
            if (tests.realCase.expect === "asks" && now - tests.realAsking > 3000) {
                const card = tests.findName("workflowCard", chat), state = tests.findName("workflowState", card)
                tests.shotWindow("4-needs-you")
                tests.realProblems = []
                tests.realAsked = state && state.text.indexOf("needs you") >= 0 ? "" : "the card says " + (state ? state.text : "nothing")
                return true
            }
            if (now - tests.realAsking > 90000) {
                tests.shotWindow("x-waiting-on-you")
                tests.realProblem("a session waits on you: " + chat.session.title + ": " + tests.chatText().slice(-600))
                return true
            }
        } else tests.realAsking = 0
        if (now - tests.realLogAt > 25 * 60000 && chat.session.status === "idle") {
            tests.shotWindow("x-stalled")
            tests.realProblem("nothing new for 25 min (" + r.state + ", waiting on " + (r.waiting || "?") + ")")
            return true
        }
        if (now > tests.realUntil) { tests.shotWindow("x-limit"); tests.realProblem("not done in " + win.argValue("--limit=") + " min"); return true }
        return false
    }
    SequentialAnimation {
        id: workflowRealSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "workflowreal"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            tests.realCase = JSON.parse(TestInput.readFile(win.argValue("--case-file=")))
            if (tests.realCase.kind === "new") sidebar.openAddProject()
            else Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            if (tests.realCase.kind === "new") {
                tests.clickItem(tests.findName("addProjectNewFolder"), "Or start a new folder…")
            } else {
                const p = tests.realProject()
                if (!p) { tests.realFinish("FAIL the project wasn't added"); return }
                win.openDraft(p.id)
            } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            if (tests.realCase.kind !== "new") return
            const d = sidebar.addProjectDialog
            d.newParent = win.argValue("--parent=")
            d.newFolderName.text = tests.realCase.folder
            const agent = tests.findName("addProjectAgent-harlo")
            if (agent) tests.clickItem(agent, "Harlo") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            if (tests.realCase.kind !== "new") return
            tests.shotPopup(sidebar.addProjectDialog, "1-add-project")
            tests.clickItem(tests.findName("addProjectAdd"), "Create and add") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const p = tests.realProject()
            if (!p) { tests.realFinish("FAIL no project at " + win.argValue("--cwd=") + (sidebar.addProjectDialog.problem ? ": " + sidebar.addProjectDialog.problem : "")); return }
            if (!chat.draftView.visible || !chat.draftView.project || chat.draftView.project.id !== p.id) win.openDraft(p.id)
            chat.draftView.harness = "harlo"
            chat.draftView.input.text = tests.realCase.task } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.shotWindow("2-new-session")
            tests.clickItem(tests.draftWorkflow(), "Run as workflow") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            if (!d.visible) { tests.realFinish("FAIL Run as workflow didn't open the start dialog"); return }
            d.criteriaArea.text = tests.realCase.criteria || ""
            d.testField.text = tests.realCase.tests || ""
            d.approvalBox.currentIndex = tests.realCase.approval === undefined ? 2 : tests.realCase.approval
            tests.realUntil = Date.now() + 60000 } }
        ScriptAction { script: tests.waitUntil(workflowRealSteps, () => sidebar.workflowDialog.implementerModels.some(m => m.id === win.argValue("--model=")) || Date.now() > tests.realUntil, 70000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            const i = d.implementerModels.findIndex(m => m.id === win.argValue("--model="))
            if (i < 0) { tests.realFinish("FAIL the dialog doesn't offer " + win.argValue("--model=") + " (" + d.implementerModels.map(m => m.id).join(", ") + ")"); return }
            d.implementerModelBox.currentIndex = i + 1
            d.implementerModelBox.activated(i + 1) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.shotWindow("3-start-dialog")
            tests.clickItem(tests.findName("workflowStart"), "Start") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const d = sidebar.workflowDialog
            if (d.visible) { tests.shotWindow("x-not-started"); tests.realFinish("FAIL it didn't start: " + d.problem.text); return }
            tests.realLogAt = Date.now()
            tests.realUntil = Date.now() + Number(win.argValue("--limit=") || 90) * 60000 } }
        ScriptAction { script: tests.waitUntil(workflowRealSteps, () => tests.realWatch(), 24 * 3600 * 1000) }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            if (tests.realCase.expect === "asks") {
                tests.realFinish(tests.realAsked === "" ? "pass: a session asked, the card said the run needs you (" + tests.findName("workflowState", tests.findName("workflowCard", chat)).text + ")"
                                 : tests.realAsked === null ? "FAIL nothing asked" : "FAIL " + tests.realAsked)
                return
            }
            if (r && r.implementation) Workspace.open(r.implementation) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.shotWindow("5-end")
            if (!r || r.state !== "done") { tests.realFinish("FAIL the run ended " + (r ? r.state : "unstarted") + " on attempt " + (r ? r.attempt : 0)); return }
            const merge = tests.findName("workflowMerge", chat)
            if (!tests.shown(merge)) { tests.realFinish("FAIL done, but its card offers no Merge"); return }
            tests.clickItem(merge, "Merge") } }
        ScriptAction { script: tests.waitUntil(workflowRealSteps, () => tests.workflowRun().merged || tests.workflowRun().log.some(e => /^Merg\S* failed: /.test(e.text)), 60000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const r = tests.workflowRun()
            tests.shotWindow("6-merged")
            tests.realFinish(r.merged ? "pass: done on attempt " + r.attempt + ", merged into " + r.base : "FAIL done on attempt " + r.attempt + ", but not merged: " + r.log[r.log.length - 1].text) } }
    }

    // A plan run reviews each step: Settings → Teamwork has it on; a
    // step whose review finds problems goes back to its helper, which fixes
    // it, and a second review lets the run go on; problems that stay pause
    // the run (scripts/fake-harlo-model.py plays helper and reviewer).
    SequentialAnimation {
        id: planReviewSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloreview"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("orchestration") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const sw = tests.findName("orchReviewSteps", settingsDialog.contentItem)
            tests.check(sw !== null && sw.checked && tests.findText("Review each step", settingsDialog.contentItem) !== null, "Settings → Teamwork: Review each step, on by default")
            tests.shotWindow("clicks-review-setting")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("make a plan please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => (JSON.parse(Workspace.planJson(tests.harloKey)).steps || []).length === 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.check(chat.planDock.expanded, "a plan made while you watch opens") }
        // The step to another model: chosen on the plan card.
        // (Its buttons show on hover.)
        ScriptAction { script: { const t = tests.findText("Write helper.txt", chat.planDock), p = t.mapToItem(null, t.width / 2, t.height / 2); TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("planStepHelper-1"), "Who does it") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            tests.check(tests.findText("Who does this step") !== null, "the step's helper is chosen in a popover")
            const box = tests.findName("delegateModelHelper")
            const i = box ? box.model.indexOf("Fake Helper") : -1
            tests.check(i > 0, "its models are offered (" + (box ? box.model : "none") + ")")
            if (i > 0) { box.currentIndex = i; box.activated(i) } } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findName("delegateStart"), "Choose") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const step = JSON.parse(Workspace.planJson(tests.harloKey)).steps[0]
            tests.check(step.agent === "harlo" && step.model === "fake/fake-helper", "the step keeps who does it (" + step.agent + ", " + step.model + ")")
            const who = tests.findName("planStepWho-1")
            tests.check(who !== null && who.visible && who.text === "Done by Harlo · Fake Helper", "the card says so (" + (who ? who.text : "none") + ")")
            tests.shotWindow("clicks-review-step-helper") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findName("planRun"), "Run with helpers") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: tests.clickItem(tests.findName("planRunTeam"), "Helpers: as this session") }
        // Its helper writes helper.txt and reports; a review starts.
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => { const r = JSON.parse(Workspace.planJson(tests.harloKey)).run; return !!r && Object.keys(r.reviewing || {}).length > 0 }, 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(tests.findContaining("being reviewed") !== null && tests.findName("planOpenReview-1") !== null, "the step shows it's being reviewed, with Open review")
            // The sidebar's rows fit (a helper's too), at any width.
            const titles = tests.findAllNames("sessionTitle", sidebar).filter(t => t.visible)
            const fits = t => Array.from(t.parent.children).filter(c => c.visible).every(c => c.x + c.width <= t.parent.width + 1)
            tests.check(titles.length >= 3 && titles.every(t => t.width >= 24 && fits(t)), "the sidebar's rows fit, each title with room (" + titles.map(t => Math.round(t.width)).join(", ") + ")")
            const planWhat = tests.findAllNames("toolWhat", chat.transcript).find(l => l.plain === "Write helper.txt")
            tests.check(planWhat && planWhat.font.family !== Theme.mono, "a plan's step reads as prose, not code")
            tests.check(tests.findText("Plan", chat.transcript) !== null && tests.findText("Plan set", chat.transcript) === null, "the plan tool is named 'Plan'")
            tests.shotWindow("clicks-review-running") } }
        // The review finds a problem; the helper fixes it; a second review
        // passes and the plan finishes.
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => { const p = JSON.parse(Workspace.planJson(tests.harloKey)); return !p.run && (p.steps || []).every(s => s.status === "done") }, 60000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            const p = JSON.parse(Workspace.planJson(tests.harloKey))
            const step = (p.steps || [])[0] || {}
            tests.check(step.status === "done" && step.summary.indexOf("Fixed") >= 0 && step.summary.indexOf("Reviewed: Looks right") >= 0, "the step is done, with its review (" + step.summary + ")")
            tests.check(TestInput.readFile(cwd + "/helper.txt") === "from the helper\nsecond line\n", "the helper fixed what the review found")
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            const reviews = kids.filter(k => k.name.indexOf("Review: Step 1") === 0)
            tests.check(reviews.length === 2 && reviews.every(k => k.status === "completed"), "two reviews, both completed (" + JSON.stringify(kids.map(k => [k.name, k.status])) + ")")
            const reqs = tests.modelRequests()
            const first = r => { const m = (r.messages || []).find(m => m.role === "user"); return m ? JSON.stringify(m.content) : "" }
            const reviewReqs = reqs.filter(r => first(r).indexOf("You review one step") >= 0)
            tests.check(reviewReqs.length > 0 && reviewReqs.every(r => first(r).indexOf("git diff ") >= 0), "the reviewer is told where the step's changes are")
            const fixes = reqs.filter(r => (r.messages || []).some(m => m.role === "user" && JSON.stringify(m.content).indexOf("A review of your step found problems") >= 0))
            tests.check(fixes.length > 0 && fixes.some(r => JSON.stringify(r.messages).indexOf("helper.txt:1 needs a second line") >= 0), "the problems went back to the step's helper")
            // Each helper's model and what it used; the run's total.
            // (The step's helper on the model chosen for it; its reviews on the run's.)
            const stepKid = kids.find(k => k.step === 1)
            tests.check(stepKid && stepKid.model === "fake/fake-helper" && reviews.every(k => k.model === "fake/fake-coder"), "the step's helper ran on the model chosen for it, its reviews on the run's (" + JSON.stringify(kids.map(k => [k.name, k.model])) + ")")
            tests.check(kids.length === 3 && kids.every(k => k.used && k.used.tokens > 0), "each helper shows what it used (" + JSON.stringify(kids.map(k => k.used)) + ")")
            const total = kids.reduce((t, k) => t + k.used.tokens, 0)
            tests.check(tests.findContaining("Helpers · all done · " + (total / 1000).toFixed(1) + "k tokens") !== null, "and the Subagents box")
            tests.shotWindow("clicks-review-done")
            // A finished plan folds to "All done" (a reopened session too).
            const done = tests.findText("All done", chat.planDock)
            tests.check(!chat.planDock.expanded && done !== null && !done.truncated, "the finished plan folds to All done, whole (" + (done ? done.width + " of " + done.implicitWidth + " px" : "none") + ")")
            tests.clickItem(tests.findName("planToggle", chat.planDock), "the plan's fold") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(chat.planDock.expanded && tests.findText("Write helper.txt", chat.planDock) !== null, "opened by hand, it shows its steps")
            chat.planDock.settleFold()
            tests.check(chat.planDock.expanded, "and stays open for that session")
            tests.clickItem(tests.findName("planToggle", chat.planDock), "fold it again") } }
        // Opening a session with a plan to do: folded, the next step in its
        // header; opened, its finished steps fold to a line.
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            chat.planDock.edit({ op: "add", title: "Write notes.txt" })
            chat.planDock.foldedByHand = ({})
            chat.planDock.sessionOpened() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const folded = tests.findName("planFoldedStep", chat.planDock)
            tests.check(!chat.planDock.expanded && tests.shown(folded) && tests.findText("Write notes.txt", folded) !== null && tests.findText("Next", folded) !== null, "a session opens with its plan folded, the next step in the header")
            tests.check(chat.planDock.height < 60, "folded, the card is one line (" + Math.round(chat.planDock.height) + " px)")
            {
                const t = tests.findText("Write notes.txt", folded), toggle = tests.findName("planToggle", chat.planDock)
                const gap = toggle.mapToItem(null, 0, 0).x - t.mapToItem(null, t.width, 0).x
                tests.check(gap >= 0 && gap <= 16, "folded, the next step sits at the right, by the toggle (" + gap.toFixed(1) + " px from it)")
                // (With room for it: a narrow window elides it.)
                tests.check(win.width < 900 || !t.truncated, "a short step's title shows whole (" + t.width + " of " + t.implicitWidth + " px)")
            }
            tests.clickItem(tests.findName("planToggle", chat.planDock), "open the plan") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fin = tests.findName("planFinishedToggle", chat.planDock)
            tests.check(tests.shown(fin) && tests.findText("1 finished step", fin) !== null && tests.findText("Write helper.txt", chat.planDock) === null && tests.findContaining("Write notes.txt", chat.planDock) !== null,
                "opened, the finished step folds to a line and the step to do shows")
            tests.check(chat.planDock.maxStepsHeight <= Math.max(120, chat.height * 0.2) + 1, "the steps scroll past a fifth of the chat's height (" + Math.round(chat.planDock.maxStepsHeight) + " px)")
            tests.clickItem(fin, "show the finished steps") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findText("Write helper.txt", chat.planDock) !== null, "a click shows the finished steps")
            tests.shotWindow("clicks-review-plan-open")
            const p = JSON.parse(Workspace.planJson(tests.harloKey))
            chat.planDock.edit({ op: "remove", id: p.steps[p.steps.length - 1].id })
            tests.clickItem(tests.findName("planToggle", chat.planDock), "fold it again") } }
        // A long plan opens at once (like a project's: 60 finished steps
        // with long reports, then 30 to do): the folded finished steps aren't
        // made, a step's buttons are made when it's hovered, and its helper,
        // commit and review rows only when it has them.
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const report = "What was done and how it was checked. ".repeat(16)
            for (let i = 1; i <= 60; i++) chat.planDock.edit({ op: "add", title: "Finished step " + i, detail: report })
            for (const st of JSON.parse(Workspace.planJson(tests.harloKey)).steps.filter(x => x.title.startsWith("Finished step")))
                chat.planDock.edit({ op: "skip", id: st.id, skipped: true })
            for (let i = 1; i <= 30; i++) chat.planDock.edit({ op: "add", title: "Long plan step " + i, detail: report })
            TestInput.moveTo(2, 2) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t0 = Date.now()
            tests.clickItem(tests.findName("planToggle", chat.planDock), "open a long plan")
            const ms = Date.now() - t0
            tests.log("plan of", chat.planDock.steps.length, "steps opened in", ms, "ms")
            tests.check(chat.planDock.steps.length === 91 && ms < 120, "a 91-step plan opens at once (" + ms + " ms)")
            tests.check(tests.findAllNames("planStepRow", chat.planDock).length === 30, "only the steps to do are made, not the 61 folded finished ones (" + tests.findAllNames("planStepRow", chat.planDock).length + ")")
            tests.check(tests.findAllNames("planStepEdits", chat.planDock).length === 0, "no step's buttons are made before it's hovered") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const t = tests.findContaining("Long plan step 1", chat.planDock), p = t.mapToItem(null, t.width / 2, t.height / 2); TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findAllNames("planStepEdits", chat.planDock).length === 1, "hovering a step shows its buttons")
            for (const st of JSON.parse(Workspace.planJson(tests.harloKey)).steps.filter(x => x.title.startsWith("Long plan step") || x.title.startsWith("Finished step")))
                chat.planDock.edit({ op: "remove", id: st.id })
            tests.clickItem(tests.findName("planToggle", chat.planDock), "fold the plan") } }
        // The project's Team: its helpers' agent, and its reviewer on
        // another model (the next run's reviews use it).
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.openProject(JSON.parse(Workspace.projectsJson())[0].id) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const box = tests.findName("teamAgent", settingsDialog.contentItem)
            tests.check(box !== null && box.model[box.currentIndex] === "Same as the session", "Team: the helpers are like the session at first (" + (box ? box.model : "none") + ")")
            tests.check(tests.findName("teamPreset", settingsDialog.contentItem) === null, "no ready-made teams (they repeated the rows below)")
            const i = box ? box.model.indexOf("Harlo") : -1
            tests.check(i > 0, "Harlo is one of them")
            if (i > 0) { box.currentIndex = i; box.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pid = JSON.parse(Workspace.projectsJson())[0].id
            tests.check(JSON.parse(Workspace.setting("team_helpers_" + pid)).agent === "harlo", "choosing it sets the team (" + Workspace.setting("team_helpers_" + pid) + ")")
            const rev = tests.findName("teamReviewer", settingsDialog.contentItem)
            const i = rev ? rev.model.indexOf("Harlo") : -1
            tests.check(rev !== null && rev.model[rev.currentIndex] === "The helpers' agent" && i > 0, "Reviews by: the helpers' agent, or another (" + (rev ? rev.model : "none") + ")")
            if (i > 0) { rev.currentIndex = i; rev.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = tests.findName("teamReviewerModel", settingsDialog.contentItem)
            const i = m ? m.model.indexOf("Fake Helper") : -1
            tests.check(i > 0, "the reviewer's model is offered (" + (m ? m.model : "none") + ")")
            if (i > 0) { m.currentIndex = i; m.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pid = JSON.parse(Workspace.projectsJson())[0].id
            const v = JSON.parse(Workspace.setting("team_helpers_" + pid))
            tests.check(v.reviewer && v.reviewer.agent === "harlo" && v.reviewer.model === "fake/fake-helper", "the team's reviewer is saved (" + JSON.stringify(v) + ")")
            tests.scrollIntoView(tests.findName("teamReviewerModel", settingsDialog.contentItem))
            tests.shotWindow("clicks-review-team")
            settingsDialog.close() } }
        // Problems that stay: the run pauses for you.
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("make a bad plan please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => (JSON.parse(Workspace.planJson(tests.harloKey)).steps || []).length === 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 600 }
        // This run from the phone, with the team's helpers; its helper asks
        // before writing (the session asks first: its helpers start so),
        // and the phone approves.
        ScriptAction { script: {
            const call = m => JSON.parse(Workspace.phoneTestCall(JSON.stringify(m)))
            const plan = call({ op: "plan", key: tests.harloKey })
            tests.check(plan.team && plan.team.agent === "harlo" && plan.team.team === true && plan.team.reviewer && plan.team.reviewer.model === "fake/fake-helper",
                      "the phone's plan says who the team's helpers are (" + JSON.stringify(plan.team) + ")")
            Workspace.setApprovalMode(tests.harloKey, "always-ask")
            const run = call({ op: "planRun", key: tests.harloKey, agent: "" })
            tests.check(run.run && run.run.agent === "harlo" && !run.error, "the phone runs it with the team's helpers (" + JSON.stringify(run.run || run) + ")") } }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "helpers", key: tests.harloKey }))).helpers.some(h => h.asks && h.asks.approval), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const call = m => JSON.parse(Workspace.phoneTestCall(JSON.stringify(m)))
            const h = call({ op: "helpers", key: tests.harloKey }).helpers.find(h => h.asks && h.asks.approval)
            tests.check(h.step === 1 && h.model === "fake/fake-coder" && h.asks.options.indexOf("Approve") >= 0, "the phone sees the helper's model and its request (" + JSON.stringify([h.model, h.asks]) + ")")
            tests.check(!!call({ op: "answerHelper", key: tests.harloKey, helper: "live:none", id: h.asks.id, value: "Approve" }).error, "a session that isn't its helper can't be answered")
            const after = call({ op: "answerHelper", key: tests.harloKey, helper: h.id, id: h.asks.id, value: "Approve" })
            tests.check(Array.isArray(after.helpers) && !after.helpers.some(x => x.id === h.id && x.asks && x.asks.id === h.asks.id), "the phone approves it (" + JSON.stringify((after.helpers || []).map(x => x.asks)) + ")")
            tests.approvedHelper = h.id
            Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const h = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "helpers", key: tests.harloKey }))).helpers.find(x => x.id === tests.approvedHelper)
            tests.check(h && !h.asks && h.status !== "waiting", "the helper went on (" + (h ? h.status : "gone") + ")") } }
        ScriptAction { script: tests.waitUntil(planReviewSteps, () => { const r = JSON.parse(Workspace.planJson(tests.harloKey)).run; return !!r && (r.paused || "") !== "" }, 60000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.planJson(tests.harloKey))
            const step = (p.steps || [])[0] || {}
            tests.check(p.run && p.run.paused.indexOf("its review still found problems after a round of fixes") >= 0 && p.run.paused.indexOf("helper.txt:1") >= 0, "problems after a round of fixes pause the run (" + (p.run ? p.run.paused : "no run") + ")")
            tests.check(step.status === "pending" && step.summary.indexOf("Not done: its review found problems") === 0, "and the step waits again (" + step.status + ": " + step.summary + ")")
            tests.check(tests.findText("· paused") !== null && tests.findText("Run again") !== null, "the plan card says paused, with Run again")
            tests.check(tests.findName("planPaused").text.indexOf("Paused at step 1.") === 0 && tests.findName("planPaused").text.indexOf("helper.txt:1") < 0, "the pause names the step and what to do; the problem is said once, at the step (" + tests.findName("planPaused").text + ")")
            const kids = JSON.parse(Workspace.orchestratedJson(tests.harloKey))
            const reviews = kids.filter(k => k.review !== null && k.review !== undefined)
            tests.check(reviews.length === 2 && reviews.every(k => k.model === "fake/fake-helper") && kids.filter(k => k.step === 1).every(k => k.model === "fake/fake-coder"),
                      "the team's reviewer reviews, on its model; the helper is the team's (" + JSON.stringify(kids.map(k => [k.name, k.model])) + ")")
            tests.shotWindow("clicks-review-paused")
            tests.summary(); Qt.quit() } }
    }

    // Settings → Agents → Harlo: the subagents' model and the model to go
    // on with, typed in; a new session's subagent runs on the first, and a
    // server that stays busy hands the turn to the second.
    // Settings → Models: Harlo's built-in providers, a
    // key refused then taken (saved for harlo-agent, checked against a
    // stand-in provider), its models in the picker, then removed; the
    // switch that gives SSH machines the keys; and a coding plan's key, and
    // a Claude session on the plan.
    SequentialAnimation {
        id: harloCloudSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlocloud"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("models") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => (JSON.parse(Accounts.cloud("harlo")).list || []).length > 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, st = id => { const l = tests.findName("cloudStatus-" + id, d); return l ? l.text : "(none)" }
            const ids = JSON.parse(Accounts.cloud("harlo")).list.map(p => p.id)
            tests.check(ids.length === 16 && ["anthropic", "gemini", "openrouter", "xai", "opencode-go"].every(i => ids.indexOf(i) >= 0), "Cloud models lists Harlo's providers (" + ids.join(", ") + ")")
            tests.check(st("anthropic") === "No key", "Anthropic has no key yet (" + st("anthropic") + ")")
            tests.check(tests.findName("cloudProvider-deepseek-anthropic", d) === null, "an API variant sharing another's key has no row of its own")
            tests.clickItem(tests.findName("cloudProvider-deepseek", d), "DeepSeek's row") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, v = tests.findName("cloudVariants-deepseek", d)
            tests.check(v !== null && v.text.indexOf("DeepSeek (Anthropic API)") > 0, "it's inside that one's row (" + (v ? v.text : "none") + ")")
            tests.clickItem(tests.findName("cloudProvider-deepseek", d), "DeepSeek's row")
            tests.check(tests.findName("cloudAdd", d) === null && tests.findName("cloudSearch", d) !== null, "with no key yet, the providers are listed straight away, with a search")
            tests.check(tests.findName("cloudProvider-gemini", d) !== null && tests.findName("cloudProvider-mistral", d) === null && tests.findName("cloudMore", d) !== null,
                        "the common ones first, the rest under More providers")
            tests.clickItem(tests.findName("cloudSearch", d), "the providers' search")
            TestInput.typeText("anthro") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, shown = id => { const r = tests.findName("cloudProvider-" + id, d); return r !== null && r.visible }
            tests.check(shown("anthropic") && !shown("gemini") && !shown("deepseek"), "searching narrows the list to Anthropic")
            tests.scrollIntoView(tests.findName("cloudProvider-anthropic", d))
            tests.clickItem(tests.findName("cloudProvider-anthropic", d), "Anthropic's row") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("Claude Opus 5.5", d) !== null && tests.findText("$4 / $20", d) === null && tests.findText("Prices ↗", d) !== null, "opened: its models, no prices, a link to its prices")
            tests.clickItem(tests.findName("cloudKey-anthropic", d), "the key field")
            TestInput.typeText("sk-ant-wrong")
            tests.clickItem(tests.findName("cloudSave-anthropic", d), "Save") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => { const r = (JSON.parse(Accounts.cloud("harlo")).results || {}).anthropic; return r && r.state !== "busy" }, 30000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const r = JSON.parse(Accounts.cloud("harlo")).results.anthropic
            tests.check(r.state === "error" && r.message.indexOf("refused the key") >= 0 && r.message.indexOf("invalid x-api-key") >= 0, "a wrong key: saved, and the provider's refusal said (" + r.message + ")")
            const d = settingsDialog.contentItem
            tests.check(tests.findName("cloudKey-anthropic", d).text === "", "the field empties once it's sent")
            tests.clickItem(tests.findName("cloudKey-anthropic", d), "the key field")
            TestInput.typeText("sk-ant-good")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => { const v = JSON.parse(Accounts.cloud("harlo")); const r = (v.results || {}).anthropic; return r && r.state === "ok" && v.state === "ready" }, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = JSON.parse(Accounts.cloud("harlo")).results.anthropic
            tests.check(r.message === "Saved; it works: Anthropic took the key", "the right key works (" + r.message + ")")
            const st = tests.findName("cloudStatus-anthropic", settingsDialog.contentItem)
            tests.check(st && st.text === "Key saved", "and it says so (" + (st ? st.text : "none") + ")")
            const add = tests.findName("cloudAdd", settingsDialog.contentItem), gemini = tests.findName("cloudProvider-gemini", settingsDialog.contentItem)
            tests.check(add !== null && gemini === null, "with a key, it's listed first and the rest wait behind Add a provider")
            const saved = TestInput.readFile(win.selftestDir + "/config/harlo-agent/keys.json")
            tests.check(saved.indexOf("sk-ant-good") >= 0 && saved.indexOf("sk-ant-wrong") < 0, "kept in harlo-agent's keys file, the last one only")
            const log = TestInput.readFile(win.selftestDir + "/cloud-log.jsonl")
            tests.check(log.indexOf("/anthropic/models") >= 0, "checked with the provider (the stand-in)")
            tests.scrollIntoView(tests.findName("cloudProvider-anthropic", settingsDialog.contentItem))
            tests.shotWindow("clicks-harlo-cloud")
            settingsDialog.close()
            tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => chat.session.modelName !== "", 15000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.clickItem(tests.findName("modelButton"), "the model button") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => chat.session.models.some(m => m.split("\t")[1] === "claude-opus-5-5"), 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const opus = chat.session.models.find(m => m.split("\t")[1] === "claude-opus-5-5") || ""
            tests.check(opus.split("\t")[0] === "anthropic", "the picker gets Anthropic's model once its key is saved (" + opus.replace(/\t/g, " | ") + ")")
            tests.check(tests.findText("$4 / $20") === null, "with no price beside it")
            TestInput.key(Qt.Key_Escape, 0)
            settingsDialog.openPage("models") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.scrollIntoView(tests.findName("cloudProvider-anthropic", d))
            const remove = tests.findName("cloudRemove-anthropic", d)
            if (!remove || !remove.visible) tests.clickItem(tests.findName("cloudProvider-anthropic", d), "Anthropic's row") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const share = tests.findName("harloShareKeys", settingsDialog.contentItem)
            tests.check(share && share.checked, "with a key saved, SSH machines get the keys by default")
            tests.scrollIntoView(share)
            tests.clickItem(share, "the SSH keys switch") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.setting("harlo_share_keys") === "0", "turned off")
            const remove = tests.findName("cloudRemove-anthropic", settingsDialog.contentItem)
            tests.scrollIntoView(remove)
            tests.clickItem(remove, "Remove") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => { const v = JSON.parse(Accounts.cloud("harlo")); const p = (v.list || []).find(p => p.id === "anthropic"); return v.state === "ready" && p && p.key === "none" }, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const st = tests.findName("cloudStatus-anthropic", settingsDialog.contentItem)
            tests.check(st && st.text === "No key", "removed (" + (st ? st.text : "none") + ")")
            tests.check(TestInput.readFile(win.selftestDir + "/config/harlo-agent/keys.json").indexOf("sk-ant-good") < 0, "and gone from the file")
            tests.check(tests.findName("harloShareKeys", settingsDialog.contentItem) === null, "with no key left, the SSH keys switch goes")
            // Coding plans for Claude Code: harlo's own list and keys,
            // checked by one short request of the stand-in claude.
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            const plans = JSON.parse(Accounts.cloud("claude")).list || []
            tests.check(plans.map(p => p.id).join(",") === "glm-coding,kimi-code,minimax-plan", "the coding plans are listed (" + plans.map(p => p.id) + ")")
            const glm = tests.findName("cloudProvider-glm-coding", settingsDialog.contentItem)
            tests.scrollIntoView(glm)
            tests.clickItem(tests.findName("cloudProvider-glm-coding", settingsDialog.contentItem), "the GLM Coding Plan's row") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findText("GLM 5.3 Flash", d) !== null && tests.findText("Plan ↗", d) !== null, "opened: its models and a link to its plans")
            const key = tests.findName("cloudKey-glm-coding", d)
            tests.clickItem(key, "its key field")
            TestInput.typeText("plan-bad")
            tests.clickItem(tests.findName("cloudSave-glm-coding", d), "Save") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => { const r = (JSON.parse(Accounts.cloud("claude")).results || {})["glm-coding"]; return r && r.state !== "busy" }, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const res = tests.findName("cloudResult-glm-coding", settingsDialog.contentItem)
            tests.check(res && res.text.indexOf("Saved, but the check failed") === 0 && res.text.indexOf("invalid api key") > 0, "a wrong key: saved, and the plan's refusal said (" + (res ? res.text : "none") + ")")
            tests.clickItem(tests.findName("cloudKey-glm-coding", settingsDialog.contentItem), "its key field")
            TestInput.typeText("plan-good")
            tests.clickItem(tests.findName("cloudSave-glm-coding", settingsDialog.contentItem), "Save") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => { const v = JSON.parse(Accounts.cloud("claude")); const r = (v.results || {})["glm-coding"]; return r && r.state === "ok" && v.state === "ready" }, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const res = tests.findName("cloudResult-glm-coding", d)
            tests.check(res && res.text === "Saved; it works: GLM Coding Plan answered through Claude Code.", "the right key works (" + (res ? res.text : "none") + ")")
            const st = tests.findName("cloudStatus-glm-coding", d)
            tests.check(st && st.text === "Key saved", "and it says so (" + (st ? st.text : "none") + ")")
            const file = win.selftestDir + "/config/harlo/coding-plans.json"
            tests.check(JSON.parse(TestInput.readFile(file))["glm-coding"] === "plan-good", "kept in harlo's coding-plans.json")
            const log = TestInput.readFile(win.argValue("--cwd=") + "/../home/plan-log.jsonl").trim().split("\n").map(l => JSON.parse(l))
            const last = log[log.length - 1] || {}
            tests.check(log.length === 2 && last.baseUrl === "https://api.z.ai/api/anthropic" && last.model === "glm-5.3-flash[1m]" && !last.persist,
                        "checked through Claude Code on the plan's endpoint, its small model, nothing kept (" + JSON.stringify(last) + ")")
            const notes = tests.findText("A MiniMax M Plan key (sk-cp-…, from your plan's page) works here too.")
            tests.check(notes === null || !notes.visible, "MiniMax's note shows only when its row is open")
            // A Claude session on the plan: picked in the composer, under
            // the plan's name; claude restarted with the plan's settings
            // file (the key in it, never on the command line); and back.
            settingsDialog.close()
            tests.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickItem(tests.findName("modelButton"), "the model button") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => chat.session.models.some(m => m.split("\t")[0] === "glm-coding"), 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findText("GLM CODING PLAN") !== null, "the plan's models are under its name in the picker")
            tests.clickItem(tests.findText("GLM 5.3"), "GLM 5.3") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => chat.session.modelName === "GLM 5.3", 15000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("hello plan") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => tests.chatText().indexOf("Hello from fake claude") >= 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const text = tests.chatText()
            tests.check(text.indexOf("model glm-5.3[1m]") >= 0 && text.indexOf("via https://api.z.ai/api/anthropic") >= 0, "claude runs on the plan's endpoint with its model (" + text.slice(-160) + ")")
            const starts = TestInput.readFile(win.argValue("--cwd=") + "/../home/claude-starts.jsonl").trim().split("\n").map(l => JSON.parse(l))
            const last = starts[starts.length - 1] || { args: [] }
            tests.check(last.args.indexOf("--settings") >= 0 && last.key === "plan-good" && !last.args.some(a => a.indexOf("plan-good") >= 0), "with the key in its settings file, not on the command line")
            // Its icon: the model's maker (Z.ai), Claude Code as a tag.
            const makers = tests.findAllNamed("sessionMaker", sidebar, []).map(m => m.provider)
            tests.check(makers.indexOf("zai-coding-plan") >= 0 && tests.findAllNamed("sessionProvider", sidebar, []).every(m => m.provider !== "claude"),
                        "the session's icon shows its model's maker, Claude Code as a tag (" + makers + ")")
            tests.shotWindow("clicks-harlo-plan")
            chat.session.setModel("anthropic", "claude-sonnet-5") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => chat.session.modelName !== "GLM 5.3", 15000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.sendByKeys("hello again") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: tests.waitUntil(harloCloudSteps, () => tests.chatText().split("Hello from fake claude").length === 3, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const text = tests.chatText(), reply = text.slice(text.lastIndexOf("Hello from fake claude"))
            tests.check(reply.indexOf("model claude-sonnet-5,") >= 0 && reply.indexOf("via ") < 0, "back on Anthropic's model, without the plan (" + reply.slice(0, 120) + ")")
            const starts = TestInput.readFile(win.argValue("--cwd=") + "/../home/claude-starts.jsonl").trim().split("\n").map(l => JSON.parse(l))
            const last = starts[starts.length - 1] || { args: [] }
            tests.check(last.args.indexOf("--settings") < 0 && last.args.some(a => a.startsWith("--resume")), "restarted without the plan, the same conversation (" + starts.length + " starts)")
            const marks = tests.findAllNamed("sessionProvider", sidebar, []).map(m => m.provider)
            tests.check(tests.findAllNamed("sessionMaker", sidebar, []).every(m => m.provider !== "zai-coding-plan") && marks.indexOf("claude") >= 0, "on Claude again: Claude's own mark (" + marks + ")")
            tests.summary(); Qt.quit() } }
    }

    SequentialAnimation {
        id: harloSettingsSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlosettings"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("lsp") } }
        PauseAnimation { duration: 600 }
        // Settings → Code intelligence: each language's server, found where it's
        // usually installed too (a stand-in gopls in the home's go/bin).
        ScriptAction { script: tests.waitUntil(harloSettingsSteps, () => Accounts.languageServers !== "" && JSON.parse(Accounts.languageServers).state === "ready", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const list = JSON.parse(Accounts.languageServers).list || []
            const go = list.find(l => l.language === "Go") || {}
            tests.check(list.length === 5 && list.every(l => l.state === "ready" || l.install !== ""), "Code intelligence lists five languages, with how to install the missing (" + list.map(l => l.language + ":" + l.state).join(", ") + ")")
            tests.check(go.state === "ready" && go.path.endsWith("/home/go/bin/gopls"), "a server outside the PATH is found where it's usually installed (" + go.state + " " + go.path + ")")
            const rows = tests.findName("harloLanguageServers", settingsDialog.contentItem)
            tests.check(rows && rows.visible && rows.children.length >= 6, "and shown under it (" + (rows ? rows.children.length : "none") + ")")
            tests.clickItem(tests.findName("harloLanguageServersCheck", settingsDialog.contentItem), "Check again") } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: tests.waitUntil(harloSettingsSteps, () => JSON.parse(Accounts.languageServers).state === "ready", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check((JSON.parse(Accounts.languageServers).list || []).length === 5, "Check again asks again")
            tests.scrollIntoView(tests.findName("harloLanguageServers", settingsDialog.contentItem))
            tests.shotWindow("clicks-harlo-language-servers")
            tests.check(settingsDialog.current.title === "Code intelligence", "on a page of its own, Code intelligence")
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(tests.findName("harloLanguageServers", settingsDialog.contentItem) === null, "and no longer under Providers")
            // Picked from Harlo's models and the budget's choices.
            const pick = (name, text) => {
                const box = tests.findName(name, settingsDialog.contentItem)
                const i = box ? box.model.findIndex(t => t.indexOf(text) >= 0) : -1
                tests.check(i > 0, name + " offers " + text + " (" + (box ? box.model.join(", ") : "none") + ")")
                if (i > 0) { box.currentIndex = i; box.activated(i) }
            }
            pick("harloTaskModel", "Fake Helper")
            pick("harloFallback", "Go on with Fake Helper")
            pick("harloHelperBudget", "500k tokens")
            pick("harloAdvisor", "Fake Helper")
            pick("harloSteering", "When it finishes")
            const memory = tests.findName("harlo_memory", settingsDialog.contentItem)
            tests.check(memory !== null && !memory.checked, "Notes between sessions is there, off")
            if (memory) { tests.scrollIntoView(memory); memory.toggle(); memory.toggled() } } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(Workspace.setting("harlo_task_model") === "fake/fake-helper" && Workspace.setting("harlo_fallback") === "fake/fake-helper" && Workspace.setting("harlo_helper_budget") === "500k",
                      "the three are saved as picked (" + [Workspace.setting("harlo_task_model"), Workspace.setting("harlo_fallback"), Workspace.setting("harlo_helper_budget")].join(", ") + ")")
            tests.check(Workspace.setting("harlo_advisor") === "fake/fake-helper" && Workspace.setting("harlo_memory") === "1", "the reviewer and the notes too (" + Workspace.setting("harlo_advisor") + ", " + Workspace.setting("harlo_memory") + ")")
            tests.check(Workspace.setting("harlo_steering") === "wait", "messages sent while it works wait for the turn's end (" + Workspace.setting("harlo_steering") + ")")
            tests.shotWindow("clicks-harlo-extras")
            tests.shotWindow("clicks-harlo-settings")
            settingsDialog.openPage("git") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = tests.findName("commitModel", settingsDialog.contentItem)
            const i = box ? box.model.indexOf("Fake Helper") : -1
            tests.check(box && box.model[box.currentIndex] === "Harlo's default" && i > 0, "Git: the commit messages' model is picked from Harlo's (" + (box ? box.model.join(", ") : "none") + ")")
            if (i > 0) { box.currentIndex = i; box.activated(i) }
            tests.check(Workspace.setting("commit_model") === "fake/fake-helper", "and saved as its id (" + Workspace.setting("commit_model") + ")")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSettingsSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("use a subagent please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSettingsSteps, () => tests.chatText().indexOf("Done:") >= 0, 15000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            // (A subagent's: its first message is the task.)
            const first = b => JSON.stringify(((b.messages || []).find(m => m.role === "user") || {}).content || "")
            const sub = tests.modelRequests().filter(b => first(b).indexOf("SUBTASK") >= 0)
            tests.check(sub.length > 0 && sub.every(b => b.model === "fake-helper"), "the subagent runs on the subagents' model (" + sub.map(b => b.model).join() + ")")
            // (The reviewer's requests: its own instructions, on the model picked.)
            const reviewing = b => JSON.stringify(((b.messages || [])[0] || {}).content || "").indexOf("You review a coding agent's work") >= 0
            const own = tests.modelRequests().filter(b => first(b).indexOf("use a subagent") >= 0 && !reviewing(b))
            tests.check(own.length > 0 && own.every(b => b.model === "fake-coder"), "the session stays on its own")
            const reviews = tests.modelRequests().filter(reviewing)
            tests.check(reviews.length === 1 && reviews[0].model === "fake-helper", "the reviewer reads the turn once, on the model picked (" + reviews.map(b => b.model).join() + ")")
            tests.check(own.every(b => (b.tools || []).some(t => (t.function || t).name === "memory")), "and the session has its notes tool")
            tests.sendByKeys("be busy now") } }
        PauseAnimation { duration: 300 }
        // (Retries take 1 + 2 + 4 s.)
        ScriptAction { script: tests.waitUntil(harloSettingsSteps, () => tests.chatText().indexOf("[fake-helper]") >= 0 && chat.session.status === "idle", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = tests.chatText()
            tests.check(t.indexOf("ok: be busy now [fake-helper]") >= 0, "a server that stays busy: it goes on with the model set (" + t.slice(-200) + ")")
            tests.check(chat.session.modelName === "Fake Helper", "and says so: the session's model is now that one (" + chat.session.modelName + ")")
            tests.shotWindow("clicks-harlo-fallback")
            tests.summary(); Qt.quit() } }
    }

    // MCP for Harlo: a server that wants a sign-in (click-tests.sh: the
    // stand-in server in Harlo's mcp.json, and a stand-in browser that
    // follows the link) signs in from its page; a project's own server is
    // not allowed for Harlo until it's saved here; a session uses both.
    SequentialAnimation {
        id: harloMcpSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlomcp"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => (JSON.parse(Mcp.statusJson("global")).locked || {}).state === "needs_auth", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pill = tests.findName("mcpStatus-locked", settingsDialog.contentItem)
            tests.check(pill !== null && pill.text === "Needs sign-in", "a server that wants a sign-in says so (" + (pill ? pill.text : "none") + ")")
            tests.clickItem(tests.findName("mcpServer-locked", settingsDialog.contentItem), "locked") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const b = tests.findName("mcpSignIn", settingsDialog.contentItem)
            tests.check(b !== null, "its page has Sign in (for Harlo)")
            tests.shotPopup(settingsDialog, "clicks-mcp-signin")
            tests.clickItem(b, "Sign in") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => signInDialog.flow !== null && signInDialog.flow.state !== "running", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(signInDialog.flow !== null && signInDialog.flow.state === "done", "the sign-in finishes in the browser (" + (signInDialog.flow ? signInDialog.flow.state + " " + signInDialog.flow.message : "none") + ")")
            tests.shotWindow("clicks-mcp-signed-in")
            signInDialog.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => (JSON.parse(Mcp.statusJson("global")).locked || {}).state === "connected", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const st = JSON.parse(Mcp.statusJson("global")).locked || {}
            tests.check(st.state === "connected" && (st.tools || []).length === 1, "then it's tested again: connected, its tool listed (" + JSON.stringify(st) + ")")
            tests.check(TestInput.readFile(win.selftestDir + "/config/harlo-agent/mcp-auth.json").indexOf("/mcp") >= 0, "the sign-in is kept in Harlo's mcp-auth.json")
            tests.clickItem(tests.findName("mcpBack", settingsDialog.contentItem), "All servers") } }
        PauseAnimation { duration: 400 }
        // The project's own server (the repo's .harlo/mcp.json).
        ScriptAction { script: tests.clickItem(tests.findName("mcpScope", settingsDialog.contentItem), "scope") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: tests.clickText("repo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(tests.findName("mcpServer-dice", d) !== null && tests.findText("not allowed for Harlo", d) !== null, "a project's own server: not allowed for Harlo")
            tests.shotPopup(settingsDialog, "clicks-mcp-project")
            tests.clickItem(tests.findName("mcpServer-dice", d), "dice") } }
        PauseAnimation { duration: 500 }
        // Its tool marked as only looking (the server doesn't say).
        // (Pressed, not clicked: scrolled into view, it's under the floating
        // Save bar.)
        ScriptAction { script: tests.findName("mcpTest", settingsDialog.contentItem).clicked() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => { const c = tests.findName("mcpLooks-roll", settingsDialog.contentItem); return c !== null && c.visible }, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, c = tests.findName("mcpLooks-roll", d)
            tests.check(c !== null && c.visible && c.enabled && !c.checked, "a tested tool can be marked as only looking, for Harlo (unmarked: the server doesn't say)")
            tests.check(tests.findName("mcpLooksHint", d) !== null && tests.findName("mcpLooksHint", d).visible, "the page says what marking does")
            c.click()
            const f = tests.ancestorWith(c, "contentHeight")
            if (f) f.contentY = Math.max(0, f.contentHeight - f.height) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const c = tests.findName("mcpLooks-roll", settingsDialog.contentItem)
            tests.check(c.checked, "ticked")
            tests.shotPopup(settingsDialog, "clicks-mcp-looks") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.clickItem(tests.findName("mcpSave", settingsDialog.contentItem), "Save") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const file = TestInput.readFile(win.argValue("--cwd=") + "/.harlo/mcp.json")
            tests.check(JSON.stringify((JSON.parse(file).mcpServers.dice || {}).readOnly) === JSON.stringify(["roll"]), "saved in Harlo's entry as readOnly (" + file.replace(/\s+/g, " ") + ")")
            tests.check(tests.findText("not allowed for Harlo", settingsDialog.contentItem) === null, "saved here, it's allowed")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("what time is it? mcp clock") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => tests.chatText().indexOf("Done:") >= 0, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Done: It's 3 pm in Tokyo") >= 0, "Harlo calls the signed-in server's tool (" + tests.chatText().slice(-160) + ")")
            tests.sendByKeys("roll one: mcp roll") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloMcpSteps, () => tests.chatText().indexOf("You rolled") >= 0 || chat.session.status === "idle" && tests.chatText().split("Done:").length > 2, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Done: You rolled 4") >= 0, "and the project's server, now allowed (" + tests.chatText().slice(-160) + ")")
            tests.shotWindow("clicks-mcp-harlo")
            tests.summary(); Qt.quit() } }
    }

    // An MCP server harlo installs with a click (click-tests.sh: a stand-in
    // GitHub whose release is the real context-mode, then a newer one):
    // turned on for Harlo (downloaded), tested, updated, used by a session,
    // turned off.
    SequentialAnimation {
        id: catalogSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlocatalog"
        function offered() { return JSON.parse(Mcp.catalogJson("global"))[0] || {} }
        function entry() {
            const text = TestInput.readFile(win.selftestDir + "/config/harlo-agent/mcp.json") || "{}"
            return (JSON.parse(text).mcpServers || {})["context-mode"] || null
        }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => catalogSteps.offered().loading === false && catalogSteps.offered().latest === "2.1.7" && catalogSteps.offered().stars > 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogSteps.offered()
            const card = tests.findName("mcpCatalog-context-mode", d), sw = tests.findName("mcpCatalogToggle-context-mode", d), u = tests.findName("mcpCatalogUpdate-context-mode", d)
            tests.check(card !== null && card.visible, "Ready to install lists context-mode")
            tests.check(sw !== null && !sw.checked && sw.enabled, "off at first, and it can be turned on")
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.7 is the newest", "not installed, and its newest release (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.check(u === null || !u.visible, "no Update before it's installed")
            const repo = tests.findName("mcpCatalogRepo-context-mode", d), stars = tests.findName("mcpCatalogStars-context-mode", d)
            tests.check(repo !== null && repo.visible && repo.text === "mksglu/context-mode" && repo.url === "https://github.com/mksglu/context-mode", "its repository, a link to it (" + (repo ? repo.text + " " + repo.url : "none") + ")")
            tests.check(stars !== null && stars.visible && stars.text === "12k" && stars.ToolTip.text === "12,345 stars on GitHub", "its stars (" + (stars ? stars.text + ", " + stars.ToolTip.text : "none") + ")")
            tests.check(b.version === "" && b.node !== "" && b.nodeError === "", "nothing downloaded yet; a Node found (" + JSON.stringify(b) + ")")
            tests.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") === "" && catalogSteps.entry() === null, "nothing in harlo's folder or Harlo's servers yet")
            tests.shotPopup(settingsDialog, "clicks-catalog-off")
            tests.clickItem(sw, "turn on") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => (catalogSteps.offered().agents || []).length > 0 || (catalogSteps.offered().error || "") !== "", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = catalogSteps.offered(), e = catalogSteps.entry() || {}
            tests.check(JSON.stringify(b.agents) === JSON.stringify(["harlo"]) && b.error === "" && b.version === "2.1.7", "downloaded and on for Harlo (" + JSON.stringify(b) + ")")
            const data = win.selftestDir + "/data/harlo/mcp/context-mode"
            tests.check(JSON.parse(TestInput.readFile(data + "/2.1.7/package.json") || "{}").version === "2.1.7", "in Harlo's folder")
            tests.check(e.command && e.command.startsWith("/") && e.command.endsWith("/node"), "the entry runs Node by its path (" + e.command + ")")
            tests.check(JSON.stringify(e.args) === JSON.stringify([data + "/current/server.mjs", "--agent", "claude-code"]), "harlo's copy, through current (" + JSON.stringify(e.args) + ")")
            tests.check(JSON.stringify(e.hiddenTools) === JSON.stringify(["ctx_upgrade", "ctx_doctor"]) && e.enabled === true, "its own updating hidden from Harlo (" + JSON.stringify(e) + ")") } }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => (JSON.parse(Mcp.statusJson("global"))["context-mode"] || {}).state === "connected", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const pill = tests.findName("mcpCatalogStatus-context-mode", d)
            tests.check(pill.text === "Connected · 9 tools", "tested: connected (" + pill.text + ")")
            tests.check(tests.findName("mcpServer-context-mode", d) === null, "not listed again under Your servers")
            tests.shotPopup(settingsDialog, "clicks-catalog-on")
            // (Looking again finds the newer release.)
            tests.clickItem(tests.findName("mcpRefresh", d), "refresh") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => catalogSteps.offered().latest === "2.1.8", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, u = tests.findName("mcpCatalogUpdate-context-mode", d)
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.7 · 2.1.8 is out" && u !== null && u.visible, "a newer release: Update (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.clickItem(u, "Update") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => catalogSteps.offered().busy === "" && (catalogSteps.offered().message !== "" || catalogSteps.offered().error !== ""), 30000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogSteps.offered()
            tests.check(b.message === "context-mode is updated: 2.1.7 → 2.1.8. New sessions use it." && b.version === "2.1.8", "updated from the release (" + b.message + b.error + ")")
            const pkg = TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") || "{}"
            tests.check(JSON.parse(pkg).version === "2.1.8", "current is the new one")
            tests.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/2.1.7/package.json") !== "", "the one before stays, for sessions still running it")
            const u = tests.findName("mcpCatalogUpdate-context-mode", d)
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.8 · up to date" && (u === null || !u.visible), "up to date: no Update (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.check(tests.findName("mcpCatalogNote-context-mode", d).text === b.message, "it says so")
            tests.shotPopup(settingsDialog, "clicks-catalog-updated")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        // A Harlo session uses it.
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: tests.sendByKeys("how much was kept out? mcp ctx_stats") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => tests.chatText().indexOf("Done:") >= 0, 40000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.chatText().indexOf("Done: ctx_stats:") >= 0, "Harlo calls context-mode's tool (" + tests.chatText().slice(-200) + ")")
            const req = tests.modelRequests().slice(-1)[0] || {}
            const names = (req.tools || []).map(t => t.function.name)
            tests.check(names.includes("mcp__context-mode__ctx_execute") && !names.includes("mcp__context-mode__ctx_upgrade") && !names.includes("mcp__context-mode__ctx_doctor"), "offered its tools but the two hidden (" + names.filter(n => n.indexOf("context") >= 0).join() + ")")
            const system = ((req.messages || [])[0] || {}).content || ""
            tests.check(system.indexOf('<mcp_server name="context-mode">') >= 0 && system.indexOf("ctx_execute") >= 0, "and its instructions")
            tests.shotWindow("clicks-catalog-session")
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: tests.clickItem(tests.findName("mcpCatalogToggle-context-mode", settingsDialog.contentItem), "turn off") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogSteps, () => catalogSteps.offered().busy === "" && (catalogSteps.offered().agents || []).length === 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(catalogSteps.entry() === null, "off: its entry is gone")
            tests.check(tests.findName("mcpCatalogStatus-context-mode", d).text === "Off" && tests.findName("mcpCatalogFor-context-mode-harlo", d).checked, "Off, and Harlo stays ticked for next time")
            tests.summary(); Qt.quit() } }
    }

    // The same on a host (the test's SSH server, its home in the test
    // folder): with box the instance shown, Ready to install downloads here,
    // copies it there, finds box's Node, writes box's entries and tests it
    // there; Update and off likewise; this computer's card is its own.
    SequentialAnimation {
        id: catalogHostSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlocataloghost"
        readonly property string hostHome: win.selftestDir + "/host-home"
        readonly property string copies: hostHome + "/.local/share/harlo/mcp/context-mode"
        function offered(scope) { return JSON.parse(Mcp.catalogJson(scope || "host:box"))[0] || {} }
        function entry() {
            const text = TestInput.readFile(catalogHostSteps.hostHome + "/.config/harlo-agent/mcp.json") || "{}"
            return (JSON.parse(text).mcpServers || {})["context-mode"] || null
        }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setProviderEnabled("claude", false)
            tests.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => JSON.parse(Remotes.hostsJson())[0].state === "connected", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { Workspace.switchInstance("box"); settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().loading === false && catalogHostSteps.offered().latest === "2.1.7", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogHostSteps.offered()
            const card = tests.findName("mcpCatalog-context-mode", d), sw = tests.findName("mcpCatalogToggle-context-mode", d)
            tests.check(card !== null && card.visible && sw !== null && !sw.checked && sw.enabled, "box's global servers offer it too, off")
            tests.check(tests.findName("mcpCatalogLede", d).text.indexOf("sets it up on box for the agents you tick") > 0, "it says it's set up on box (" + tests.findName("mcpCatalogLede", d).text + ")")
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.7 is the newest", "not installed there (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.check(b.node !== "" && b.nodeError === "" && (b.error || "") === "", "box's Node found (" + JSON.stringify(b) + ")")
            tests.check(TestInput.readFile(catalogHostSteps.copies + "/current/package.json") === "" && catalogHostSteps.entry() === null, "nothing on box yet")
            tests.shotPopup(settingsDialog, "clicks-catalog-host-off")
            tests.clickItem(sw, "turn on") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && ((catalogHostSteps.offered().agents || []).length > 0 || (catalogHostSteps.offered().error || "") !== ""), 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = catalogHostSteps.offered(), e = catalogHostSteps.entry() || {}, c = catalogHostSteps.copies
            tests.check(JSON.stringify(b.agents) === JSON.stringify(["harlo"]) && b.error === "" && b.version === "2.1.7", "downloaded and on for Harlo on box (" + JSON.stringify(b) + ")")
            tests.check(JSON.parse(TestInput.readFile(c + "/current/package.json") || "{}").version === "2.1.7" && TestInput.readFile(c + "/current/server.mjs") !== "", "copied to box, its current link there")
            tests.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") === "", "not installed on this computer")
            tests.check(e.command && e.command.startsWith("/") && e.command.endsWith("/node"), "box's entry runs box's Node by its path (" + e.command + ")")
            tests.check(JSON.stringify(e.args) === JSON.stringify([c + "/current/server.mjs", "--agent", "claude-code"]), "box's copy, through current (" + JSON.stringify(e.args) + ")")
            tests.check(JSON.stringify(e.hiddenTools) === JSON.stringify(["ctx_upgrade", "ctx_doctor"]) && e.enabled === true, "its own updating hidden from Harlo there") } }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => (JSON.parse(Mcp.statusJson("host:box"))["context-mode"] || {}).state === "connected", 40000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const pill = tests.findName("mcpCatalogStatus-context-mode", d)
            tests.check(pill !== null && pill.text === "Connected · 9 tools", "tested on box: connected (" + (pill ? pill.text : "") + " " + Mcp.statusJson("host:box").slice(0, 600) + ")")
            tests.check(tests.findName("mcpServer-context-mode", d) === null, "not listed again under Your servers")
            tests.shotPopup(settingsDialog, "clicks-catalog-host-on")
            tests.clickItem(tests.findName("mcpRefresh", d), "refresh") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().latest === "2.1.8" && catalogHostSteps.offered().version === "2.1.7", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, u = tests.findName("mcpCatalogUpdate-context-mode", d)
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.7 · 2.1.8 is out" && u !== null && u.visible, "a newer release: Update (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.clickItem(u, "Update") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && ((catalogHostSteps.offered().message || "") !== "" || (catalogHostSteps.offered().error || "") !== ""), 60000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogHostSteps.offered(), c = catalogHostSteps.copies
            tests.check(b.message === "context-mode is updated on box: 2.1.7 → 2.1.8. New sessions use it." && b.version === "2.1.8", "updated on box (" + b.message + b.error + ")")
            tests.check(JSON.parse(TestInput.readFile(c + "/current/package.json") || "{}").version === "2.1.8", "box's current is the new one")
            tests.check(TestInput.readFile(c + "/2.1.7/package.json") !== "", "the one before stays there")
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.8 · up to date", "up to date")
            tests.shotPopup(settingsDialog, "clicks-catalog-host-updated")
            tests.clickItem(tests.findName("mcpCatalogToggle-context-mode", d), "turn off") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && (catalogHostSteps.offered().agents || []).length === 0, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            tests.check(catalogHostSteps.entry() === null && TestInput.readFile(catalogHostSteps.copies + "/current/package.json") !== "", "off: box's entry is gone, its copy stays")
            tests.check(tests.findName("mcpCatalogStatus-context-mode", d).text === "Off", "Off")
            Workspace.switchInstance("") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: tests.waitUntil(catalogHostSteps, () => catalogHostSteps.offered("global").loading === false, 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const b = catalogHostSteps.offered("global"), d = settingsDialog.contentItem
            tests.check(b.version === "" && (b.agents || []).length === 0, "this computer's own: not installed here (" + JSON.stringify(b) + ")")
            tests.check(tests.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.8 is the newest", "and its card says so (" + tests.findName("mcpCatalogVersion-context-mode", d).text + ")")
            tests.summary(); Qt.quit() } }
    }

    // Claude Code's skills and agent profiles in a Harlo session (the test's
    // ~/.claude/skills/greet and the repo's .claude/agents/reviewer, from
    // click-tests.sh), and the commit message Harlo writes.
    SequentialAnimation {
        id: harloSkillSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharloskills"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => chat.session.modelName !== "" && chat.session.commands.length > 0, 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.check(chat.session.commands.some(c => c.indexOf("skill:greet") >= 0), "the command list offers skill:greet (" + chat.session.commands.filter(c => c.indexOf("skill") >= 0).join() + ")")
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.typeText("/skill:gr") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            tests.check(chat.composerItem.completionVisible && chat.composerItem.completionItems.some(c => JSON.stringify(c).indexOf("skill:greet") >= 0), "typing / suggests it")
            tests.shotWindow("clicks-harlo-skill-completion")
            // Each command a row of its own: its description on one line,
            // nothing reaching into the next row.
            const listView = chat.composerItem.completionList
            const names = tests.findAllNames("commandName", listView).filter(n => n.visible)
            const notes = tests.findAllNames("commandDescription", listView).filter(n => n.visible)
            const inside = n => { let row = n; while (row && row.parent && row.parent !== listView.contentItem) row = row.parent
                                  const p = n.mapToItem(row, 0, 0); return p.y >= 0 && p.y + n.height <= row.height + 0.5 }
            const grammar = notes.find(n => n.text.indexOf("Checks the grammar") === 0)
            tests.check(names.length >= 2 && notes.length >= 2 && notes.every(n => n.lineCount === 1) && names.concat(notes).every(inside),
                      "every suggestion fits its row, its description on one line (" + names.length + " rows, " + notes.map(n => n.lineCount).join(",") + " lines)")
            tests.check(grammar && grammar.text.indexOf("\n") < 0 && grammar.text.indexOf("SKIP") < 0 && grammar.truncated,
                      "a long description runs together and is cut at the row's end (" + (grammar ? grammar.text.slice(0, 60) : "none") + ")")
            const hint = tests.findAllNames("commandHint", listView).find(n => n.visible && n.text.indexOf("file") >= 0)
            tests.check(hint && hint.text === "<file> [--strict]" && hint.contentWidth > 40, "an argument hint with <…> shows as written (" + (hint ? hint.text + ", " + Math.round(hint.contentWidth) + " px" : "none") + ")")
            const footer = tests.findName("completionFooter", listView.parent)
            tests.check(footer && footer.visible && footer.text.indexOf("Tab or Enter") >= 0, "its footer says how to choose")
            // (The rest typed out: the suggestion isn't picked.)
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("/skill:greet the team")
            TestInput.key(Qt.Key_Escape, 0)
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => tests.modelRequests().length > 0 && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = tests.modelRequests()[0] || {}
            const sys = JSON.stringify((b.messages || []).filter(m => m.role === "system")), user = JSON.stringify((b.messages || []).filter(m => m.role === "user").slice(-1))
            tests.check(sys.indexOf("greet") >= 0 && sys.indexOf("Greets the user warmly.") >= 0, "Harlo's instructions list the skill with its description")
            tests.check(user.indexOf("<skill name=\\\"greet\\\"") >= 0 && user.indexOf("Say hello in three languages.") >= 0 && user.indexOf("the team") >= 0, "/skill:greet sends the skill with the prompt (" + user.slice(0, 200) + ")")
            tests.sendByKeys("use the reviewer please") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => tests.chatText().split("Done:").length > 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const first = b => JSON.stringify(((b.messages || []).find(m => m.role === "user") || {}).content || "")
            const sub = tests.modelRequests().find(b => first(b).indexOf("SUBTASK review it") >= 0)
            const tools = sub ? (sub.tools || []).map(t => t.function.name) : []
            tests.check(sub !== undefined && JSON.stringify(sub.messages).indexOf("REVIEWER-PROFILE") >= 0, "a task names Claude's agent profile: its instructions are the subagent's")
            tests.check(tools.includes("read") && tools.includes("grep") && !tools.includes("write") && !tools.includes("bash"), "with its tools only (" + tools.join() + ")")
            // /plugin: Claude Code's plugin command (the stand-in's), run
            // by harlo in the project, never sent to the model.
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            tests.pluginAsked = tests.modelRequests().length
            tests.check(chat.session.commands.some(c => c.indexOf("plugin") === 0), "the / list offers plugin")
            tests.sendByKeys("/plugin install hello@demo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => tests.findAllNames("backgroundHeadline", chat.transcript).some(h => h.text === "Plugin installed"), 10000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const log = TestInput.readFile(win.argValue("--cwd=") + "/../home/plugin-log.txt").trim().split("\n").map(l => JSON.parse(l))
            tests.check(log.length === 1 && JSON.stringify(log[0].args) === '["install","hello@demo"]' && log[0].cwd.endsWith("/repo"), "harlo runs claude plugin install in the project (" + JSON.stringify(log) + ")")
            tests.check(tests.modelRequests().length === tests.pluginAsked, "and the model isn't asked")
            const cmd = tests.findAllNames("backgroundCommand", chat.transcript).find(c => c.visible && c.text.indexOf("claude plugin") >= 0)
            tests.check(cmd && cmd.text === "$ claude plugin install hello@demo", "its notice shows the command (" + (cmd ? cmd.text : "none") + ")")
            tests.shotWindow("clicks-harlo-plugin-installed") } }
        // Harlo starts again (idle): the plugin's skill is in its / list.
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => chat.session.commands.some(c => c.indexOf("skill:hello:wave") >= 0), 15000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            tests.check(chat.session.commands.some(c => c.indexOf("skill:hello:wave") >= 0), "Harlo loads the plugin's skill (" + chat.session.commands.filter(c => c.indexOf("skill") >= 0).join() + ")")
            tests.sendByKeys("/plugin install missing@demo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => tests.findAllNames("backgroundHeadline", chat.transcript).some(h => h.text === "Plugin not installed"), 10000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const head = tests.findAllNames("backgroundHeadline", chat.transcript).find(h => h.text === "Plugin not installed")
            const note = head ? head.parent.parent : null
            tests.check(head && head.color === Theme.error && note && tests.findContaining("exit code 1", note) !== null, "one that fails says so, in red, with its exit code")
            tests.clickItem(head, "the failed install")
            tests.pluginNote = note } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const open = tests.pluginNote ? tests.findName("backgroundOpen", tests.pluginNote) : null
            tests.check(open && open.visible && tests.findContaining("not found in any marketplace", open) !== null, "and why, a click away")
            tests.check(tests.modelRequests().length === tests.pluginAsked, "still nothing sent to the model")
            tests.shotWindow("clicks-harlo-plugin-failed")
            // A change to commit, and the message Harlo writes for it.
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello\nmore\n")
            windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: tests.clickItem(tests.findName("commitWrite"), "Write the message") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSkillSteps, () => !git.generating, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(changes.commitText.indexOf("[fake-coder]") >= 0 && !git.error, "Harlo writes the commit message (" + JSON.stringify(changes.commitText).slice(0, 120) + " " + git.message + ")")
            tests.shotWindow("clicks-harlo-commit")
            tests.summary(); Qt.quit() } }
    }

    // Another session, mentioned and searched: the project's past Harlo
    // session (click-tests.sh writes it) picked from the composer's `@`
    // list goes with the message as an excerpt; the model finds it with
    // list_sessions' query and reads it with read_session.
    SequentialAnimation {
        id: harloSessionSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlosessions"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(tests.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSessionSteps, () => chat.session.modelName !== "" && JSON.parse(Workspace.sessionMentions(chat.session.cwd, chat.session.key, "")).length > 0, 15000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const all = JSON.parse(Workspace.sessionMentions(chat.session.cwd, chat.session.key, ""))
            tests.check(all.length === 1 && all[0].mention === "@session:a1b2c3d4", "the project's past session is offered, not this one (" + JSON.stringify(all).slice(0, 200) + ")")
            tests.clickItem(tests.findName("composerInput"), "composer")
            TestInput.typeText("@sandbox") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const items = chat.composerItem.completionItems
            tests.check(chat.composerItem.completionVisible && items.length > 0 && items[0].kind === "session" && items[0].name.indexOf("sandbox commands") >= 0,
                      "typing @ and its words lists the session first (" + JSON.stringify(items.slice(0, 2)) + ")")
            tests.shotWindow("clicks-harlo-session-mention")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            tests.check(tests.findName("composerInput").text.indexOf("@session:a1b2c3d4 ") === 0, "picking it writes its mention (" + JSON.stringify(tests.findName("composerInput").text) + ")")
            TestInput.typeText("what did we pick?")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSessionSteps, () => tests.modelRequests().length > 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const user = JSON.stringify((tests.modelRequests()[0].messages || []).filter(m => m.role === "user").slice(-1))
            tests.check(user.indexOf("PICKED-BWRAP") >= 0 && user.indexOf("Session \\\"How should we sandbox commands?\\\"") >= 0 && user.indexOf("what did we pick?") >= 0,
                      "its messages go with the message (" + user.slice(0, 300) + ")")
            tests.check(tests.chatText().indexOf("what did we pick?") >= 0 && tests.chatText().indexOf("PICKED-BWRAP") < 0, "the transcript shows the message, not the excerpt")
            tests.check(chat.session.title.indexOf("@session:") < 0 && chat.session.title.indexOf("what did we pick?") >= 0, "the title leaves the mention out (" + chat.session.title + ")")
            tests.shotWindow("clicks-harlo-session-sent")
            // (The search is slowed to 1.5 s: the window keeps drawing.)
            longTick.reset(); longTick.start()
            tests.sendByKeys("search past sessions for bubblewrap") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSessionSteps, () => tests.chatText().split("Done:").length > 1 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = tests.modelRequests(), last = reqs[reqs.length - 1]
            const result = JSON.stringify((last.messages || []).filter(m => m.role === "tool").slice(-1))
            tests.check(result.indexOf("@session:a1b2c3d4") >= 0 && result.indexOf("**bubblewrap**") >= 0, "list_sessions with a query finds it, with the passage (" + result.slice(0, 300) + ")")
            longTick.stop()
            tests.check(longTick.worst < 400, "the window kept drawing while it searched (longest gap " + longTick.worst + " ms)")
            tests.sendByKeys("read past session @session:a1b2c3d4") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(harloSessionSteps, () => tests.chatText().split("Done:").length > 2 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = tests.modelRequests(), last = reqs[reqs.length - 1]
            const result = JSON.stringify((last.messages || []).filter(m => m.role === "tool").slice(-1))
            tests.check(result.indexOf("closed") >= 0 && result.indexOf("Assistant: PICKED-BWRAP") >= 0, "read_session reads the past one (" + result.slice(0, 300) + ")")
            tests.shotWindow("clicks-harlo-session-read")
            tests.summary(); Qt.quit() } }
    }

    // Plan limits from a Harlo session on the Codex sign-in (click-tests.sh:
    // a stand-in ChatGPT backend whose replies carry the plan's usage): the
    // meter shows them, under ChatGPT's logo.
    SequentialAnimation {
        id: codexLimitSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickscodexlimits"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { tests.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(codexLimitSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            tests.check(chat.session.modelName === "Fake Codex" && tests.findName("planMeter") === null, "a Harlo session on the Codex sign-in; no meter yet (" + chat.session.modelName + ")")
            tests.sendByKeys("hello") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: tests.waitUntil(codexLimitSteps, () => tests.chatText().indexOf("hi from codex") >= 0 && tests.findName("planMeter") !== null, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = tests.findName("planMeter"), q = Workspace.quotaJson
            tests.check(tests.chatText().indexOf("hi from codex") >= 0, "it answers")
            tests.check(m !== null && tests.findText("41%", m) !== null && tests.findName("planMeterLogo", m).provider === "openai-codex", "the meter shows the plan's fullest window from the reply (41%) under ChatGPT's logo")
            tests.check(q.indexOf("tester@example.com") >= 0 && q.indexOf("13") >= 0, "both windows, for the signed-in account")
            tests.shotWindow("clicks-codex-meter")
            tests.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const o = tests.findName("meterPick-openai-codex")
            tests.check(o !== null && o.hint === "41%", "its menu lists ChatGPT (41%)")
            tests.summary(); Qt.quit() } }
    }
}
