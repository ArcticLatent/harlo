import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.qmlmodels
import harlo

// The transcript of one session: virtualized rows with one delegate per row kind.
Item {
    id: root
    required property ChatSession session
    // A row added to the transcript fades in.
    component AddFade: NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    property alias follow: view.follow
    property alias count: view.count
    /// Side margin of the rows; the composer below uses it too so edges align.
    readonly property alias gutter: view.gutter
    /// A file-editing tool row asked to show its diff.
    signal openDiff(string path)
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
        if (kind === "markdown" && prev === "markdown") return 8
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
    onSessionChanged: wantModelNames = false
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
        property string replyModel
        readonly property string shownModel: root.replyModelName(replyModel)
        // Another model than now: its name comes with the agent's list.
        readonly property bool unnamed: visible && replyModel !== "" && shownModel === replyModel
        onUnnamedChanged: if (unnamed) root.wantModelNames = true
        Component.onCompleted: if (unnamed) root.wantModelNames = true
        visible: false
        x: (view.width - view.contentWidthLimit) / 2
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
                ring: Theme.canvas
            }
            Label { text: Theme.harnessLabel(root.session.harness); color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.DemiBold }
            // The model only when the reply's isn't the session's (the
            // header names that one).
            Label { visible: head.shownModel !== "" && head.shownModel !== root.session.modelName; text: Theme.modelLabel(head.shownModel); color: Theme.textMuted; font.pixelSize: Theme.px(12.5); anchors.baseline: parent.children[1].baseline }
        }
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
    function local(t) {
        if (here === "" || !t) return t || ""
        const cd = "cd " + here + " && "
        if (t.startsWith(cd)) return t.slice(cd.length)
        if (t.startsWith("cd \"" + here + "\" && ")) return t.slice(here.length + 9)
        return t.split(here + "/").join("")
    }
    // A tool's name, the same for every agent: "bash" and "Bash" both
    // "Bash", "web_search" "Web search" (an MCP tool's as Theme says).
    // The shared plan's tools, by what they do.
    readonly property var planTools: ({ plan_set: "Plan", plan_read: "Read the plan", plan_start: "Start a step", plan_done: "Finish a step" })
    function toolName(t) {
        const p = /^(?:mcp__harlo__|harlo_)?(plan_[a-z]+)$/.exec(t || "")
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
    // A step on a run's line: a dot whose colour is how it went (a
    // thought's is hollow). The ring in the canvas colour cuts the line.
    component RunNode: Item {
        id: node
        property string step: "done"   // done, error, running, asking, thought
        // (For tests: the old marks' names.)
        readonly property string name: step === "error" ? "close" : step === "asking" ? "question" : step === "running" ? "loader-4" : "check"
        implicitWidth: 14
        implicitHeight: 14
        Rectangle {
            anchors.centerIn: parent
            width: 11
            height: 11
            radius: 5.5
            color: Theme.canvas
            Rectangle {
                id: dot
                anchors.centerIn: parent
                width: 7
                height: width
                radius: width / 2
                color: node.step === "error" ? Theme.error : node.step === "asking" ? Theme.warning : node.step === "running" ? Theme.primary
                     : node.step === "thought" ? Theme.canvas : Theme.textFaint
                border.width: node.step === "thought" ? 1.25 : 0
                border.color: Theme.textFaint
                SequentialAnimation on opacity {
                    running: node.step === "running" && Theme.motion > 0
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) dot.opacity = 1
                    NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
                }
            }
        }
    }
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
        Rectangle { x: -6; width: parent.width + 12; height: parent.height; radius: 6; color: runHover.hovered ? Theme.hover : "transparent" }
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
        TapHandler { onTapped: view.toggleRun(runHead.group, runHead.dflt) }
    }
    // A thought in a run: one line, its text a click away.
    component ThoughtLine: ColumnLayout {
        id: thought
        property string rowKey
        property string text
        property string rowState
        readonly property bool open: view.expanded[rowKey] === true
        spacing: 4
        Item {
            Layout.fillWidth: true
            implicitHeight: thoughtHead.implicitHeight + 6
            Rectangle { x: -6; width: parent.width + 12; height: parent.height; radius: 6; color: thoughtHover.hovered ? Theme.hover : "transparent" }
            RowLayout {
                id: thoughtHead
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 8
                RunNode { step: thought.rowState === "streaming" ? "running" : "thought" }
                Label { text: thought.rowState === "streaming" ? "Thinking…" : "Thought"; color: Theme.textMuted; font.pixelSize: Theme.px(13) }
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
            TapHandler { onTapped: view.toggle(thought.rowKey) }
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
    // A tool call: one line (what it is, what it touched, what it came to)
    // and, when there's something to see, one well under it: a write's or
    // an edit's lines, a command's last lines.
    component ToolCard: ColumnLayout {
        id: card
        property string rowKey
        property string title
        property string subtitle
        property string text
        property string rowState
        property string callId
        property string result
        property string diff
        property real took
        property string label
        property string command
        // Waiting for your approval (the row's own).
        property bool asking: false
        property var askLines: []
        readonly property bool open: view.expanded[rowKey] === true
        readonly property var agents: callId !== "" ? root.subagents.filter(a => a.parentToolCallId === callId) : []
        // A write: every line added, numbered; an edit: lines out and in.
        readonly property bool preview: diff !== "" && !asking
        readonly property bool newFile: preview && result.indexOf("−") < 0
        readonly property int previewLines: 6
        readonly property int diffCount: diff === "" ? 0 : diff.split("\n").length
        // (A write's own count: its diff stops at 400 lines.)
        readonly property int lineCount: newFile && /^\+\d+$/.test(result) ? Number(result.slice(1)) : diffCount
        readonly property var lines: preview ? JSON.parse(root.session.diffLines(subtitle.replace(/…$/, ""), diff, open ? 400 : previewLines))
            : code ? code.lines.map(l => ({ s: " ", n: l.n, c: l.c, h: l.h })) : []
        // Output that is a file's lines (a printed file, grep -n in one
        // file, a read): numbered and highlighted like a file, its first
        // lines (where it starts matters), never wrapped.
        readonly property var code: {
            if (preview || asking || text === "" || rowState === "running") return null
            const j = root.session.codeView(title, command, subtitle.replace(/…$/, ""), text, open ? 400 : previewLines)
            return j === "" ? null : JSON.parse(j)
        }
        readonly property bool shell: command !== "" && !asking
        readonly property string shownCommand: shell ? root.local(command) : ""
        // The command in the well when its line doesn't say it all.
        readonly property bool commandInWell: shell && (label !== "" || shownCommand.indexOf("\n") >= 0 || shownCommand.length > 90)
        readonly property var outLines: text === "" ? [] : text.split("\n").filter(l => l.trim() !== "")
        readonly property int tailLines: 3
        readonly property var tail: outInWell && shell && !open && !code ? outLines.slice(-tailLines) : []
        readonly property var tailGutters: root.numbered(tail)
        // (One line of output that's what the line already says, "48 passed": not again.)
        readonly property bool echoed: shell && outLines.length === 1 && result !== "" && outLines[0].trim() === result.trim()
        readonly property bool outInWell: shell ? outLines.length > 0 && rowState !== "running" && !echoed : open && text !== "" && !preview
        // (A read's lines show once it's opened; a command's at once.)
        readonly property bool codeShown: !!code && (shell || open)
        readonly property bool failed: rowState === "error" || /^(exit|failed)|failed$/.test(result)
        readonly property int hidden: preview ? lineCount - previewLines : code ? code.total - previewLines : shell ? outLines.length - tailLines : 0
        readonly property bool canOpen: (text !== "" && !shell && !preview) || hidden > 0 || (commandInWell && shownCommand.split("\n").length > 2)
        // (harlo's tools about a session: its title, not its key.)
        readonly property bool aboutSession: /^(mcp__harlo__)?(bring_over|send_message|read_session|stop_session)$/.test(title) && /^[\w-]+:\S+$/.test(subtitle)
        readonly property string sessionTitle: aboutSession ? Workspace.sessionTitle(subtitle) : ""
        readonly property string target: label !== "" ? label : sessionTitle !== "" ? sessionTitle : subtitle !== "" || rowState !== "pending" ? root.local(subtitle) : Theme.toolWriting(title)
        // Code type only for code: a command, a path, a pattern, a link. A
        // title, a question or a task's description is prose.
        readonly property bool codeLike: label === "" && sessionTitle === "" && (shell || command !== "" || /^(bash|shell|exec|run_?command|read|edit|multi_?edit|write|apply_?patch|patch|create|ls|find|glob|grep|lsp|web_?fetch|fetch|notebook)/i.test(title) || !/\s/.test(target))
        readonly property bool isPath: label === "" && !shell && /^[^\s]+$/.test(target) && /[\/.]/.test(target)
        readonly property real headHeight: headBox.height
        spacing: 4
        Item {
            id: headBox
            objectName: "toolHead"
            Layout.fillWidth: true
            implicitHeight: head.implicitHeight + 6
            Rectangle { x: -6; width: parent.width + 12; height: parent.height; radius: 6; color: headHover.hovered && card.canOpen ? Theme.hover : "transparent" }
            RowLayout {
                id: head
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 8
                RunNode {
                    objectName: "toolMark"
                    step: card.asking ? "asking" : card.rowState === "error" || (card.failed && card.rowState === "done") ? "error" : card.rowState === "done" ? "done" : "running"
                }
                Label { text: root.toolName(card.title); color: Theme.text; font.weight: Font.Medium; font.pixelSize: Theme.px(13) }
                Label {
                    objectName: "toolWhat"
                    Layout.fillWidth: true
                    // (Plain, for tests and copying: the words shown.)
                    readonly property string plain: card.target
                    textFormat: card.isPath ? Text.StyledText : Text.PlainText
                    text: card.isPath ? root.pathHtml(card.target) : card.target
                    color: Theme.textMuted
                    elide: Text.ElideRight
                    font.family: card.codeLike ? Theme.mono : Theme.uiFont
                    font.pixelSize: card.codeLike ? Theme.mpx(12) : Theme.px(13)
                }
                FlatButton {
                    // An edit or a write: its file's diff, in the Changes panel.
                    visible: headHover.hovered && /^(edit|multi_?edit|write|apply_?patch|patch|create)/i.test(card.title) && card.subtitle !== "" && card.rowState !== "error"
                    text: "diff"
                    font.pixelSize: Theme.px(11)
                    padding: 2
                    outlined: false
                    accent: Theme.info
                    onClicked: root.openDiff(card.subtitle.replace(/…$/, ""))
                }
                Label {
                    objectName: "toolResult"
                    visible: text !== ""
                    textFormat: Text.StyledText
                    text: {
                        const faint = s => s === "" ? "" : "<font color='" + Theme.rgb(Theme.textFaint) + "'>" + s + "</font>"
                        let r = card.newFile ? (card.lineCount === 1 ? "1 line" : card.lineCount + " lines")
                              : card.result.replace(/^\+(\d+) −(\d+)$/, "<font color='" + Theme.rgb(Theme.diffAdd) + "'>+$1</font> <font color='" + Theme.rgb(Theme.diffRemove) + "'>−$2</font>")
                        if (card.failed && r !== "") r = "<font color='" + Theme.rgb(Theme.error) + "'>" + r + "</font>"
                        return [r, faint(root.seconds(card.took))].filter(x => x !== "").join("&nbsp;&nbsp;")
                    }
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
            }
            HoverHandler { id: headHover; cursorShape: card.canOpen ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { enabled: card.canOpen; onTapped: view.toggle(card.rowKey) }
        }
        // The subagents this call started (the task tool), as a tree.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            visible: card.agents.length > 0
            spacing: 6
            Rectangle { Layout.fillHeight: true; Layout.leftMargin: 4; implicitWidth: 2; radius: 1; color: Theme.border }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: card.agents
                    SubagentRow {
                        required property var modelData
                        Layout.fillWidth: true
                        a: modelData
                        session: root.session
                        onViewRequested: (agent, steer) => root.subagentRequested(agent, steer)
                    }
                }
            }
        }
        // The approval: its details (reason, command…) and the answer.
        // A command that can't be undone says so, and its confirm names
        // what it does rather than looking like good news.
        readonly property bool destructive: askLines.some(l => l.startsWith("Reason: this can't be undone"))
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            visible: text !== ""
            text: card.askLines.filter(l => l.startsWith("Reason:")).map(l => {
                const r = l.slice(7).trim()
                return r.charAt(0).toUpperCase() + r.slice(1)
            }).join("\n")
            // (The card is already the warning; only "can't be undone" is news.)
            color: card.destructive ? Theme.error : Theme.textMuted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            // (Not the command again when the line above shows it.)
            readonly property string details: card.askLines.filter(l => !l.startsWith("Reason:") && l !== "Outside the sandbox"
                && l.replace(/^Command:\s*/, "").trim() !== root.local(card.subtitle).replace(/…$/, "").trim()).join("\n").trim()
            visible: card.asking && details !== ""
            implicitHeight: askDetails.implicitHeight + 12
            radius: 4
            color: Theme.codeBg
            Label {
                id: askDetails
                x: 8
                y: 6
                width: parent.width - 16
                text: parent.details
                color: Theme.foreground
                font.family: Theme.mono
                font.pixelSize: Theme.mpx(12)
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 14
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 22
            Layout.topMargin: 2
            visible: card.asking
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: root.approvalQuestion(card.title, card.askLines)
                color: Theme.text
                font.pixelSize: Theme.px(13)
                elide: Text.ElideRight
            }
            FlatButton { text: "Deny"; font.pixelSize: Theme.px(12.5); onClicked: root.session.answerValue("Deny") }
            FlatButton {
                objectName: "alwaysAllowInline"
                visible: root.session.requestOptions.indexOf("Always allow") >= 0
                text: parent.width < 420 ? "Always" : "Always allow"
                font.pixelSize: Theme.px(12.5)
                ToolTip.visible: hovered
                ToolTip.delay: 400
                ToolTip.text: { try { return "Remember: " + (JSON.parse(root.session.requestJson).alwaysHint || "this") } catch (e) { return "" } }
                onClicked: root.session.answerValue("Always allow")
            }
            FlatButton {
                objectName: "approveInline"
                text: card.destructive ? "Run anyway" : "Approve"
                variant: card.destructive ? "danger" : "primary"
                accent: card.destructive ? Theme.error : Theme.foreground
                font.pixelSize: Theme.px(12.5)
                onClicked: root.session.answerValue("Approve")
            }
        }
        // The well: a write's or an edit's lines; a command (when its line
        // doesn't say it) and its last lines; any call's output, opened.
        Rectangle {
            id: toolWellBox
            objectName: card.preview ? "toolPreview" : "toolWell"
            Layout.fillWidth: true
            Layout.leftMargin: 22
            visible: card.preview || card.commandInWell || card.outInWell || card.codeShown
            // (As tall as it is: a run's box is what scrolls.)
            implicitHeight: wellCol.implicitHeight + 12
            radius: 6
            color: Theme.codeBg
            // Line numbers on a column of their own, from the code's first
            // line (under a command, if any) to the well's bottom.
            readonly property bool gutter: (card.preview && card.newFile) || (card.codeShown && card.code.lines.some(l => l.n !== ""))
            Rectangle {
                visible: parent.gutter
                y: card.commandInWell ? 6 + cmdLabel.height + 3 : 0
                width: wellCol.numberWidth - 6
                height: parent.height - y
                color: Theme.codeGutter
                topLeftRadius: y === 0 ? 6 : 0
                bottomLeftRadius: 6
            }
            Item {
                id: wellFlick
                y: 6
                width: parent.width
                height: parent.height - 12
                clip: true
                Column {
                    id: wellCol
                    width: wellFlick.width
                    spacing: 0
                    // (Numbers: 8 px either side in their column, then 6 px to the code.)
                    readonly property real numberWidth: card.newFile || card.code ? numberProbe.implicitWidth + 22 : 22
                    Label {
                        id: numberProbe
                        visible: false
                        text: card.code ? card.code.lines.reduce((w, l) => l.n.length > w.length ? l.n : w, "0") : String(card.lines.length)
                        font.family: Theme.mono
                        font.pixelSize: Theme.mpx(12)
                    }
                    Label {
                        id: cmdLabel
                        objectName: "toolCommand"
                        visible: card.commandInWell
                        x: 10
                        width: wellCol.width - 20
                        text: "<font color='" + Theme.rgb(Theme.textFaint) + "'>$</font> " + card.shownCommand.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>")
                        textFormat: Text.StyledText
                        color: Theme.foreground
                        wrapMode: Text.WrapAnywhere
                        maximumLineCount: card.open ? 400 : 2
                        elide: Text.ElideRight
                        font.family: Theme.mono
                        font.pixelSize: Theme.mpx(12)
                    }
                    Item { visible: card.commandInWell && (card.outInWell || card.codeShown); width: 1; height: 6 }
                    Repeater {
                        model: card.preview || card.codeShown ? card.lines : []
                        Rectangle {
                            required property var modelData
                            required property int index
                            objectName: card.code ? "codeLine" : ""
                            width: wellCol.width
                            implicitHeight: lineText.implicitHeight + 2
                            readonly property bool added: !card.newFile && modelData.s === "+"
                            readonly property bool removed: modelData.s === "-"
                            color: added ? Qt.rgba(Theme.diffAdd.r, Theme.diffAdd.g, Theme.diffAdd.b, 0.10) : removed ? Qt.rgba(Theme.diffRemove.r, Theme.diffRemove.g, Theme.diffRemove.b, 0.10) : "transparent"
                            Label {
                                width: wellCol.numberWidth - (card.newFile || card.code ? 14 : 10)
                                horizontalAlignment: Text.AlignRight
                                anchors.verticalCenter: parent.verticalCenter
                                text: card.code ? parent.modelData.n : card.newFile ? String(index + 1) : parent.added ? "+" : parent.removed ? "−" : ""
                                color: parent.added ? Theme.diffAdd : parent.removed ? Theme.diffRemove : Theme.textFaint
                                font.family: Theme.mono
                                font.pixelSize: Theme.mpx(12)
                            }
                            Label {
                                id: lineText
                                x: wellCol.numberWidth
                                width: parent.width - x - 10
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.RichText
                                text: Theme.htmlStyle + "<pre style=\"margin:0;\">" + (modelData.h === "" ? " " : modelData.h) + "</pre>"
                                // (grep's context lines, around the matches: dimmer.)
                                opacity: modelData.c ? 0.6 : 1
                                color: Theme.foreground
                                font.family: Theme.mono
                                font.pixelSize: Theme.mpx(12)
                            }
                        }
                    }
                    // A command's last lines (each one shortened to the well), or
                    // all of a call's output, opened.
                    Repeater {
                        model: card.tail
                        Label {
                            required property string modelData
                            required property int index
                            objectName: "toolTail"
                            x: 10
                            width: wellCol.width - 20
                            readonly property var row: card.tailGutters ? card.tailGutters[index] : null
                            text: row ? root.numberedHtml(row, true) : modelData
                            color: Theme.textMuted
                            elide: Text.ElideRight
                            textFormat: row ? Text.StyledText : Text.PlainText
                            font.family: Theme.mono
                            font.pixelSize: Theme.mpx(12)
                        }
                    }
                    TextEdit {
                        visible: card.outInWell && (card.open || !card.shell) && !card.code
                        x: 10
                        width: wellCol.width - 20
                        // (Line numbers at the start of its lines as a gutter.)
                        readonly property var gutters: visible ? root.numbered(card.text.split("\n")) : null
                        textFormat: gutters ? TextEdit.RichText : TextEdit.PlainText
                        text: !visible ? "" : gutters ? "<p style='margin:0;white-space:pre-wrap;'>" + gutters.map(r => root.numberedHtml(r, false)).join("<br>") + "</p>" : card.text
                        color: Theme.muted
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        font.family: Theme.mono
                        font.pixelSize: Theme.mpx(12)
                    }
                    // What's left out, a click away.
                    Label {
                        objectName: "toolMoreLines"
                        visible: card.hidden > 0
                        x: toolWellBox.gutter ? wellCol.numberWidth : 10
                        topPadding: 3
                        text: card.open ? "Fewer lines" : card.hidden + (card.hidden === 1 ? " more line" : " more lines")
                        color: moreHover.hovered ? Theme.muted : Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        HoverHandler { id: moreHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: view.toggle(card.rowKey) }
                    }
                }
            }
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
        // (Read again as the list changes: a finished run rarely does.)
        readonly property var rows: (view.count, summary, JSON.parse(root.session.runJson(group)))
        readonly property real maxHeight: Math.max(Theme.px(160), Math.min(Theme.px(380), view.height * 0.45))
        objectName: "runBox"
        spacing: 2
        RunHeader { Layout.fillWidth: true; group: box.group; summary: box.summary; dflt: box.dflt }
        Item {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(boxCol.implicitHeight + 6, box.maxHeight)
        Flickable {
            id: boxFlick
            objectName: "runBoxScroll"
            anchors.fill: parent
            contentHeight: boxCol.implicitHeight + 6
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {
                id: boxBar
                policy: boxFlick.contentHeight > boxFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                onPressedChanged: boxFlick.pinned = !pressed && boxFlick.atEnd()
            }
            // At its end until scrolled by hand, and again once scrolled back
            // there (its cards measure themselves after it's made, and a
            // Flickable keeps its contentY when the content grows).
            property bool pinned: true
            function atEnd() { return contentY >= contentHeight - height - 2 }
            function place() { if (pinned && height > 0) contentY = Math.max(0, contentHeight - height) }
            onContentHeightChanged: place()
            onHeightChanged: place()
            onMovementStarted: pinned = false
            onMovementEnded: pinned = atEnd()
            // The run's line, from its first step to its last.
            Rectangle {
                x: boxCol.x + 7
                y: 4
                width: 1
                height: Math.max(0, boxCol.implicitHeight - 12)
                color: Theme.border
            }
            ColumnLayout {
                id: boxCol
                width: boxFlick.width - (boxBar.visible ? 14 : 0)
                spacing: 6
                Repeater {
                    model: box.rows
                    Loader {
                        id: cell
                        required property var modelData
                        Layout.fillWidth: true
                        sourceComponent: modelData.kind === "thinking" ? thoughtCell : toolCell
                        Component {
                            id: thoughtCell
                            ThoughtLine { width: cell.width; rowKey: cell.modelData.rowKey; text: cell.modelData.text; rowState: cell.modelData.rowState }
                        }
                        Component {
                            id: toolCell
                            ToolCard {
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
                            }
                        }
                    }
                }
            }
        }
        // More above or below: the edge fades out.
        Rectangle {
            width: parent.width - (boxBar.visible ? 10 : 0)
            height: 22
            visible: boxFlick.contentY > 1
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
    /// Brings row `i` to the middle and flashes it (search results).
    /// Row `i`'s end is on screen (an approval's buttons are there), its
    /// run open. (Re-read as the view scrolls: it reads `contentY`.)
    function rowEndInView(i) {
        if (i < 0) return false
        const item = view.itemAtIndex(i)
        if (!item || item.folded || !item.visible || item.height <= 0) return false
        const bottom = item.y + item.height - view.contentY
        return bottom > 24 && bottom <= view.height + 1
    }

    function revealIndex(i) {
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
        })
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
        readonly property real gutter: Math.max(rail.visible ? 36 : 16, Math.min(64, width * 0.04), (width - Theme.px(Theme.chatMaxWidth)) / 2)
        readonly property real contentWidthLimit: Math.max(120, width - 2 * gutter)
        // A turn's rows use the whole column (the avatars sit beside the
        // names), so they line up with the composer under them.
        readonly property real columnX: (width - contentWidthLimit) / 2
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
                SequentialAnimation on opacity {
                    running: root.session.status === "busy"
                    loops: Animation.Infinite
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
                        x: (view.width - view.contentWidthLimit) / 2
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
                                id: userText
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
                    readonly property real gap: root.gapAbove("markdown", ListView.previousSection)
                    // The code block (in a list) just copied: its link says "Copied" a moment.
                    property int copiedBlock: -1
                    Timer { id: blockCopiedTimer; interval: 1200; onTriggered: mdRow.copiedBlock = -1 }
                    AgentHead { replyModel: mdRow.rowModel; visible: root.opensTurn(mdRow.ListView.previousSection); y: mdRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: md.implicitHeight + gap
                    TextEdit {
                        font.family: Theme.uiFont
                        id: md
                        x: view.columnX
                        y: mdRow.gap
                        width: view.columnWidth
                        textFormat: TextEdit.RichText
                        text: Theme.htmlStyle + Theme.proseHtml(parent.html, mdRow.copiedBlock)
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

            DelegateChoice {
                roleValue: "code"
                Item {
                    id: codeRow
                    required property string rowModel
                    ListView.onAdd: if (Theme.motion > 0) codeRowFade.start()
                    AddFade { id: codeRowFade; target: codeRow }
                    required property string html
                    required property string text
                    required property string title
                    readonly property real gap: root.gapAbove("code", ListView.previousSection)
                    // A Mermaid diagram: drawn (Diagrams), its code a click away.
                    readonly property bool mermaid: title.toLowerCase() === "mermaid"
                    property bool showCode: false
                    readonly property var diagram: mermaid && Diagrams.revision >= 0
                        ? JSON.parse(Diagrams.diagram(text, Theme.isLight, JSON.stringify({ background: String(Theme.codeBg), surface: String(Theme.elevated), text: String(Theme.foreground),
                                                                                             muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
                        : null
                    readonly property bool drawn: diagram !== null && diagram.state === "ready" && !showCode
                    AgentHead { replyModel: codeRow.rowModel; visible: root.opensTurn(codeRow.ListView.previousSection); y: codeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: codeBox.height + gap
                    Rectangle {
                        id: codeBox
                        x: view.columnX
                        y: codeRow.gap
                        width: view.columnWidth
                        height: codeHeader.height + (codeRow.drawn ? diagramBox.height : codeFlick.height) + (diagramNote.visible ? diagramNote.height : 0)
                        radius: 10
                        color: Theme.codeBg
                        border.color: Theme.hairline
                        RowLayout {
                            id: codeHeader
                            width: parent.width
                            height: 28
                            Label { Layout.leftMargin: 10; text: codeRow.title || "text"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            Item { Layout.fillWidth: true }
                            FlatButton {
                                visible: codeRow.mermaid && codeRow.diagram !== null && codeRow.diagram.state === "ready"
                                Layout.preferredHeight: 22
                                text: codeRow.showCode ? "Diagram" : "Code"
                                font.pixelSize: Theme.px(11)
                                onClicked: codeRow.showCode = !codeRow.showCode
                            }
                            FlatButton {
                                visible: codeRow.mermaid
                                Layout.preferredHeight: 22
                                text: "Open in browser"
                                font.pixelSize: Theme.px(11)
                                onClicked: Diagrams.openInBrowser(codeRow.text, Theme.isLight)
                            }
                            FlatButton {
                                id: copyCode
                                Layout.rightMargin: 4
                                Layout.preferredHeight: 22
                                // "Copied" for a moment after a click.
                                property bool copied: false
                                text: copied ? "Copied" : "Copy"
                                font.pixelSize: Theme.px(11)
                                onClicked: { clipboardHelper.copyText(codeRow.text); copied = true; copiedTimer.restart() }
                                Timer { id: copiedTimer; interval: 1200; onTriggered: copyCode.copied = false }
                            }
                        }
                        // Drawing it, or why it isn't drawn.
                        Label {
                            id: diagramNote
                            anchors.top: codeHeader.bottom
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
                        Item {
                            id: diagramBox
                            objectName: "diagram"
                            anchors.top: codeHeader.bottom
                            visible: codeRow.drawn
                            width: parent.width
                            height: visible ? diagramImage.height + 20 : 0
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
                                TapHandler { onTapped: Qt.openUrlExternally(codeRow.diagram.url) }
                                ToolTip.visible: diagramHover.hovered
                                ToolTip.text: "Open full size"
                                ToolTip.delay: 600
                            }
                        }
                        Flickable {
                            id: codeFlick
                            anchors.top: diagramNote.visible ? diagramNote.bottom : codeHeader.bottom
                            visible: !codeRow.drawn
                            width: parent.width
                            height: codeText.implicitHeight + 16
                            // Settings → Appearance → Word wrap: long lines wrap.
                            contentWidth: Theme.wrapCode ? width : codeText.implicitWidth + 20
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            flickableDirection: Flickable.HorizontalFlick
                            ScrollBar.horizontal: ScrollBar { policy: codeFlick.contentWidth > codeFlick.width ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                            TextEdit {
                                id: codeText
                                x: 10
                                y: 4
                                textFormat: TextEdit.RichText
                                text: Theme.htmlStyle + codeRow.html
                                color: Theme.foreground
                                selectionColor: Theme.selection
                                readOnly: true
                                selectByMouse: true
                                font.family: Theme.mono
                                font.pixelSize: Theme.mpx(13)
                                width: Theme.wrapCode ? codeFlick.width - 20 : implicitWidth
                                wrapMode: Theme.wrapCode ? TextEdit.WrapAnywhere : TextEdit.NoWrap
                            }
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
                    AgentHead { replyModel: thinkRow.rowModel; visible: root.opensTurn(thinkRow.ListView.previousSection); y: thinkRow.gap - height - 10 }
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
                                  : thinkLine.implicitHeight + gap + head + 10
                    visible: implicitHeight > 0
                    RunHeader {
                        visible: thinkRow.head > 0 && !thinkRow.boxed
                        x: view.columnX
                        y: thinkRow.gap
                        width: view.columnWidth
                        group: thinkRow.group
                        summary: thinkRow.summary
                        dflt: thinkRow.groupOpen
                    }
                    Loader {
                        id: thinkBox
                        active: thinkRow.boxed && thinkRow.head > 0
                        x: view.columnX
                        y: thinkRow.gap
                        width: view.columnWidth
                        sourceComponent: RunBox { width: thinkBox.width; group: thinkRow.group; summary: thinkRow.summary; dflt: thinkRow.groupOpen }
                    }
                    ThoughtLine {
                        id: thinkLine
                        visible: !thinkRow.folded && !thinkRow.boxed
                        x: view.columnX
                        y: thinkRow.gap + thinkRow.head + 5
                        width: view.columnWidth
                        rowKey: thinkRow.rowKey
                        text: thinkRow.text
                        rowState: thinkRow.rowState
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
                    AgentHead { replyModel: toolRow.rowModel; visible: root.opensTurn(toolRow.ListView.previousSection); y: toolRow.gap - height - 10 }
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
                    RunHeader {
                        visible: toolRow.head > 0 && !toolRow.boxed
                        x: view.columnX
                        y: toolRow.gap
                        width: view.columnWidth
                        group: toolRow.group
                        summary: toolRow.summary
                        dflt: toolRow.groupOpen
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
                        height: toolCard.implicitHeight + (toolRow.asking ? 20 : 10)
                        radius: 8
                        // (Solid when it asks: a row under it doesn't show through.)
                        color: toolRow.asking ? Qt.tint(Theme.raised, Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.06)) : "transparent"
                        border.color: toolRow.asking ? Theme.warning : "transparent"
                        ToolCard {
                            id: toolCard
                            x: 8
                            y: toolRow.asking ? 10 : 5
                            width: parent.width - 16
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
                            asking: toolRow.asking
                            askLines: toolRow.askLines
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
                    // harlo's message to the agent (a helper's report, …): it
                    // opens a turn, as harlo's; `subtitle` is the session
                    // it's about.
                    readonly property bool harlo: title === "harlo"
                    // Startup notices can be huge (e.g. a list of every MCP
                    // tool); harlo's show their first lines.
                    readonly property bool long: harlo ? text.split("\n").length > 2 || text.length > 200 : text.length > 240 || text.split("\n").length > 3
                    readonly property bool open: harlo ? view.expanded[rowKey] === true : !long || view.expanded[rowKey] === true
                    readonly property real gap: root.gapAbove(harlo ? "harlo" : "notice", ListView.previousSection)
                    AgentHead { replyModel: noticeRow.rowModel; visible: !noticeRow.harlo && root.opensTurn(noticeRow.ListView.previousSection); y: noticeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: (harlo ? harloCol.implicitHeight + 6 : noticeText.implicitHeight + 4) + gap
                    Label {
                        id: noticeText
                        visible: !noticeRow.harlo
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
                        x: (view.width - view.contentWidthLimit) / 2
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
                    Column {
                        id: harloCol
                        objectName: "harloMessage"
                        visible: noticeRow.harlo
                        x: view.columnX
                        y: noticeRow.gap
                        width: view.columnWidth
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
                            id: harloText
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

        FlatButton {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 16
            objectName: "latestButton"
            visible: !view.follow
            // (Narrow: the arrow alone, so it covers less of the text.)
            text: view.width < 600 ? "↓" : "↓ Latest"
            ToolTip.visible: hovered && view.width < 600
            ToolTip.text: "Latest"
            ToolTip.delay: 400
            accent: Theme.primary
            radius: 14
            onClicked: { view.follow = true; view.scrollToEnd() }
        }
    }

    PromptRail {
        id: rail
        visible: root.promptRail && count >= 2
        anchors.right: parent.right
        // Clear of the scrollbar.
        anchors.rightMargin: 14
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
