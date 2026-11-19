import QtCore
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import harlo

ApplicationWindow {
    id: win
    width: 1280
    height: 840
    // The open panes at their narrowest.
    minimumWidth: leftRail.width + rail.width + (sidebar.visible ? 160 : 0) + chatMinWidth + (rightPanel.visible ? 240 : 0)
    minimumHeight: 420
    // `--background` (start at login): stay in the tray, if there is one,
    // or without a window while workflow runs left under way go on.
    readonly property bool startHidden: Qt.application.arguments.includes("--background") && (selftestDir === "" || argValue("--tray=") === "1" || argValue("--background-runs=") === "1")
        && (Workspace.setting("tray") !== "0" || Workspace.workflowsUnderWay() > 0)
    visible: !startHidden
    title: chat.hasSession && chat.session.title ? chat.session.title + " — Harlo" : "harlo"
    color: Theme.background
    // Default text size for controls without their own (scaled in Settings).
    font.pixelSize: Theme.px(13)
    font.family: Theme.uiFont
    // Controls left in their default look (labels, check boxes, tooltips)
    // still pick up the theme.
    palette.window: Theme.background
    palette.windowText: Theme.foreground
    palette.base: Theme.background
    palette.alternateBase: Theme.subtle
    palette.text: Theme.foreground
    palette.button: Theme.elevated
    palette.buttonText: Theme.foreground
    palette.brightText: Theme.foreground
    palette.highlight: Theme.primary
    palette.highlightedText: Theme.textOnPrimary
    palette.placeholderText: Theme.muted
    palette.toolTipBase: Theme.elevated
    palette.toolTipText: Theme.foreground
    palette.light: Theme.hover
    palette.midlight: Theme.border
    palette.mid: Theme.border
    palette.dark: Theme.subtle
    palette.shadow: "#000000"

    onActiveChanged: {
        Workspace.setWindowActive(active)
        // (Changes made while it was in the back show at once.)
        if (active) git.refresh(false)
    }
    // Every hover tip (`ToolTip.text` on a control) is the one shared
    // ToolTip: dressed once here like the pills (their surface) and faded
    // in, for every window.
    Item {
        id: tipStyle
        readonly property var tip: ToolTip.toolTip
        Component {
            id: tipBackground
            // The pills' own surface (fill, border, top light line, shadow).
            ControlSurface { control: null; radius: 8 }
        }
        Component {
            id: tipText
            Label {
                property var tip: null
                text: tip ? tip.text : ""
                color: Theme.foreground
                font.family: Theme.uiFont
                font.pixelSize: Theme.px(12)
                lineHeight: 1.15
                wrapMode: Text.Wrap
            }
        }
        Component { id: tipIn; Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.hoverMotion; easing.type: Easing.OutCubic } } }
        Component { id: tipOut; Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.5) } } }
        Component.onCompleted: {
            const t = tip
            // (Owned by tipStyle, but out of its scene: the tip places
            // only items without a visual parent.)
            const back = tipBackground.createObject(tipStyle), text = tipText.createObject(tipStyle, { tip: t })
            back.parent = null
            text.parent = null
            t.background = back
            t.contentItem = text
            t.topPadding = 6
            t.bottomPadding = 6
            t.leftPadding = 10
            t.rightPadding = 10
            t.margins = 8
            t.enter = tipIn.createObject(t)
            t.exit = tipOut.createObject(t)
            // Long tips wrap (at most about 60 characters a line).
            t.width = Qt.binding(() => Math.min(t.implicitWidth, Theme.px(420)))
            // Centred over its control, a little above it (and safe while it
            // has none).
            t.x = Qt.binding(() => t.parent ? (t.parent.width - t.width) / 2 : 0)
            t.y = Qt.binding(() => -t.implicitHeight - 6)
        }
    }
    // What draws the window (GraphicsInfo attaches to items, not windows).
    Item { id: graphicsInfo; readonly property int api: GraphicsInfo.api }
    // The first frame: the graphics choice works (see graphics.rs).
    property bool drawnOnce: false
    Connections {
        target: win
        enabled: !win.drawnOnce
        function onFrameSwapped() { win.drawnOnce = true; Workspace.graphicsDrawn() }
    }
    // Settings → General: keep running in the tray instead of quitting (on
    // unless turned off; without a tray, closing quits).
    onClosing: close => {
        if (Workspace.setting("close_to_tray") !== "0" && trayLoader.item && trayLoader.item.available) {
            close.accepted = false
            win.hide()
        } else if (Workspace.workflowsUnderWay() > 0) {
            close.accepted = false
            win.keepForRuns()
        }
    }
    Loader {
        id: trayLoader
        active: Workspace.setting("tray") !== "0" && (win.selftestDir === "" || win.argValue("--tray=") === "1")
        source: "TrayIcon.qml"
        onLoaded: {
            item.window = win
            if (win.selftestDir !== "") console.log("selftest: tray loaded, available:", item.available)
        }
        Connections {
            target: trayLoader.item
            function onNewSessionRequested() { win.openDraft(-2) }
            function onDropDownRequested() { win.toggleDropDown() }
            function onMiniChatRequested() { win.toggleMiniChat() }
            function onQuitRequested() { win.requestQuit() }
        }
    }
    // The drop-down window (toggled by `harlo --toggle-dropdown`, e.g. from a
    // desktop shortcut): layer-shell on KDE Wayland, a plain window elsewhere.
    property var dropDown: null
    // The shortcuts sheet (from a viewer's ⋯ menu).
    function showShortcuts() { shortcutsDialog.open() }
    function showMainWindow() {
        win.show()
        win.raise()
        win.requestActivate()
    }
    // The mini-chat window (Ctrl+Shift+M, the tray, `harlo --mini-chat`).
    property var miniChat: null
    function toggleMiniChat() {
        if (!miniChat) {
            miniChat = Qt.createComponent("MiniChat.qml").createObject(null)
            if (!miniChat) return
            miniChat.openInWindowRequested.connect(() => { miniChat.visible = false; win.showMainWindow() })
        }
        miniChat.visible = !miniChat.visible
        if (miniChat.visible) {
            miniChat.raise()
            miniChat.requestActivate()
            miniChat.focusInput()
        }
    }
    function toggleDropDown() {
        if (!dropDown) {
            let c = Qt.createComponent("DropDownLayer.qml")
            if (c.status !== Component.Ready) {
                console.log("drop-down: layer-shell unavailable (" + c.errorString().trim() + "); using a plain window")
                c = Qt.createComponent("DropDownPlain.qml")
            }
            dropDown = c.createObject(null)
            if (!dropDown) return
            dropDown.openInWindowRequested.connect(() => { dropDown.visible = false; win.showMainWindow() })
        }
        dropDown.visible = !dropDown.visible
        if (dropDown.visible) {
            dropDown.requestActivate()
            dropDown.focusInput()
        }
    }
    Connections {
        target: Workspace
        function onRemoteCommand(command) {
            if (command === "toggle-dropdown") win.toggleDropDown()
            else if (command === "toggle-mini-chat") win.toggleMiniChat()
            else if (command === "setup") { win.showMainWindow(); win.openSetupGuide() }
            else win.showMainWindow()
        }
        function onHostConnected(host) { Remotes.adoptHost(host) }
    }
    // Plan limits: soon after starting, then every 10 minutes.
    // Self-tests leave the real accounts alone.
    // (The last limits seen show at once; fresh ones soon after.)
    // `--check-start` (a package's check): the window loaded; then quit.
    Timer {
        interval: 4000
        running: Qt.application.arguments.includes("--check-start")
        onTriggered: { console.log("check-start: ok"); Qt.quit() }
    }
    Timer { interval: 1500; running: win.selftestDir === ""; onTriggered: Workspace.refreshQuota() }
    Timer { interval: 600000; running: win.selftestDir === ""; repeat: true; onTriggered: Workspace.refreshQuota() }
    Timer { interval: 30000; running: win.selftestDir === ""; repeat: true; onTriggered: Workspace.checkLimits() }
    // Scheduled prompts: due ones start every 30 s; the first check, soon
    // after starting, catches up runs missed while harlo was closed.
    Timer { interval: 4000; running: true; onTriggered: Workspace.tickSchedules() }
    Timer { interval: 30000; running: true; repeat: true; onTriggered: Workspace.tickSchedules() }
    // Started hidden but no tray to come back from: show the window.
    Timer {
        running: win.startHidden
        interval: 3000
        onTriggered: {
            if (trayLoader.item && trayLoader.item.available) return
            if (Workspace.workflowsUnderWay() > 0) win.keptForRuns = true
            else { win.show(); win.raise() }
        }
    }
    /// Hidden while workflow runs go on (no tray to come back from): harlo
    /// quits a moment after the last one is over (done, paused or stopped);
    /// launching it again shows the window.
    property bool keptForRuns: false
    // Test hook.
    property alias runsOverQuit: runsOverQuit
    onVisibleChanged: if (visible) keptForRuns = false
    Timer {
        id: runsOverQuit
        objectName: "runsOverQuit"
        interval: 3000
        running: win.keptForRuns && !win.visible && Workspace.workflowsRevision >= 0 && Workspace.workflowsUnderWay() === 0
        onTriggered: Qt.quit()
    }
    /// Quit (the menu, the tray, the shortcut): asks first while runs are
    /// under way.
    function requestQuit() {
        const n = Workspace.workflowsUnderWay()
        if (n === 0) { Qt.quit(); return }
        showMainWindow()
        quitRunsDialog.count = n
        quitRunsDialog.open()
    }
    function keepForRuns() {
        keptForRuns = true
        win.hide()
        Workspace.workflowsBackgrounded()
    }
    QuitRunsDialog { id: quitRunsDialog; onKeepRequested: win.keepForRuns() }
    function setTrayEnabled(on) {
        Workspace.setSetting("tray", on ? "1" : "0")
        trayLoader.active = on && win.selftestDir === ""
    }
    Connections {
        target: Workspace
        function onRaiseRequested() { win.show(); win.raise(); win.requestActivate() }
    }

    // Window geometry and sidebar width survive restarts.
    Settings {
        id: windowSettings
        category: "window"
        property alias x: win.x
        property alias y: win.y
        property alias width: win.width
        property alias height: win.height
        property real sidebarWidth: 300
        property real changesWidth: 440
        /// Side panel shown next to the chat: context | changes | files | "".
        property string rightMode: ""
        property bool sidebarOpen: true
        property bool terminalOpen: false
        property real terminalHeight: 260
    }

    // Working-tree state of the active session's folder.
    GitPanel {
        id: git
        // Keep the sidebar's change counts in step with the Changes panel
        // (commits, staging, saves).
        onFilesChanged: Workspace.refreshChanges()
    }
    function syncGitFolder() { git.setFolder(chat.session.cwd) }
    /// Changed files in the session's folder: its repository's, or (a folder
    /// of repositories) all of theirs.
    readonly property int changeTotal: git.reposJson !== "" ? JSON.parse(git.reposJson).reduce((n, r) => n + r.changes, 0) : git.files.length
    Connections {
        target: chat.session
        function onCwdChanged() { win.syncGitFolder() }
        // A tool call ended: what it changed shows (at most every 0.7 s,
        // the window in front or not).
        function onToolEnded() { if (!gitKick.running) gitKick.start() }
        function onStatusChanged() {
            // A finished run is when files most likely changed.
            if (win.lastStatus === "busy" && chat.session.status !== "busy")
                git.refresh(false)
            win.lastStatus = chat.session.status
        }
    }
    property string lastStatus: ""
    // Worktree setup, merges and removals change files behind the panel's back.
    Connections {
        target: Workspace
        function onNoticeChanged() { git.refresh(false) }
    }
    Component.onCompleted: {
        // Phone access, if it's on.
        Phone.autostart()
        updateNotesProject()
        syncGitFolder()
        fitPanes()
        // `--size=WxH` (testing): override the saved window size.
        const size = argValue("--size=").split("x")
        if (size.length === 2) { width = Number(size[0]); height = Number(size[1]) }
        // `--theme-mode=` (self-tests): every scenario in that appearance.
        if (selftestDir !== "" && argValue("--theme-mode=")) Workspace.chooseThemeMode(argValue("--theme-mode="))
    }
    Timer { id: gitKick; interval: 700; onTriggered: git.refresh(false) }
    // Test hook: no polling, only what happens refreshes the panel.
    property bool gitPollOff: false
    // Cheap `git status` poll to catch edits made elsewhere: often while the
    // panel is open (in front or not), slowly otherwise (the header shows
    // the change count); at once when the window comes to the front.
    Timer {
        interval: changes.visible ? 4000 : 15000
        repeat: true
        running: !win.gitPollOff && (win.active || changes.visible && win.visibility !== Window.Minimized && win.visibility !== Window.Hidden)
        onTriggered: git.refresh(false)
    }

    // Fit the side panes into the window: saved widths when there is room,
    // otherwise shrink the sidebar, then the changes panel, so the chat
    // keeps `chatMinWidth` and nothing runs past the window edge.
    readonly property int chatMinWidth: 320
    function fitPanes() {
        // Grow to fit a pane just opened (not every platform does).
        if (win.width < win.minimumWidth) {
            win.width = win.minimumWidth
            return
        }
        const avail = win.width - rail.width - leftRail.width
        let side = sidebar.visible ? Math.min(windowSettings.sidebarWidth, Math.max(180, avail * 0.32)) : 0
        let panel = 0
        if (rightPanel.visible) {
            panel = Math.min(windowSettings.changesWidth, avail - side - chatMinWidth)
            if (panel < 260 && sidebar.visible) {
                side = Math.max(160, avail - chatMinWidth - 260)
                panel = Math.max(240, avail - side - chatMinWidth)
            }
        }
        // (Whole pixels: the chat beside them starts on one, so its text
        // doesn't jump a pixel as things around it repaint.)
        side = Math.round(side)
        panel = Math.round(panel)
        if (rightPanel.visible) {
            rightPanel.target = panel
            if (!rightPanel.sliding)
                rightPanel.SplitView.preferredWidth = panel
        }
        if (sidebar.visible) {
            sidebarPane.target = side
            if (!sidebarPane.sliding)
                sidebarPane.SplitView.preferredWidth = side
        }
    }
    onWidthChanged: Qt.callLater(fitPanes)
    onMinimumWidthChanged: Qt.callLater(fitPanes)

    function toggleTerminal() {
        windowSettings.terminalOpen = !windowSettings.terminalOpen
        if (windowSettings.terminalOpen)
            Qt.callLater(terminal.focusTerminal)
        else
            chat.composerItem.focusInput()
    }
    // Ctrl+Shift+T: another shell (opening the panel shows the first one).
    function newTerminalTab() {
        if (!windowSettings.terminalOpen) toggleTerminal()
        else terminal.newTab()
    }
    // Ctrl+Shift+W: only while a terminal has the focus.
    function closeTerminalTab() {
        if (terminal.terminalFocused) terminal.closeTab(terminal.entry.active)
    }
    function togglePanel(mode) { windowSettings.rightMode = windowSettings.rightMode === mode ? "" : mode }
    /// The Wiki panel, its search ready to type in (closed if it's open).
    function toggleWiki() {
        togglePanel("wiki")
        if (windowSettings.rightMode === "wiki") Qt.callLater(() => wikiPanel.focusSearch())
    }
    function toggleChanges() { togglePanel("changes") }

    function argValue(prefix) {
        for (const a of Qt.application.arguments)
            if (a.startsWith(prefix))
                return a.substring(prefix.length)
        return ""
    }

    Item {
        id: page
        anchors.fill: parent
        // What glass menus and dialogs blur.
        Component.onCompleted: Theme.backdrop = page

        LeftRail {
            id: leftRail
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            sidebarOpen: windowSettings.sidebarOpen
            settingsOpen: settingsDialog.visible
            onSidebarToggled: win.toggleSidebar()
            onSettingsRequested: settingsDialog.open()
            onShortcutsRequested: shortcutsDialog.open()
            onPairDeviceRequested: pairDeviceDialog.open()
            onPlanRequested: { settingsDialog.page = "usage"; settingsDialog.open() }
            onServersRequested: settingsDialog.openPage("servers")
        }

        RightRail {
            id: rail
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            mode: windowSettings.rightMode
            counts: ({ changes: win.changeTotal })
            onPicked: m => windowSettings.rightMode = m
            terminalOpen: windowSettings.terminalOpen
            onTerminalToggled: win.toggleTerminal()
        }

        SplitView {
            id: split
            anchors.left: leftRail.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: rail.left
            // Save pane widths when a drag ends (not during layout, which
            // would feed intermediate sizes back into the preferred widths).
            onResizingChanged: if (!resizing) {
                windowSettings.sidebarWidth = sidebarPane.width
                if (rightPanel.visible)
                    windowSettings.changesWidth = rightPanel.width
            }
            handle: Rectangle {
                implicitWidth: 1
                color: SplitHandle.hovered || SplitHandle.pressed ? Theme.primary : Theme.border
            }
            // The sidebar slides open and shut, its contents keeping their width.
            Item {
                id: sidebarPane
                readonly property bool open: windowSettings.sidebarOpen
                property real reveal: open ? 1 : 0
                Behavior on reveal { enabled: Theme.paneMotion > 0; NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.OutCubic } }
                readonly property bool sliding: reveal > 0 && reveal < 1
                /// Its width when open (fitPanes).
                property real target: windowSettings.sidebarWidth
                onOpenChanged: if (!open) target = width
                onRevealChanged: if (sliding) SplitView.preferredWidth = Math.max(1, target * reveal)
                onSlidingChanged: if (!sliding) Qt.callLater(win.fitPanes)
                visible: reveal > 0
                clip: sliding
                onVisibleChanged: Qt.callLater(win.fitPanes)
                SplitView.minimumWidth: sliding ? 0 : 160
                SplitView.maximumWidth: 520
                Sidebar {
                    id: sidebar
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    width: sidebarPane.sliding ? sidebarPane.target : sidebarPane.width
                    onManageInstancesRequested: settingsDialog.openPage("remote")
                    onProjectSettingsRequested: projectId => settingsDialog.openProject(projectId)
                    onSettingsPageRequested: page => settingsDialog.openPage(page)
                    onDraftWorkflowStarted: chat.draftView.workflowStarted()
                    onNewSessionRequested: projectId => win.openDraft(projectId)
                    onExportRequested: (key, title) => win.exportMarkdown(key, title)
                    onSearchMessagesRequested: text => searchDialog.openSearch(text)
                }
            }
            // The chat, with the terminal panel under it.
            SplitView {
                orientation: Qt.Vertical
                SplitView.fillWidth: true
                SplitView.minimumWidth: win.chatMinWidth
                onResizingChanged: if (!resizing && terminal.visible && !terminalPane.sliding) windowSettings.terminalHeight = terminalPane.height
                handle: Rectangle {
                    implicitHeight: 1
                    color: SplitHandle.hovered || SplitHandle.pressed ? Theme.primary : Theme.border
                }
            ChatView {
                id: chat
                onAddProjectRequested: sidebar.openAddProject()
                onWorkflowRequested: (projectId, task, agent, model) => {
                    const p = JSON.parse(Workspace.projectsJson()).find(x => x.id === projectId)
                    if (p) sidebar.workflowDialog.showFor(p.id, p.name, p.harness, task, agent, model)
                }
                onSignInOnHost: (agent, host) => signInDialog.openFor(agent + "@" + host, Theme.harnessLabel(agent) + " on " + host)
                SplitView.fillHeight: true
                SplitView.minimumHeight: 200
                // Changed files (a partly staged file counts once).
                changesCount: win.changeTotal
                changesOpen: windowSettings.rightMode === "changes"
                onToggleChanges: win.toggleChanges()
                onFileSaved: git.refresh(false)
                onFilesUndone: git.refresh(true)
                // A file an agent edited: its changes in the diff tab.
                onOpenDiff: path => {
                    const cwd = chat.session.cwd
                    // (On a remote host, an absolute path is that host's.)
                    const host = cwd.startsWith("ssh://") ? cwd.substring(0, cwd.indexOf("/", 6)) : ""
                    Workspace.openDiffTab(path.startsWith("ssh://") ? path : path.startsWith("/") ? host + path : cwd + "/" + path, false)
                }
            }
            // The terminal slides up and down (its shells keep their height
            // meanwhile, clipped).
            Item {
                id: terminalPane
                readonly property bool open: windowSettings.terminalOpen
                property real reveal: open ? 1 : 0
                Behavior on reveal { enabled: Theme.paneMotion > 0; NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.OutCubic } }
                readonly property bool sliding: reveal > 0 && reveal < 1
                property real target: windowSettings.terminalHeight
                onOpenChanged: target = open ? windowSettings.terminalHeight : height
                onRevealChanged: if (sliding) SplitView.preferredHeight = Math.max(1, target * reveal)
                onSlidingChanged: if (!sliding && open) SplitView.preferredHeight = windowSettings.terminalHeight
                visible: reveal > 0
                clip: sliding
                SplitView.preferredHeight: windowSettings.terminalHeight
                SplitView.minimumHeight: sliding ? 0 : 110
                TerminalPanel {
                    id: terminal
                    // (Its own: closed, it starts no shell and takes no keys.)
                    visible: terminalPane.visible
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: terminalPane.sliding ? terminalPane.target : terminalPane.height
                    cwd: chat.session.cwd
                    onCloseRequested: win.toggleTerminal()
                }
            }
            }
            // The panel the rail picks. It slides open and shut (Settings →
            // Appearance → Animations); its panel keeps its width meanwhile.
            Item {
                id: rightPanel
                readonly property bool open: windowSettings.rightMode !== ""
                property real reveal: open ? 1 : 0
                Behavior on reveal { enabled: Theme.paneMotion > 0; NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.OutCubic } }
                readonly property bool sliding: reveal > 0 && reveal < 1
                /// Its width when open (fitPanes).
                property real target: windowSettings.changesWidth
                /// The panel shown: the one picked, kept while it slides shut.
                property string shownMode: windowSettings.rightMode
                Connections {
                    target: windowSettings
                    function onRightModeChanged() { if (windowSettings.rightMode !== "") rightPanel.shownMode = windowSettings.rightMode }
                }
                onOpenChanged: if (!open) target = width
                onRevealChanged: if (sliding) SplitView.preferredWidth = Math.max(1, target * reveal)
                onSlidingChanged: if (!sliding) Qt.callLater(win.fitPanes)
                visible: reveal > 0
                clip: sliding
                SplitView.minimumWidth: sliding ? 0 : 240
                onVisibleChanged: Qt.callLater(win.fitPanes)
                Item {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: rightPanel.sliding ? rightPanel.target : rightPanel.width
                    ContextPanel {
                        anchors.fill: parent
                        session: chat.session
                        visible: rightPanel.shownMode === "context"
                        onCloseRequested: windowSettings.rightMode = ""
                    }
                    ActivityPanel {
                        id: activity
                        anchors.fill: parent
                        session: chat.session
                        visible: rightPanel.shownMode === "activity"
                        onCloseRequested: windowSettings.rightMode = ""
                    }
                    ChangesPanel {
                        id: changes
                        anchors.fill: parent
                        git: git
                        harness: chat.session.harness
                        agentBusy: chat.session.status === "busy"
                        canSend: chat.session.key !== ""
                        onSendToAgent: message => {
                            chat.session.send(message, chat.session.status === "busy" ? "followUp" : "normal")
                            Workspace.open(chat.session.key)
                        }
                        visible: rightPanel.shownMode === "changes"
                        onCloseRequested: windowSettings.rightMode = ""
                    }
                    NotesPanel {
                        id: notes
                        anchors.fill: parent
                        projectId: win.notesProject
                        projectName: win.notesProjectName
                        canInsert: Workspace.activeKey !== "" && !Workspace.activeKey.startsWith("view:") && !Workspace.activeKey.startsWith("file:")
                        visible: rightPanel.shownMode === "notes"
                        onCloseRequested: windowSettings.rightMode = ""
                        onInsertRequested: text => {
                            const c = chat.composerItem
                            c.text = c.text.trim() !== "" ? c.text + "\n\n" + text : text
                            c.focusInput()
                        }
                    }
                    WikiPanel {
                        id: wikiPanel
                        anchors.fill: parent
                        // (On the new-session page: the project picked there.)
                        sessionKey: chat.isDraft ? "project:" + chat.draftView.projectId : chat.session.key
                        visible: rightPanel.shownMode === "wiki"
                        onCloseRequested: windowSettings.rightMode = ""
                        onOpenFileRequested: (path, line) => chat.requestOpenFileAt(path, line)
                    }
                    FilesPanel {
                        id: files
                        anchors.fill: parent
                        folder: chat.session.cwd
                        visible: rightPanel.shownMode === "files"
                        onCloseRequested: windowSettings.rightMode = ""
                        onOpenRequested: path => chat.requestOpenFile(path)
                        onViewImagesRequested: folder => {
                            const list = JSON.parse(Images.list(folder))
                            if (list.length === 0) return
                            chat.fileEditor.gridNext = true
                            chat.requestOpenFile(list[0])
                        }
                    }
                }
            }
        }

        // Frame-rate overlay for benchmarking (forces continuous rendering while on).
        FrameAnimation { id: frames; running: fpsLabel.visible }
        Label {
            id: fpsLabel
            visible: Qt.application.arguments.includes("--fps")
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 8
            z: 100
            color: Theme.primary
            text: frames.smoothFrameTime > 0 ? Math.round(1 / frames.smoothFrameTime) + " fps · " + chat.transcript.count + " rows" : ""
        }
    }

    // ---- keyboard shortcuts: one registry drives the bindings and the
    // Ctrl+/ cheat sheet -----------------------------------------------------
    // The new-session page, for a project (-1: home folder).
    // -2: the project of the session last worked in.
    function openDraft(projectId) {
        Workspace.open("view:new")
        chat.draftView.openFor(projectId === -2 ? win.lastProjectId : projectId)
    }
    function newSessionHere() { win.openDraft(-2) }
    property int lastProjectId: -1
    // The notes panel follows the active session's project.
    property int notesProject: -1
    property string notesProjectName: ""
    function updateNotesProject() {
        const id = Workspace.activeProjectId()
        const p = Workspace.projectList().map(l => l.split("\t")).find(p => Number(p[0]) === id)
        notesProject = p ? id : -1
        notesProjectName = p ? p[1] : ""
    }
    Connections {
        target: Workspace
        function onActiveKeyChanged() {
            win.updateNotesProject()
            const k = Workspace.activeKey
            if (k !== "" && !k.startsWith("view:") && !k.startsWith("file:"))
                win.lastProjectId = Workspace.activeProjectId()
        }
        // (A project renamed or removed in Settings: its name here follows.)
        function onProjectsRevisionChanged() { win.updateNotesProject() }
    }
    function toggleSidebar() { windowSettings.sidebarOpen = !windowSettings.sidebarOpen }
    readonly property var shortcutList: {
        const nav = "Navigation", sess = "Sessions and tabs", ui = "Window", panels = "Panels"
        const list = [
            { id: "palette", seq: Keymap.seq("palette"), label: "Go to a session or action", group: nav, icon: "command", run: () => palette.openWith("") },
            { id: "goToSession", seq: Keymap.seq("goToSession"), label: "Go to a session or action", group: nav, icon: "command", run: () => palette.openWith("") },
            { id: "searchAll", seq: Keymap.seq("searchAll"), label: "Search all sessions", group: nav, icon: "search", run: () => searchDialog.openSearch() },
            { id: "findInSession", seq: Keymap.seq("findInSession"), label: "Find in this session or file", group: nav, icon: "search-eye", run: () => chat.openFind() },
            { id: "actions", seq: Keymap.seq("actions"), label: "Actions only", group: nav, icon: "play-circle", run: () => palette.openWith(">") },
            { id: "jumpPrompt", seq: Keymap.seq("jumpPrompt"), label: "Jump to a prompt", group: nav, icon: "bookmark", run: () => { if (chat.transcript.visible) chat.transcript.openPromptRail() } },
            { id: "prevPrompt", seq: Keymap.seq("prevPrompt"), label: "Previous prompt in the session", group: nav, icon: "arrow-up-s", run: () => { if (chat.transcript.visible) chat.transcript.stepPrompt(-1) } },
            { id: "nextPrompt", seq: Keymap.seq("nextPrompt"), label: "Next prompt in the session", group: nav, icon: "arrow-down-s", run: () => { if (chat.transcript.visible) chat.transcript.stepPrompt(1) } },
            { id: "focusRuns", seq: Keymap.seq("focusRuns"), label: "Go to the session's runs", group: nav, icon: "terminal-box", run: () => { if (chat.transcript.visible) chat.transcript.focusLastRun() } },
            { id: "nextTab", seq: Keymap.seq("nextTab"), label: "Next tab", group: sess, icon: "arrow-right-s", run: () => Workspace.nextTab(1) },
            { id: "nextTabAlt", seq: Keymap.seq("nextTabAlt"), label: "Next tab", group: sess, icon: "arrow-right-s", run: () => Workspace.nextTab(1) },
            { id: "prevTab", seq: Keymap.seq("prevTab"), label: "Previous tab", group: sess, icon: "arrow-left-s", run: () => Workspace.nextTab(-1) },
            { id: "prevTabAlt", seq: Keymap.seq("prevTabAlt"), label: "Previous tab", group: sess, icon: "arrow-left-s", run: () => Workspace.nextTab(-1) },
            { id: "newSession", seq: Keymap.seq("newSession"), label: "New session", group: sess, icon: "chat-new", run: () => win.newSessionHere() },
            { id: "closeTab", seq: Keymap.seq("closeTab"), label: "Close tab", group: sess, icon: "close", run: () => chat.requestCloseTab(Workspace.activeKey) },
            { id: "toggleSidebar", seq: Keymap.seq("toggleSidebar"), label: "Toggle sidebar", group: ui, icon: "layout-column", run: () => win.toggleSidebar() },
            { id: "settings", seq: Keymap.seq("settings"), label: "Settings", group: ui, icon: "settings", run: () => settingsDialog.open() },
            { id: "shortcuts", seq: Keymap.seq("shortcuts"), label: "Keyboard shortcuts", group: ui, icon: "keyboard", run: () => shortcutsDialog.open() },
            // (Closing the window keeps harlo in the tray: this quits.)
            { id: "quit", seq: Keymap.seq("quit"), label: "Quit Harlo", group: ui, icon: "close-circle", run: () => win.requestQuit() },
            { seq: "Ctrl+Alt+L", label: "Layout readout", hidden: true, run: () => { layoutInfo.visible = !layoutInfo.visible; console.log("layout:", layoutInfo.text) } },
            { id: "contextPanel", seq: Keymap.seq("contextPanel"), label: "Context panel", group: panels, icon: "pie", run: () => win.togglePanel("context") },
            { id: "activityPanel", seq: Keymap.seq("activityPanel"), label: "Activity panel", group: panels, icon: "list-check-2", run: () => win.togglePanel("activity") },
            { id: "changesPanel", seq: Keymap.seq("changesPanel"), label: "Changes panel", group: panels, icon: "git-branch", run: () => win.togglePanel("changes") },
            { id: "filesPanel", seq: Keymap.seq("filesPanel"), label: "Files panel", group: panels, icon: "folders", run: () => win.togglePanel("files") },
            { id: "notesPanel", seq: Keymap.seq("notesPanel"), label: "Notes panel", group: panels, icon: "note", run: () => win.togglePanel("notes") },
            { id: "wikiPanel", seq: Keymap.seq("wikiPanel"), label: "Wiki panel", group: panels, icon: "book-open", run: () => win.toggleWiki() },
            { id: "terminal", seq: Keymap.seq("terminal"), label: "Terminal", group: panels, icon: "terminal-box", run: () => win.toggleTerminal() },
            { id: "newTerminalTab", seq: Keymap.seq("newTerminalTab"), label: "New terminal tab", group: panels, icon: "add", run: () => win.newTerminalTab() },
            { id: "closeTerminalTab", seq: Keymap.seq("closeTerminalTab"), label: "Close the terminal tab", group: panels, icon: "close", run: () => win.closeTerminalTab() },
            { id: "miniChat", seq: Keymap.seq("miniChat"), label: "Mini chat window", group: ui, icon: "chat-thread", run: () => win.toggleMiniChat() },
            { id: "expandComposer", seq: Keymap.seq("expandComposer"), label: "Expand the composer", group: "Composer", icon: "fullscreen", run: () => { if (chat.composerItem.visible) chat.composerItem.toggleExpanded() } },
            { id: "dictate", seq: Keymap.seq("dictate"), label: "Start or stop dictating", group: "Composer", icon: "mic", run: () => { if (chat.composerItem.visible) chat.composerItem.toggleDictation() } },
        ]
        list.push({ seq: "Ctrl+1…9", label: "Go to tab 1–9", group: sess, icon: "layout-row" })
        for (let i = 1; i <= 9; i++)
            list.push({ seq: "Ctrl+" + i, label: "Go to tab " + i, hidden: true, run: () => Workspace.activateTab(i - 1) })
        return list
    }
    // Keys handled by the composer; listed in the shortcuts dialog only.
    readonly property var composerKeys: [
        { seq: "Enter", label: "Send", group: "Composer", icon: "send-plane" },
        { seq: "Shift+Enter", label: "New line", group: "Composer", icon: "text-wrap" },
        { seq: "Ctrl+Enter", label: "Steer the running agent", group: "Composer", icon: "compass-3" },
        { seq: "Ctrl+Shift+Enter", label: "Stop the run and send", group: "Composer", icon: "stop" },
        { seq: "Ctrl+Shift+Enter", label: "Run as workflow (a new session)", group: "Composer", icon: "list-check-2" },
        { seq: "Esc", label: "Stop the run", group: "Composer", icon: "stop" },
        { seq: "Esc", label: "Leave the expanded composer", group: "Composer", icon: "fullscreen-exit" },
        { seq: "↑", label: "Recall an earlier prompt", group: "Composer", icon: "history" },
        { seq: "↓", label: "Recall an earlier prompt", group: "Composer", icon: "history" },
        { seq: "Tab", label: "Build / Plan", group: "Composer", icon: "target" },
        { seq: "/", label: "Commands", group: "Composer", icon: "code-box" },
        { seq: "@", label: "Mention a file", group: "Composer", icon: "attachment-2" },
        // In a run (reached with "Go to the session's runs", or a click).
        { seq: "↓", label: "Step through a run", group: "Runs in a session", icon: "expand-up-down" },
        { seq: "↑", label: "Step through a run", group: "Runs in a session", icon: "expand-up-down" },
        { seq: "Enter", label: "Open or fold the run or step", group: "Runs in a session", icon: "arrow-down-s" },
        { seq: "Esc", label: "Back to the run, then to the composer", group: "Runs in a session", icon: "arrow-go-back" },
        { seq: "Ctrl+C", label: "Copy a call's listing", group: "Runs in a session", icon: "file-copy" },
        { seq: "Tab", label: "Next run", group: "Runs in a session", icon: "arrow-right-s" },
        // In a file's tab.
        { seq: "Ctrl+S", label: "Save the file", group: "File editor", icon: "check" },
        { seq: "Ctrl+H", label: "Find and replace", group: "File editor", icon: "edit" },
        { seq: "Enter", label: "Next match (in Find)", group: "File editor", icon: "arrow-down-s" },
        { seq: "Shift+Enter", label: "Previous match (in Find)", group: "File editor", icon: "arrow-up-s" },
        { seq: "Ctrl+G", label: "Go to line", group: "File editor", icon: "arrow-right-s" },
        { seq: "Ctrl+/", label: "Comment lines out or back in", group: "File editor", icon: "code-box" },
        { seq: "Tab", label: "Indent the selected lines", group: "File editor", icon: "text-wrap" },
        { seq: "Shift+Tab", label: "Outdent the lines", group: "File editor", icon: "text-wrap" },
        // An image (in a file's tab, or a chat's pictures over the chat).
        { seq: "→", label: "Next image", group: "Image viewer", icon: "arrow-right-s" },
        { seq: "J", label: "Next image", group: "Image viewer", icon: "arrow-right-s" },
        { seq: "Space", label: "Next image", group: "Image viewer", icon: "arrow-right-s" },
        { seq: "←", label: "Previous image", group: "Image viewer", icon: "arrow-left-s" },
        { seq: "K", label: "Previous image", group: "Image viewer", icon: "arrow-left-s" },
        { seq: "+", label: "Zoom in", group: "Image viewer", icon: "add" },
        { seq: "−", label: "Zoom out", group: "Image viewer", icon: "subtract" },
        { seq: "0", label: "Fit the view", group: "Image viewer", icon: "fullscreen-exit" },
        { seq: "1", label: "Actual size", group: "Image viewer", icon: "fullscreen" },
        { seq: "R", label: "Turn right", group: "Image viewer", icon: "rotate-right" },
        { seq: "Shift+R", label: "Turn left", group: "Image viewer", icon: "rotate-left" },
        { seq: "G", label: "All the images as a grid", group: "Image viewer", icon: "grid" },
        { seq: "F", label: "Fill the window", group: "Image viewer", icon: "fullscreen" },
        { seq: "T", label: "Open a chat's picture in a tab", group: "Image viewer", icon: "folder-open" },
        { seq: "P", label: "Play or pause an animation", group: "Image viewer", icon: "play" },
        { seq: "Esc", label: "Out of the grid, then close", group: "Image viewer", icon: "close" },
        // Projects in the sidebar.
        { seq: "Esc", label: "Call off dragging a project", group: "Sidebar", icon: "close" },
    ]
    Instantiator {
        model: win.shortcutList.filter(s => s.run)
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.seq
            // Unassigned, or Settings is recording a key.
            enabled: modelData.seq !== "" && !Keymap.recording
            context: Qt.ApplicationShortcut
            onActivated: modelData.run()
        }
    }

    // Actions offered by the palette, depending on what is active.
    function paletteActions() {
        const acts = []
        const pid = Workspace.activeProjectId()
        const projects = Workspace.projectList().map(l => { const p = l.split("\t"); return { id: Number(p[0]), name: p[1], harness: p[2] } })
        const here = projects.find(p => p.id === pid)
        if (here) {
            acts.push({ title: "New session in " + here.name, icon: "chat-new", hint: Keymap.seq("newSession"), run: () => win.openDraft(here.id) })
            acts.push({ title: "New worktree session in " + here.name + "…", icon: "node-tree", run: () => sidebar.openNewWorktree(here.id, here.name, here.harness) })
        }
        acts.push({ title: "New session in home folder", icon: "home", run: () => win.openDraft(-1) })
        for (const p of projects)
            if (p.id !== pid)
                acts.push({ title: "New worktree session in " + p.name + "…", icon: "node-tree", run: () => sidebar.openNewWorktree(p.id, p.name, p.harness) })
        acts.push({ title: "Add project…", icon: "add", run: () => sidebar.openAddProject() })
        acts.push({ title: "Changes panel", icon: "git-branch", hint: Keymap.seq("changesPanel"), run: () => win.togglePanel("changes") })
        acts.push({ title: "Context panel", icon: "pie", hint: Keymap.seq("contextPanel"), run: () => win.togglePanel("context") })
        acts.push({ title: "Activity panel", icon: "list-check-2", hint: Keymap.seq("activityPanel"), run: () => win.togglePanel("activity") })
        acts.push({ title: "Files panel", icon: "folders", hint: Keymap.seq("filesPanel"), run: () => win.togglePanel("files") })
        acts.push({ title: "Notes panel", icon: "note", hint: Keymap.seq("notesPanel"), run: () => win.togglePanel("notes") })
        acts.push({ title: "Wiki panel", icon: "book-open", hint: Keymap.seq("wikiPanel"), run: () => win.toggleWiki() })
        if (chat.session.key !== "") {
            acts.push({ title: "Export session as Markdown…", icon: "file-text", run: () => win.exportMarkdown(chat.session.key, chat.session.title) })
            acts.push({ title: "Copy session as Markdown", icon: "file-copy", run: () => Workspace.copyMarkdown(chat.session.key) })
        }
        acts.push({ title: "Terminal", icon: "terminal-box", hint: Keymap.seq("terminal"), run: () => win.toggleTerminal() })
        acts.push({ title: "Toggle sidebar", icon: "layout-column", hint: Keymap.seq("toggleSidebar"), run: () => win.toggleSidebar() })
        if (chat.hasSession) {
            acts.push({ title: "Close tab", icon: "close", hint: Keymap.seq("closeTab"), run: () => chat.requestCloseTab(Workspace.activeKey) })
            if (chat.session.status === "busy")
                acts.push({ title: "Stop the current run", icon: "stop", hint: "Esc", run: () => chat.session.abort() })
            if (Workspace.activeKey.startsWith("live:") && chat.session.status !== "offline")
                acts.push({ title: "Stop this session's agent process", icon: "close-circle", run: () => Workspace.closeSession(Workspace.activeKey) })
            if (Workspace.inWorktree(Workspace.activeKey))
                acts.push({ title: "Worktree: merge or remove…", icon: "node-tree", run: () => sidebar.openWorktree(Workspace.activeKey) })
        }
        acts.push({ title: "Search all sessions", icon: "search", hint: Keymap.seq("searchAll"), run: () => searchDialog.openSearch() })
        acts.push({ title: "Drop-down window", icon: "window", run: () => win.toggleDropDown() })
        acts.push({ title: "Mini chat window", icon: "chat-thread", hint: Keymap.seq("miniChat"), run: () => win.toggleMiniChat() })
        for (const r of JSON.parse(Workspace.runsJson()).slice(0, 8))
            acts.push({ title: "Run: " + r.title + (r.kept !== null && r.kept !== undefined ? " (kept)" : ""), icon: "terminal-box", run: () => Workspace.open("view:run:" + r.id) })
        acts.push({ title: "Usage", icon: "stats", run: () => Workspace.open("view:stats") })
        acts.push({ title: "Settings", icon: "settings", hint: Keymap.seq("settings"), run: () => settingsDialog.open() })
        acts.push({ title: "Setup guide…", icon: "book-open", run: () => win.openSetupGuide() })
        acts.push({ title: "Keyboard shortcuts", icon: "keyboard", hint: Keymap.seq("shortcuts"), run: () => shortcutsDialog.open() })
        return acts
    }

    // A session as Markdown: where to save it (a file named after it).
    function exportMarkdown(key, title) {
        const name = (title || "session").replace(/[\/\\:*?"<>|\n]+/g, " ").replace(/\s+/g, " ").trim().slice(0, 80) || "session"
        exportDialog.key = key
        exportDialog.selectedFile = exportDialog.currentFolder + "/" + name + ".md"
        exportDialog.open()
    }
    FileDialog {
        id: exportDialog
        property string key: ""
        title: "Export as Markdown"
        fileMode: FileDialog.SaveFile
        nameFilters: ["Markdown (*.md)", "All files (*)"]
        defaultSuffix: "md"
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: Workspace.exportMarkdown(key, decodeURIComponent(selectedFile.toString()))
    }
    // Copies through a hidden TextEdit (QML has no clipboard API).
    TextEdit {
        id: clipboard
        visible: false
        Connections {
            target: Workspace
            function onClipboardTextChanged() {
                if (Workspace.clipboardText === "") return
                clipboard.text = Workspace.clipboardText
                clipboard.selectAll()
                clipboard.copy()
                clipboard.deselect()
                clipboard.text = ""
            }
        }
    }

    CommandPalette {
        id: palette
        onNewSessionRequested: projectId => win.openDraft(projectId)
        actions: win.paletteActions
        session: chat.session
        onInsertFile: path => chat.composerItem.insertText("@" + path + " ")
    }

    SearchDialog {
        id: searchDialog
        onPicked: (path, index, query) => {
            Workspace.open(path)
            chat.revealMessage(index, query)
        }
    }
    PairDeviceDialog { id: pairDeviceDialog }
    SignInDialog {
        id: signInDialog
        // Signed in on a host: what it can use now.
        onSignedInOnHost: {
            if (chat.isDraft) chat.draftView.loadHostModels()
            else chat.session.recheckModels()
        }
        // Signed in to an MCP server: tested again.
        onMcpSignedIn: (scope, server) => Mcp.test(scope, server)
    }
    // The firewall stops a paired phone on one of harlo's addresses.
    // Merged work the wiki describes: an offer to update it, above.
    WikiOfferToast {
        objectName: "wikiOfferToast"
        anchors.right: updateToasts.right
        anchors.bottom: phoneFirewallToast.top
        // (The firewall stack's own margin already parts it from updates.)
        anchors.bottomMargin: phoneFirewallToast.height > 0 ? 8 : 0
        width: updateToasts.width
        z: 60
    }
    PhoneFirewallToast {
        id: phoneFirewallToast
        objectName: "phoneFirewallToast"
        anchors.right: updateToasts.right
        anchors.bottom: updateToasts.top
        anchors.bottomMargin: updateToasts.height > 0 ? 8 : 0
        width: updateToasts.width
        z: 60
    }
    // Newer agents (here and on the hosts): notices in the corner.
    UpdateToasts {
        id: updateToasts
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        // (Clear of the icon rail on the right.)
        anchors.rightMargin: 18 + (rail.visible ? rail.width : 0)
        anchors.bottomMargin: 18
        // (At the narrowest window, inside it on the left too.)
        width: Math.min(380, parent.width - 36 - (rail.visible ? rail.width : 0))
        z: 60
        onViewOutput: command => {
            if (command === "") return
            if (!windowSettings.terminalOpen) win.toggleTerminal()
            Qt.callLater(() => terminal.runInNewTab(command))
        }
    }
    // (settingsRevision: read again when Settings changes it.)
    readonly property bool checkUpdates: Workspace.settingsRevision >= 0 && Workspace.setting("check_updates") !== "0"
    // At startup (once things settle), then every 6 hours.
    Timer {
        interval: 6000
        running: win.selftestDir === "" && win.checkUpdates
        onTriggered: { Updates.checkLocal(); interval = 6 * 3600 * 1000; restart() }
    }
    Connections {
        target: Remotes
        function onRevisionChanged() { if (Workspace.setting("check_updates") !== "0") Updates.noteHosts(Remotes.hostsJson()) }
    }
    // The setup guide: on the first start (harlo set up before it existed,
    // with projects or sessions, doesn't get it), and from `harlo --setup`,
    // the command palette and Settings → General.
    SetupGuide {
        id: setupGuide
        onSignInRequested: (agent, label) => signInDialog.openFor(agent, label)
    }
    function openSetupGuide() {
        settingsDialog.close()
        setupGuide.show()
    }
    function maybeShowSetup() {
        if (Workspace.setting("setup_done") === "1") return
        if (JSON.parse(Workspace.projectsJson()).length > 0 || Workspace.sessionCount > 0) {
            Workspace.setSetting("setup_done", "1")
            return
        }
        win.openSetupGuide()
    }
    Timer { interval: 1500; running: win.selftestDir === ""; onTriggered: win.maybeShowSetup() }
    SettingsDialog {
        id: settingsDialog
        onSetupRequested: win.openSetupGuide()
        onPairDeviceRequested: pairDeviceDialog.open()
        onSignInRequested: (agent, label) => signInDialog.openFor(agent, label)
        onProviderSignInRequested: (agent, provider, name) => signInDialog.openForProvider(agent, provider, name)
        onMcpSignInRequested: (scope, server) => signInDialog.openForMcp(scope, server)
        onTrayToggled: on => win.setTrayEnabled(on)
        onPromptRailToggled: on => chat.transcript.promptRail = on
        onOpenFileRequested: path => {
            settingsDialog.close()
            chat.requestOpenFile(path)
        }
        onRunInTerminalRequested: command => {
            settingsDialog.close()
            if (!windowSettings.terminalOpen) win.toggleTerminal()
            Qt.callLater(() => terminal.runInNewTab(command))
        }
        shortcuts: win.shortcutList.concat(win.composerKeys)
        onShortcutsSheetRequested: shortcutsDialog.open()
    }
    ShortcutsDialog {
        id: shortcutsDialog
        shortcuts: win.shortcutList.concat(win.composerKeys)
    }
    Label {
        id: layoutInfo
        visible: false
        z: 200
        anchors.centerIn: parent
        padding: 12
        color: Theme.foreground
        font.family: Theme.mono
        font.pixelSize: Theme.px(12)
        background: Rectangle { color: Theme.elevated; border.color: Theme.primary; radius: 6 }
        text: "window " + win.width + "×" + win.height + "  content " + page.width + "×" + page.height
            + "\nscreen " + win.screen.width + "×" + win.screen.height + " @" + win.screen.devicePixelRatio
            + "  (" + win.screen.name + ")"
            + "\npanes: sidebar " + Math.round(sidebar.width) + "  chat " + Math.round(chat.width)
            + "  panel " + (rightPanel.visible ? windowSettings.rightMode + " " + Math.round(rightPanel.width) + " (x " + Math.round(rightPanel.x) + ", right " + Math.round(rightPanel.x + rightPanel.width) + ")" : "closed")
            + "\nvisibility " + win.visibility + "  font " + Qt.application.font.family + " " + Qt.application.font.pointSize + "pt"
            + "\n(Ctrl+Alt+L to hide)"
    }

    // ---- `--selftest=<dir>`: scripted runs for headless checks -------------
    // Run with QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1.
    readonly property string selftestDir: argValue("--selftest=")
    // The self-tests: made only with --selftest, given the window and the
    // parts the scenarios reach. (setSource: its initial properties are read
    // here, where these names are the window's.)
    Loader {
        Component.onCompleted: if (win.selftestDir !== "") setSource("SelfTests.qml", {
            win: win,
            activity: activity,
            changes: changes,
            chat: chat,
            clipboard: clipboard,
            files: files,
            frames: frames,
            git: git,
            graphicsInfo: graphicsInfo,
            notes: notes,
            page: page,
            pairDeviceDialog: pairDeviceDialog,
            commandPalette: palette,
            phoneFirewallToast: phoneFirewallToast,
            rail: rail,
            rightPanel: rightPanel,
            searchDialog: searchDialog,
            settingsDialog: settingsDialog,
            setupGuide: setupGuide,
            shortcutsDialog: shortcutsDialog,
            sidebar: sidebar,
            sidebarPane: sidebarPane,
            signInDialog: signInDialog,
            split: split,
            terminal: terminal,
            terminalPane: terminalPane,
            tipStyle: tipStyle,
            updateToasts: updateToasts,
            wikiPanel: wikiPanel,
            windowSettings: windowSettings
        })
    }
}
