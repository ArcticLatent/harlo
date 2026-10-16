import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.qmlmodels
import harlo

// The transcript of one session: virtualized rows with one delegate per row kind.
Item {
    id: root
    required property ChatSession session
    property alias follow: view.follow
    property alias count: view.count
    /// Side margin of the rows; the composer below uses it too so edges align.
    readonly property alias gutter: view.gutter
    /// A file-editing tool row asked to show its diff.
    signal openDiff(string path)
    /// Your message at row `index`: edit and resend it / fork the session there.
    signal editRequested(int index)
    signal forkRequested(int index)
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
    /// omp subagents of the session, shown under the tool call that started them.
    readonly property var subagents: session.subagentsJson !== "" ? JSON.parse(session.subagentsJson) : []

    function scrollToEnd() { view.follow = true; view.scrollToEnd() }
    /// Space above a row, from its kind and the kind before it: a new turn
    /// stands apart, paragraphs breathe, and tool calls and thoughts in a
    /// run sit together as one timeline.
    function gapAbove(kind, prev) {
        if (prev === "") return kind === "user" ? 8 : 0
        const act = k => k === "tool" || k === "thinking"
        // A new turn: a line between turns, then your avatar and name.
        if (kind === "user") return 34
        if (act(kind) && act(prev)) return 0
        // The reply opens with the agent's avatar and name.
        if (prev === "user") return 22 + root.headHeight
        if (kind === "markdown" && prev === "markdown") return 8
        if (act(kind) || act(prev)) return 8
        return 14
    }
    function isActivity(kind) { return kind === "tool" || kind === "thinking" }
    /// A turn's head: avatar and name, over its first row.
    readonly property real headHeight: Theme.px(30)
    // The agent at the top of its reply: avatar, name and model.
    component AgentHead: Item {
        visible: false
        x: (view.width - view.contentWidthLimit) / 2
        width: view.contentWidthLimit
        height: root.headHeight
        AgentAvatar {
            visible: !view.compact
            implicitWidth: Theme.px(32)
            implicitHeight: Theme.px(32)
            anchors.verticalCenter: parent.verticalCenter
            provider: root.session.harness
            ring: Theme.canvas
        }
        Row {
            x: view.indent
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Label { text: Theme.harnessLabel(root.session.harness); color: Theme.text; font.pixelSize: Theme.px(14.5); font.weight: Font.DemiBold }
            Label { visible: root.session.modelName !== ""; text: "· " + root.session.modelName; color: Theme.textFaint; font.pixelSize: Theme.px(13); anchors.baseline: parent.children[0].baseline }
        }
    }
    /// A run of tool calls and thoughts: folded or open as set by hand,
    /// else open while it's going (`dflt`, from the session).
    function runOpen(group, dflt) { const v = view.runs[group]; return v === undefined ? dflt : v }
    // The line through a run of tool calls and thoughts: above and below
    // the row's mark when the rows next to it are activity too (a row's
    // gap is inside its top, so the segments meet).
    component RailLine: Item {
        property real markX
        property real markY
        property bool up
        property bool down
        // Where the line above starts (below a run's summary line).
        property real from: 0
        Rectangle { visible: parent.up; x: parent.markX; y: parent.from; width: 1; height: Math.max(0, parent.markY - 7 - parent.from); color: Theme.border }
        Rectangle { visible: parent.down; x: parent.markX; y: parent.markY + 7; width: 1; height: Math.max(0, parent.height - parent.markY - 7); color: Theme.border }
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
        Rectangle { anchors.fill: parent; radius: 8; color: runHover.hovered ? Theme.hover : "transparent" }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 10
            spacing: 8
            Label { Layout.preferredWidth: 14; horizontalAlignment: Text.AlignHCenter; text: runHead.isOpen ? "▾" : "▸"; color: Theme.muted; font.pixelSize: Theme.px(12) }
            Label {
                objectName: "runSummary"
                Layout.fillWidth: true
                text: runHead.summary
                color: runHover.hovered ? Theme.foreground : Theme.muted
                elide: Text.ElideRight
                font.pixelSize: Theme.px(13)
            }
            // Still going.
            Label { visible: runHead.dflt; text: "●"; color: Theme.primary; font.pixelSize: Theme.px(9) }
        }
        HoverHandler { id: runHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: view.toggleRun(runHead.group, runHead.dflt) }
    }
    /// What an omp approval asks, in words. omp's own goal tool asks too
    /// (in approval modes other than yolo), with nothing but its `op`.
    function approvalQuestion(tool, args) {
        if (tool === "goal") {
            const op = (args.match(/op=(\w+)/) || [])[1] || ""
            return ({ complete: "Let omp mark the goal complete?", create: "Let omp set a goal?", resume: "Let omp resume the goal?",
                      drop: "Let omp drop the goal?", get: "Let omp read the goal?" })[op] || "Let omp change the goal?"
        }
        return "Allow " + tool + " to run?"
    }
    /// Brings row `i` to the middle and flashes it (search results).
    function revealIndex(i) {
        view.follow = false
        view.positionViewAtIndex(i, ListView.Center)
        Qt.callLater(() => {
            let item = view.itemAtIndex(i)
            if (!item) return
            // In a folded run: open it first.
            if (item.folded) {
                const r = Object.assign({}, view.runs)
                r[item.group] = true
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
    function scrollBy(dy) { view.contentY = Math.max(view.originY - view.topMargin, view.contentY + dy) }
    readonly property bool atEnd: view.nearEnd()
    /// [row index, its y within the viewport] for the row at the viewport's middle (tests).
    /// How far past the bottom limit the view sits (positive = out of bounds; tests).
    function overscroll() { return view.contentY - (view.originY + view.contentHeight + view.bottomMargin - view.height) }
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
        section.property: "kind"
        cacheBuffer: 1600
        topMargin: 16
        bottomMargin: 16
        boundsBehavior: Flickable.StopAtBounds
        // Whole-pixel scroll positions: smooth wheel scrolling moves in
        // fractional pixels, which makes text shimmer by a pixel.
        pixelAligned: true
        // Grabbing the scrollbar always means "stop following".
        ScrollBar.vertical: ScrollBar { onPressedChanged: if (pressed) view.follow = false }
        // A new message, card or reply fades in (not a whole session as it
        // opens: that's a reset).
        add: Transition {
            enabled: Theme.motion > 0
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
        }

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
        readonly property real gutter: Math.max(16, Math.min(64, width * 0.04), (width - Theme.px(Theme.chatMaxWidth)) / 2)
        readonly property real contentWidthLimit: Math.max(120, width - 2 * gutter)
        // A turn's text sits right of the avatars (narrow: no avatars, the
        // whole width for the text).
        readonly property bool compact: contentWidthLimit < Theme.px(480)
        readonly property real indent: compact ? 0 : Theme.px(46)
        readonly property real columnX: (width - contentWidthLimit) / 2 + indent
        readonly property real columnWidth: Math.max(80, contentWidthLimit - indent)
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
        // `follow`: the user may have scrolled away in the meantime.
        function followEnd() { if (follow) scrollToEnd() }
        // Any scroll we didn't cause decides follow from position alone, so
        // the scrollbar, wheel, keys and flicks all count the same.
        onContentYChanged: {
            if (!autoScrolling) follow = nearEnd()
            Qt.callLater(root.updateActivePrompt)
        }
        // Follow only real new content. ListView also changes contentHeight
        // whenever it re-estimates unmeasured rows (e.g. the first scroll up
        // a freshly opened session); treating that as new content snapped
        // the view back to the end mid-scroll.
        property bool contentChanging: false
        Timer { id: settleTimer; interval: 400; onTriggered: view.contentChanging = false }
        function noteContent() {
            contentChanging = true
            settleTimer.restart()
            if (follow) Qt.callLater(followEnd)
        }
        Connections {
            target: root.session
            function onRowsInserted() { view.noteContent(); promptsTimer.restart() }
            function onRowsRemoved() { view.noteContent(); promptsTimer.restart() }
            function onDataChanged() { view.noteContent() }
            function onModelReset() { view.noteContent(); promptsTimer.restart() }
        }
        onContentHeightChanged: if (follow && contentChanging) Qt.callLater(followEnd)


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
                text: turn ? "↶ Undo these changes (" + (turn.files === 1 ? "1 file" : turn.files + " files") + ")" : ""
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
                visible: root.session.status === "busy"
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
                    // Edit & resend / fork, on hover.
                    Row {
                        z: 2
                        visible: userHover.hovered && ["pi", "omp", "claude", "opencode"].includes(root.session.harness)
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
                            ToolTip.text: root.session.harness === "omp"
                                ? "Branch the session here and put this message back in the composer"
                                : "Fork into a new session from here and put this message back in the composer"
                            onClicked: root.editRequested(userRow.index)
                        }
                        FlatButton {
                            visible: root.session.harness === "omp"
                            text: "Fork"
                            font.pixelSize: Theme.px(11)
                            padding: 3
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: "Copy the session up to this message into a new session"
                            onClicked: root.forkRequested(userRow.index)
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
                        x: view.columnX - 6
                        y: Math.round(userRow.gap / 2) - 6 - height / 2
                        text: "↶ Undo from here"
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
                    // Your avatar (picked from the left rail).
                    UserAvatar {
                        objectName: "turnAvatar"
                        visible: !view.compact
                        x: (view.width - view.contentWidthLimit) / 2
                        y: userRow.gap
                        width: Theme.px(32)
                        height: width
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
                            Label {
                                height: root.headHeight * 0.55
                                verticalAlignment: Text.AlignVCenter
                                text: "You"
                                color: Theme.text
                                font.pixelSize: Theme.px(14.5)
                                font.weight: Font.DemiBold
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
                    required property string html
                    required property string rowKey
                    required property int index
                    readonly property real gap: root.gapAbove("markdown", ListView.previousSection)
                    AgentHead { visible: mdRow.ListView.previousSection === "user"; y: mdRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: md.implicitHeight + gap
                    TextEdit {
                        font.family: Theme.uiFont
                        id: md
                        x: view.columnX
                        y: mdRow.gap
                        width: view.columnWidth
                        textFormat: TextEdit.RichText
                        text: Theme.htmlStyle + parent.html
                        color: Theme.foreground
                        selectionColor: Theme.selection
                        selectedTextColor: Theme.foreground
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        font.pixelSize: Theme.px(15)
                        onLinkActivated: link => Qt.openUrlExternally(link)
                        HoverHandler { cursorShape: md.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
                    }
                }
            }

            DelegateChoice {
                roleValue: "code"
                Item {
                    id: codeRow
                    required property string html
                    required property string text
                    required property string title
                    readonly property real gap: root.gapAbove("code", ListView.previousSection)
                    AgentHead { visible: codeRow.ListView.previousSection === "user"; y: codeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: codeBox.height + gap
                    Rectangle {
                        id: codeBox
                        x: view.columnX
                        y: codeRow.gap
                        width: view.columnWidth
                        height: codeHeader.height + codeFlick.height
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
                                Layout.rightMargin: 4
                                Layout.preferredHeight: 22
                                text: "Copy"
                                font.pixelSize: Theme.px(11)
                                onClicked: clipboardHelper.copyText(codeRow.text)
                            }
                        }
                        Flickable {
                            id: codeFlick
                            anchors.top: codeHeader.bottom
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
                    required property string rowKey
                    required property string text
                    required property string rowState
                    required property string group
                    required property string summary
                    required property bool groupOpen
                    readonly property bool open: view.expanded[rowKey] === true
                    readonly property real gap: root.gapAbove("thinking", ListView.previousSection)
                    AgentHead { visible: thinkRow.ListView.previousSection === "user"; y: thinkRow.gap - height - 10 }
                    // A run sits on one card: rounded where it starts and ends.
                    Rectangle {
                        readonly property bool first: !root.isActivity(thinkRow.ListView.previousSection)
                        readonly property bool last: !root.isActivity(thinkRow.ListView.nextSection) || (thinkRow.folded && first)
                        visible: thinkRow.implicitHeight > 0 && !(thinkRow.folded && !first)
                        z: -1
                        x: view.columnX - 8
                        width: view.columnWidth + 16
                        y: first ? thinkRow.gap - 4 : 0
                        height: thinkRow.implicitHeight - y + (last ? 4 : 0)
                        color: Theme.raised
                        topLeftRadius: first ? 12 : 0
                        topRightRadius: first ? 12 : 0
                        bottomLeftRadius: last ? 12 : 0
                        bottomRightRadius: last ? 12 : 0
                    }
                    // In a folded run (the run's first row keeps its summary line).
                    readonly property bool folded: group !== "" && !root.runOpen(group, groupOpen)
                    readonly property real head: summary !== "" ? 30 : 0
                    width: view.width
                    implicitHeight: folded ? (head > 0 ? gap + head : 0) : thinkCol.implicitHeight + gap + head + 10
                    visible: implicitHeight > 0
                    RunHeader {
                        visible: thinkRow.head > 0
                        x: view.columnX
                        y: thinkRow.gap
                        width: view.columnWidth
                        group: thinkRow.group
                        summary: thinkRow.summary
                        dflt: thinkRow.groupOpen
                    }
                    RailLine {
                        anchors.fill: parent
                        visible: !thinkRow.folded
                        markX: thinkCol.x + 8 + 7
                        markY: thinkCol.y + thinkHead.height / 2
                        from: thinkRow.head > 0 ? thinkRow.gap + 26 : 0
                        up: thinkRow.head > 0 || root.isActivity(thinkRow.ListView.previousSection)
                        down: root.isActivity(thinkRow.ListView.nextSection)
                    }
                    ColumnLayout {
                        id: thinkCol
                        visible: !thinkRow.folded
                        x: view.columnX
                        y: thinkRow.gap + thinkRow.head + 5
                        width: view.columnWidth
                        spacing: 4
                        RowLayout {
                            id: thinkHead
                            Layout.leftMargin: 8
                            spacing: 8
                            Label { Layout.preferredWidth: 14; horizontalAlignment: Text.AlignHCenter; text: "✦"; color: thinkRow.rowState === "streaming" ? Theme.primary : Theme.muted; font.pixelSize: Theme.px(12) }
                            Label {
                                text: (thinkRow.rowState === "streaming" ? "Thinking…" : "Thought") + (thinkRow.open ? "  ▾" : "  ▸")
                                color: Theme.muted
                                font.italic: true
                                font.pixelSize: Theme.px(13)
                            }
                            TapHandler { onTapped: view.toggle(thinkRow.rowKey) }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                        TextEdit {
                            font.family: Theme.uiFont
                            Layout.fillWidth: true
                            Layout.leftMargin: 30
                            visible: thinkRow.open
                            text: thinkRow.text
                            color: Theme.muted
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                            font.pixelSize: Theme.px(13)
                            font.italic: true
                        }
                    }
                }
            }

            DelegateChoice {
                roleValue: "tool"
                Item {
                    id: toolRow
                    required property string rowKey
                    required property string text
                    required property string title
                    required property string subtitle
                    required property string rowState
                    required property string callId
                    required property string group
                    required property string summary
                    required property bool groupOpen
                    readonly property bool open: view.expanded[rowKey] === true
                    readonly property var agents: callId !== "" ? root.subagents.filter(a => a.parentToolCallId === callId) : []
                    // omp waits for your approval to run this call.
                    readonly property bool asking: root.session.hasRequest && root.session.requestIsApproval && root.session.approvalRow === rowKey
                    readonly property var askLines: asking ? root.session.requestTitle.split("\n").slice(1) : []
                    readonly property real gap: root.gapAbove("tool", ListView.previousSection)
                    AgentHead { visible: toolRow.ListView.previousSection === "user"; y: toolRow.gap - height - 10 }
                    // A run sits on one card: rounded where it starts and ends.
                    Rectangle {
                        readonly property bool first: !root.isActivity(toolRow.ListView.previousSection)
                        readonly property bool last: !root.isActivity(toolRow.ListView.nextSection) || (toolRow.folded && first)
                        visible: toolRow.implicitHeight > 0 && !(toolRow.folded && !first)
                        z: -1
                        x: view.columnX - 8
                        width: view.columnWidth + 16
                        y: first ? toolRow.gap - 4 : 0
                        height: toolRow.implicitHeight - y + (last ? 4 : 0)
                        color: Theme.raised
                        topLeftRadius: first ? 12 : 0
                        topRightRadius: first ? 12 : 0
                        bottomLeftRadius: last ? 12 : 0
                        bottomRightRadius: last ? 12 : 0
                    }
                    // In a folded run (the run's first row keeps its summary
                    // line); a call that asks always shows.
                    readonly property bool folded: group !== "" && !root.runOpen(group, groupOpen) && !asking
                    readonly property real head: summary !== "" ? 30 : 0
                    width: view.width
                    implicitHeight: folded ? (head > 0 ? gap + head : 0) : toolBox.height + gap + head
                    visible: implicitHeight > 0
                    RunHeader {
                        visible: toolRow.head > 0
                        x: view.columnX
                        y: toolRow.gap
                        width: view.columnWidth
                        group: toolRow.group
                        summary: toolRow.summary
                        dflt: toolRow.groupOpen
                    }
                    // No box of its own (unless it asks): a line in the run's timeline.
                    RailLine {
                        anchors.fill: parent
                        visible: !toolRow.asking && !toolRow.folded
                        markX: toolBox.x + toolCol.x + 7
                        markY: toolBox.y + toolCol.y + toolHead.height / 2
                        from: toolRow.head > 0 ? toolRow.gap + 26 : 0
                        up: toolRow.head > 0 || root.isActivity(toolRow.ListView.previousSection)
                        down: root.isActivity(toolRow.ListView.nextSection)
                    }
                    Rectangle {
                        id: toolBox
                        visible: !toolRow.folded
                        x: view.columnX
                        y: toolRow.gap + toolRow.head
                        width: view.columnWidth
                        height: toolCol.implicitHeight + (toolRow.asking ? 20 : 10)
                        radius: 8
                        color: toolRow.asking ? Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.06)
                             : toolHover.hovered && toolRow.text !== "" ? Theme.hover : "transparent"
                        border.color: toolRow.asking ? Theme.warning : "transparent"
                        HoverHandler { id: toolHover }
                        ColumnLayout {
                            id: toolCol
                            x: 8
                            y: toolRow.asking ? 10 : 5
                            width: parent.width - 18
                            spacing: 6
                            RowLayout {
                                id: toolHead
                                Layout.fillWidth: true
                                spacing: 8
                                Label {
                                    Layout.preferredWidth: 14
                                    horizontalAlignment: Text.AlignHCenter
                                    text: toolRow.asking ? "?" : toolRow.rowState === "running" ? "●" : toolRow.rowState === "error" ? "✗" : toolRow.rowState === "done" ? "✓" : "○"
                                    color: toolRow.asking ? Theme.warning : toolRow.rowState === "running" ? Theme.primary : toolRow.rowState === "error" ? Theme.error : Theme.success
                                    font.bold: toolRow.asking
                                    font.pixelSize: Theme.px(12)
                                }
                                Label { text: toolRow.title; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(13) }
                                Label {
                                    Layout.fillWidth: true
                                    text: toolRow.subtitle
                                    color: Theme.muted
                                    elide: Text.ElideRight
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.mpx(12)
                                }
                                FlatButton {
                                    // edit/write/patch tools: jump to the file's diff.
                                    visible: /^(edit|multi_?edit|write|apply_?patch|patch|create)/i.test(toolRow.title)
                                             && toolRow.subtitle !== "" && toolRow.rowState !== "error"
                                    text: "diff"
                                    font.pixelSize: Theme.px(11)
                                    padding: 2
                                    outlined: false
                                    accent: Theme.info
                                    onClicked: root.openDiff(toolRow.subtitle.replace(/…$/, ""))
                                }
                                Label { text: toolRow.open ? "▾" : "▸"; color: Theme.muted; visible: toolRow.text !== "" }
                            }
                            // The subagents this call started (omp's task tool), as a tree.
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.leftMargin: 22
                                visible: toolRow.agents.length > 0
                                spacing: 6
                                Rectangle { Layout.fillHeight: true; Layout.leftMargin: 4; implicitWidth: 2; radius: 1; color: Theme.border }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Repeater {
                                        model: toolRow.agents
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
                            // The approval: omp's details (reason, command…) and the answer.
                            Label {
                                Layout.fillWidth: true
                                visible: text !== ""
                                text: toolRow.askLines.filter(l => l.startsWith("Reason:")).join("\n")
                                color: Theme.warning
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                readonly property string details: toolRow.askLines.filter(l => !l.startsWith("Reason:")).join("\n").trim()
                                visible: toolRow.asking && details !== ""
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
                                visible: toolRow.asking
                                spacing: 6
                                Label {
                                    Layout.fillWidth: true
                                    text: root.approvalQuestion(toolRow.title, toolRow.subtitle)
                                    color: Theme.foreground
                                    font.pixelSize: Theme.px(12)
                                    elide: Text.ElideRight
                                }
                                FlatButton { text: "Deny"; accent: Theme.error; font.pixelSize: Theme.px(12); onClicked: root.session.answerValue("Deny") }
                                FlatButton {
                                    objectName: "alwaysAllowInline"
                                    visible: root.session.requestOptions.indexOf("Always allow") >= 0
                                    text: parent.width < 420 ? "Always" : "Always allow"
                                    font.pixelSize: Theme.px(12)
                                    ToolTip.visible: hovered
                                    ToolTip.delay: 400
                                    ToolTip.text: { try { return "Remember: " + (JSON.parse(root.session.requestJson).alwaysHint || "this") } catch (e) { return "" } }
                                    onClicked: root.session.answerValue("Always allow")
                                }
                                FlatButton { text: "Approve"; accent: Theme.success; font.pixelSize: Theme.px(12); onClicked: root.session.answerValue("Approve") }
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.leftMargin: 22
                                visible: toolRow.open && toolRow.text !== ""
                                implicitHeight: Math.min(outText.implicitHeight + 12, 320)
                                color: Theme.codeBg
                                radius: 4
                                ScrollView {
                                    anchors.fill: parent
                                    anchors.margins: 6
                                    clip: true
                                    TextEdit {
                                        id: outText
                                        text: toolRow.text
                                        color: Theme.foreground
                                        readOnly: true
                                        selectByMouse: true
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.mpx(12)
                                    }
                                }
                            }
                        }
                        TapHandler { enabled: toolRow.text !== ""; onTapped: view.toggle(toolRow.rowKey) }
                    }
                }
            }

            DelegateChoice {
                roleValue: "notice"
                Item {
                    id: noticeRow
                    required property string rowKey
                    required property string text
                    required property string title
                    // Startup notices can be huge (e.g. omp listing every MCP tool).
                    readonly property bool long: text.length > 240 || text.split("\n").length > 3
                    readonly property bool open: !long || view.expanded[rowKey] === true
                    readonly property real gap: root.gapAbove("notice", ListView.previousSection)
                    AgentHead { visible: noticeRow.ListView.previousSection === "user"; y: noticeRow.gap - height - 10 }
                    width: view.width
                    implicitHeight: noticeText.implicitHeight + 4 + gap
                    Label {
                        id: noticeText
                        x: view.columnX
                        y: noticeRow.gap
                        width: view.columnWidth
                        text: (noticeRow.long ? (noticeRow.open ? "▾ " : "▸ ") : "") + noticeRow.text
                        wrapMode: Text.Wrap
                        maximumLineCount: noticeRow.open ? 100000 : 2
                        elide: Text.ElideRight
                        font.pixelSize: Theme.px(12)
                        color: noticeRow.title === "error" ? Theme.error : noticeRow.title === "warning" ? Theme.warning : Theme.muted
                        TapHandler { enabled: noticeRow.long; onTapped: view.toggle(noticeRow.rowKey) }
                        HoverHandler { enabled: noticeRow.long; cursorShape: Qt.PointingHandCursor }
                    }
                }
            }
        }

        FlatButton {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 16
            visible: !view.follow
            text: "↓ Latest"
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
