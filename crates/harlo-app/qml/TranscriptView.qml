pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Qt.labs.qmlmodels
import harlo

// The transcript of one session: virtualized rows with one delegate per row kind.
Item {
    id: root
    // (Its window shown: not minimized, not in the tray. Polling waits
    // otherwise; Item.visible doesn't change when the window hides.)
    readonly property bool windowShown: Window.visibility !== Window.Minimized && Window.visibility !== Window.Hidden
    required property ChatSession session
    // A row added to the transcript fades in.
    component AddFade: NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    property alias follow: view.follow
    // The reading column, for what floats over it (the Latest button).
    readonly property real columnX: view.columnX
    readonly property real columnWidth: view.columnWidth
    property alias count: view.count
    /// Side margin of the rows; the composer below uses it too so edges align.
    readonly property alias gutter: view.gutter
    /// A file-editing tool row asked to show its diff.
    signal openDiff(string path)
    // A file at a line (a listing's line number, or a file's name in it).
    signal openFileAt(string path, int line)
    /// A file to open in the file tab (a picture from the viewer).
    signal openFile(string path)
    // Escape from a run's line, or typing in a run: back to the composer
    // (with what was typed).
    signal leaveRuns(string text)
    /// Your message at row `index`: edit and resend it (the session forks there).
    signal editRequested(int index)
    /// Undo the turn of one of your messages (its row key).
    signal undoRequested(string rowKey)
    /// Your messages a turn can be undone to (`ChatSession.undoJson`): the
    /// button sits at the end of the turn, under the agent's reply.
    readonly property var undoable: session && session.undoJson !== "" ? JSON.parse(session.undoJson) : ({})
    /// The latest turn's message, when the turn changed files ("" if not).
    readonly property string latestUndo: {
        for (const k in undoable) if (undoable[k].next === "" && undoable[k].files > 0) return k
        return ""
    }
    /// The turn that ends where your message `userKey` starts ("" if none).
    function undoBefore(userKey) {
        for (const k in undoable) if (undoable[k].next === userKey) return k
        return ""
    }
    /// A subagent under a task tool row: view (or steer) it.
    signal subagentRequested(var agent, bool steer)
    /// The subagents of the session, shown under the tool call that started them.
    readonly property var subagents: session.subagentsJson !== "" ? JSON.parse(session.subagentsJson) : []

    function scrollToEnd() { view.follow = true; view.scrollToEnd() }
    // (Tests: the list itself.)
    readonly property alias listView: view
    // A picture in the chat (one you sent, a tool returned, a reply has):
    // the image viewer over the conversation, stepping through all of them.
    function showPicture(path) {
        const all = JSON.parse(root.session.sessionPictures())
        if (all.indexOf(path) < 0) all.push(path)
        root.pictureLabelCache = JSON.parse(root.session.pictureLabels())
        chatPictures.show(all, path, root.pictureLabelCache)
    }
    // Where each picture came from, in words, for the thumbnails' tips and
    // names (asked again for one not known yet; forgotten with the session).
    property var pictureLabelCache: ({})
    function pictureLabel(p) {
        // (Filled in place: a binding asking mustn't set off the others.)
        if (!(p in pictureLabelCache) && root.session) {
            const fresh = JSON.parse(root.session.pictureLabels())
            for (const k in fresh) pictureLabelCache[k] = fresh[k]
        }
        return pictureLabelCache[p] || p.substring(p.lastIndexOf("/") + 1)
    }
    Popup {
        id: chatPictures
        objectName: "chatPictures"
        parent: root
        x: 0
        y: 0
        width: root.width
        height: root.height
        padding: 0
        modal: true
        focus: true
        // (Esc is the viewer's: out of the grid first, then closed.)
        closePolicy: Popup.NoAutoClose
        // (The viewer is built when a picture opens and let go when it closes.)
        readonly property var viewer: viewerLoader.item
        function show(list, path, labels) {
            viewerLoader.active = true
            const v = viewerLoader.item
            v.gridMode = false
            v.labels = labels || ({})
            v.pictureList = list
            v.path = path
            v.source = path
            open()
            Qt.callLater(() => { if (viewerLoader.item) viewerLoader.item.focusView() })
        }
        background: Rectangle { color: Theme.stage }
        Overlay.modal: Rectangle { color: "transparent" }
        enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Math.round(Theme.motion * 0.5) } }
        exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.4) } }
        contentItem: Loader {
            id: viewerLoader
            active: false
            focus: true
            sourceComponent: ImageViewer {
                pictureList: []
                backdrop: Theme.stage
                onCloseRequested: chatPictures.close()
                onOpenInTab: p => { chatPictures.close(); root.openFile(p) }
            }
        }
        onClosed: viewerLoader.active = false
    }
    property alias chatPictures: chatPictures

    // A reply's picture full size, inside harlo.
    PictureViewer { id: pictureViewer }
    property alias pictureViewer: pictureViewer
    function viewDiagram(d, source) {
        pictureViewer.show("Diagram", [{ label: "Diagram", url: d.url, width: d.width, height: d.height }], 0, "Diagram",
                           () => Diagrams.openInBrowser(source, Theme.isLight), 2)
    }
    function viewMockup(m, source, before) {
        if (!m) return
        const pics = []
        if (m.before && m.before.state === "ready") pics.push({ label: "Before", url: m.before.url, width: m.before.width, height: m.before.height, before: true })
        if (m.after && m.after.state === "ready") pics.push({ label: "After", url: m.after.url, width: m.after.width, height: m.after.height, before: false })
        if (pics.length === 0) return
        const at = Math.max(0, pics.findIndex(p => p.before === before))
        pictureViewer.show(m.title || "Mockup", pics, at, "Mockup",
                           i => Visuals.openMockup(source, root.session.cwd || "", pics[i].before))
    }
    /// Space above a row, from its kind and the kind before it: a new turn
    /// stands apart, paragraphs breathe, and tool calls and thoughts in a
    /// run sit together as one timeline.
    function gapAbove(kind, prev) {
        if (prev === "") return root.opensTurn(kind) ? 8 : 0
        const act = k => k === "tool" || k === "thinking"
        // A new turn: a line between turns, then your avatar and name (or
        // harlo's, for its messages to the agent).
        if (root.opensTurn(kind)) return 34
        if (act(kind) && act(prev)) return 0
        // The reply opens with the agent's avatar and name.
        if (root.opensTurn(prev)) return 22 + root.headHeight
        if ((kind === "markdown" || kind === "images") && prev === "markdown") return 8
        if (act(kind) || act(prev)) return 8
        return 14
    }
    /// Rows that start a turn: your messages and harlo's to the agent.
    function opensTurn(section) { return section === "user" || section === "harlo" }
    function isActivity(kind) { return kind === "tool" || kind === "thinking" }
    /// A turn's head: avatar and name, over its first row.
    readonly property real headHeight: Theme.px(30)
    // A reply's model has no name yet: the agent's list is read once, as
    // soon as the agent runs (not started just for that).
    property bool wantModelNames: false
    function readModelNames() {
        if (wantModelNames && session.models.length === 0 && !session.modelsAnswered
                && (session.status === "idle" || session.status === "busy")) {
            wantModelNames = false
            session.loadModels()
        }
    }
    onWantModelNamesChanged: readModelNames()
    onSessionChanged: { wantModelNames = false; pictureLabelCache = ({}) }
    Connections {
        target: root.session
        function onStatusChanged() { root.readModelNames() }
    }
    /// A reply's model as the head shows it (its name when the agent
    /// listed it; "" unknown: the session's).
    function replyModelName(id) {
        for (const s of [session, namesFrom]) {
            if (!s || s.modelId === "")
                continue
            if (id === "" || s.modelId === id || s.modelId.endsWith("/" + id))
                return s.modelName
        }
        for (const s of [session, namesFrom]) {
            const known = s ? s.models.map(m => m.split("\t")).find(p => p[1] === id || p[0] + "/" + p[1] === id) : null
            if (known && known[2])
                return known[2]
        }
        return id === "" ? session.modelName : id
    }
    /// Another session whose model names it may use (a branch's preview:
    /// the session open in the chat).
    property ChatSession namesFrom: null
    // The agent at the top of its reply: avatar, name and model (the
    // reply's own: the session may have switched since).
    component AgentHead: Item {
        id: head
        objectName: "agentHead"
        property string replyModel
        readonly property string shownModel: root.replyModelName(replyModel)
        // Another model than now: its name comes with the agent's list.
        readonly property bool unnamed: visible && replyModel !== "" && shownModel === replyModel
        onUnnamedChanged: if (unnamed) root.wantModelNames = true
        Component.onCompleted: if (unnamed) root.wantModelNames = true
        visible: false
        x: view.columnX
        width: view.contentWidthLimit
        height: root.headHeight
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            AgentAvatar {
                implicitWidth: Theme.px(20)
                implicitHeight: Theme.px(20)
                anchors.verticalCenter: parent.verticalCenter
                provider: root.session.harness
                maker: Workspace.modelMaker(root.session.modelId)
                ring: Theme.canvas
            }
            Label { text: Theme.harnessLabel(root.session.harness); color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.DemiBold }
            // The model only when the reply's isn't the session's (the
            // header names that one).
            Label { visible: head.shownModel !== "" && head.shownModel !== root.session.modelName; text: Theme.modelLabel(head.shownModel); color: Theme.textMuted; font.pixelSize: Theme.px(12.5); anchors.baseline: parent.children[1].baseline }
        }
    }
    // The agent's head over a reply's first row, made only there (each row
    // of a reply would otherwise hold a hidden one).
    component HeadSlot: Loader {
        id: slot
        property string replyModel
        sourceComponent: AgentHead { replyModel: slot.replyModel; visible: true }
    }
    /// How a run of tool calls and thoughts shows: "folded" (its summary
    /// line, as folded by hand), "boxed" (finished: open in a box of its
    /// own height that scrolls, so it doesn't cover the session) or "rows"
    /// (still going, or opened in full by a search).
    function runMode(group, dflt) {
        if (group === "") return "rows"
        const v = view.runs[group]
        if (v === false) return "folded"
        return dflt || v === "all" ? "rows" : "boxed"
    }
    function runOpen(group, dflt) { return runMode(group, dflt) !== "folded" }
    // Paths in the session's folder from there; a command's `cd <the
    // folder> &&` left out.
    readonly property string here: (session.cwd || "").replace(/^ssh:\/\/[^\/]+/, "").replace(/\/$/, "")
    // A remote session's host ("ssh://box"), put back on the paths it names.
    readonly property string host: (/^ssh:\/\/[^\/]+/.exec(session.cwd || "") || [""])[0]
    // A path a listing names, as a file to open: from the command's folder
    // (`cd dir && …`), else the session's; on its host.
    function fullPath(p, command, cwd) {
        if (!p) return ""
        const host = (/^ssh:\/\/[^\/]+/.exec(cwd || "") || [""])[0]
        const here = (cwd || "").slice(host.length).replace(/\/$/, "")
        if (p.startsWith("ssh://")) return p
        if (p.startsWith("/")) return host + p
        const cd = /^cd\s+("?)([^"&;]+?)\1\s*&&/.exec(command || "")
        const dir = cd ? (cd[2].startsWith("/") ? cd[2] : here + "/" + cd[2]) : here
        return host + dir.replace(/\/$/, "") + "/" + p.replace(/^\.\//, "")
    }
    function fileAt(p, command) { return fullPath(p, command, session.cwd || "") }

    // Keeps `item`'s bottom at `y` (in the window) once what's above it has
    // grown or shrunk: the scroll areas around it move by what it moved,
    // the nearest first (a run's box, then the session; not while the
    // session follows its end, which keeps it in place already).
    function holdPlace(item, y) {
        if (isNaN(y)) return
        holdTimer.item = item
        holdTimer.y0 = y
        holdTimer.tries = 3
        Qt.callLater(holdTimer.hold)
        holdTimer.restart()
    }
    Timer {
        id: holdTimer
        property var item: null
        property real y0: 0
        property int tries: 0
        interval: 30
        onTriggered: { hold(); if (--tries > 0) restart() }
        function hold() {
            if (!item) return
            let delta = item.mapToItem(null, 0, item.height).y - y0
            for (let p = item.parent; p && Math.abs(delta) > 0.5; p = p.parent) {
                if (p.contentY === undefined || p.contentHeight === undefined || p.contentHeight <= p.height + 0.5) continue
                if (p === view && view.follow) break
                // (Past its content by its margins, a list's last step's.)
                const lo = (p.originY || 0) - (p.topMargin || 0), hi = (p.originY || 0) + p.contentHeight + (p.bottomMargin || 0) - p.height
                const to = Math.max(lo, Math.min(hi, p.contentY + delta))
                delta -= to - p.contentY
                p.contentY = to
            }
        }
    }

    // ---- the keyboard in a run: its line takes Tab, Up and Down go
    // through its steps, Escape back to its line.
    function runSteps(group) {
        const out = []
        const walk = it => {
            for (const c of it.children) {
                if (!c.visible) continue
                if ((c.objectName === "toolHead" || c.objectName === "thoughtHead") && c.stepGroup === group) out.push(c)
                else walk(c)
            }
        }
        walk(view.contentItem)
        return out.map(s => ({ s, y: s.mapToItem(view.contentItem, 0, 0).y })).sort((a, b) => a.y - b.y).map(o => o.s)
    }
    /// Step `j` of a run's box `list`: brought into its view (so it's
    /// made), its line (null: there's none).
    function boxStep(list, j) {
        if (!list || j < 0 || j >= list.count) return null
        list.pinned = false
        list.restoreY = -1
        list.positionViewAtIndex(j, ListView.Contain)
        const it = list.itemAtIndex(j)
        const find = x => {
            for (const c of x.children) {
                if (!c.visible) continue
                if (c.objectName === "toolHead" || c.objectName === "thoughtHead") return c
                const f = find(c)
                if (f) return f
            }
            return null
        }
        return it ? find(it) : null
    }
    /// The list in run `group`'s box (null: it isn't in one).
    function runBoxList(group) {
        let found = null
        const walk = it => {
            for (const c of it.children) {
                if (found || !c.visible) continue
                if (c.objectName === "runBox" && c.group === group) found = findIn(c, "runBoxScroll")
                else walk(c)
            }
        }
        const findIn = (it, name) => {
            for (const c of it.children) {
                if (c.objectName === name) return c
                const f = findIn(c, name)
                if (f) return f
            }
            return null
        }
        walk(view.contentItem)
        return found
    }
    function runHeaderOf(group) {
        let found = null
        const walk = it => {
            for (const c of it.children) {
                if (found || !c.visible) continue
                if (c.objectName === "runHeader" && c.group === group) found = c
                else walk(c)
            }
        }
        walk(view.contentItem)
        return found
    }
    function focusStep(item) {
        if (!item) return
        view.follow = false
        item.forceActiveFocus(Qt.OtherFocusReason)
        // Into view: its run's box, then the session.
        for (let f = item.parent; f; f = f.parent) {
            if (f.contentY === undefined || f.contentItem === undefined || f.height <= 0) continue
            const y = item.mapToItem(f.contentItem, 0, 0).y
            if (f.pinned !== undefined) f.pinned = false
            if (f.restoreY !== undefined) f.restoreY = -1
            if (y < f.contentY + 8) f.contentY = y - 8
            else if (y + item.height > f.contentY + f.height - 8) f.contentY = y + item.height - f.height + 8
            if (f.objectName === "runBoxScroll") {
                f.contentY = Math.max(f.originY, Math.min(f.contentY, f.originY + f.contentHeight + f.bottomMargin - f.height))
                f.keep()
            }
        }
    }
    // Into the runs from elsewhere (a key): the last run's line in view, or
    // a call on its own.
    function focusLastRun() {
        const all = []
        const walk = it => {
            for (const c of it.children) {
                if (!c.visible) continue
                if (c.objectName === "runHeader" || (c.objectName === "toolHead" && c.activeFocusOnTab)) all.push(c)
                else walk(c)
            }
        }
        walk(view.contentItem)
        const at = all.map(c => ({ c, y: c.mapToItem(view.contentItem, 0, 0).y })).sort((a, b) => a.y - b.y)
        const seen = at.filter(o => o.y + o.c.height > view.contentY && o.y < view.contentY + view.height)
        const pick = (seen.length ? seen : at).pop()
        if (pick) focusStep(pick.c)
        return !!pick
    }
    // Focus from the keyboard (not a click): what shows its ring.
    // (The step last focused by a click; a key, any key, clears it.)
    property Item clickedStep: null
    function keyFocus(item) { return item.activeFocus && clickedStep !== item }
    function clickFocus(item) { clickedStep = item; item.forceActiveFocus(Qt.MouseFocusReason) }
    // Typing in a run goes on to the composer.
    function typedOn(e) {
        if (e.text === "" || e.text < " " || (e.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return false
        root.leaveRuns(e.text)
        return true
    }
    // A step's keys (Up, Down, Escape); true when it moved.
    function stepKey(step, key) {
        // In a run's box: by its place in the run (those far off aren't made).
        let cell = step
        while (cell && cell.boxList === undefined) cell = cell.parent
        if (cell && (key === Qt.Key_Down || key === Qt.Key_Up)) {
            const j = cell.index + (key === Qt.Key_Down ? 1 : -1)
            if (j < 0) { focusStep(runHeaderOf(step.stepGroup)); return true }
            const next = boxStep(cell.boxList, j)
            if (next) focusStep(next)
            return next !== null
        }
        const steps = runSteps(step.stepGroup), i = steps.indexOf(step)
        if (key === Qt.Key_Down && i >= 0 && i + 1 < steps.length) focusStep(steps[i + 1])
        else if (key === Qt.Key_Up && i > 0) focusStep(steps[i - 1])
        else if (key === Qt.Key_Up || key === Qt.Key_Escape) focusStep(runHeaderOf(step.stepGroup))
        else return false
        return true
    }
    function local(t) {
        if (here === "" || !t) return t || ""
        const cd = "cd " + here + " && "
        if (t.startsWith(cd)) return t.slice(cd.length)
        if (t.startsWith("cd \"" + here + "\" && ")) return t.slice(here.length + 9)
        return t.split(here + "/").join("")
    }
    // A tool's name, the same for every agent: "bash" and "Bash" both
    // "Bash", "web_search" "Web search" (an MCP tool's as Theme says).
    // The shared plan's tools and the wiki's, by what they do.
    readonly property var planTools: ({ plan_set: "Plan", plan_read: "Read the plan", plan_start: "Start a step", plan_done: "Finish a step",
                                        openwiki_search: "Search the wiki", openwiki_read: "Read the wiki", openwiki_list_workspaces: "List wiki workspaces", openwiki_list_wikis: "List linked wikis",
                                        openwiki_begin: "Start the wiki run", openwiki_submit_plan: "Plan the wiki", openwiki_next_page: "Next wiki page",
                                        openwiki_inspect_page_claims: "Check a page's claims", openwiki_submit_page: "Write a wiki page", openwiki_finish: "Finish the wiki" })
    function toolName(t) {
        const p = /^(?:mcp__harlo__|harlo_)?((?:plan|openwiki)_[a-z_]+)$/.exec(t || "")
        if (p && planTools[p[1]]) return planTools[p[1]]
        const l = Theme.toolLabel(t)
        return /^[a-z][a-z_]*$/.test(l) ? (l.charAt(0).toUpperCase() + l.slice(1)).replace(/_/g, " ") : l
    }
    // Output whose lines start with line numbers (grep -n with its
    // context lines, a path and a number, cat -n): each line as
    // { gutter, body, context } with the gutters padded to one width, or
    // null when it isn't (a few lines that only look like it stay text).
    function numbered(lines) {
        const grep = /^((?:[^\s:]*[\/.][^\s:]*:)?(?:\d+[:-])+)(.*)$/
        const cat = /^\s*(\d+)\t(.*)$/
        const rows = lines.map(l => {
            const g = grep.exec(l)
            // (Not a date or a time: "2026-11-08 …", "12:30:45 …".)
            if (g && !/^\d/.test(g[2])) return { gutter: g[1].slice(0, -1), body: g[2], context: g[1].endsWith("-"), path: /[\/.]/.test(g[1].split(":")[0]) }
            const c = cat.exec(l)
            return c ? { gutter: c[1], body: c[2], context: false, path: false } : null
        })
        const hits = rows.filter(r => r !== null).length
        const real = lines.filter(l => l.trim() !== "").length
        if (hits === 0 || (hits < 2 && !rows.some(r => r && r.path)) || hits < real * 0.6) return null
        const width = Math.max(...rows.map(r => r ? r.gutter.length : 0))
        return rows.map((r, i) => r ? { gutter: r.gutter.padStart(width), body: r.body, context: r.context }
                                     : { gutter: "".padStart(width), body: lines[i], context: true })
    }
    // One numbered line as rich text: the gutter faint, a context line's
    // text dimmer than a matching one's. `nbsp`: spaces kept (StyledText).
    function numberedHtml(r, nbsp) {
        const esc = t => { const e = t.replace(/&/g, "&amp;").replace(/</g, "&lt;"); return nbsp ? e.replace(/ /g, "&nbsp;") : e }
        return "<font color='" + Theme.rgb(Theme.textFaint) + "'>" + esc(r.gutter) + (nbsp ? "&nbsp;&nbsp;" : "  ") + "</font>"
            + "<font color='" + Theme.rgb(r.context ? Theme.textMuted : Theme.text) + "'>" + esc(r.body) + "</font>"
    }
    // A path's folder faint, its name plain.
    function pathHtml(t) {
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;")
        const i = t.lastIndexOf("/")
        return i < 0 ? esc(t) : "<font color='" + Theme.rgb(Theme.textFaint) + "'>" + esc(t.slice(0, i + 1)) + "</font>" + esc(t.slice(i + 1))
    }
    // (Under a second: not worth a word.)
    function seconds(ms) { return ms < 1000 ? "" : ms < 10000 ? (ms / 1000).toFixed(1) + "s" : ms < 60000 ? Math.round(ms / 1000) + "s" : Math.floor(ms / 60000) + "m " + Math.round(ms % 60000 / 1000) + "s" }
    // A run's summary line ("Thought, edited 1 file, ran 2 commands"):
    // folds and unfolds the run.
    component RunHeader: Item {
        id: runHead
        property string group
        property string summary
        property bool dflt
        readonly property bool isOpen: root.runOpen(group, dflt)
        objectName: "runHeader"
        implicitHeight: 30
        // The keyboard: Tab to a run; Enter or Space folds it, Down goes
        // into its steps.
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Run: " + runHead.summary + (runHead.isOpen ? ", open" : ", folded")
        Keys.onPressed: e => {
            root.clickedStep = null
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) { view.toggleRun(runHead.group, runHead.dflt); e.accepted = true }
            else if (e.key === Qt.Key_Down && runHead.isOpen) {
                // (A long run's box makes its first steps when they're wanted.)
                const list = root.runBoxList(runHead.group)
                const s = list ? [root.boxStep(list, 0)].filter(x => x) : root.runSteps(runHead.group)
                if (s.length) { root.focusStep(s[0]); e.accepted = true }
            }
            else if (e.key === Qt.Key_Escape) { root.leaveRuns(""); e.accepted = true }
            else e.accepted = root.typedOn(e)
        }
        Rectangle {
            x: -6; width: parent.width + 12; height: parent.height; radius: 6
            color: runHover.hovered ? Theme.hover : "transparent"
            border.width: root.keyFocus(runHead) ? 1 : 0
            border.color: Theme.accent
        }
        RowLayout {
            anchors.fill: parent
            spacing: 8
            Icon { name: runHead.isOpen ? "arrow-down-s" : "arrow-right-s"; implicitWidth: 14; implicitHeight: 14; color: Theme.textFaint }
            Label {
                objectName: "runSummary"
                Layout.fillWidth: true
                text: runHead.summary
                color: runHover.hovered ? Theme.foreground : Theme.muted
                elide: Text.ElideRight
                font.pixelSize: Theme.px(13)
            }
            // Still going (waiting for you: the warning colour).
            Rectangle { objectName: "runLive"; visible: runHead.dflt; implicitWidth: 6; implicitHeight: 6; radius: 3; color: root.session.hasRequest ? Theme.warning : Theme.primary }
        }
        HoverHandler { id: runHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: { root.clickFocus(runHead); view.toggleRun(runHead.group, runHead.dflt) } }
    }
    // A thought in a run: one line, its text a click away.
    component ThoughtLine: ColumnLayout {
        id: thought
        property string group
        property string rowKey
        property string text
        property string rowState
        readonly property bool open: view.expanded[rowKey] === true
        spacing: 4
        Item {
            id: thoughtBox
            objectName: "thoughtHead"
            readonly property string stepGroup: thought.group
            Layout.fillWidth: true
            implicitHeight: thoughtHead.implicitHeight + 6
            Accessible.role: Accessible.Button
            Accessible.name: "Thought: " + thought.text.trim().split("\n")[0] + (thought.open ? ", open" : "")
            Keys.onPressed: e => {
                root.clickedStep = null
                if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) { view.toggle(thought.rowKey); e.accepted = true }
                else e.accepted = root.stepKey(thoughtBox, e.key) || root.typedOn(e)
            }
            Rectangle { x: thoughtWord.x - 6; width: parent.width - x + 6; height: parent.height; radius: 6; color: thoughtHover.hovered ? Theme.hover : "transparent" }
            // (Focused: ringed from its word, the run's line left clear.)
            Rectangle {
                visible: root.keyFocus(thoughtBox)
                x: thoughtWord.x - 6; width: parent.width - x + 6; height: parent.height; radius: 6
                color: "transparent"; border.width: 1; border.color: Theme.accent
            }
            RowLayout {
                id: thoughtHead
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 8
                RunNode { step: thought.rowState === "streaming" ? "running" : "thought" }
                Label { id: thoughtWord; text: thought.rowState === "streaming" ? "Thinking…" : "Thought"; color: Theme.textMuted; font.pixelSize: Theme.px(13) }
                // Its first line, folded: what it was about at a glance.
                Label {
                    objectName: "thoughtPreview"
                    Layout.fillWidth: true
                    visible: !thought.open
                    text: thought.text.trim().split("\n")[0].replace(/^[#*_>\s-]+/, "")
                    color: thoughtHover.hovered ? Theme.textMuted : Theme.textFaint
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    font.pixelSize: Theme.px(13)
                }
                Item { Layout.fillWidth: true; visible: thought.open }
            }
            HoverHandler { id: thoughtHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: { root.clickFocus(thoughtBox); view.toggle(thought.rowKey) } }
        }
        TextEdit {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            visible: thought.open
            text: thought.text
            color: Theme.textMuted
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            font.family: Theme.uiFont
            font.pixelSize: Theme.px(13)
        }
    }
    // A finished run, open: its summary line, then its thoughts and calls
    // in a box of its own height that scrolls (starting at its end, next
    // to the reply).
    component RunBox: ColumnLayout {
        id: box
        property string group
        property string summary
        property bool dflt
        // (Asked again as the list changes, parsed only when the run did:
        // a new list of steps would make all their cards again.)
        readonly property string runText: (view.count, summary, root.session.runJson(group))
        readonly property var rows: JSON.parse(runText)
        // (Room for a listing or two; the session stays in view around it.)
        readonly property real maxHeight: Math.max(Theme.px(160), Math.min(Theme.px(560), view.height * 0.6))
        objectName: "runBox"
        spacing: 2
        RunHeader { Layout.fillWidth: true; group: box.group; summary: box.summary; dflt: box.dflt }
        Item {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(boxFlick.contentHeight + 6, box.maxHeight)
        // Its steps as a list of their own: only those in view and near it
        // are made (a run of hundreds of calls makes a screenful or two).
        ListView {
            id: boxFlick
            objectName: "runBoxScroll"
            anchors.fill: parent
            model: box.rows
            spacing: 6
            bottomMargin: 6
            // (A run of up to 40 steps is made whole, as a column would be,
            // so its cards keep their place and height; a longer one keeps two
            // boxes' worth around the view, the step a key moves to with it.)
            cacheBuffer: box.rows.length <= 40 ? 1000000 : Math.round(box.maxHeight * 2)
            // (No current step: a ListView keeps its current one made.)
            currentIndex: -1
            keyNavigationEnabled: false
            clip: true
            interactive: contentHeight + 6 > height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {
                id: boxBar
                // (Shown while it's used or hovered: the prompt rail is beside it.)
                policy: boxFlick.contentHeight + 6 > boxFlick.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                onPressedChanged: { boxFlick.pinned = !pressed && boxFlick.atEnd(); boxFlick.restoreY = -1; if (!pressed) boxFlick.keep() }
            }
            // At its end until scrolled by hand, and again once scrolled back
            // there (its cards measure themselves after they're made).
            // (A run of one call opens at its top, its call's line in view.)
            // (Where it was left is kept by run: made again after scrolling
            // the transcript away and back, it opens there, not at its end.)
            readonly property var keptPlace: view.kept[box.group + "|box"]
            property bool pinned: keptPlace ? keptPlace.pinned : (box.rows || []).filter(r => r.kind !== "thinking").length !== 1
            property real restoreY: keptPlace && !keptPlace.pinned ? keptPlace.y : -1
            function atEnd() { return atYEnd || contentHeight + 6 <= height }
            function place() {
                if (pinned && height > 0) positionViewAtEnd()
                else if (restoreY >= 0 && height > 0 && contentHeight + 6 >= restoreY + height) { contentY = restoreY; restoreY = -1 }
            }
            function keep() { view.kept[box.group + "|box"] = { pinned: pinned, y: contentY } }
            onContentHeightChanged: place()
            onHeightChanged: place()
            onCountChanged: place()
            onMovementStarted: { pinned = false; restoreY = -1 }
            onMovementEnded: { pinned = atEnd(); keep() }
            // The run's line, from its first step to its last.
            Rectangle {
                parent: boxFlick.contentItem
                x: 7
                y: boxFlick.originY + 4
                width: 1
                height: Math.max(0, boxFlick.contentHeight - 12)
                color: Theme.border
            }
            delegate: Loader {
                id: cell
                required property var modelData
                required property int index
                // (For the keys: the list it's in.)
                readonly property var boxList: boxFlick
                width: boxFlick.width - (boxBar.visible ? 14 : 0)
                sourceComponent: modelData.kind === "thinking" ? thoughtCell : toolCell
                Component {
                    id: thoughtCell
                    ThoughtLine { width: cell.width; group: box.group; rowKey: cell.modelData.rowKey; text: cell.modelData.text; rowState: cell.modelData.rowState }
                }
                Component {
                    id: toolCell
                    ToolCard {
                        transcript: root
                        width: cell.width
                        rowKey: cell.modelData.rowKey
                        title: cell.modelData.title
                        subtitle: cell.modelData.subtitle
                        text: cell.modelData.text
                        rowState: cell.modelData.rowState
                        callId: cell.modelData.callId
                        result: cell.modelData.result
                        diff: cell.modelData.diff
                        took: cell.modelData.took
                        label: cell.modelData.label
                        command: cell.modelData.command
                        images: cell.modelData.images || ""
                        readsAfter: (view.count, root.session.readsAfter(box.group, cell.modelData.rowKey))
                        group: box.group
                    }
                }
            }
        }
        // More above or below: the edge fades out.
        Rectangle {
            width: parent.width - (boxBar.visible ? 10 : 0)
            height: 22
            visible: !boxFlick.atYBeginning
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.canvas }
                GradientStop { position: 1; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
            }
        }
        Rectangle {
            y: parent.height - height
            width: parent.width - (boxBar.visible ? 10 : 0)
            height: 22
            visible: !boxFlick.atEnd()
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
                GradientStop { position: 1; color: Theme.canvas }
            }
        }
        }
    }
    /// What an approval asks, in words (Harlo's request to run a command
    /// outside its sandbox says so).
    function approvalQuestion(tool, askLines) {
        const shell = /^(bash|shell|exec|command|run|terminal|local_bash)$/i.test(tool)
        const what = shell ? "this command" : Theme.toolLabel(tool).replace(/^\w/, c => c.toUpperCase())
        return askLines.indexOf("Outside the sandbox") >= 0 ? "Run " + what + " outside the sandbox?" : shell ? "Run this command?" : "Allow " + what + " to run?"
    }
    /// Row `i`'s end is on screen (an approval's buttons are there), its
    /// run open. (Re-read as the view scrolls: it reads `contentY`.)
    function rowEndInView(i) {
        if (i < 0) return false
        const item = view.itemAtIndex(i)
        if (!item || item.folded || !item.visible || item.height <= 0) return false
        const bottom = item.y + item.height - view.contentY
        return bottom > 24 && bottom <= view.height + 1
    }

    /// Brings row `i` to the middle and flashes it (search results, find).
    /// With `query`, its words are marked: for a few seconds, or with `keep`
    /// until `clearMarks` (find keeps them while it's open).
    function revealIndex(i, query, keep) {
        view.follow = false
        view.positionViewAtIndex(i, ListView.Center)
        Qt.callLater(() => {
            let item = view.itemAtIndex(i)
            if (!item) return
            // In a folded run, or its box: open it in full first.
            if (item.folded || item.boxed) {
                const r = Object.assign({}, view.runs)
                r[item.group] = "all"
                view.runs = r
                view.forceLayout()
                view.positionViewAtIndex(i, ListView.Center)
                item = view.itemAtIndex(i)
                if (!item) return
            }
            const p = item.mapToItem(root, 0, 0)
            flash.x = p.x; flash.y = p.y; flash.width = item.width; flash.height = item.height
            flashAnim.restart()
            root.markWords(item, query || "", keep === true)
        })
    }
    // ---- marked words (where a search or find landed) ----
    /// `[{x, y, w, h}]` in the list's content, so they scroll with it.
    property var wordMarks: []
    function textIn(item) {
        if (!item || !item.visible) return null
        if (item.positionToRectangle !== undefined && item.getText !== undefined && item.length > 0) return item
        for (let k = 0; k < item.children.length; k++) {
            const t = textIn(item.children[k])
            if (t) return t
        }
        return null
    }
    function markWords(item, query, keep) {
        const out = []
        const te = textIn(item)
        const words = query.toLowerCase().split(/\s+/).map(w => w.replace(/[\s"'“”‘’`*()\[\]{}<>,;:!?]/g, "")).filter(w => w !== "")
        if (te && words.length > 0) {
            const text = te.getText(0, te.length).toLowerCase()
            for (const w of words) {
                for (let at = text.indexOf(w); at >= 0 && out.length < 60; at = text.indexOf(w, at + w.length)) {
                    const a = te.positionToRectangle(at), b = te.positionToRectangle(at + w.length)
                    if (Math.abs(a.y - b.y) >= 1 || b.x <= a.x) continue
                    const p = te.mapToItem(view.contentItem, a.x, a.y)
                    out.push({ x: p.x, y: p.y, w: b.x - a.x, h: a.height })
                }
            }
        }
        wordMarks = out
        marksLayer.opacity = 1
        if (keep) marksFade.stop()
        else marksFade.restart()
    }
    function clearMarks() { marksFade.stop(); wordMarks = [] }
    /// The first row at the top of the view (find starts from it).
    function topIndex() { return Math.max(0, view.indexAt(view.width / 2, view.contentY + 12)) }
    Item {
        id: marksLayer
        objectName: "wordMarks"
        parent: view.contentItem
        z: 10
        Repeater {
            model: root.wordMarks
            Rectangle {
                required property var modelData
                x: modelData.x - 1
                y: modelData.y
                width: modelData.w + 2
                height: modelData.h
                radius: 3
                color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, Theme.isLight ? 0.35 : 0.3)
            }
        }
        SequentialAnimation {
            id: marksFade
            PauseAnimation { duration: 6000 }
            NumberAnimation { target: marksLayer; property: "opacity"; to: 0; duration: 800 }
            ScriptAction { script: root.wordMarks = [] }
        }
    }
    function copyToClipboard(t) { clipboardHelper.copyText(t) }
    /// Scrolls like the scrollbar does (sets contentY directly; for tests).
    function scrollBy(dy) {
        if (dy < 0) view.follow = false
        view.contentY = Math.max(view.originY - view.topMargin, view.contentY + dy)
    }
    readonly property bool atEnd: view.nearEnd()
    /// [row index, its y within the viewport] for the row at the viewport's middle (tests).
    /// How far past the bottom limit the view sits (positive = out of bounds; tests).
    function overscroll() { return view.contentY - (view.originY + view.contentHeight + view.bottomMargin - view.height) }
    // How many runs were folded by hand (for the click tests).
    function debugFoldedRuns() { return Object.keys(view.runs).filter(k => view.runs[k] === false).length }
    function debugContentY() { return view.contentY.toFixed(1) + " originY " + view.originY.toFixed(1) + " h " + view.contentHeight.toFixed(0) }
    function probe() {
        const idx = view.indexAt(view.width / 2, view.contentY + view.height / 2)
        const item = idx >= 0 ? view.itemAtIndex(idx) : null
        return item ? [idx, item.y - view.contentY] : [-1, 0]
    }
    function positionAt(index) { view.follow = false; view.positionViewAtIndex(index, ListView.Center) }

    // ---- prompt rail: your messages, to jump between ----------------------
    property bool promptRail: true
    /// The rail's keyboard mode ended; focus can go back to the composer.
    signal promptRailFinished()
    property var prompts: []
    /// Index into `prompts` of the turn at the top of the view.
    property int activePrompt: -1
    readonly property alias rail: rail
    function refreshPrompts() {
        prompts = JSON.parse(session.userPrompts())
        updateActivePrompt()
    }
    function updateActivePrompt() {
        if (prompts.length === 0) { activePrompt = -1; return }
        let row = -1
        for (const y of [24, 40, 64, view.height / 4]) {
            row = view.indexAt(view.width / 2, view.contentY + y)
            if (row >= 0) break
        }
        if (row < 0) return
        // The last prompt at or above that row.
        let lo = 0, hi = prompts.length - 1, found = 0
        while (lo <= hi) {
            const m = (lo + hi) >> 1
            if (prompts[m].i <= row) { found = m; lo = m + 1 } else hi = m - 1
        }
        activePrompt = found
    }
    /// Puts row `i` at the top of the view.
    function jumpToRow(i) {
        view.follow = false
        view.positionViewAtIndex(i, ListView.Beginning)
        // Rows above may only now have been measured: settle once more.
        Qt.callLater(() => {
            view.positionViewAtIndex(i, ListView.Beginning)
            view.contentY = Math.max(view.originY - view.topMargin, view.contentY - view.spacing)
            root.updateActivePrompt()
        })
    }
    /// Previous (-1) or next (+1) prompt. Back from partway into a turn goes
    /// to that turn's start first; forward past the last goes to the end.
    function stepPrompt(delta) {
        if (prompts.length === 0) return
        updateActivePrompt()
        const k = Math.max(0, activePrompt)
        let target = k + delta
        if (delta < 0) {
            const item = view.itemAtIndex(prompts[k].i)
            if (!(item && item.y >= view.contentY - 4)) target = k
        }
        if (target >= prompts.length) { scrollToEnd(); return }
        if (target < 0) target = 0
        jumpToRow(prompts[target].i)
    }
    function openPromptRail() { if (promptRail) rail.openKeyboard() }
    Timer { id: promptsTimer; interval: 150; onTriggered: root.refreshPrompts() }
    Component.onCompleted: promptsTimer.restart()

    // Copies text through a hidden TextEdit (QML has no clipboard API).
    TextEdit {
        id: clipboardHelper
        visible: false
        // Not named `copy`: that would shadow TextEdit.copy() and recurse.
        function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" }
    }

    ListView {
        id: view
        anchors.fill: parent
        clip: true
        model: root.session
        // Rows space themselves (gapAbove); the kinds of their neighbours
        // come from sections (no section headers).
        spacing: 0
        section.property: "rowSection"
        cacheBuffer: 1600
        topMargin: 16
        bottomMargin: 16
        boundsBehavior: Flickable.StopAtBounds
        // Whole-pixel scroll positions: smooth wheel scrolling moves in
        // fractional pixels, which makes text shimmer by a pixel.
        pixelAligned: true
        // Grabbing the scrollbar always means "stop following".
        ScrollBar.vertical: ScrollBar { id: vbar; onPressedChanged: if (pressed) view.follow = false }
        // A new message, card or reply fades in (not a whole session as it
        // opens: that's a reset): each row itself (AddFade), not the list's
        // add transition, which kept a row added as the one before it grew
        // (a call turning into an approval card) at that one's old height,
        // over its buttons.

        // Expanded state of thinking/tool rows, keyed by row key, so it
        // survives delegate destruction while scrolling.
        property var expanded: ({})
        function toggle(key) { const e = Object.assign({}, view.expanded); e[key] = !e[key]; view.expanded = e }
        // Boxes wrapped by hand (their Wrap), by row key.
        property var wraps: ({})
        function setWrap(key, on) { const w = Object.assign({}, view.wraps); if (on) w[key] = true; else delete w[key]; view.wraps = w }
        // Choices made inside rows (Code or Diagram, a mockup's before or
        // after, where a run's box was scrolled to), by key: read when a row
        // is made again after scrolling away. Changed in place, not
        // reassigned: only rows being made read it, so nothing else re-runs.
        property var kept: ({})
        // Runs folded or opened by hand, keyed by run.
        property var runs: ({})
        function toggleRun(group, dflt) { const r = Object.assign({}, view.runs); r[group] = !root.runOpen(group, dflt); view.runs = r }

        // Follow the end while new content streams in; stop when the user
        // scrolls up, resume when they return to the bottom.
        property bool follow: true
        property bool autoScrolling: false
        // Rows use the full width minus side gutters that grow with the
        // window (16 px narrow, up to 64 px wide), so the chat follows
        // resizes; wider than a comfortable reading width, the column stays
        // that wide, centred.
        // (With the prompt rail at the right, room for it: it never sits on the text.)
        // (Whole pixels: text at a fractional x snaps to the pixel grid
        // differently as things around it repaint, and jumps a pixel on hover.)
        readonly property real gutter: Math.floor(Math.max(rail.visible ? 36 : 16, Math.min(64, width * 0.04), (width - Theme.px(Theme.chatMaxWidth)) / 2))
        readonly property real contentWidthLimit: Math.max(120, width - 2 * gutter)
        // A turn's rows use the whole column (the avatars sit beside the
        // names), so they line up with the composer under them.
        readonly property real columnX: Math.round((width - contentWidthLimit) / 2)
        readonly property real columnWidth: Math.max(80, contentWidthLimit)
        function scrollToEnd() {
            autoScrolling = true
            positionViewAtEnd()
            // positionViewAtEnd stops short of the bottom margin; go all the way.
            contentY = Math.max(originY - topMargin, originY + contentHeight + bottomMargin - height)
            autoScrolling = false
        }
        // Bottom within a small tolerance. originY matters: with variable
        // row heights ListView's origin drifts away from 0 in long lists.
        function nearEnd() { return contentY + height >= originY + contentHeight + bottomMargin - 48 }
        // Deferred, so several changes in one frame scroll once. Re-check
        // `follow`: the user may have scrolled away in the meantime. Not
        // while you drag, flick or turn the wheel: that would undo it.
        function followEnd() { if (follow && !moving) scrollToEnd() }
        // Following stops only by your doing: scrolling up (a drag, a flick
        // or the wheel: `moving`), the scrollbar or a jump (they say so).
        // It starts again when a scroll down ends at the bottom, the
        // scrollbar is taken there, or Latest. ListView moves contentY itself
        // too, by a pixel or more as it measures rows: never taken for yours
        // (that stopped the following mid-reply, and pulled a first scroll up
        // a long session back down).
        property real lastY: 0
        property real moveStart: 0
        onMovementStarted: moveStart = contentY
        onMovementEnded: {
            if (contentY > moveStart && nearEnd()) follow = true
            followEnd()
        }
        onContentYChanged: {
            if (!autoScrolling) {
                if (moving) {
                    if (contentY < lastY) follow = false
                    // A scroll down that reaches the end (as the reply
                    // still grows below it: no waiting for it to stop).
                    else if (contentY > moveStart && nearEnd()) follow = true
                } else if (vbar.pressed) {
                    follow = nearEnd()
                } else if (follow && !nearEnd()) {
                    // (Moved off the end by ListView: back to it.)
                    Qt.callLater(followEnd)
                }
            }
            lastY = contentY
            Qt.callLater(root.updateActivePrompt)
        }
        function noteContent() { if (follow) Qt.callLater(followEnd) }
        Connections {
            target: root.session
            function onRowsInserted() { view.noteContent(); promptsTimer.restart() }
            function onRowsRemoved() { view.noteContent(); promptsTimer.restart() }
            function onDataChanged() { view.noteContent() }
            function onModelReset() { view.noteContent(); promptsTimer.restart() }
        }
        // Any growth while following, new text or not (a row measuring
        // itself, a running tool's line): it stays at the end. (Scrolling up
        // has stopped the following first, so a row measured then doesn't
        // pull the view back.)
        onContentHeightChanged: if (follow) Qt.callLater(followEnd)


        footer: Item {
            width: view.width
            height: root.session.status === "busy" ? 36 : latestUndoButton.visible ? 46 : 0
            // (Following the end: the button comes into view.)
            onHeightChanged: if (view.follow) Qt.callLater(view.followEnd)
            // The latest turn changed files: undo them, right under the reply.
            FlatButton {
                id: latestUndoButton
                objectName: "undoLatest"
                readonly property var turn: root.latestUndo !== "" ? root.undoable[root.latestUndo] : null
                visible: root.session.status !== "busy" && turn !== null
                x: view.columnX
                anchors.verticalCenter: parent.verticalCenter
                text: turn ? "Undo these changes (" + (turn.files === 1 ? "1 file" : turn.files + " files") + ")" : ""
                font.pixelSize: Theme.px(12.5)
                ToolTip.visible: hovered
                ToolTip.delay: 400
                ToolTip.text: "Put the folder's files back as they were before this turn"
                onClicked: root.undoRequested(root.latestUndo)
            }
            Label {
                id: workingLabel
                objectName: "workingLabel"
                // The run is paused on you: say so rather than "Working…".
                readonly property bool waiting: root.session.hasRequest
                // (Not when the request is on screen: its card or the
                // question panel says it.)
                readonly property bool shownElsewhere: waiting && (!root.session.requestIsApproval || root.session.approvalRow !== "")
                visible: root.session.status === "busy" && !shownElsewhere
                x: view.columnX
                anchors.verticalCenter: parent.verticalCenter
                text: !waiting ? "Working…" : root.session.requestIsApproval ? "Waiting for your approval" : "Waiting for your answer"
                color: waiting ? Theme.warning : Theme.primary
                // (Pulsing only while it shows, in a window that's shown;
                // stopped, it's back at full opacity.)
                SequentialAnimation on opacity {
                    running: workingLabel.visible && root.windowShown
                    loops: Animation.Infinite
                    onStopped: workingLabel.opacity = 1
                    NumberAnimation { to: 0.35; duration: 700 }
                    NumberAnimation { to: 1; duration: 700 }
                }
            }
        }

        delegate: DelegateChooser {
            role: "kind"

            DelegateChoice {
                roleValue: "user"
                Item {
                    id: userRow
                    ListView.onAdd: if (Theme.motion > 0) userRowFade.start()
                    AddFade { id: userRowFade; target: userRow }
                    required property int index
                    required property string rowKey
                    required property string text
                    required property string title
                    required property string subtitle
                    required property string images
                    // Attached file paths, one per line.
                    readonly property var files: title !== "" ? title.split("\n") : []
                    readonly property real gap: root.gapAbove("user", ListView.previousSection)
                    width: view.width
                    implicitHeight: bubble.height + gap
                    HoverHandler { id: userHover }
                    // Edit & resend, on hover.
                    Row {
                        z: 2
                        visible: userHover.hovered && ["claude", "harlo"].includes(root.session.harness)
                        anchors.right: bubble.right
                        anchors.rightMargin: 6
                        anchors.top: bubble.top
                        anchors.topMargin: 6
                        spacing: 4
                        FlatButton {
                            text: "Edit & resend"
                            font.pixelSize: Theme.px(11)
                            padding: 3
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: "Fork into a new session from here and put this message back in the composer"
                            onClicked: root.editRequested(userRow.index)
                        }
                    }
                    // Between turns: a hairline.
                    Rectangle {
                        visible: userRow.index > 0
                        x: view.columnX
                        y: Math.round(userRow.gap / 2) - 6
                        width: view.contentWidthLimit
                        height: 1
                        color: Theme.hairline
                    }
                    // The end of the turn before: undo it (and those after it).
                    FlatButton {
                        readonly property string turn: root.undoBefore(userRow.rowKey)
                        objectName: "undoTurn-" + userRow.index
                        visible: turn !== ""
                        // (On the line between turns while you point at it.)
                        readonly property bool shown: userHover.hovered || hovered || activeFocus
                        opacity: shown ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.hoverMotion } }
                        x: view.columnX - 2
                        y: Math.round(userRow.gap / 2) - 6 - height / 2
                        text: "Undo from here"
                        outlined: false
                        padding: 2
                        accent: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        background: Rectangle { color: Theme.background; radius: 6 }
                        ToolTip.visible: hovered
                        ToolTip.delay: 400
                        ToolTip.text: "Put the folder's files back as they were before this turn (this turn and the ones after it)"
                        onClicked: root.undoRequested(turn)
                    }
                    Item {
                        id: bubble
                        x: view.columnX
                        y: userRow.gap
                        width: view.columnWidth
                        height: userCol.implicitHeight + 6
                        Column {
                            id: userCol
                            x: 0
                            y: 0
                            width: parent.width
                            spacing: 6
                            Row {
                                spacing: 8
                                // Your avatar (picked from the left rail).
                                UserAvatar {
                                    objectName: "turnAvatar"
                                    width: Theme.px(20)
                                    height: width
                                }
                                Label {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "You"
                                    color: Theme.text
                                    font.pixelSize: Theme.px(13.5)
                                    font.weight: Font.DemiBold
                                }
                            }
                            PictureStrip {
                                width: parent.width
                                labelOf: p => root.pictureLabel(p)
                                paths: userRow.images !== "" ? userRow.images.split("\n") : []
                                onOpenRequested: path => root.showPicture(path)
                            }
                            Flow {
                                width: parent.width
                                spacing: 6
                                visible: userRow.files.length > 0
                                Repeater {
                                    model: userRow.files
                                    AttachmentChip { required property string modelData; path: modelData }
                                }
                            }
                            TextEdit {
                                font.family: Theme.uiFont
                                width: parent.width
                                visible: text !== ""
                                text: userRow.text
                                color: Theme.text
                                selectionColor: Theme.selection
                                selectedTextColor: Theme.foreground
                                readOnly: true
                                selectByMouse: true
                                wrapMode: TextEdit.Wrap
                                font.pixelSize: Theme.px(15)
                            }
                            // Tags: image count, "Ask first".
                            Row {
                                visible: userRow.subtitle !== ""
                                spacing: 6
                                Repeater {
                                    model: userRow.subtitle !== "" ? userRow.subtitle.split(" · ") : []
                                    Rectangle {
                                        required property string modelData
                                        implicitWidth: tagRow.implicitWidth + 12
                                        implicitHeight: 20
                                        radius: 10
                                        color: Theme.subtle
                                        border.color: Theme.border
                                        Row {
                                            id: tagRow
                                            anchors.centerIn: parent
                                            spacing: 4
                                            Icon { visible: modelData === "Ask first"; anchors.verticalCenter: parent.verticalCenter; name: "question"; color: Theme.primary; implicitWidth: 12; implicitHeight: 12 }
                                            Label { anchors.verticalCenter: parent.verticalCenter; text: modelData; color: Theme.muted; font.pixelSize: Theme.px(11) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            DelegateChoice {
                roleValue: "markdown"
                Item {
                    id: mdRow
                    required property string rowModel
                    ListView.onAdd: if (Theme.motion > 0) mdRowFade.start()
                    AddFade { id: mdRowFade; target: mdRow }
                    required property string html
                    required property string rowKey
                    required property int index
                    required property string label
                    // A piece of a long table after its first: right under it.
                    readonly property bool more: label === "part-middle" || label === "part-last"
                    readonly property real gap: more ? 0 : root.gapAbove("markdown", ListView.previousSection)
                    // The code block (in a list) just copied: its link says "Copied" a moment.
                    property int copiedBlock: -1
                    Timer { id: blockCopiedTimer; interval: 1200; onTriggered: mdRow.copiedBlock = -1 }
                    HeadSlot { replyModel: mdRow.rowModel; active: !mdRow.more && root.opensTurn(mdRow.ListView.previousSection); y: mdRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: md.implicitHeight + gap
                    TextEdit {
                        font.family: Theme.uiFont
                        id: md
                        x: view.columnX
                        y: mdRow.gap
                        width: view.columnWidth
                        textFormat: TextEdit.RichText
                        text: Theme.htmlStyle + Theme.proseHtml(parent.html, mdRow.copiedBlock, root.opensTurn(mdRow.ListView.previousSection))
                        color: Theme.foreground
                        selectionColor: Theme.selection
                        selectedTextColor: Theme.foreground
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        font.pixelSize: Theme.px(15)
                        // (A code block in a list: its Copy link.)
                        onLinkActivated: link => {
                            if (link.startsWith("harlo-copy:")) {
                                const n = parseInt(link.substring(11))
                                clipboardHelper.copyText(Theme.proseCode(mdRow.html, n))
                                mdRow.copiedBlock = n
                                blockCopiedTimer.restart()
                            } else Qt.openUrlExternally(link)
                        }
                        HoverHandler { cursorShape: md.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
                    }
                }
            }

            // A reply's pictures (Markdown images, files it names, ones the
            // model made).
            DelegateChoice {
                roleValue: "images"
                Item {
                    id: picsRow
                    required property string rowModel
                    required property string images
                    ListView.onAdd: if (Theme.motion > 0) picsRowFade.start()
                    AddFade { id: picsRowFade; target: picsRow }
                    readonly property real gap: root.gapAbove("images", ListView.previousSection)
                    HeadSlot { replyModel: picsRow.rowModel; active: root.opensTurn(picsRow.ListView.previousSection); y: picsRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: strip.paths.length > 0 ? strip.height + picsRow.gap : 0
                    visible: implicitHeight > 0
                    PictureStrip {
                        id: strip
                        x: view.columnX
                        y: picsRow.gap
                        width: view.columnWidth
                        side: 160
                        labelOf: p => root.pictureLabel(p)
                        paths: picsRow.images !== "" ? picsRow.images.split("\n") : []
                        onOpenRequested: path => root.showPicture(path)
                    }
                }
            }

            DelegateChoice {
                roleValue: "code"
                Item {
                    id: codeRow
                    required property string rowModel
                    required property string rowKey
                    ListView.onAdd: if (Theme.motion > 0) codeRowFade.start()
                    AddFade { id: codeRowFade; target: codeRow }
                    required property string html
                    required property string text
                    required property string title
                    required property string label
                    required property string subtitle
                    required property string summary
                    // A piece of a long block after its first: under the one
                    // before, one card with it (no header, gap or corners
                    // between), its numbers going on; wrapped as the first is.
                    readonly property bool more: label === "part-middle" || label === "part-last"
                    readonly property bool goesOn: label === "part-first" || label === "part-middle"
                    readonly property string wrapKey: summary !== "" ? summary : rowKey
                    readonly property real gap: more ? 0 : root.gapAbove("code", ListView.previousSection)
                    // A Mermaid diagram: drawn (Diagrams), its code a click away.
                    readonly property bool mermaid: title.toLowerCase() === "mermaid"
                    property bool showCode: view.kept[wrapKey + "|code"] === true
                    onShowCodeChanged: view.kept[wrapKey + "|code"] = showCode
                    readonly property var diagram: mermaid && Diagrams.revision >= 0
                        ? JSON.parse(Diagrams.diagram(text, Theme.isLight, JSON.stringify({ background: String(Theme.canvas), surface: String(Theme.raised), text: String(Theme.foreground),
                                                                                             muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
                        : null
                    readonly property bool drawn: diagram !== null && diagram.state === "ready" && !showCode
                    // Options drawn as cards, a mockup as its picture (Visuals);
                    // their code a click away too. (A mockup's file is looked
                    // at again every few seconds: an edit shows.)
                    readonly property string kind: title.toLowerCase()
                    readonly property var options: kind === "options" ? JSON.parse(Visuals.options(text)) : null
                    property int mockTick: 0
                    readonly property var mock: kind === "mockup" && Visuals.revision >= 0 && mockTick >= 0 ? JSON.parse(Visuals.mockup(text, root.session.cwd || "")) : null
                    readonly property bool canShow: (options !== null && !options.error) || (mock !== null && !mock.error)
                    readonly property bool visual: canShow && !showCode
                    property bool showBefore: view.kept[wrapKey + "|before"] === true
                    onShowBeforeChanged: view.kept[wrapKey + "|before"] = showBefore
                    Timer { interval: 2500; repeat: true; running: codeRow.mock !== null && codeRow.visual && root.windowShown; onTriggered: codeRow.mockTick++ }
                    HeadSlot { replyModel: codeRow.rowModel; active: !codeRow.more && root.opensTurn(codeRow.ListView.previousSection); y: codeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: codeBox.height + gap
                    Rectangle {
                        id: codeBox
                        x: view.columnX
                        y: codeRow.gap
                        width: view.columnWidth
                        objectName: "replyCode"
                        HoverHandler { id: blockHover }
                        height: codeRule.y + codeRule.height + (codeRow.drawn ? diagramBox.height : codeRow.visual ? visualBox.height : blockCode.height) + (diagramNote.visible ? diagramNote.height : 0)
                        // (The same well as a tool's code: its header, a rule, the
                        // numbers on their own column.)
                        topLeftRadius: codeRow.more ? 0 : 6
                        topRightRadius: codeRow.more ? 0 : 6
                        bottomLeftRadius: codeRow.goesOn ? 0 : 6
                        bottomRightRadius: codeRow.goesOn ? 0 : 6
                        // (Cards or a picture aren't code: on the page, in a
                        // hairline frame; their code goes back in the well.)
                        color: codeRow.visual || codeRow.drawn ? "transparent" : Theme.codeBg
                        border.width: codeRow.visual || codeRow.drawn ? 1 : 0
                        border.color: Theme.hairline
                        RowLayout {
                            id: codeHeader
                            width: parent.width
                            height: codeRow.more ? 0 : 28
                            visible: !codeRow.more
                            Label { Layout.leftMargin: 10; text: codeRow.kind === "options" ? "Options" : codeRow.kind === "mockup" ? "Mockup" : codeRow.mermaid ? "Diagram" : root.session.languageName(codeRow.title); color: Theme.muted; font.pixelSize: Theme.px(12) }
                            Item { Layout.fillWidth: true }
                            FlatButton {
                                visible: (codeRow.mermaid && codeRow.diagram !== null && codeRow.diagram.state === "ready") || codeRow.canShow
                                Layout.preferredHeight: 22
                                text: !codeRow.showCode ? "Code" : codeRow.mermaid ? "Diagram" : codeRow.options !== null ? "Cards" : "Picture"
                                font.pixelSize: Theme.px(11)
                                onClicked: codeRow.showCode = !codeRow.showCode
                            }
                            FlatButton {
                                // (Narrow, it's in the full-size view the picture opens.)
                                visible: (codeRow.mermaid || (codeRow.mock !== null && !codeRow.mock.error)) && codeHeader.width >= 480
                                Layout.preferredHeight: 22
                                text: "Open in browser"
                                font.pixelSize: Theme.px(11)
                                onClicked: codeRow.mermaid ? Diagrams.openInBrowser(codeRow.text, Theme.isLight) : Visuals.openMockup(codeRow.text, root.session.cwd || "", codeRow.showBefore)
                            }
                            CodeCopy {
                                Layout.rightMargin: wrapBlock.visible ? 0 : 6
                                Layout.preferredHeight: 22
                                // (On hover, as a tool's: the language stays.)
                                opacity: blockHover.hovered || copied || activeFocus ? 1 : 0
                                copyText: codeRow.text
                            }
                            // (At the edge, so it never stands beside Copy's empty place.)
                            WrapButton { id: wrapBlock; list: view; Layout.rightMargin: 6; Layout.preferredHeight: 22; key: codeRow.wrapKey; code: blockCode }
                        }
                        Rectangle { id: codeRule; y: codeHeader.height; width: parent.width; height: codeRow.more ? 0 : 1; visible: !codeRow.more; color: codeRow.visual || codeRow.drawn ? Theme.hairline : Theme.codeRule }
                        // Drawing it, or why it isn't drawn.
                        Label {
                            id: diagramNote
                            anchors.top: codeRule.bottom
                            x: 10
                            width: parent.width - 20
                            visible: codeRow.mermaid && codeRow.diagram !== null && codeRow.diagram.state !== "ready"
                            height: visible ? implicitHeight + 4 : 0
                            text: !codeRow.diagram ? "" : codeRow.diagram.state === "drawing" ? "Drawing the diagram…" : (codeRow.diagram.error || "")
                            color: codeRow.diagram && codeRow.diagram.state === "failed" ? Theme.warning : Theme.muted
                            font.pixelSize: Theme.px(11.5)
                            wrapMode: Text.Wrap
                        }
                        // The picture, fitted to the column (the chat scrolls past
                        // it; clicking it opens it full size).
                        // (Made only for a diagram drawn; as are the cards and
                        // the mockup below: a code row is mostly code.)
                        Loader {
                            id: diagramBox
                            anchors.top: codeRule.bottom
                            active: codeRow.drawn
                            width: parent.width
                            sourceComponent: Item {
                                objectName: "diagram"
                                height: diagramImage.height + 20
                                Image {
                                    id: diagramImage
                                    readonly property real fit: codeRow.drawn ? Math.min(1, (diagramBox.width - 20) / Math.max(1, codeRow.diagram.width)) : 1
                                    x: (diagramBox.width - width) / 2
                                    y: 10
                                    source: codeRow.drawn ? codeRow.diagram.url : ""
                                    width: codeRow.drawn ? Math.round(codeRow.diagram.width * fit) : 0
                                    height: codeRow.drawn ? Math.round(codeRow.diagram.height * fit) : 0
                                    sourceSize.width: width * 2
                                    fillMode: Image.PreserveAspectFit
                                    smooth: true
                                    mipmap: true
                                    asynchronous: true
                                    cache: false
                                    HoverHandler { id: diagramHover; cursorShape: Qt.PointingHandCursor }
                                    TapHandler { onTapped: root.viewDiagram(codeRow.diagram, codeRow.text) }
                                    // (From the keyboard too: Tab to it, Enter or Space.)
                                    activeFocusOnTab: true
                                    Keys.onReturnPressed: root.viewDiagram(codeRow.diagram, codeRow.text)
                                    Keys.onSpacePressed: root.viewDiagram(codeRow.diagram, codeRow.text)
                                    Accessible.role: Accessible.Button
                                    Accessible.name: "Diagram. Open full size"
                                    Rectangle {
                                        anchors.fill: parent
                                        anchors.margins: -3
                                        visible: diagramImage.activeFocus
                                        color: "transparent"
                                        border.width: 2
                                        border.color: Theme.primary
                                        radius: 4
                                    }
                                    ToolTip.visible: diagramHover.hovered
                                    ToolTip.text: "Open full size"
                                    ToolTip.delay: 600
                                }
                            }
                        }
                        // The options' cards, or the mockup's picture.
                        Loader {
                            id: visualBox
                            anchors.top: codeRule.bottom
                            active: codeRow.visual
                            width: parent.width
                            sourceComponent: Item {
                                objectName: "replyVisual"
                                height: codeRow.options !== null ? optionCards.implicitHeight + 20 : mockupView.implicitHeight
                                OptionsCards {
                                    id: optionCards
                                    visible: codeRow.options !== null
                                    x: 10
                                    y: 10
                                    width: parent.width - 20
                                    choices: codeRow.options && codeRow.options.choices ? codeRow.options.choices : []
                                    // (Looked at again as rows come: your answer is one.)
                                    readonly property var after: visible && root.count >= 0 ? JSON.parse(root.session.answerAfter(codeRow.rowKey)) : null
                                    answer: after ? after.text : ""
                                    choosing: after !== null && !after.answered && root.session.status !== "busy" && !root.session.hasRequest
                                    onChosen: name => root.session.send(name, "normal")
                                }
                                MockupView {
                                    id: mockupView
                                    objectName: "mockupView"
                                    visible: codeRow.mock !== null
                                    width: parent.width
                                    mock: codeRow.mock && !codeRow.mock.error ? codeRow.mock : null
                                    showBefore: codeRow.showBefore
                                    onOpenRequested: before => root.viewMockup(codeRow.mock, codeRow.text, before)
                                    onBeforeChosen: before => codeRow.showBefore = before
                                }
                            }
                        }
                        Rectangle {
                            visible: !codeRow.drawn && !codeRow.visual
                            anchors.top: blockCode.top
                            width: blockCode.gutterWidth - 6
                            height: blockCode.height
                            color: Theme.codeGutter
                            bottomLeftRadius: codeRow.goesOn ? 0 : 6
                        }
                        CodeText {
                            id: blockCode
                            objectName: "replyCodeView"
                            anchors.top: diagramNote.visible ? diagramNote.bottom : codeRule.bottom
                            visible: !codeRow.drawn && !codeRow.visual
                            width: parent.width
                            html: codeRow.html
                            wrap: Theme.wrapCode || view.wraps[codeRow.wrapKey] === true
                            fontSize: Theme.mpx(13)
                            firstNumber: codeRow.more && codeRow.subtitle !== "" ? Number(codeRow.subtitle) : 1
                            minDigits: codeRow.label !== "" ? 4 : 1
                            topPad: codeRow.more ? 0 : 8
                            bottomPad: codeRow.goesOn ? 0 : 8
                        }
                    }
                }
            }

            DelegateChoice {
                roleValue: "thinking"
                Item {
                    id: thinkRow
                    required property string rowModel
                    ListView.onAdd: if (Theme.motion > 0) thinkRowFade.start()
                    AddFade { id: thinkRowFade; target: thinkRow }
                    required property string rowKey
                    required property string text
                    required property string rowState
                    required property string group
                    required property string summary
                    required property bool groupOpen
                    readonly property string mode: root.runMode(group, groupOpen)
                    // Folded, or in its run's box (the run's first row draws
                    // its summary line or the box; the others take no room).
                    readonly property bool folded: mode === "folded"
                    readonly property bool boxed: mode === "boxed"
                    readonly property real gap: root.gapAbove("thinking", ListView.previousSection)
                    readonly property real head: summary !== "" ? 30 : 0
                    HeadSlot { replyModel: thinkRow.rowModel; active: root.opensTurn(thinkRow.ListView.previousSection); y: thinkRow.gap - height - 10 }
                    // A run's line through its rows, open (a box draws its own).
                    Rectangle {
                        readonly property bool first: !root.isActivity(thinkRow.ListView.previousSection)
                        readonly property bool last: !root.isActivity(thinkRow.ListView.nextSection)
                        // (One step alone has no line.)
                        visible: thinkRow.implicitHeight > 0 && !thinkRow.folded && !thinkRow.boxed && !(first && last)
                        z: -1
                        x: view.columnX + 7
                        width: 1
                        y: first ? thinkRow.gap + thinkRow.head + 10 : 0
                        height: Math.max(0, thinkRow.implicitHeight - y - (last ? 14 : 0))
                        color: Theme.border
                    }
                    width: view.width
                    implicitHeight: thinkRow.boxed ? (head > 0 ? gap + thinkBox.implicitHeight + 4 : 0)
                                  : thinkRow.folded ? (head > 0 ? gap + head : 0)
                                  : (thinkLine.item ? thinkLine.item.implicitHeight : 0) + gap + head + 10
                    visible: implicitHeight > 0
                    // (Made only on a run's first row: the others, or a
                    // boxed run's, would each hold a hidden one.)
                    Loader {
                        active: thinkRow.head > 0 && !thinkRow.boxed
                        x: view.columnX
                        y: thinkRow.gap
                        width: view.columnWidth
                        sourceComponent: RunHeader { group: thinkRow.group; summary: thinkRow.summary; dflt: thinkRow.groupOpen }
                    }
                    Loader {
                        id: thinkBox
                        active: thinkRow.boxed && thinkRow.head > 0
                        x: view.columnX
                        y: thinkRow.gap
                        width: view.columnWidth
                        sourceComponent: RunBox { width: thinkBox.width; group: thinkRow.group; summary: thinkRow.summary; dflt: thinkRow.groupOpen }
                    }
                    // (Made only while it shows on its own: folded or in its
                    // run's box, a long run's rows would each hold one.)
                    Loader {
                        id: thinkLine
                        active: !thinkRow.folded && !thinkRow.boxed
                        x: view.columnX
                        y: thinkRow.gap + thinkRow.head + 5
                        width: view.columnWidth
                        sourceComponent: ThoughtLine {
                            group: thinkRow.group
                            width: thinkLine.width
                            rowKey: thinkRow.rowKey
                            text: thinkRow.text
                            rowState: thinkRow.rowState
                        }
                    }
                }
            }

            DelegateChoice {
                roleValue: "tool"
                Item {
                    id: toolRow
                    required property string rowModel
                    ListView.onAdd: if (Theme.motion > 0) toolRowFade.start()
                    AddFade { id: toolRowFade; target: toolRow }
                    required property string rowKey
                    required property string text
                    required property string title
                    required property string subtitle
                    required property string rowState
                    required property string callId
                    required property string group
                    required property string summary
                    required property bool groupOpen
                    required property string result
                    required property string diff
                    required property real took
                    required property string label
                    required property string command
                    required property string images
                    // The agent waits for your approval to run this call.
                    readonly property bool asking: root.session.hasRequest && root.session.requestIsApproval && root.session.approvalRow === rowKey
                    // (Over the rows after it: its buttons are never covered.)
                    z: asking ? 2 : 0
                    readonly property var askLines: asking ? root.session.requestTitle.split("\n").slice(1) : []
                    // Folded, or in its run's box (see the thinking row); a
                    // call that asks always shows.
                    readonly property string mode: asking ? "rows" : root.runMode(group, groupOpen)
                    readonly property bool folded: mode === "folded"
                    readonly property bool boxed: mode === "boxed"
                    readonly property real gap: root.gapAbove("tool", ListView.previousSection)
                    readonly property real head: summary !== "" ? 30 : 0
                    HeadSlot { replyModel: toolRow.rowModel; active: root.opensTurn(toolRow.ListView.previousSection); y: toolRow.gap - height - 10 }
                    // A run's line through its rows, open (a box draws its own).
                    Rectangle {
                        readonly property bool first: !root.isActivity(toolRow.ListView.previousSection)
                        readonly property bool last: !root.isActivity(toolRow.ListView.nextSection)
                        // (One step alone has no line.)
                        visible: toolRow.implicitHeight > 0 && !toolRow.folded && !toolRow.boxed && !(first && last)
                        z: -1
                        x: view.columnX + 7
                        width: 1
                        y: first ? toolRow.gap + toolRow.head + 10 : 0
                        height: Math.max(0, toolRow.implicitHeight - y - (last ? 14 : 0))
                        color: Theme.border
                    }
                    width: view.width
                    implicitHeight: toolRow.boxed ? (head > 0 ? gap + toolRunBox.implicitHeight + 4 : 0)
                                  : toolRow.folded ? (head > 0 ? gap + head : 0)
                                  : toolBox.height + gap + head
                    visible: implicitHeight > 0
                    // (Made only on a run's first row: the others, or a
                    // boxed run's, would each hold a hidden one.)
                    Loader {
                        active: toolRow.head > 0 && !toolRow.boxed
                        x: view.columnX
                        y: toolRow.gap
                        width: view.columnWidth
                        sourceComponent: RunHeader { group: toolRow.group; summary: toolRow.summary; dflt: toolRow.groupOpen }
                    }
                    Loader {
                        id: toolRunBox
                        active: toolRow.boxed && toolRow.head > 0
                        x: view.columnX
                        y: toolRow.gap
                        width: view.columnWidth
                        sourceComponent: RunBox { width: toolRunBox.width; group: toolRow.group; summary: toolRow.summary; dflt: toolRow.groupOpen }
                    }
                    Rectangle {
                        id: toolBox
                        visible: !toolRow.folded && !toolRow.boxed
                        x: view.columnX - 8
                        y: toolRow.gap + toolRow.head
                        width: view.columnWidth + 8
                        height: (toolCard.item ? toolCard.item.implicitHeight : 0) + (toolRow.asking ? 20 : 10)
                        radius: 8
                        // (Solid when it asks: a row under it doesn't show through.)
                        color: toolRow.asking ? Qt.tint(Theme.raised, Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.06)) : "transparent"
                        border.color: toolRow.asking ? Theme.warning : "transparent"
                        // (Made only while it shows on its own: see the thinking row.)
                        Loader {
                            id: toolCard
                            active: !toolRow.folded && !toolRow.boxed
                            x: 8
                            y: toolRow.asking ? 10 : 5
                            width: parent.width - 16
                            sourceComponent: ToolCard {
                                transcript: root
                                width: toolCard.width
                                rowKey: toolRow.rowKey
                                title: toolRow.title
                                subtitle: toolRow.subtitle
                                text: toolRow.text
                                rowState: toolRow.rowState
                                callId: toolRow.callId
                                result: toolRow.result
                                diff: toolRow.diff
                                took: toolRow.took
                                label: toolRow.label
                                command: toolRow.command
                                images: toolRow.images
                                asking: toolRow.asking
                                askLines: toolRow.askLines
                                readsAfter: (view.count, root.session.readsAfter(toolRow.group, toolRow.rowKey))
                                group: toolRow.group
                            }
                        }
                    }
                }
            }

            DelegateChoice {
                roleValue: "notice"
                Item {
                    id: noticeRow
                    required property int index
                    required property string rowModel
                    ListView.onAdd: if (Theme.motion > 0) noticeRowFade.start()
                    AddFade { id: noticeRowFade; target: noticeRow }
                    required property string rowKey
                    required property string text
                    required property string title
                    required property string subtitle
                    required property string label
                    required property string command
                    required property string result
                    required property string rowState
                    // A background command that ended: how it went (label,
                    // subtitle), the command, its last lines (result).
                    readonly property bool background: title === "command"
                    // harlo's message to the agent (a helper's report, …): it
                    // opens a turn, as harlo's; `subtitle` is the session
                    // it's about.
                    readonly property bool harlo: title === "harlo"
                    // Startup notices can be huge (e.g. a list of every MCP
                    // tool); harlo's show their first lines.
                    readonly property bool long: harlo ? text.split("\n").length > 2 || text.length > 200 : text.length > 240 || text.split("\n").length > 3
                    readonly property bool open: harlo ? view.expanded[rowKey] === true : !long || view.expanded[rowKey] === true
                    readonly property real gap: root.gapAbove(harlo ? "harlo" : "notice", ListView.previousSection)
                    HeadSlot { replyModel: noticeRow.rowModel; active: !noticeRow.harlo && root.opensTurn(noticeRow.ListView.previousSection); y: noticeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: (harlo ? harloSlot.height + 6 : background ? bgSlot.height + 4 : noticeText.implicitHeight + 4) + gap
                    // (Each kind made only for its own notices.)
                    Loader {
                        id: bgSlot
                        active: noticeRow.background
                        x: view.columnX
                        y: noticeRow.gap
                        width: view.columnWidth
                        sourceComponent: Column {
                            id: bgCol
                            objectName: "backgroundNotice"
                            spacing: 4
                            readonly property bool failed: noticeRow.rowState === "error"
                            readonly property bool open: view.expanded[noticeRow.rowKey] === true
                            // (The command's first line: the rest, and its last
                            // lines, a click away.)
                            readonly property string firstLine: noticeRow.command.split("\n")[0].trim()
                            readonly property bool more: noticeRow.command.indexOf("\n") >= 0 || noticeRow.result !== ""
                            Row {
                                id: bgHead
                                spacing: 7
                                width: parent.width
                                Icon {
                                    name: "terminal-box"
                                    width: Theme.px(14)
                                    height: width
                                    color: bgCol.failed ? Theme.error : Theme.textMuted
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                PlainLabel {
                                    objectName: "backgroundHeadline"
                                    text: noticeRow.label
                                    color: bgCol.failed ? Theme.error : Theme.textMuted
                                    font.pixelSize: Theme.px(12.5)
                                    font.weight: Font.Medium
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                PlainLabel {
                                    visible: noticeRow.subtitle !== ""
                                    // (Cut short when the column is narrow.)
                                    width: Math.max(0, Math.min(implicitWidth, bgHead.width - x - (bgMore.visible ? bgMore.width + bgHead.spacing : 0)))
                                    text: noticeRow.subtitle
                                    elide: Text.ElideRight
                                    color: Theme.textFaint
                                    font.pixelSize: Theme.px(12)
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                Icon {
                                    id: bgMore
                                    visible: bgCol.more || bgLine.truncated
                                    name: bgCol.open ? "arrow-down-s" : "arrow-right-s"
                                    color: Theme.textFaint
                                    width: Theme.px(14)
                                    height: width
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                TapHandler { enabled: bgCol.more || bgLine.truncated; onTapped: view.toggle(noticeRow.rowKey) }
                                HoverHandler { enabled: bgCol.more || bgLine.truncated; cursorShape: Qt.PointingHandCursor }
                            }
                            // The command: one line, cut at the end; opened, all of it.
                            Item {
                                width: parent.width
                                height: bgLine.implicitHeight
                                visible: !bgCol.open
                                Label {
                                    id: bgLine
                                    objectName: "backgroundCommand"
                                    x: Theme.px(14) + 7
                                    width: parent.width - x
                                    text: "$ " + bgCol.firstLine + (noticeRow.command.indexOf("\n") >= 0 ? " …" : "")
                                    textFormat: Text.PlainText
                                    color: Theme.textFaint
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.px(12)
                                    elide: Text.ElideRight
                                    maximumLineCount: 1
                                    wrapMode: Text.NoWrap
                                }
                                TapHandler { enabled: bgCol.more || bgLine.truncated; onTapped: view.toggle(noticeRow.rowKey) }
                                HoverHandler { enabled: bgCol.more || bgLine.truncated; cursorShape: Qt.PointingHandCursor }
                                ToolTip.visible: bgHover.hovered && (bgCol.more || bgLine.truncated)
                                ToolTip.delay: 600
                                ToolTip.text: "Show the whole command" + (noticeRow.result !== "" ? " and its last lines" : "")
                                HoverHandler { id: bgHover }
                            }
                            Rectangle {
                                objectName: "backgroundOpen"
                                visible: bgCol.open
                                width: parent.width
                                height: bgOpen.implicitHeight + 16
                                radius: 6
                                color: Theme.codeBg
                                Column {
                                    id: bgOpen
                                    x: 10
                                    y: 8
                                    width: parent.width - 20
                                    spacing: 8
                                    TextEdit {
                                        width: parent.width
                                        readOnly: true
                                        selectByMouse: true
                                        text: "$ " + noticeRow.command
                                        textFormat: TextEdit.PlainText
                                        wrapMode: TextEdit.Wrap
                                        color: Theme.textMuted
                                        selectionColor: Theme.selection
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.px(12)
                                    }
                                    TextEdit {
                                        visible: noticeRow.result !== ""
                                        width: parent.width
                                        readOnly: true
                                        selectByMouse: true
                                        text: noticeRow.result
                                        textFormat: TextEdit.PlainText
                                        wrapMode: TextEdit.Wrap
                                        color: Theme.text
                                        selectionColor: Theme.selection
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.px(12)
                                    }
                                }
                            }
                        }
                    }
                    PlainLabel {
                        id: noticeText
                        visible: !noticeRow.harlo && !noticeRow.background
                        x: view.columnX
                        y: noticeRow.gap
                        width: view.columnWidth
                        text: (noticeRow.long ? (noticeRow.open ? "▾ " : "▸ ") : "") + noticeRow.text
                        wrapMode: Text.Wrap
                        maximumLineCount: noticeRow.open ? 100000 : 2
                        elide: Text.ElideRight
                        font.pixelSize: Theme.px(12)
                        color: noticeRow.title === "error" ? Theme.error : noticeRow.title === "warning" ? Theme.warning : Theme.muted
                        TapHandler { enabled: noticeRow.long && !noticeRow.harlo; onTapped: view.toggle(noticeRow.rowKey) }
                        HoverHandler { enabled: noticeRow.long && !noticeRow.harlo; cursorShape: Qt.PointingHandCursor }
                    }
                    HoverHandler { id: noticeHover }
                    // harlo's: a line between turns, as before yours.
                    Rectangle {
                        visible: noticeRow.harlo && noticeRow.index > 0
                        x: view.columnX
                        y: Math.round(noticeRow.gap / 2) - 6
                        width: view.contentWidthLimit
                        height: 1
                        color: Theme.hairline
                    }
                    // The end of the turn before: undo it (and those after it).
                    FlatButton {
                        readonly property string turn: noticeRow.harlo ? root.undoBefore(noticeRow.rowKey) : ""
                        visible: turn !== ""
                        readonly property bool shown: noticeHover.hovered || hovered || activeFocus
                        opacity: shown ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.hoverMotion } }
                        x: view.columnX - 2
                        y: Math.round(noticeRow.gap / 2) - 6 - height / 2
                        text: "Undo from here"
                        outlined: false
                        padding: 2
                        accent: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        background: Rectangle { color: Theme.background; radius: 6 }
                        ToolTip.visible: hovered
                        ToolTip.delay: 400
                        ToolTip.text: "Put the folder's files back as they were before this turn (this turn and the ones after it)"
                        onClicked: root.undoRequested(turn)
                    }
                    Loader {
                        id: harloSlot
                        active: noticeRow.harlo
                        x: view.columnX
                        y: noticeRow.gap
                        width: view.columnWidth
                        sourceComponent: Column {
                            objectName: "harloMessage"
                            spacing: 4
                            Row {
                                spacing: 8
                                Image {
                                    source: "qrc:/qt/qml/harlo/icons/app/harlo.svg"
                                    width: Theme.px(20)
                                    height: width
                                    sourceSize: Qt.size(width * 2, height * 2)
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                Label { text: "harlo"; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
                                Label { text: "to the agent"; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); anchors.verticalCenter: parent.verticalCenter }
                            }
                            // What happened, in a line; what harlo told the agent
                            // (session keys, tool names) a click away.
                            Label {
                                objectName: "harloMessageText"
                                width: parent.width
                                text: noticeRow.label !== "" ? noticeRow.label : noticeRow.text
                                wrapMode: Text.Wrap
                                maximumLineCount: 3
                                elide: Text.ElideRight
                                font.pixelSize: Theme.px(13.5)
                                color: Theme.text
                                textFormat: Text.PlainText
                            }
                            Row {
                                spacing: 6
                                topPadding: 2
                                FlatButton {
                                    objectName: "openHelper"
                                    visible: noticeRow.subtitle !== ""
                                    text: "Open helper"
                                    padding: 4
                                    font.pixelSize: Theme.px(12)
                                    ToolTip.visible: hovered
                                    ToolTip.delay: 400
                                    ToolTip.text: "Show the session this is about"
                                    onClicked: Workspace.open(noticeRow.subtitle)
                                }
                                FlatButton {
                                    objectName: "harloSentToggle"
                                    visible: noticeRow.label !== "" && noticeRow.label !== noticeRow.text
                                    text: noticeRow.open ? "Hide what harlo sent" : "What harlo sent"
                                    outlined: false
                                    padding: 4
                                    accent: Theme.textMuted
                                    font.pixelSize: Theme.px(12)
                                    onClicked: view.toggle(noticeRow.rowKey)
                                }
                            }
                            Label {
                                objectName: "harloSent"
                                visible: noticeRow.open && noticeRow.label !== "" && noticeRow.label !== noticeRow.text
                                width: parent.width
                                topPadding: 4
                                text: noticeRow.text
                                wrapMode: Text.Wrap
                                font.pixelSize: Theme.px(12.5)
                                color: Theme.textMuted
                                textFormat: Text.PlainText
                            }
                        }
                    }
                }
            }
        }

    }

    PromptRail {
        id: rail
        visible: root.promptRail && count >= 2
        anchors.right: parent.right
        // Just outside the column's right edge, by the text it steps
        // through; never under the scrollbar.
        anchors.rightMargin: Math.max(14, root.gutter - 16 - width)
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(12, Math.min(28, root.gutter - 14))
        prompts: root.prompts
        activeIndex: root.activePrompt
        maxPanelHeight: root.height - 16
        maxPanelWidth: root.width - 96
        onPicked: row => root.jumpToRow(row)
        onFinished: root.promptRailFinished()
    }

    Rectangle {
        id: flash
        opacity: 0
        radius: 8
        color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.18)
        border.color: Theme.primary
        SequentialAnimation {
            id: flashAnim
            NumberAnimation { target: flash; property: "opacity"; to: 1; duration: 150 }
            PauseAnimation { duration: 900 }
            NumberAnimation { target: flash; property: "opacity"; to: 0; duration: 600 }
        }
    }
}
