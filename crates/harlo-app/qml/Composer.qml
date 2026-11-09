import QtQuick
import QtQuick.Effects
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import harlo

// Message input with `/command` and `@file` / `@session` completion, model and thinking
// pickers, and prompt history.
Rectangle {
    id: root
    required property ChatSession session
    property alias text: input.text
    readonly property bool completionVisible: completion.visible
    readonly property bool inputFocused: input.activeFocus
    signal submitted(string text, string mode)
    /// Sign this agent in on that host.
    signal signInOnHost(string agent, string host)

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

    // Per-session modes, kept by the workspace.
    /// The session's approval mode; "" for an agent without a shield.
    readonly property string approvalMode: Workspace.modesRevision >= 0 ? Workspace.approvalMode(session.key) : ""
    readonly property bool planOn: Workspace.modesRevision >= 0 && Workspace.planMode(session.key)
    function togglePlan() { Workspace.setPlanMode(session.key, !planOn) }
    // Helpers it started are at work (Stop stops them too, as it waits).
    readonly property bool helpersWorking: Workspace.orchestrationRevision >= 0 && session.key !== "" && Workspace.helpersWorking(session.key)
    function stop() {
        if (session.status === "busy") session.abort()
        Workspace.stopHelpersOf(session.key)
    }
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
    function cursorToEnd() { input.cursorPosition = input.length }
    function insertText(t) { input.insert(input.cursorPosition, t); input.forceActiveFocus() }
    // For `--selftest`: popups live outside the page item.
    function grabCompletion(path) { list.grabToImage(r => r.saveToFile(path)) }

    // The text's height plus what's around it (the input's margins, the bar
    // and its bottom margin): any less and the input scrolls, clipping a line.
    implicitHeight: Math.min(Math.max(input.implicitHeight, 44), 240) + 16 + bar.height + bar.anchors.bottomMargin
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
    // This folder's other sessions matching the `@` word (theirs go with
    // the message as an excerpt).
    property var sessionMatches: []

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
            sessionMatches = root.session.cwd !== "" ? JSON.parse(Workspace.sessionMentions(root.session.cwd, root.session.key, completionQuery)) : []
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
        if (completionMode === "file") {
            const files = root.session.fileMatches.map(f => ({ name: f, detail: "", hint: "", kind: "file" }))
            const sessions = sessionMatches.map(s => ({ name: s.title, detail: "session · " + Theme.ago(s.modified, Date.now()) + (s.harness === "claude" ? " · Claude Code" : ""),
                                                        hint: "", kind: "session", mention: s.mention, harness: s.harness, path: s.path }))
            // (Named: sessions first; just `@`: files first, a few sessions after.)
            return completionQuery === "" ? files.slice(0, 30).concat(sessions.slice(0, 3)) : sessions.concat(files)
        }
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
        const insert = completionMode === "command" ? "/" + item.name + " " : item.kind === "session" ? item.mention + " " : "@" + item.name + " "
        if (item.kind === "session")
            root.session.mentionSession(item.mention, item.harness, item.path)
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
                    FileIcon { visible: root.completionMode === "file" && item.modelData.kind !== "session"; path: visible ? item.modelData.name : ""; Layout.preferredWidth: 16; Layout.preferredHeight: 16 }
                    Icon { visible: item.modelData.kind === "session"; name: "chat-thread"; Layout.preferredWidth: 16; Layout.preferredHeight: 16 }
                    Label {
                        text: (root.completionMode === "command" ? "/" : "") + item.modelData.name
                        color: Theme.foreground
                        font.family: root.completionMode === "file" && item.modelData.kind !== "session" ? Theme.mono : Qt.application.font.family
                        font.pixelSize: Theme.px(13)
                        elide: Text.ElideMiddle
                        Layout.maximumWidth: item.modelData.kind === "session" ? list.width - 190 : root.completionMode === "file" ? list.width - 46 : 220
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
                visible: root.session.models.length === 0 && !root.session.modelsAnswered
                text: "Loading models…"
                color: Theme.muted
            }
            // None: nothing to pick until the agent has a login, which is
            // made where it runs.
            ColumnLayout {
                id: noModels
                objectName: "noModels"
                visible: root.session.models.length === 0 && root.session.modelsAnswered
                Layout.fillWidth: true
                spacing: 8
                readonly property string host: (/^ssh:\/\/([^\/]+)/.exec(root.session.cwd) || [])[1] || ""
                readonly property string agent: Theme.harnessLabel(root.session.harness)
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    text: noModels.host === ""
                        ? "No models. " + noModels.agent + " needs a login or an API key: add one in Settings → Providers."
                        : "No models. " + noModels.agent + " needs its own login or API key on " + noModels.host + "."
                }
                RowLayout {
                    spacing: 6
                    FlatButton {
                        objectName: "modelsSignIn"
                        visible: noModels.host !== ""
                        text: "Sign in on " + noModels.host
                        onClicked: { modelPicker.close(); root.signInOnHost(root.session.harness, noModels.host) }
                    }
                    FlatButton {
                        objectName: "modelsCheckAgain"
                        text: "Check again"
                        onClicked: root.session.recheckModels()
                    }
                }
            }
            ListView {
                id: modelList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                highlightMoveDuration: 0
                // Models served on this computer or the local network first,
                // under "Local"; then the rest, under "Online".
                model: {
                    const all = root.session.models
                        .map(m => { const p = m.split("\t"); return { provider: p[0], id: p[1], name: p[2] || p[1], reasoning: p[3] === "1", local: p[4] === "1" } })
                        .filter(m => (m.provider + " " + m.id + " " + m.name).toLowerCase().includes(modelFilter.text.toLowerCase()))
                    return all.filter(m => m.local).concat(all.filter(m => !m.local))
                }
                function pick(i) {
                    const m = model[i]
                    if (m) { root.session.setModel(m.provider, m.id); modelPicker.close() }
                }
                delegate: ItemDelegate {
                    id: mrow
                    required property var modelData
                    required property int index
                    // The first of each group: its name above it (when there
                    // are local models at all).
                    readonly property real heading: ListView.view.model[0].local && (index === 0 || ListView.view.model[index - 1].local !== modelData.local) ? 30 : 0
                    width: ListView.view.width
                    height: 30 + heading
                    topInset: heading
                    topPadding: heading + bottomPadding
                    highlighted: ListView.isCurrentItem
                    background: Rectangle { radius: 4; color: mrow.highlighted ? Theme.selected : mrow.hovered ? Theme.hover : "transparent" }
                    Label {
                        visible: mrow.heading > 0
                        x: 8
                        y: mrow.heading - height - 3
                        text: mrow.modelData.local ? "LOCAL" : "ONLINE"
                        color: Theme.label
                        font.pixelSize: Theme.px(11)
                        font.weight: Font.Medium
                        font.letterSpacing: Theme.px(11) * 0.07
                    }
                    contentItem: RowLayout {
                        Label {
                            text: Theme.modelLabel(mrow.modelData.name) + (mrow.modelData.name === root.session.modelName ? "  ✓" : "")
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13)
                        }
                        Item { Layout.fillWidth: true }
                        // (A local model: its engine.)
                        Label {
                            readonly property string engine: mrow.modelData.local ? Theme.modelEngine(root.session.harness, mrow.modelData.provider + "/" + mrow.modelData.id) : ""
                            text: engine || Theme.providerOf(mrow.modelData.provider + ":" + mrow.modelData.id)
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    onClicked: modelList.pick(index)
                }
            }
        }
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
        AppMenuItem { objectName: "delegateSplit"; text: "Split into helpers…"; hint: "parts at once"; onTriggered: delegate.show("split") }
    }
    // The thinking levels, asked for when the model is known (ready for
    // the first click) and again when the menu opens; "reading" until the
    // agent answers (it may have to start first).
    property bool levelsReading: false
    function readLevels() {
        if (!root.session || root.session.harness === "demo") return
        levelsReading = true
        levelsWait.restart()
        root.session.loadThinkingLevels()
    }
    Timer { id: levelsWait; interval: 15000; onTriggered: root.levelsReading = false }
    Connections {
        target: root.session
        function onThinkingLevelsChanged() { root.levelsReading = false }
        function onModelIdChanged() { root.readLevels() }
    }
    AppMenu {
        id: thinkingMenu
        // (Nothing to pick: say so rather than an empty menu.)
        AppMenuItem {
            visible: root.session.thinkingLevels.length === 0
            enabled: false
            text: root.levelsReading ? "Reading thinking levels…" : "No thinking levels for this model"
        }
        Repeater {
            model: root.session.thinkingLevels
            AppMenuItem {
                required property string modelData
                text: modelData
                // (No level set yet: the agent's default.)
                marked: modelData === root.session.thinking || (modelData === "default" && root.session.thinking === "")
                onTriggered: {
                    root.session.setThinkingLevel(modelData)
                    Workspace.rememberThinking(root.session.key, modelData)
                }
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
            // (Narrow: short enough for one line; the rest is in the help.)
            placeholderText: !bar.roomForHint ? (root.session.status === "busy" ? "Follow up, or steer (Ctrl+Enter)…" : root.planOn ? "Describe what to plan…" : "Ask anything…")
                : root.session.status === "busy"
                ? "Queue a follow-up (Enter) or steer now (Ctrl+Enter)…"
                : root.session.status === "offline" || root.session.status === "exited"
                ? "Send a message to resume this session with " + Theme.harnessLabel(root.session.harness) + "…"
                : root.planOn ? "Plan: describe what to plan (no files change)…"
                : "Ask anything… / for commands, @ for files and sessions"
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
                // An image or files on the clipboard: attached, not typed.
                if (event.matches(StandardKey.Paste)) {
                    const files = Workspace.pasteFiles()
                    for (let i = 0; i < files.length; i++)
                        root.session.addAttachment(files[i])
                    if (files.length > 0) event.accepted = true
                    return
                }
                if (event.key === Qt.Key_Escape && root.session.status === "busy") {
                    root.stop()
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
        // Each gives way at its own width (the reading column is about
        // 555px here): the thinking level's word first, then the context
        // meter, then the placeholder's / and @ hint.
        readonly property bool roomForLevel: width >= 540
        readonly property bool roomForMeter: width >= 500
        readonly property bool roomForHint: width >= 470

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
        // What runs without asking; click cycles.
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

        // Context use, then the run's status line, filling the middle. One
        // outlined bar for every agent: the context used fills it, and the
        // stretch past where the agent compacts is shaded, behind a line.
        Rectangle {
            id: ctxBar
            visible: root.session.contextText !== "" && bar.roomForMeter
            Layout.preferredWidth: 52
            Layout.preferredHeight: 10
            radius: height / 2
            color: Theme.well
            border.width: 1
            border.color: Theme.textFaint
            // How far to where it compacts (or to the window's end).
            readonly property real way: root.session.compactFraction > 0 ? root.session.contextFraction / root.session.compactFraction : root.session.contextFraction
            readonly property bool marked: root.session.compactFraction > 0 && root.session.compactFraction < 1
            Item {
                id: ctxInner
                anchors.fill: parent
                anchors.margins: 2
                // Past where it compacts.
                Rectangle {
                    visible: ctxBar.marked
                    x: Math.round(ctxInner.width * root.session.compactFraction)
                    width: ctxInner.width - x
                    height: parent.height
                    radius: height / 2
                    color: Theme.controlBorder
                }
                Rectangle {
                    width: Math.max(height, ctxInner.width * Math.min(1, root.session.contextFraction))
                    visible: root.session.contextFraction > 0
                    height: parent.height
                    radius: height / 2
                    color: ctxBar.way > 0.85 ? Theme.error : ctxBar.way > 0.6 ? Theme.warning : Theme.success
                }
                // Where it compacts.
                Rectangle {
                    visible: ctxBar.marked
                    x: Math.round(ctxInner.width * root.session.compactFraction) - 1
                    width: 1
                    height: parent.height
                    color: Theme.textMuted
                }
            }
            HoverHandler { id: ctxHover }
            ToolTip.visible: ctxHover.hovered
            ToolTip.text: "Context: " + root.session.contextText
        }
        // How full the context is, in words beside its bar.
        Label {
            objectName: "contextPercent"
            visible: ctxBar.visible && root.session.contextFraction > 0
            text: Math.round(root.session.contextFraction * 100) + "% context"
            color: ctxBar.way > 0.85 ? Theme.error : ctxBar.way > 0.6 ? Theme.warning : Theme.textMuted
            font.pixelSize: Theme.px(11)
            font.features: { "tnum": 1 }
        }
        Label {
            Layout.fillWidth: true
            // (Only what's left over: the model's name comes first.)
            Layout.preferredWidth: 0
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
                              : "Build: the agent makes changes")
                 + "\nClick or Tab to switch"
            onClicked: { root.togglePlan(); input.forceActiveFocus() }
        }
        ComposerLabelButton {
            objectName: "thinkingButton"
            visible: !bar.tight
            iconName: "brain-ai-3"
            // Narrow: the icon alone (the level is in the tooltip).
            text: !bar.roomForLevel ? "" : root.session.thinking || "default"
            tint: root.session.thinking && root.session.thinking !== "off" ? Theme.info : Theme.muted
            maxLabel: 120
            enabled: root.session.harness !== "demo"
            tip: "Thinking: " + (root.session.thinking || "default")
            onClicked: { root.readLevels(); thinkingMenu.popup() }
        }
        ComposerLabelButton {
            id: modelButton
            objectName: "modelButton"
            // "openai-codex/gpt-5.6-luna" → its provider and "gpt-5.6-luna".
            readonly property string full: root.session.modelId !== "" ? root.session.modelId : root.session.modelName
            readonly property string provider: full.indexOf("/") > 0 ? full.split("/")[0] : ""
            // Tight: the provider's logo alone (the name is in the tooltip).
            text: bar.tight && provider !== "" ? "" : root.session.modelName !== "" ? Theme.modelLabel(root.session.modelName).split("/").pop() : "Model"
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
            visible: root.session.status === "busy" || root.helpersWorking
            tip: root.session.status === "busy" ? "Stop the run, and its helpers (Esc)" : "Stop its helpers"
            onClicked: root.stop()
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
