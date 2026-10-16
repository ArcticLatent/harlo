import QtQuick
import QtQuick.Effects
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import harlo

// Message input with `/command` and `@file` completion, model and thinking
// pickers, and prompt history.
Rectangle {
    id: root
    required property ChatSession session
    property alias text: input.text
    readonly property bool completionVisible: completion.visible
    readonly property bool inputFocused: input.activeFocus
    signal submitted(string text, string mode)

    // Sent prompts, newest last; Up/Down in an empty composer walks them.
    property var history: []
    property int historyIndex: -1

    function focusInput() { input.forceActiveFocus() }
    /// The bar shows icons without labels (a narrow chat).
    readonly property bool compactBar: bar.tight
    /// Expanded: the composer takes the chat's height for a long message
    /// (the chat view hides the transcript meanwhile).
    property bool expanded: false
    function toggleExpanded() { expanded = !expanded; input.forceActiveFocus() }
    onSessionChanged: expanded = false

    // Per-session modes, kept by the workspace (which writes omp's overlay).
    /// omp's approval mode for this session; "" for pi.
    readonly property string approvalMode: Workspace.modesRevision >= 0 ? Workspace.approvalMode(session.key) : ""
    readonly property bool planOn: Workspace.modesRevision >= 0 && Workspace.planMode(session.key)
    function togglePlan() { Workspace.setPlanMode(session.key, !planOn) }
    readonly property var approvalModes: ["always-ask", "write", "yolo"]
    function cycleApproval() {
        const i = approvalModes.indexOf(approvalMode)
        Workspace.setApprovalMode(session.key, approvalModes[(i + 1) % approvalModes.length])
    }
    Binding { target: root.session; property: "planMode"; value: root.planOn }

    // ---- dictation (built-in whisper.cpp; Settings → Voice) ---------------
    /// This composer started it (others ignore its text).
    property bool dictationMine: false
    /// First use: the model is downloading; listening starts once it's in.
    property bool awaitingModel: false
    // (Both read every time, so the binding always follows the state.)
    readonly property bool dictationActive: { const recordingOrBusy = Dictation.state !== "idle"; return dictationMine && (awaitingModel || recordingOrBusy) }
    readonly property bool recording: dictationMine && Dictation.state === "recording"
    property int dictationSeconds: 0
    property real micLevel: 0
    /// A problem to show in the bar for a while.
    property string dictationNote: ""
    readonly property string whisperModel: Workspace.setting("dictation_model") || "base"
    function showNote(t) { dictationNote = t; noteTimer.restart() }
    function toggleDictation() {
        if (recording) { finishDictation(); return }
        if (awaitingModel || Dictation.state !== "idle") return
        dictationNote = ""
        dictationMine = true
        input.forceActiveFocus()
        if (!Dictation.modelReady(whisperModel)) {
            awaitingModel = true
            if (Dictation.downloading === "") Dictation.download(whisperModel)
            return
        }
        startRecording()
    }
    function startRecording() {
        dictationSeconds = 0
        Dictation.start(session.key)
        if (Dictation.state !== "recording") {
            dictationMine = false
            showNote(Dictation.error)
        }
    }
    function finishDictation() { Dictation.stop(whisperModel, Workspace.setting("dictation_language") || "auto") }
    function cancelDictation() {
        if (awaitingModel) Dictation.cancelDownload()
        else Dictation.cancel()
        awaitingModel = false
        dictationMine = false
    }
    /// Puts dictated text at the cursor, spaced from what's around it.
    function insertDictation(t) {
        const pos = input.cursorPosition
        const before = input.text.substring(0, pos), after = input.text.substring(pos)
        const lead = before !== "" && !/\s$/.test(before) ? " " : ""
        const trail = after !== "" && !/^\s/.test(after) ? " " : ""
        input.insert(pos, lead + t + trail)
        input.cursorPosition = pos + lead.length + t.length
        input.forceActiveFocus()
    }
    Connections {
        target: Dictation
        function onTranscribed(text, target) { if (root.dictationMine) { root.dictationMine = false; root.insertDictation(text) } }
        function onStateChanged() {
            if (Dictation.state === "idle" && root.dictationMine && !root.awaitingModel) {
                root.dictationMine = false
                if (Dictation.error !== "") root.showNote(Dictation.error)
            }
        }
        function onDownloadingChanged() {
            if (Dictation.downloading !== "" || !root.awaitingModel) return
            root.awaitingModel = false
            if (Dictation.modelReady(root.whisperModel)) {
                root.startRecording()
            } else {
                root.dictationMine = false
                if (Dictation.downloadError !== "") root.showNote(Dictation.downloadError)
            }
        }
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.recording
        // Long recordings stop at five minutes.
        onTriggered: { root.dictationSeconds++; if (root.dictationSeconds >= 300) root.finishDictation() }
    }
    Timer { interval: 80; repeat: true; running: root.recording; onTriggered: root.micLevel = Dictation.level(); onRunningChanged: root.micLevel = 0 }
    Timer { id: noteTimer; interval: 8000; onTriggered: root.dictationNote = "" }
    // Test hook.
    function openBtw() { btw.open() }
    property alias btwPopup: btw
    property alias goalPopup: goalPopup
    function cursorToEnd() { input.cursorPosition = input.length }
    function insertText(t) { input.insert(input.cursorPosition, t); input.forceActiveFocus() }
    // For `--selftest`: popups live outside the page item.
    function grabCompletion(path) { list.grabToImage(r => r.saveToFile(path)) }

    implicitHeight: Math.min(Math.max(input.implicitHeight, 44), 240) + 48
    radius: 14
    color: Theme.raised
    border.color: input.activeFocus ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.6) : Theme.controlBorder

    // The composer's shadow: a soft drop pulled in under the card.
    RectangularShadow {
        z: -1
        anchors.fill: parent
        radius: root.radius
        offset.y: Theme.isLight ? 8 : 10
        blur: Theme.isLight ? 24 : 28
        spread: -16
        color: Qt.rgba(0, 0, 0, Theme.isLight ? 0.18 : 0.45)
    }

    function send(mode) {
        const t = input.text
        // Attachments alone can be sent.
        if (t.trim() === "" && root.session.attachments.length === 0)
            return
        if (t.trim() !== "" && history[history.length - 1] !== t)
            history.push(t)
        historyIndex = -1
        completion.close()
        expanded = false
        root.submitted(t, mode)
    }

    // ---- completion ------------------------------------------------------
    // "command" while the text so far is `/word`; "file" while the word at
    // the cursor starts with `@`.
    property string completionMode: ""
    property string completionQuery: ""
    property int tokenStart: 0

    function updateCompletion() {
        const before = input.text.substring(0, input.cursorPosition)
        let m = before.match(/^\/(\S*)$/)
        if (m && root.session.commands.length > 0) {
            completionMode = "command"
            completionQuery = m[1]
            tokenStart = 0
        } else if ((m = before.match(/(^|\s)@(\S*)$/))) {
            completionMode = "file"
            completionQuery = m[2]
            tokenStart = before.length - m[2].length - 1
            root.session.searchFiles(completionQuery)
        } else {
            completionMode = ""
        }
        if (completionMode !== "" && completionItems.length > 0)
            completion.open()
        else
            completion.close()
    }

    readonly property var completionItems: {
        if (completionMode === "command") {
            const q = completionQuery.toLowerCase()
            const all = root.session.commands.map(c => { const p = c.split("\t"); return { name: p[0], detail: p[1], hint: p[2] } })
            const starts = all.filter(c => c.name.toLowerCase().startsWith(q))
            const contains = all.filter(c => !c.name.toLowerCase().startsWith(q) && c.name.toLowerCase().includes(q))
            return starts.concat(contains).slice(0, 40)
        }
        if (completionMode === "file")
            return root.session.fileMatches.map(f => ({ name: f, detail: "", hint: "" }))
        return []
    }
    onCompletionItemsChanged: {
        list.currentIndex = 0
        if (completionMode !== "" && completionItems.length > 0 && input.activeFocus)
            completion.open()
        else if (completionItems.length === 0)
            completion.close()
    }

    function acceptCompletion() {
        const item = completionItems[list.currentIndex]
        if (!item)
            return
        const after = input.text.substring(input.cursorPosition)
        const insert = completionMode === "command" ? "/" + item.name + " " : "@" + item.name + " "
        input.text = input.text.substring(0, tokenStart) + insert + after.replace(/^\S*/, "")
        input.cursorPosition = tokenStart + insert.length
        completion.close()
        completionMode = ""
    }

    Popup {
        id: completion
        y: -height - 6
        x: 8
        width: Math.min(root.width - 16, 560)
        height: Math.min(list.contentHeight + 12, 300)
        padding: 6
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        background: GlassSurface { radius: 10 }
        ListView {
            id: list
            anchors.fill: parent
            clip: true
            model: root.completionItems
            boundsBehavior: Flickable.StopAtBounds
            highlightMoveDuration: 0
            delegate: ItemDelegate {
                id: item
                required property var modelData
                required property int index
                width: ListView.view.width
                height: 28
                highlighted: ListView.isCurrentItem
                background: Rectangle { radius: 4; color: item.highlighted ? Theme.selected : item.hovered ? Theme.hover : "transparent" }
                contentItem: RowLayout {
                    spacing: 10
                    FileIcon { visible: root.completionMode === "file"; path: visible ? item.modelData.name : ""; Layout.preferredWidth: 16; Layout.preferredHeight: 16 }
                    Label {
                        text: (root.completionMode === "command" ? "/" : "") + item.modelData.name
                        color: Theme.foreground
                        font.family: root.completionMode === "file" ? Theme.mono : Qt.application.font.family
                        font.pixelSize: Theme.px(13)
                        elide: Text.ElideMiddle
                        Layout.maximumWidth: root.completionMode === "file" ? list.width - 46 : 220
                    }
                    Label {
                        Layout.fillWidth: true
                        text: item.modelData.hint ? item.modelData.hint + "  " + item.modelData.detail : item.modelData.detail
                        color: Theme.muted
                        font.pixelSize: Theme.px(12)
                        elide: Text.ElideRight
                    }
                }
                onClicked: { list.currentIndex = index; root.acceptCompletion() }
            }
        }
    }

    // ---- model picker ----------------------------------------------------
    Popup {
        id: modelPicker
        y: -height - 6
        x: root.width - width - 8
        width: 420
        height: 380
        padding: 8
        background: GlassSurface { radius: 10 }
        onOpened: { modelFilter.text = ""; modelFilter.forceActiveFocus(); root.session.loadModels() }
        ColumnLayout {
            anchors.fill: parent
            TextField {
                id: modelFilter
                Layout.fillWidth: true
                placeholderText: "Filter models"
                placeholderTextColor: Theme.muted
                color: Theme.foreground
                background: Rectangle { radius: 5; color: Theme.background; border.color: Theme.border }
                Keys.onDownPressed: modelList.incrementCurrentIndex()
                Keys.onUpPressed: modelList.decrementCurrentIndex()
                onAccepted: modelList.pick(modelList.currentIndex)
            }
            Label {
                visible: root.session.models.length === 0
                text: "Loading models…"
                color: Theme.muted
            }
            ListView {
                id: modelList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                highlightMoveDuration: 0
                model: root.session.models
                    .map(m => { const p = m.split("\t"); return { provider: p[0], id: p[1], name: p[2] || p[1], reasoning: p[3] === "1" } })
                    .filter(m => (m.provider + " " + m.id + " " + m.name).toLowerCase().includes(modelFilter.text.toLowerCase()))
                function pick(i) {
                    const m = model[i]
                    if (m) { root.session.setModel(m.provider, m.id); modelPicker.close() }
                }
                delegate: ItemDelegate {
                    id: mrow
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    height: 30
                    highlighted: ListView.isCurrentItem
                    background: Rectangle { radius: 4; color: mrow.highlighted ? Theme.selected : mrow.hovered ? Theme.hover : "transparent" }
                    contentItem: RowLayout {
                        Label {
                            text: mrow.modelData.name + (mrow.modelData.name === root.session.modelName ? "  ✓" : "")
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13)
                        }
                        Item { Layout.fillWidth: true }
                        Label { text: mrow.modelData.provider; color: Theme.muted; font.pixelSize: Theme.px(11) }
                    }
                    onClicked: modelList.pick(index)
                }
            }
        }
    }

    // Goal and side question open over the composer's left end (from their
    // buttons, or the ⋯ menu in a narrow chat).
    GoalPopover {
        id: goalPopup
        session: root.session
        draft: root.text
        x: 8
        y: -height - 6
        onDraftUsed: root.text = ""
    }
    // Hand off, second opinion, committee (orchestration).
    property alias delegatePopover: delegate
    DelegatePopover {
        id: delegate
        session: root.session
        draft: root.text
        x: 8
        y: -height - 6
        onDraftUsed: root.text = ""
    }
    AppMenu {
        id: delegateMenu
        AppMenuItem { objectName: "delegateHandoff"; text: "Hand off…"; hint: "to a new session"; onTriggered: delegate.show("handoff") }
        AppMenuItem { objectName: "delegateAdvisor"; text: "Second opinion…"; hint: "from another agent"; onTriggered: delegate.show("advisor") }
        AppMenuItem { objectName: "delegateCommittee"; text: "Committee…"; hint: "two agents plan"; onTriggered: delegate.show("committee") }
    }
    BtwPopover {
        id: btw
        session: root.session
        x: 8
        y: -height - 6
    }
    // A narrow chat: goal, side question and Ask first in one menu.
    AppMenu {
        id: moreMenu
        AppMenuItem { text: root.session.goalJson !== "" ? "Goal…" : "Set a goal…"; onTriggered: goalPopup.open() }
        AppMenuItem { text: "Side question (btw)…"; onTriggered: btw.open() }
        AppMenuItem { text: "Ask first"; marked: root.session.askFirst; onTriggered: root.session.setAskFirstForSession(!root.session.askFirst) }
    }

    AppMenu {
        id: thinkingMenu
        Repeater {
            model: root.session.thinkingLevels
            AppMenuItem {
                required property string modelData
                text: modelData
                marked: modelData === root.session.thinking
                onTriggered: root.session.setThinkingLevel(modelData)
            }
        }
    }

    // ---- input -----------------------------------------------------------
    ScrollView {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: bar.top
        anchors.margins: 8
        anchors.rightMargin: 36
        TextArea {
            id: input
            objectName: "composerInput"
            focus: true
            placeholderText: root.session.status === "busy"
                ? "Queue a follow-up (Enter) or steer now (Ctrl+Enter)…"
                : root.session.status === "offline" || root.session.status === "exited"
                ? "Send a message to resume this session with " + Theme.harnessLabel(root.session.harness) + "…"
                : root.planOn ? "Plan: describe what to plan (no files change)…"
                : "Ask anything… / for commands, @ for files"
            placeholderTextColor: Theme.muted
            color: Theme.foreground
            selectionColor: Theme.selection
            wrapMode: TextEdit.Wrap
            font.pixelSize: Theme.px(14)
            background: null
            // Deferred: textChanged fires before the cursor moves past the new text.
            onTextChanged: Qt.callLater(root.updateCompletion)
            onCursorPositionChanged: Qt.callLater(root.updateCompletion)
            onActiveFocusChanged: if (!activeFocus) completion.close()

            Keys.onPressed: event => {
                const popup = completion.visible
                if (popup && (event.key === Qt.Key_Down || event.key === Qt.Key_Up)) {
                    event.key === Qt.Key_Down ? list.incrementCurrentIndex() : list.decrementCurrentIndex()
                    event.accepted = true
                    return
                }
                if (popup && (event.key === Qt.Key_Tab || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
                    root.acceptCompletion()
                    event.accepted = true
                    return
                }
                // Recording: Enter inserts the text, Esc drops it.
                if (root.recording && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Escape)) {
                    event.key === Qt.Key_Escape ? root.cancelDictation() : root.finishDictation()
                    event.accepted = true
                    return
                }
                // Tab / Shift+Tab: Build ↔ Plan.
                if (!popup && (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab)
                        && (event.modifiers & (Qt.ControlModifier | Qt.AltModifier)) === 0) {
                    root.togglePlan()
                    event.accepted = true
                    return
                }
                if (popup && event.key === Qt.Key_Escape) {
                    completion.close()
                    event.accepted = true
                    return
                }
                // History: Up/Down when the composer is empty or already browsing.
                if ((event.key === Qt.Key_Up || event.key === Qt.Key_Down) && root.history.length > 0
                        && (input.text === "" || root.historyIndex >= 0)) {
                    let i = root.historyIndex < 0 ? root.history.length : root.historyIndex
                    i += event.key === Qt.Key_Up ? -1 : 1
                    if (i >= root.history.length) { root.historyIndex = -1; input.text = "" }
                    else { root.historyIndex = Math.max(0, i); input.text = root.history[root.historyIndex] }
                    input.cursorPosition = input.length
                    event.accepted = true
                    return
                }
                if (event.key === Qt.Key_Escape && root.expanded) {
                    root.expanded = false
                    event.accepted = true
                    return
                }
                if (event.key === Qt.Key_Escape && root.session.status === "busy") {
                    root.session.abort()
                    event.accepted = true
                    return
                }
                if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                    return
                if (event.modifiers & Qt.ShiftModifier)
                    return
                event.accepted = true
                root.send(event.modifiers & Qt.ControlModifier ? "steer" : "")
            }
        }
    }

    FileDialog {
        id: attachDialog
        title: "Attach files"
        fileMode: FileDialog.OpenFiles
        currentFolder: root.session.cwd !== "" && !root.session.cwd.startsWith("ssh://") ? "file://" + root.session.cwd : ""
        onAccepted: { for (const f of selectedFiles) root.session.addAttachment(f.toString()) }
    }

    // ---- bottom bar -------------------------------------------------------
    // ComposerIconButton / ComposerLabelButton, shared with New session.

    ComposerIconButton {
        objectName: "expandButton"
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 6
        iconName: root.expanded ? "fullscreen-exit" : "fullscreen"
        tint: Theme.muted
        tip: (root.expanded ? "Back to the chat (Esc)" : "Expand to write a long message") + Keymap.hint("expandComposer")
        onClicked: root.toggleExpanded()
    }

    RowLayout {
        id: bar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        anchors.bottomMargin: 6
        height: 32
        spacing: tight ? 2 : 6
        // A narrow chat: labels give way so every control (Send above all)
        // stays on the bar.
        readonly property bool narrow: width < 560
        readonly property bool tight: width < 420
        readonly property bool omp: root.session.harness === "omp" || root.session.harness === "demo"

        ComposerIconButton {
            objectName: "attachButton"
            visible: !root.dictationActive
            iconName: "add-circle"
            tip: "Attach files (or drop them here)"
            onClicked: attachDialog.open()
        }
        // Recording / transcribing, in place of the buttons on the left.
        RowLayout {
            objectName: "dictationStrip"
            visible: root.dictationActive
            spacing: 8
            Layout.leftMargin: 4
            Rectangle {
                visible: root.recording
                implicitWidth: 9
                implicitHeight: 9
                radius: 4.5
                color: Theme.error
                SequentialAnimation on opacity {
                    running: root.recording
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.3; duration: 600 }
                    NumberAnimation { to: 1; duration: 600 }
                }
            }
            Label {
                text: root.awaitingModel ? "Getting the speech model (once) " + Math.round(Dictation.downloadProgress * 100) + "%"
                    : root.recording ? "Listening " + Math.floor(root.dictationSeconds / 60) + ":" + String(root.dictationSeconds % 60).padStart(2, "0")
                    : "Transcribing…"
                color: root.recording ? Theme.foreground : Theme.muted
                font.pixelSize: Theme.px(12.5)
                font.weight: Font.Medium
            }
            // How loud the microphone hears you.
            Rectangle {
                objectName: "micLevel"
                visible: root.recording
                implicitWidth: 36
                implicitHeight: 4
                radius: 2
                color: Theme.border
                Rectangle {
                    width: parent.width * Math.min(1, Math.sqrt(root.micLevel))
                    height: parent.height
                    radius: 2
                    color: Theme.success
                    Behavior on width { NumberAnimation { duration: 70 } }
                }
            }
            FlatButton {
                objectName: "dictationCancel"
                visible: root.recording || root.awaitingModel
                text: "Cancel"
                font.pixelSize: Theme.px(12)
                onClicked: root.cancelDictation()
                ToolTip.visible: hovered
                ToolTip.text: root.awaitingModel ? "Stop the download" : "Drop the recording (Esc)"
            }
        }
        // Other agents take over, advise, or plan (orchestration).
        ComposerIconButton {
            objectName: "delegateButton"
            visible: Workspace.orchestrationOn && !root.dictationActive && root.session.harness !== "demo" && root.session.harness !== ""
            iconName: "robot-2"
            on: delegateMenu.opened || delegate.opened
            tip: "Delegate: hand off to another agent, get a second opinion, or form a committee"
            onClicked: delegateMenu.popup(this, 0, -delegateMenu.implicitHeight - 6)
        }
        // omp: a goal it keeps working toward across turns.
        ComposerIconButton {
            id: goalButton
            objectName: "goalButton"
            visible: bar.omp && !root.dictationActive && !bar.tight
            readonly property bool hasGoal: root.session.goalJson !== ""
            iconName: hasGoal ? "target-fill" : "target"
            tint: hasGoal ? Theme.info : Theme.foreground
            on: goalPopup.opened
            tip: hasGoal ? "Goal: see it above the composer" : "Set a goal: omp keeps working toward it across turns"
            onClicked: goalPopup.opened ? goalPopup.close() : goalPopup.open()
        }
        // omp: a side question that doesn't interrupt the agent.
        ComposerIconButton {
            id: btwButton
            objectName: "btwButton"
            visible: bar.omp && !root.dictationActive && !bar.tight
            iconName: "chat-thread"
            on: btw.opened
            tip: "Side question (btw): ask about the session without interrupting the agent"
            onClicked: btw.opened ? btw.close() : btw.open()
        }
        // omp: ask the agent to clarify with its `ask` tool before starting.
        ComposerIconButton {
            id: askFirstToggle
            objectName: "askFirstButton"
            visible: bar.omp && !root.dictationActive && !bar.tight
            readonly property bool askOn: root.session.askFirst
            iconName: "question"
            tint: askOn ? Theme.primary : Theme.foreground
            on: askOn
            tip: askOn ? "Ask first is on: the agent checks with you (its ask tool) before deciding anything you didn't specify"
                         : "Ask first: have the agent ask about unclear points and decisions before it starts"
            onClicked: root.session.setAskFirstForSession(!askOn)
        }
        ComposerIconButton {
            objectName: "moreButton"
            visible: bar.omp && bar.tight && !root.dictationActive
            iconName: "more"
            tint: root.session.askFirst || root.session.goalJson !== "" ? Theme.primary : Theme.foreground
            on: moreMenu.opened
            tip: "Goal, side question, Ask first"
            onClicked: moreMenu.popup(this, 0, -moreMenu.implicitHeight - 6)
        }
        // omp: what runs without asking (tools.approvalMode); click cycles.
        ComposerIconButton {
            id: permissionsButton
            objectName: "permissionsButton"
            visible: root.approvalMode !== "" && !root.dictationActive
            iconName: root.approvalMode === "always-ask" ? "shield-user" : root.approvalMode === "write" ? "shield-star" : "shield-check"
            tint: root.approvalMode === "always-ask" ? Theme.foreground : root.approvalMode === "write" ? Theme.success : Theme.info
            tip: (root.approvalMode === "always-ask" ? "Permissions: ask — the agent asks before it edits files or runs commands"
                  : root.approvalMode === "write" ? "Permissions: edits — the agent edits files freely, asks before running commands"
                  : "Permissions: auto — the agent edits and runs commands without asking")
                 + "\nClick to switch"
            onClicked: root.cycleApproval()
        }

        // Context use, then the run's status line, filling the middle.
        Rectangle {
            visible: root.session.contextText !== "" && !bar.narrow
            Layout.preferredWidth: 36
            Layout.preferredHeight: 4
            radius: 2
            color: Theme.border
            Rectangle {
                width: parent.width * Math.min(1, root.session.contextFraction)
                height: parent.height
                radius: 2
                color: root.session.contextFraction > 0.85 ? Theme.error : root.session.contextFraction > 0.6 ? Theme.warning : Theme.success
            }
            HoverHandler { id: ctxHover }
            ToolTip.visible: ctxHover.hovered
            ToolTip.text: "Context: " + root.session.contextText
        }
        Label {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            text: root.dictationNote !== "" ? root.dictationNote : root.session.statusLine
            color: root.dictationNote !== "" ? Theme.warning : Theme.muted
            elide: Text.ElideRight
            font.pixelSize: Theme.px(11)
            horizontalAlignment: Text.AlignRight
        }

        // Build or Plan (Tab): plan first, without changing files.
        ComposerLabelButton {
            objectName: "modeButton"
            iconName: "ai-agent"
            text: bar.tight ? "" : root.planOn ? "Plan" : "Build"
            tint: root.planOn ? Theme.warning : Theme.success
            tip: (root.planOn ? "Plan: the agent explores and proposes a plan; no files change"
                              + (bar.omp ? " (edits are blocked, commands need your approval)" : "")
                              : "Build: the agent makes changes")
                 + "\nClick or Tab to switch"
            onClicked: { root.togglePlan(); input.forceActiveFocus() }
        }
        ComposerLabelButton {
            objectName: "thinkingButton"
            visible: !bar.tight
            iconName: "brain-ai-3"
            // Narrow: the icon alone (the level is in the tooltip).
            text: bar.narrow ? "" : root.session.thinking || "thinking"
            tint: root.session.thinking && root.session.thinking !== "off" ? Theme.info : Theme.muted
            maxLabel: 120
            enabled: root.session.harness !== "demo"
            tip: "Thinking: " + (root.session.thinking || "default")
            onClicked: { root.session.loadThinkingLevels(); thinkingMenu.popup() }
        }
        ComposerLabelButton {
            id: modelButton
            objectName: "modelButton"
            // "openai-codex/gpt-5.6-luna" → its provider and "gpt-5.6-luna".
            readonly property string full: root.session.modelId !== "" ? root.session.modelId : root.session.modelName
            readonly property string provider: full.indexOf("/") > 0 ? full.split("/")[0] : ""
            // Tight: the provider's logo alone (the name is in the tooltip).
            text: bar.tight && provider !== "" ? "" : root.session.modelName !== "" ? root.session.modelName.split("/").pop() : "Model"
            maxLabel: bar.tight ? 90 : bar.narrow ? 140 : 240
            // Short of room, the model's name elides before Send is pushed off.
            // (A layout keeps an item that doesn't fill at its preferred width.)
            Layout.fillWidth: true
            Layout.minimumWidth: Math.min(implicitWidth, 48)
            Layout.maximumWidth: implicitWidth
            enabled: root.session.harness !== "demo"
            tip: full !== "" ? full : "Choose a model"
            onClicked: modelPicker.opened ? modelPicker.close() : modelPicker.open()
            ProviderLogo { objectName: "providerLogo"; provider: modelButton.provider; implicitWidth: 17; implicitHeight: 17; visible: modelButton.provider !== "" }
        }

        // Dictation: talk, and the text goes into the composer.
        ComposerIconButton {
            objectName: "micButton"
            iconName: "mic"
            tint: root.recording ? Theme.error : Theme.foreground
            on: root.recording
            enabled: (root.dictationMine && !root.awaitingModel) || (Dictation.state === "idle" && Dictation.downloading === "")
            tip: root.recording ? "Stop and insert the text (Enter)" : "Dictate: speak, and the text goes here" + Keymap.hint("dictate")
            onClicked: root.toggleDictation()
        }
        // Stop while the agent runs; Send (or queue, while it runs).
        ComposerIconButton {
            objectName: "stopButton"
            visible: root.session.status === "busy"
            tip: "Stop the run (Esc)"
            onClicked: root.session.abort()
            contentItem: Item {
                Rectangle { anchors.centerIn: parent; width: 14; height: 14; radius: 3; color: Qt.rgba(Theme.error.r, Theme.error.g, Theme.error.b, 0.22); border.color: Theme.error; border.width: 1.5 }
            }
        }
        ComposerIconButton {
            objectName: "sendButton"
            iconName: "send-plane-2"
            tint: Theme.primary
            enabled: input.text.trim() !== ""
            tip: root.session.status === "busy" ? "Queue as a follow-up (Enter) — Ctrl+Enter steers now" : "Send (Enter)"
            onClicked: root.send("")
        }
    }
}
