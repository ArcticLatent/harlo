import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The active session: header, transcript, blocking dialogs and composer.
Rectangle {
    id: root
    color: Theme.canvas
    property alias session: session
    property alias transcript: transcript
    property alias composerItem: composer
    property alias fileEditor: fileEditor
    property alias askPanel: askPanel
    property alias draftView: draftView
    property alias subagentDock: subagentDock
    property alias todoDock: todoDock
    property alias goalDock: goalDock
    property alias requestDock: requestDock
    property alias branchDialog: branchDialog
    property alias subagentDialog: subagentDialog
    property alias statsView: statsView
    property alias diffView: diffView
    property alias unsavedDialog: unsavedDialog
    property alias undoDialog: undoDialog
    property alias reviewStore: reviewStore
    // Review comments on diffs, by repository (`{ root: [ { path, side, line,
    // code, text } ] }`), until sent or cleared.
    QtObject {
        id: reviewStore
        property var all: ({})
        function put(repo, index, c) {
            const a = Object.assign({}, all), list = (a[repo] || []).slice()
            if (index >= 0) list[index] = c
            else list.push(c)
            list.sort((x, y) => x.path === y.path ? x.line - y.line : x.path < y.path ? -1 : 1)
            a[repo] = list
            all = a
        }
        function remove(repo, index) {
            const a = Object.assign({}, all), list = (a[repo] || []).slice()
            list.splice(index, 1)
            a[repo] = list
            all = a
        }
        function clear(repo) {
            const a = Object.assign({}, all)
            delete a[repo]
            all = a
        }
    }
    UndoDialog { id: undoDialog; session: root.session; onUndone: { root.filesUndone(); root.session.loadUndo() } }
    // Which messages can be undone to: when the transcript or the session's
    // state changes (a turn starts or ends), a moment later.
    Timer { id: undoTimer; interval: 800; onTriggered: root.session.loadUndo() }
    Connections {
        target: root.session
        function onStatusChanged() { undoTimer.restart() }
        function onKeyChanged() { undoTimer.restart() }
        function onRowsInserted() { undoTimer.restart() }
        function onModelReset() { undoTimer.restart() }
    }
    // Back on the session's tab (files may have changed meanwhile): again.
    Connections {
        target: Workspace
        function onActiveKeyChanged() { if (Workspace.activeKey === root.session.key) undoTimer.restart() }
    }
    readonly property bool hasSession: session.key !== ""
    // Set by the window: the changes panel state for the header toggle.
    property int changesCount: 0
    property bool changesOpen: false
    signal toggleChanges()
    signal openDiff(string path)
    signal fileSaved(string path)
    signal addProjectRequested()
    /// A turn was undone: the folder's files changed.
    signal filesUndone()

    // File tabs (`file:<path>`) show the editor; the chat session stays
    // bound underneath so the side panels keep following it.
    readonly property bool isFile: Workspace.activeKey.startsWith("file:")
    readonly property string filePath: isFile ? Workspace.activeKey.substring(5) : ""
    // App views (`view:stats`) cover the chat the same way.
    readonly property bool isView: Workspace.activeKey.startsWith("view:")
    // The new-session page also shows when no tab is open.
    readonly property bool isDraft: Workspace.activeKey === "view:new" || Workspace.activeKey === ""
    // The diff tab (`diff:<0|1>:<path>`): a file's changes side by side.
    // (It keeps its file while another tab is in front, so coming back
    // finds it as it was.)
    readonly property bool isDiff: Workspace.activeKey.startsWith("diff:")
    property string diffPath: ""
    property bool diffStaged: false
    function followDiff() {
        if (!isDiff) return
        diffStaged = Workspace.activeKey.charAt(5) === "1"
        diffPath = Workspace.activeKey.substring(7)
    }
    onIsDiffChanged: followDiff()
    Connections { target: Workspace; function onActiveKeyChanged() { root.followDiff() } }
    readonly property bool covered: isFile || isView || isDraft || isDiff
    // Unsaved editor text per file path.
    property var fileDrafts: ({})

    // Leaving a file with unsaved edits (open another, close the tab) asks
    // Save / Discard / Cancel first; `then` runs once the edits are handled.
    function guardUnsaved(then) {
        if (isFile && fileDrafts[filePath] !== undefined) {
            unsavedDialog.then = then
            unsavedDialog.open()
        } else {
            then()
        }
    }
    function requestOpenFile(path) {
        if (path === filePath) { Workspace.openFileTab(path); return }
        guardUnsaved(() => Workspace.openFileTab(path))
    }
    function requestCloseTab(key) {
        if (key.startsWith("file:") && key === Workspace.activeKey)
            guardUnsaved(() => Workspace.closeTab(key))
        else if (key.startsWith("file:") && fileDrafts[key.substring(5)] !== undefined)
            Workspace.open(key)   // show it first, so the question is about the visible file
        else
            Workspace.closeTab(key)
    }
    AppDialog {
        id: unsavedDialog
        property var then: null
        title: "Save changes to " + root.filePath.split("/").pop() + "?"
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Save | Dialog.Discard | Dialog.Cancel
        Label {
            width: 340
            wrapMode: Text.Wrap
            text: "This file has unsaved edits."
            color: Theme.muted
        }
        onAccepted: {
            fileEditor.save()
            if (root.fileDrafts[root.filePath] === undefined && then) then()
        }
        onDiscarded: {
            delete root.fileDrafts[root.filePath]
            root.fileDrafts = Object.assign({}, root.fileDrafts)
            close()
            if (then) then()
        }
    }

    ChatSession { id: session }

    // Drop image files anywhere on the chat to attach them.
    DropArea {
        anchors.fill: parent
        enabled: root.hasSession
        onEntered: drag => { drag.accepted = drag.hasUrls }
        onDropped: drop => {
            let rejected = 0
            for (const url of drop.urls)
                if (!session.addAttachment(url.toString())) rejected++
            dropHint.text = rejected > 0 ? "Folders can't be attached; drop the files inside instead." : ""
        }
        Rectangle {
            anchors.fill: parent
            visible: parent.containsDrag
            color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.13)
            border.color: Theme.primary
            z: 10
            Label { anchors.centerIn: parent; text: "Drop files to attach"; color: Theme.primary; font.pixelSize: Theme.px(16) }
        }
    }

    // Unsent text per session, so switching sessions keeps drafts.
    property var drafts: ({})

    // A branch or fork hands the message back for editing.
    Connections {
        target: root.session
        function onEditRequested(text) {
            composer.text = text
            composer.focusInput()
        }
        function onStatusChanged() { root.refreshBranches() }
    }
    // Branches of the active session's file (header button when > 1).
    property int branchCount: 0
    // (Read off the UI thread: a remote session's file over SSH.)
    function refreshBranches() {
        const file = Workspace.sessionFile(Workspace.activeKey)
        if (file === "") branchCount = 0
        else Workspace.requestSessionBranches(file)
    }
    Connections {
        target: Workspace
        function onSessionBranchesReady(path, json) {
            if (path === Workspace.sessionFile(Workspace.activeKey)) root.branchCount = JSON.parse(json).length
        }
    }
    BranchDialog {
        id: branchDialog
        onContinued: key => {}
    }

    function bindActive() {
        // No tab open: let go of the last session (side panels follow).
        if (Workspace.activeKey === "" && session.key !== "") {
            session.bind("")
            return
        }
        if (covered)
            return
        if (session.key !== "")
            drafts[session.key] = composer.text
        session.bind(Workspace.activeKey)
        Qt.callLater(root.refreshBranches)
        composer.text = drafts[Workspace.activeKey] || ""
        transcript.scrollToEnd()
        composer.focusInput()
    }
    Component.onCompleted: bindActive()
    Connections {
        target: Workspace
        function onActiveKeyChanged() { root.bindActive() }
    }

    // A search result: open at message `index` once its rows have loaded.
    property string pendingReveal: ""
    property int revealTries: 0
    function revealMessage(index) {
        pendingReveal = "h" + index + ":"
        revealTries = 0
        revealTimer.restart()
    }
    Timer {
        id: revealTimer
        interval: 150
        repeat: true
        onTriggered: {
            const i = session.indexOfKeyPrefix(root.pendingReveal)
            if (i >= 0) { transcript.revealIndex(i); stop(); root.pendingReveal = "" }
            else if (++root.revealTries > 40) { stop(); root.pendingReveal = "" }
        }
    }

    function submit(text, mode) {
        if ((text.trim() === "" && session.attachments.length === 0) || !hasSession)
            return
        // Held at a spending limit: the text stays (the banner says why).
        if (Workspace.spendHold(session.key) !== "")
            return
        const busy = session.status === "busy"
        session.send(text, busy ? (mode === "steer" ? "steer" : "followUp") : "normal")
        composer.text = ""
        drafts[session.key] = ""
        transcript.scrollToEnd()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TabStrip {
            Layout.fillWidth: true
            visible: Workspace.tabs.length > 0
            dirtyPaths: root.fileDrafts
            onCloseRequested: key => root.requestCloseTab(key)
        }

        FileEditor {
            id: fileEditor
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isFile
            path: root.filePath
            drafts: root.fileDrafts
            onDraftsEdited: root.fileDrafts = Object.assign({}, root.fileDrafts)
            onSaved: p => root.fileSaved(p)
        }
        DiffView {
            id: diffView
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isDiff
            path: root.diffPath
            staged: root.diffStaged
            onOpenFile: p => root.requestOpenFile(p)
            // (The Changes panel catches up.)
            onRepoChanged: root.fileSaved(root.diffPath)
            // (Off in Settings → Git: no comments. Read as a diff opens.)
            review: root.isDiff && Workspace.setting("review_comments") !== "0" ? reviewStore : null
            // The session the diff is for: this view's, when the file is in its folder.
            readonly property string cwd: root.session.cwd
            canSend: root.session.key !== "" && cwd !== "" && git.root !== ""
                     && (root.diffPath.startsWith(cwd + "/") || cwd === git.root || cwd.startsWith(git.root + "/"))
            sessionTitle: root.session.title !== "" ? root.session.title : "this session"
            onSendReview: message => {
                root.session.send(message, root.session.status === "busy" ? "followUp" : "")
                Workspace.open(root.session.key)
            }
        }
        DraftView {
            id: draftView
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isDraft
            onStartedSession: (key, askFirst) => {
                if (key !== "" && askFirst)
                    session.setAskFirstForSession(true)
                if (Workspace.tabs.some(t => t.startsWith("view:new\t")))
                    Workspace.closeTab("view:new")
            }
            onAddProjectRequested: root.addProjectRequested()
        }
        RunView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Workspace.activeKey.startsWith("view:run:")
            runId: visible ? Workspace.activeKey.substring(9) : ""
        }
        StatsView {
            id: statsView
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Workspace.activeKey === "view:stats"
        }

        // Header: the session's name, and what runs it under it.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: headerRow.implicitHeight + 30
            color: Theme.canvas
            visible: root.hasSession && !root.covered
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline }
            RowLayout {
                id: headerRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 28
                anchors.rightMargin: 18
                spacing: 6
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 3
                    Label {
                        Layout.fillWidth: true
                        text: session.title || Workspace.activeTitle || "New session"
                        color: Theme.text
                        font.pixelSize: Theme.px(19)
                        font.weight: Font.DemiBold
                        font.letterSpacing: -0.2
                        elide: Text.ElideRight
                    }
                    Row {
                        Layout.fillWidth: true
                        spacing: 12
                        clip: true
                        // What runs this session.
                        Row {
                            objectName: "sessionProviderChip"
                            visible: session.harness !== ""
                            spacing: 6
                            ProviderMark { anchors.verticalCenter: parent.verticalCenter; provider: session.harness; tip: ""; implicitWidth: 13; implicitHeight: 13; color: Theme.agentColor(session.harness).ink }
                            Label { anchors.verticalCenter: parent.verticalCenter; text: Theme.harnessLabel(session.harness); color: Theme.textMuted; font.pixelSize: Theme.px(13) }
                        }
                        Label {
                            visible: session.modelName !== ""
                            text: session.modelName + (session.thinking ? " · " + session.thinking : "")
                            color: Theme.textMuted
                            font.pixelSize: Theme.px(13)
                        }
                        Label {
                            visible: session.status !== "" && session.status !== "idle" && session.status !== "offline"
                            text: session.status === "busy" ? "working" : session.status
                            color: Theme.statusColor(session.status)
                            font.pixelSize: Theme.px(13)
                        }
                    }
                }
                FlatButton {
                    visible: root.branchCount > 1
                    text: root.branchCount + " branches"
                    outlined: false
                    accent: Theme.textMuted
                    font.pixelSize: Theme.px(13)
                    ToolTip.visible: hovered
                    ToolTip.text: "Browse this session's branches"
                    onClicked: branchDialog.show(Workspace.sessionFile(Workspace.activeKey), session.harness, session.cwd)
                }
                FlatButton {
                    text: "Changes"
                    outlined: false
                    font.pixelSize: Theme.px(13)
                    accent: root.changesOpen ? Theme.accent : Theme.textMuted
                    ToolTip.visible: hovered
                    ToolTip.text: root.changesCount > 0
                        ? root.changesCount + " changed file" + (root.changesCount === 1 ? "" : "s") + Keymap.hint("changesPanel")
                        : "Working-tree changes" + Keymap.hint("changesPanel")
                    onClicked: root.toggleChanges()
                    CountBadge { count: root.changesCount; anchors.top: parent.top; anchors.right: parent.right }
                }
            }
        }

        Label {
            Layout.fillWidth: true
            visible: Workspace.errorText !== ""
            text: Workspace.errorText
            color: Theme.error
            wrapMode: Text.Wrap
            padding: 10
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !root.covered && !composer.expanded

            TranscriptView {
                id: transcript
                anchors.fill: parent
                // `root.session`: a bare `session` here would name this property itself.
                session: root.session
                onOpenDiff: path => root.openDiff(path)
                onEditRequested: row => root.session.editAndResend(row)
                onForkRequested: row => root.session.forkAt(row)
                onUndoRequested: key => undoDialog.show(key)
                onSubagentRequested: (agent, steer) => subagentDialog.show(agent, steer)
                promptRail: Workspace.setting("prompt_rail") !== "0"
                onPromptRailFinished: composer.focusInput()
                visible: root.hasSession
            }

            Column {
                anchors.centerIn: parent
                visible: !root.hasSession
                spacing: 10
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "harlo"
                    color: Theme.primary
                    font.pixelSize: Theme.px(28)
                    font.bold: true
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: Workspace.projectCount === 0
                        ? "Add a project folder to get started, or open one of your sessions."
                        : "Pick a session on the left, or start a new one with +."
                    color: Theme.muted
                }
            }

        }

        // omp's `ask`: all of one call's questions together.
        AskPanel {
            id: askPanel
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            visible: session.hasRequest && session.requestMethod === "ask" && !root.covered
            request: visible && session.requestJson !== "" ? JSON.parse(session.requestJson) : ({})
            maxQuestionsHeight: Math.max(160, root.height * 0.45)
            onSubmitted: result => session.answerAsk(JSON.stringify(result))
            onSkipped: session.cancelRequest()
        }

        // Blocking dialogs from the harness (approvals, questions, inputs).
        Rectangle {
            id: requestDock
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            visible: session.hasRequest && session.requestMethod !== "ask" && !root.covered
            // `select` options may carry descriptions (`optionDetails`).
            readonly property var details: {
                if (!visible || session.requestJson === "") return []
                const r = JSON.parse(session.requestJson)
                return r.optionDetails || []
            }
            readonly property bool detailed: session.requestMethod === "select" && details.some(d => d && d.description)
            // An approval shown on its tool row: just a reminder here.
            readonly property bool inlineApproval: session.requestIsApproval && session.approvalRow !== ""
            implicitHeight: reqCol.implicitHeight + 20
            radius: 8
            color: Theme.elevated
            border.color: session.requestIsApproval ? Theme.warning : Theme.primary
            ColumnLayout {
                id: reqCol
                x: 12
                y: 10
                width: parent.width - 24
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    visible: requestDock.inlineApproval
                    spacing: 6
                    Label {
                        Layout.fillWidth: true
                        readonly property string tool: session.requestTitle.split("\n")[0].replace(/^Allow tool:\s*/, "")
                        text: tool === "goal" ? "Approval needed: omp wants to update its goal" : "Approval needed: " + tool
                        color: Theme.foreground
                        font.bold: true
                        elide: Text.ElideRight
                    }
                    FlatButton { text: "Show"; onClicked: transcript.revealIndex(session.indexOfKey(session.approvalRow)) }
                    FlatButton { text: "Deny"; accent: Theme.error; onClicked: session.answerValue("Deny") }
                    // (Always allow: on the tool row, which Show reaches.)
                    FlatButton { text: "Approve"; accent: Theme.success; onClicked: session.answerValue("Approve") }
                }
                Label {
                    Layout.fillWidth: true
                    visible: !requestDock.inlineApproval
                    text: session.requestTitle
                    color: Theme.foreground
                    font.bold: true
                    wrapMode: Text.Wrap
                }
                Label {
                    Layout.fillWidth: true
                    visible: text !== "" && !requestDock.inlineApproval
                    text: session.requestMessage
                    color: Theme.muted
                    wrapMode: Text.Wrap
                }
                TextArea {
                    id: reqInput
                    Layout.fillWidth: true
                    visible: session.requestMethod === "input" || session.requestMethod === "editor"
                    text: session.requestPrefill
                    color: Theme.foreground
                    wrapMode: TextEdit.Wrap
                    background: Rectangle { radius: 5; color: Theme.background; border.color: Theme.border }
                }
                // Options with descriptions: one row each.
                Repeater {
                    model: requestDock.detailed ? session.requestOptions : []
                    Rectangle {
                        id: detailRow
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        implicitHeight: detailCol.implicitHeight + 14
                        radius: 7
                        color: detailHover.hovered ? Theme.hover : "transparent"
                        border.color: detailHover.hovered ? Theme.primary : Theme.border
                        HoverHandler { id: detailHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: session.answerValue(detailRow.modelData) }
                        ColumnLayout {
                            id: detailCol
                            x: 10
                            y: 7
                            width: parent.width - 20
                            spacing: 2
                            Label { text: detailRow.modelData; color: Theme.foreground; font.pixelSize: Theme.px(13) }
                            Label {
                                Layout.fillWidth: true
                                visible: text !== ""
                                text: (requestDock.details[detailRow.index] || {}).description || ""
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }
                Flow {
                    Layout.fillWidth: true
                    visible: !requestDock.inlineApproval
                    spacing: 6
                    Repeater {
                        model: session.requestMethod === "select" && !requestDock.detailed ? session.requestOptions : []
                        FlatButton {
                            required property string modelData
                            text: modelData
                            accent: modelData === "Deny" ? Theme.error : modelData === "Approve" ? Theme.success : Theme.foreground
                            onClicked: session.answerValue(modelData)
                        }
                    }
                    FlatButton { visible: session.requestMethod === "confirm"; text: "Yes"; accent: Theme.success; onClicked: session.answerConfirm(true) }
                    FlatButton { visible: session.requestMethod === "confirm"; text: "No"; onClicked: session.answerConfirm(false) }
                    FlatButton { visible: reqInput.visible; text: "Submit"; accent: Theme.primary; onClicked: session.answerValue(reqInput.text) }
                    FlatButton { text: "Dismiss"; accent: Theme.muted; onClicked: session.cancelRequest() }
                }
            }
        }

        // omp goal mode.
        GoalDock {
            id: goalDock
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            session: root.session
            visible: goal !== null && dismissed !== goal.id && !root.covered
        }

        // omp's todo list.
        TodoDock {
            id: todoDock
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            session: root.session
            visible: tasks.length > 0 && !root.covered
        }

        // omp subagents (task tool).
        SubagentDock {
            id: subagentDock
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            session: root.session
            visible: agents.length > 0 && !root.covered
            onViewRequested: (agent, steer) => subagentDialog.show(agent, steer)
        }
        SubagentDialog {
            id: subagentDialog
            parentSession: root.session
        }

        // Messages waiting for the current run.
        Column {
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter + 4
            Layout.rightMargin: transcript.gutter + 4
            Layout.topMargin: 6
            visible: session.queued.length > 0 && !root.covered
            spacing: 2
            Repeater {
                model: session.queued
                Item {
                    id: queuedRow
                    required property string modelData
                    required property int index
                    // omp can drop queued messages and send follow-ups now.
                    readonly property bool editable: session.harness === "omp" || session.harness === "demo"
                    width: parent.width
                    implicitHeight: 22
                    HoverHandler { id: queuedHover }
                    RowLayout {
                        anchors.fill: parent
                        spacing: 6
                        Label {
                            Layout.fillWidth: true
                            text: "⏳ " + queuedRow.modelData
                            color: Theme.muted
                            elide: Text.ElideRight
                            font.pixelSize: Theme.px(12)
                        }
                        FlatButton {
                            visible: queuedRow.editable && queuedHover.hovered && queuedRow.modelData.startsWith("Next:")
                            text: "Send now"
                            font.pixelSize: Theme.px(11)
                            padding: 2
                            outlined: false
                            accent: Theme.primary
                            onClicked: session.promoteQueued(queuedRow.index)
                        }
                        FlatButton {
                            visible: queuedRow.editable && queuedHover.hovered
                            text: "Remove"
                            font.pixelSize: Theme.px(11)
                            padding: 2
                            outlined: false
                            accent: Theme.muted
                            onClicked: session.removeQueued(queuedRow.index)
                        }
                    }
                }
            }
        }

        // A spending limit holds this session (or every one today).
        Rectangle {
            id: spendBanner
            objectName: "spendBanner"
            readonly property var spend: Workspace.spendJson !== "" ? JSON.parse(Workspace.spendJson) : ({ held: {}, day: "", limits: { session: 0, day: 0 } })
            readonly property string why: root.hasSession ? (spend.held[root.session.key] || "") : ""
            readonly property bool daily: why !== "" && why === spend.day
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.topMargin: 8
            visible: why !== "" && !root.covered
            implicitHeight: spendRow.implicitHeight + 18
            radius: 10
            color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.1)
            border.color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.35)
            RowLayout {
                id: spendRow
                anchors.fill: parent
                anchors.margins: 9
                anchors.leftMargin: 12
                spacing: 10
                Icon { name: "alert"; color: Theme.warning; implicitWidth: 16; implicitHeight: 16 }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: spendBanner.why + " harlo holds its messages until you allow more."
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12.5)
                }
                FlatButton {
                    objectName: "spendAllow"
                    readonly property real amount: spendBanner.daily ? spendBanner.spend.limits.day : spendBanner.spend.limits.session
                    text: "Allow $" + (amount % 1 === 0 ? amount.toFixed(0) : amount.toFixed(2)) + " more" + (spendBanner.daily ? " today" : "")
                    font.pixelSize: Theme.px(12)
                    onClicked: Workspace.allowMoreSpending(spendBanner.daily ? "" : root.session.key)
                }
            }
        }

        // Pending attachments
        Flow {
            Layout.fillWidth: true
            Layout.leftMargin: transcript.gutter + 2
            Layout.rightMargin: transcript.gutter + 2
            Layout.topMargin: 6
            visible: (session.attachments.length > 0 || dropHint.text !== "") && !root.covered
            spacing: 6
            Repeater {
                model: session.attachments
                AttachmentChip {
                    required property string modelData
                    required property int index
                    path: modelData
                    removable: true
                    onRemoveClicked: session.removeAttachment(index)
                }
            }
            Label { id: dropHint; visible: text !== ""; color: Theme.warning; font.pixelSize: Theme.px(12) }
        }

        Composer {
            id: composer
            Layout.fillWidth: true
            Layout.fillHeight: expanded
            Layout.leftMargin: transcript.gutter
            Layout.rightMargin: transcript.gutter
            Layout.bottomMargin: 16
            Layout.topMargin: 8
            visible: root.hasSession && !root.covered
            session: root.session
            onSubmitted: (text, mode) => root.submit(text, mode)
        }
    }
}
