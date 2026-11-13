import QtCore
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import harlo

ApplicationWindow {
    id: win
    width: 1280
    height: 840
    // The open panes at their narrowest.
    minimumWidth: leftRail.width + rail.width + (sidebar.visible ? 160 : 0) + chatMinWidth + (rightPanel.visible ? 240 : 0)
    minimumHeight: 420
    // `--background` (start at login): stay in the tray, if there is one.
    readonly property bool startHidden: Qt.application.arguments.includes("--background") && Workspace.setting("tray") !== "0" && (selftestDir === "" || argValue("--tray=") === "1")
    visible: !startHidden
    title: chat.hasSession && chat.session.title ? chat.session.title + " — harlo" : "harlo"
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

    onActiveChanged: Workspace.setWindowActive(active)
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
        Component { id: tipIn; Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120; easing.type: Easing.OutCubic } } }
        Component { id: tipOut; Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 80 } } }
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
        onTriggered: if (!trayLoader.item || !trayLoader.item.available) { win.show(); win.raise() }
    }
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
    // Cheap `git status` poll to catch edits made elsewhere: often while the
    // panel is open, slowly otherwise (the header shows the change count).
    Timer { interval: changes.visible ? 4000 : 15000; repeat: true; running: win.active; onTriggered: git.refresh(false) }

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
                    onNewSessionRequested: projectId => win.openDraft(projectId)
                    onExportRequested: (key, title) => win.exportMarkdown(key, title)
                    onSearchMessagesRequested: text => searchDialog.openSearch(text)
                }
            }
            // The chat, with the terminal panel under it.
            SplitView {
                id: centerSplit
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
                    id: rightContent
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
            { id: "quit", seq: Keymap.seq("quit"), label: "Quit harlo", group: ui, icon: "close-circle", run: () => Qt.quit() },
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
        anchors.bottomMargin: phoneFirewallToast.height > 0 || updateToasts.height > 0 ? 8 : 0
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
        width: Math.min(380, parent.width - 36)
        z: 60
        onViewOutput: command => {
            if (command === "") return
            if (!windowSettings.terminalOpen) win.toggleTerminal()
            Qt.callLater(() => terminal.runInNewTab(command))
        }
    }
    readonly property bool checkUpdates: Workspace.setting("check_updates") !== "0"
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
            const top = item.mapToItem(f.contentItem, 0, 0).y
            if (top < f.contentY) f.contentY = Math.max(0, top - 12)
            else if (top + item.height > f.contentY + f.height) f.contentY = Math.min(f.contentHeight - f.height, top + item.height - f.height + 12)
        }
    }
    /// Clicks the middle of `item` with a real mouse event.
    /// Every git service card under `root` done checking its sign-in.
    function gitHostsChecked(root) {
        return ["github", "gitlab", "forgejo"].every(s => { const c = findName("gitHost-" + s, root); return c !== null && c.st.state !== "checking" })
    }
    function clickItem(item, what) {
        if (!item) { check(false, "find " + what); return }
        scrollIntoView(item)
        const p = item.mapToItem(null, item.width / 2, item.height / 2)
        TestInput.click(p.x, p.y)
    }
    function clickText(text, root) { clickItem(findText(text, root), "\"" + text + "\"") }
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
            win.log("waited", Date.now() - since, "ms")
            anim.resume()
        }
    }
    TextFile { id: testFile }
    function sidebarCount() { let n = 0; while (Workspace.keyAt(n) !== "") n++; return n }
    property int sessionsBefore: 0
    Connections {
        target: chat.transcript
        function onCountChanged() {
            if (win.t0 > 0 && chat.transcript.count >= 5000) {
                win.log("5000 rows after", Date.now() - win.t0, "ms")
                win.t0 = 0
            }
        }
    }

    // Demo harness: history, streaming, scroll.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "demo"
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.t0 = Date.now(); chat.session.loadDemoHistory(500) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.log("rows after history:", chat.transcript.count, "follow:", chat.transcript.follow) }
        ScriptAction { script: { chat.transcript.scrollToEnd(); chat.submit("stream a demo reply fast", "") } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: win.shot("demo-streaming") }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: win.log("rows at end:", chat.transcript.count, "status:", chat.session.status) }
        ScriptAction { script: win.shot("demo-done") }
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
                win.log("status:", chat.session.status, "modelId:", chat.session.modelId, "model:", chat.session.modelName, "thinking:", chat.session.thinking,
                        "commands:", chat.session.commands.length, "context:", chat.session.contextText, "cwd:", chat.session.cwd)
                chat.session.loadModels()
                chat.session.loadThinkingLevels()
                chat.session.searchFiles("transcript")
            }
        }
        PauseAnimation { duration: 3000 }
        ScriptAction {
            script: {
                win.log("models:", chat.session.models.length, "first:", chat.session.models[0])
                win.log("thinking levels:", chat.session.thinkingLevels.join(","))
                win.log("files for 'transcript':", chat.session.fileMatches.slice(0, 3).join(" | "))
                composerProbe.start()
            }
        }
    }
    // Types "/" into the composer to show the command popup.
    SequentialAnimation {
        id: composerProbe
        ScriptAction { script: { chat.composerItem.focusInput(); chat.composerItem.text = "/"; chat.composerItem.cursorToEnd() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.log("completion:", chat.composerItem.completionMode, chat.composerItem.completionItems.length, "popup:", chat.composerItem.completionVisible, "focus:", chat.composerItem.inputFocused, "active:", win.active) }
        ScriptAction { script: win.shot("harness-" + win.argValue("--harness=")) }
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
                win.log("repo:", git.isRepo, "branch:", git.branch, "files:", git.files.length, "staged:", git.stagedCount, "unstaged:", git.unstagedCount)
                git.select("src/app.rs", false)
            }
        }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("selected:", git.selectedPath, git.diffStats); win.shot("changes-diff") } }
        ScriptAction { script: git.stageHunk(0) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after stage hunk: staged", git.stagedCount, "unstaged", git.unstagedCount, "msg:", git.message); win.shot("changes-staged") } }
        ScriptAction { script: git.commit("harlo selftest commit") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after commit: staged", git.stagedCount, "unstaged", git.unstagedCount, "error:", git.error, "msg:", git.message); git.commitAll("harlo selftest commit all") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.log("after commit all: staged", git.stagedCount, "unstaged", git.unstagedCount, "error:", git.error, "msg:", git.message) }
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
            win.approvalNext = modes[(modes.indexOf(d.shownApproval) + 1) % 3]
            win.check(modes.includes(d.shownApproval) && win.findName("draftPermissionsButton", d).visible, "the new-session page has the shield (" + d.shownApproval + ")")
            win.clickItem(win.findName("draftPermissionsButton", d), "the new-session shield") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.draftView.shownApproval === win.approvalNext, "clicking it switches to " + win.approvalNext)
            win.clickItem(win.findName("draftInput"), "draft input"); TestInput.typeText("fast: please approve the listing"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            win.check(Workspace.approvalMode(chat.session.key) === win.approvalNext, "the session starts in that mode (" + Workspace.approvalMode(chat.session.key) + ")")
            win.check(chat.session.requestIsApproval, "the prompt asks for approval")
            win.check(win.findText("Run this command?", chat.transcript) !== null, "the bash row asks 'Run this command?'")
            win.check(win.findName("sessionStatus", chat).text === "needs you" && Qt.colorEqual(win.findName("sessionStatus", chat).color, Theme.warning), "the header says it needs you, not 'working'")
            win.check(win.findAllNames("runLive", chat.transcript).filter(d => d.visible).every(d => Qt.colorEqual(d.color, Theme.warning)), "the run's dot is the warning colour while it waits")
            win.check(win.findText("Waiting for your approval", chat.transcript) === null, "the footer doesn't say it again")
            // (The bar only when the row is out of view.)
            win.check(!win.shown(chat.requestDock), "with its row in view, no bar besides")
            win.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.check(!chat.session.hasRequest && win.findText("Waiting for your approval", chat.transcript) === null, "Approve on the row runs it"); win.sendByKeys("fast: approve again") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.requestIsApproval && win.findText("Run this command?", chat.transcript) !== null, "the next one asks again")
            const q = win.findAllNames("approvalQuestion", chat.transcript)[0]
            win.check(q && !q.truncated && q.lineCount >= 1, "its question whole, wrapped when narrow (" + (q ? q.lineCount + " lines" : "none") + ")")
            win.shotWindow("clicks-approval-again") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Deny", chat.transcript) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(!chat.session.hasRequest && win.findAllNames("toolMark", chat.transcript).some(m => m.name === "close"), "Deny fails the call"); win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.slideWas = sidebarPane.width; win.togglePanel("activity") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(rightPanel.sliding && rightPanel.width > 0 && rightPanel.width < rightPanel.target, "the side panel slides open (" + Math.round(rightPanel.width) + " of " + Math.round(rightPanel.target) + ")")
            win.check(activity.width === rightPanel.target, "its panel keeps its width meanwhile (" + activity.width + ")")
            win.shotWindow("clicks-motion-panel") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(!rightPanel.sliding && rightPanel.width >= 240 && activity.width === rightPanel.width, "then it's open, its panel as wide (" + Math.round(rightPanel.width) + ")")
            win.toggleSidebar() } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(sidebarPane.sliding && sidebarPane.width < win.slideWas && sidebar.width === sidebarPane.target, "the sidebar slides shut, its contents whole (" + Math.round(sidebarPane.width) + ")")
            win.shotWindow("clicks-motion-sidebar") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(!sidebar.visible, "then it's gone")
            win.toggleSidebar() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            // (Its width: as fitPanes fits it beside the open panel.)
            win.check(sidebar.visible && !sidebarPane.sliding && sidebarPane.width >= 160 && sidebar.width === sidebarPane.width, "and back, its contents as wide (" + Math.round(sidebarPane.width) + ")")
            Workspace.chooseLook("motion", "0")
            win.togglePanel("activity") } }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(!rightPanel.visible, "Animations at 0: it shuts at once")
            Workspace.chooseLook("motion", "600")
            win.toggleTerminal() } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(terminalPane.sliding && terminalPane.height < terminalPane.target && terminal.height === terminalPane.target, "the terminal slides up, whole (" + Math.round(terminalPane.height) + " of " + Math.round(terminalPane.target) + ")") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(!terminalPane.sliding && terminal.visible && terminal.height === terminalPane.height && terminalPane.height >= 110, "then it's open (" + Math.round(terminalPane.height) + ")")
            win.toggleTerminal() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(!terminal.visible, "and slides shut")
            win.summary(); Qt.quit() } }
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
            win.check(req.service === "forgejo" && req.repo === "octo/app" && req.head === "fix/login-loop" && req.canCreate && req.url === "", "a Forgejo server of one's own: none yet, and harlo can open one (" + git.requestJson + ")")
            win.clickItem(win.findName("requestButton"), "Create pull request") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("requestCreate", changes.requestDialog.footer), "Create") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            win.check(req.url.endsWith("/octo/app/pulls/3") && req.number === 3, "created through Forgejo's API (" + git.requestJson + ")")
            win.check(win.findName("checksSummary").text === "Checks running · 0 of 1 done", "its checks show, still running")
            TestInput.writeFile(win.selftestDir + "/forgejo-statuses.json", JSON.stringify([
                { context: "ci / build (push)", status: "success" },
                { context: "ci / lint (push)", status: "failure", target_url: "http://forgejo/octo/app/actions/runs/1" }]))
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const n = win.lastNote()
            win.check(n.kind === "ci" && n.title === "Checks failed: Pull request #3", "harlo notifies when they finish (" + Workspace.lastNotification + ")")
            git.loadRequest() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("checksSummary").text === "Failed: ci / lint (push)", "and the panel shows what failed")
            win.shot("clicks-forgejo")
            win.summary(); Qt.quit() } }
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
            win.check(win.repoNames().join() === "alpha:2,gamma:0,group/beta:1", "the folder's repositories, a level down too (" + win.repoNames() + ")")
            win.check(git.isRepo && git.root.endsWith("/alpha") && git.files.length === 2, "the first with changes is shown (" + git.root + ", " + git.files.length + ")")
            const req = win.findName("repoRequest-gamma", changes)
            win.check(req !== null && req.children[1].text === "#9" && String(req.children[0].color) === String(Theme.error),
                      "gamma has no changes but an open pull request: listed, with #9 and its failed checks (" + git.requestsJson + ")")
            win.check(win.findName("repoShowClean", changes) === null, "nothing is folded away")
            win.check(win.changeTotal === 3, "the panel's count is all of them (" + win.changeTotal + ")")
            win.check(win.repoNames().join() === "alpha:2,gamma:0,group/beta:1", "(the repositories as before)")
            win.shot("clicks-repos") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("repo-group/beta", changes), "group/beta") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(git.root.endsWith("/group/beta") && git.files.length === 1 && git.files[0].startsWith("README.md"), "a click shows another (" + git.root + ")")
            git.commitAll("Beta's change") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(win.repoNames().join() === "alpha:2,gamma:0,group/beta:0" && git.root.endsWith("/group/beta"), "committing there leaves the others as they were (" + win.repoNames() + ")")
            win.clickItem(win.findName("repoShowClean", changes), "without changes") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findName("repo-group/beta", changes) !== null, "beta, now without changes, folds away and unfolds")
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
            win.check(Object.keys(undo).length === 1 && String(first.snapshot).startsWith("@") && first.files === 2, "the turn has a snapshot of the folder's repositories (" + chat.session.undoJson + ")")
            chat.session.previewUndo(Object.keys(undo)[0]) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson)
            win.check(plan.restore.join() === "gamma/README.md" && plan.remove.join() === "group/beta/added.txt", "undoing it covers both, by their place in the folder (" + chat.session.undoPlanJson + ")")
            chat.session.undoTurn(plan.row) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const multi = win.argValue("--cwd=") + "/../multi"
            win.check(JSON.parse(chat.session.undoPlanJson).state === "done" && TestInput.readFile(multi + "/gamma/README.md") === "one\nfix\n" && TestInput.readFile(multi + "/group/beta/added.txt") === "", "and puts both back")
            win.check(TestInput.readFile(multi + "/alpha/notes.md") === "new\n", "your own changes from before the turn stay")
            win.summary(); Qt.quit() } }
    }

    // A session as Markdown: saved to a file, and copied.
    property string exportKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksexport"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); win.exportKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("fast: run the tests") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: Workspace.exportMarkdown(win.exportKey, win.selftestDir + "/session.md") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const md = TestInput.readFile(win.selftestDir + "/session.md")
            win.check(md.startsWith("# fast: run the tests\n\n*Demo · Claude Demo · "), "Export writes the title and what it ran on (" + md.slice(0, 80) + ")")
            win.check(md.indexOf("## You") > 0 && md.indexOf("## Demo · ") > 0 && md.indexOf("\n## Summary") < 0 && md.indexOf("- **bash** `cargo test` → 12 passed") > 0, "your message, the reply and its command (" + md.length + " characters)")
            win.check(Workspace.notice.startsWith("Saved as ") && !Workspace.noticeError, "and says where (" + Workspace.notice + ")")
            Workspace.copyMarkdown(win.exportKey) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(Workspace.clipboardText === TestInput.readFile(win.selftestDir + "/session.md"), "Copy as Markdown gives the same")
            win.check(win.paletteActions().some(a => a.title === "Export session as Markdown…"), "the command palette has it")
            win.summary(); Qt.quit() } }
    }

    // The Activity panel: what a session ran and touched, from its messages.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksactivity"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("fast: edit the notes") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: win.sendByKeys("fast: run the tests") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: win.clickItem(win.findName("rail-activity") || null, "the rail's Activity button") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(windowSettings.rightMode === "activity", "the rail opens the Activity panel")
            const log = activity.log
            win.check(log && log.actions.length === 2 && log.commands === 1 && log.failed === 0 && log.changed.join() === "notes.md",
                      "it lists the edit and the command (" + JSON.stringify(log) + ")")
            const first = win.findName("activity-0", activity)
            win.check(first !== null && log.actions[1].target === "cargo test" && log.actions[1].result === "12 passed", "newest first, with how it went (" + (log ? log.actions[1].result : "") + ")")
            win.shot("clicks-activity") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("activityFilter-change", activity), "files changed") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(activity.shown.length === 1 && activity.shown[0].kind === "edit", "the filter shows only the changes")
            win.check(activity.markdown().indexOf("**bash** `cargo test` — 12 passed") > 0, "Copy gives Markdown")
            win.summary(); Qt.quit() } }
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
            win.check(Accounts.setServerPrice("harlo", "fake", 100, 1000) === "", "the scripted model gets a price")
            Workspace.setSpendLimits(0.25, 0)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.harloSpendKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSpendSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSpendSteps, () => (win.spend().costs || {})[win.harloSpendKey] > 0 && chat.session.status !== "busy", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const cost = win.spend().costs[win.harloSpendKey]
            win.check(Math.abs(cost - 0.14) < 1e-6 && !win.spend().held[win.harloSpendKey], "one request priced: $0.14, under the limit ($" + cost + ")")
            win.check(chat.session.contextText.indexOf("$0.14") >= 0, "the context line says it (" + chat.session.contextText + ")")
            win.sendByKeys("/btw what did I say?") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSpendSteps, () => chat.session.asideJson !== "" && JSON.parse(chat.session.asideJson).done, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.sendByKeys("and once more") }
        PauseAnimation { duration: 300 }
        // ($0.14 each: the prompt, the side question, this one.)
        ScriptAction { script: win.waitUntil(harloSpendSteps, () => !!win.spend().held[win.harloSpendKey], 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const why = win.spend().held[win.harloSpendKey] || ""
            win.check(why === "This session has cost $0.42, over its $0.25 limit.", "the side question counted, and past the limit it's held (" + why + ")")
            win.check(win.findName("spendBanner") !== null, "the chat says so")
            chat.session.loadContext() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const r = chat.session.contextJson ? JSON.parse(chat.session.contextJson) : {}
            win.check(r.costKind === "priced" && Math.abs(r.cost - 0.42) < 1e-6, "the context report has it, priced (" + r.costKind + ", " + r.cost + ")")
            Workspace.setSpendLimits(0, 0)
            settingsDialog.openPage("servers") } }
        PauseAnimation { duration: 800 }
        // An online server: no price yet, so it says what that means.
        ScriptAction { script: {
            win.check(win.findName("serverPrice-fake") === null || !win.findName("serverPrice-fake").visible, "a server on this computer has no price field (nothing extra)")
            const row = win.findName("serverPrice-gw")
            win.check(row !== null && row.visible, "an online server has one")
            win.check(win.findText("No price: what Harlo's sessions on it cost isn't known, so spending limits can't count it.") !== null, "and says what no price means")
            win.shotPopup(settingsDialog, "clicks-harlo-spend-servers")
            win.clickItem(win.findName("serverPriceIn-gw"), "price in")
            TestInput.typeText("3")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.clickItem(win.findName("serverPriceOut-gw"), "price out")
            TestInput.typeText("15")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSpendSteps, () => JSON.parse(Accounts.servers("harlo")).some(s => s.name === "gw" && JSON.stringify(s.prices) === "[[3,15]]"), 5000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const gw = JSON.parse(Accounts.servers("harlo")).find(s => s.name === "gw")
            win.check(gw && JSON.stringify(gw.prices) === "[[3,15]]", "the price is kept for its models (" + (gw ? JSON.stringify(gw.prices) : "no gw") + ")")
            win.check(win.findText("Harlo counts what its sessions cost at this price.") !== null, "and it says it counts")
            win.shotPopup(settingsDialog, "clicks-harlo-spend-priced")
            win.summary(); Qt.quit() } }
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
        onTriggered: { const n = Date.now(); if (last > 0) { worst = Math.max(worst, n - last); if (n - last > 50) { slow++; gaps.push([last - win.longAt, n - last]) } } last = n }
    }
    property real longAt: 0
    // (A turn seen to start and end. The model's log isn't read while it
    // streams: each request carries the whole 20 MB history.)
    property bool longBusy: false
    Connections {
        target: win.argValue("--scenario=") === "clicksharlolong" ? chat.transcript : null
        function onCountChanged() {
            const n = chat.transcript.count
            if (win.longTimes.shown === undefined && n > 0) win.longTimes.shown = Date.now() - win.longAt
            if (win.longTimes.all === undefined && n >= win.longTimes.turns * 6) win.longTimes.all = Date.now() - win.longAt
        }
    }
    property bool longDone: false
    Connections {
        target: win.argValue("--scenario=") === "clicksharlolong" ? chat.session : null
        function onStatusChanged() {
            if (chat.session.status === "busy") win.longBusy = true
            else if (win.longBusy && chat.session.status === "idle") win.longDone = true
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
            win.longTimes.turns = Number(TestInput.readFile(win.selftestDir + "/long-turns"))
            longTick.reset(); longTick.start(); win.longAt = Date.now()
            // (After this step: opening takes a moment, and the steps after
            // a long one are caught up at once, a wait among them.)
            const path = win.selftestDir + "/data/harlo-agent/sessions/--" + cwd.replace(/^\//, "").replace(/\//g, "-") + "--/long0000-0000-0000-0000-000000000000.jsonl"
            Qt.callLater(() => Workspace.open(path)) } }
        // (Longer than opening blocks: the wait isn't caught up past.)
        PauseAnimation { duration: 1500 }
        // (6 rows a turn.)
        // (And its agent ready: the rows it read itself replace the file's.)
        ScriptAction { script: win.waitUntil(harloLongSteps, () => chat.transcript.listView.count >= win.longTimes.turns * 6 && chat.session.status === "idle", 120000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.longTimes.rows = chat.transcript.listView.count
            win.longTimes.openWorst = longTick.worst; win.longTimes.openSlow = longTick.slow
            win.check(win.longTimes.rows >= win.longTimes.turns * 6, "the whole session is there (" + win.longTimes.rows + " rows)") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { longTick.reset(); win.longAt = Date.now(); win.sendByKeys("write a long reply") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloLongSteps, () => win.longDone, 60000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.longTimes.stream = Date.now() - win.longAt
            win.longTimes.streamWorst = longTick.worst; win.longTimes.streamSlow = longTick.slow
            win.log("long stream gaps", JSON.stringify(longTick.gaps))
            win.check(chat.transcript.listView.count > win.longTimes.rows + 30, "the long reply is in (" + chat.transcript.listView.count + " rows)")
            // (Scrolled up as you would: it stops following the end.)
            chat.transcript.listView.follow = false
            longTick.reset(); win.longAt = Date.now(); longScroll.ticks = 0; longScroll.start() } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: win.waitUntil(harloLongSteps, () => !longScroll.running, 60000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.longTimes.scroll = Date.now() - win.longAt
            win.longTimes.scrollWorst = longTick.worst; win.longTimes.scrollSlow = longTick.slow; win.longTimes.scrollTicks = longScroll.ticks
            longTick.stop()
            win.log("long session", JSON.stringify(win.longTimes))
            // (Loose bounds, the numbers logged. Before the rows were
            // indexed and the prompt rail made only the ticks near its
            // window, 146 of 301 frames scrolling took over 50 ms, and a
            // turn's end 270 ms.)
            win.check(win.longTimes.all < 5000, "it opens in time (all rows in " + win.longTimes.all + " ms)")
            win.check(win.longTimes.streamWorst < 150, "frames keep coming while the reply streams (longest " + win.longTimes.streamWorst + " ms)")
            win.check(win.longTimes.scrollSlow < 30 && win.longTimes.scrollWorst < 150, "and while it scrolls up (" + win.longTimes.scrollSlow + " of " + win.longTimes.scrollTicks + " frames over 50 ms, longest " + win.longTimes.scrollWorst + " ms)")
            win.shot("clicks-harlo-long-session")
            win.summary(); Qt.quit() } }
    }

    // Stop while a turn's snapshot is still being taken (slowed to 2.5 s):
    // the message never reaches the model and comes back to the box; the
    // next one goes as usual.
    function modelGot(text) { return win.modelRequests().some(b => JSON.stringify(b).indexOf(text) >= 0) }
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
        ScriptAction { script: win.waitUntil(harloStopSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("never send this walrus") }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(chat.session.preparing && chat.session.status !== "busy", "preparing while the snapshot is taken, not yet busy (" + chat.session.status + ")")
            const stop = win.findName("stopButton")
            win.check(stop !== null && stop.visible, "Stop is offered")
            win.clickItem(stop, "stop") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(!chat.session.preparing && chat.session.status !== "busy", "stopped at once (" + chat.session.status + ")")
            win.check(chat.composerItem.text === "never send this walrus", "the words are back in the box (" + chat.composerItem.text + ")")
            win.check(win.findText("Stopped before it was sent: your message is back in the box.") !== null, "the chat says so")
            win.shot("clicks-harlo-stop-unsent") } }
        // (Past the snapshot's 2.5 s: it ends, and still nothing is sent.)
        PauseAnimation { duration: 3200 }
        ScriptAction { script: {
            win.check(!win.modelGot("never send this walrus"), "the model never got it (" + win.modelRequests().length + " requests)")
            chat.composerItem.text = ""
            win.sendByKeys("send this one instead") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloStopSteps, () => win.modelGot("send this one instead") && chat.session.status !== "busy", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.modelGot("send this one instead"), "the next message goes, after its snapshot")
            win.check(!win.modelGot("never send this walrus"), "and the stopped one still never went")
            win.summary(); Qt.quit() } }
    }

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
        ScriptAction { script: { win.clickItem(win.findName("spendSession"), "per session"); TestInput.typeText("0.50"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.spend().limits.session === 0.5 && win.spend().limits.day === 0, "the per-session limit saves (" + Workspace.spendJson + ")")
            win.shotPopup(settingsDialog, "clicks-spend-settings") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { settingsDialog.close(); win.spendKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("fast: one") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(win.spend().costs[win.spendKey] === 0.25 && !win.spend().held[win.spendKey], "under the limit: it runs ($" + win.spend().costs[win.spendKey] + ")")
            win.sendByKeys("two: a long answer") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const why = win.spend().held[win.spendKey] || ""
            win.check(why === "This session has cost $0.50, over its $0.50 limit.", "at the limit it's held (" + why + ")")
            win.check(chat.session.status !== "busy", "and its run stopped (" + chat.session.status + ")")
            win.check(win.findName("spendBanner") !== null, "the chat says so")
            win.shot("clicks-spend-banner") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            chat.composerItem.text = "fast: three"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(chat.composerItem.text === "fast: three" && win.userMessages() === 2, "a message isn't sent while held; it stays in the box")
            win.clickItem(win.findName("spendAllow"), "Allow more") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!win.spend().held[win.spendKey] && win.findName("spendBanner") === null, "Allow $0.50 more lets it go on")
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(win.userMessages() === 3, "and the message goes")
            // The day: $0.75 so far.
            win.check(Math.abs(win.spend().today - 0.75) < 1e-9, "today counts every prompt ($" + win.spend().today + ")")
            Workspace.setSpendLimits(0, 0.8)
            win.sendByKeys("fast: four") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const s = win.spend()
            win.check(s.day.indexOf("over the $0.80 daily limit") > 0 && s.held[win.spendKey] === s.day, "past the day's limit every session is held (" + s.day + ")")
            win.check(win.findName("spendAllow").text === "Allow $0.80 more today", "with Allow … today")
            win.spendKey = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(Workspace.spendHold(win.spendKey) !== "", "a new session too")
            win.clickItem(win.findName("spendAllow"), "Allow more today") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Workspace.spendHold(win.spendKey) === "" && win.spend().day === "", "Allow … today lifts it")
            win.summary(); Qt.quit() } }
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
            win.check(!win.findName("saveTemplate", chat.draftView), "no Save as template on an empty page")
            win.clickItem(win.findName("draftInput"), "draft input")
            TestInput.typeText("fast: review the open pull request")
            win.clickItem(win.findName("draftPermissionsButton", chat.draftView), "permissions") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("saveTemplate", chat.draftView), "Save as template") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(chat.draftView.templateDialog.opened && win.findName("templateName").text === "fast: review the open pull", "the dialog opens, named from the message (" + win.findName("templateName").text + ")")
            win.shotPopup(chat.draftView.templateDialog, "clicks-templates-save") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("PR review")
            win.clickItem(win.findName("templateSave"), "Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = win.templateList()
            win.check(l.length === 1 && l[0].name === "PR review" && l[0].harness === "demo" && l[0].text === "fast: review the open pull request" && l[0].approval === chat.draftView.shownApproval,
                      "Save keeps the agent, permissions and message (" + Workspace.templatesJson + ")")
            // A different draft, then the template.
            chat.draftView.approval = ""
            chat.draftView.input.text = "" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("template-PR review", chat.draftView), "the template") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = chat.draftView, t = win.templateList()[0]
            win.check(d.input.text === t.text && d.effectiveHarness === "demo" && d.shownApproval === t.approval, "picking it fills the page (" + d.input.text + ", " + d.shownApproval + ")")
            win.shot("clicks-templates-page") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: TestInput.key(Qt.Key_Return, 0) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(chat.session && chat.session.harness === "demo" && Workspace.approvalMode(Workspace.activeKey) === win.templateList()[0].approval, "and Enter starts the session with it")
            win.openDraft(-1) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = chat.draftView
            d.input.text = "fast: a better review"
            d.templateMenu.template = win.templateList()[0]
            d.templateMenu.popup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("templateUpdate"), "Replace with this draft") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findName("templateName").text === "PR review", "replacing keeps its name")
            win.clickItem(win.findName("templateSave"), "Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = win.templateList()
            win.check(l.length === 1 && l[0].text === "fast: a better review", "Replace with this draft updates it (" + Workspace.templatesJson + ")")
            chat.draftView.templateMenu.template = l[0]
            chat.draftView.templateMenu.popup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("templateDelete"), "Delete") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.templateList().length === 0 && !win.findName("template-PR review", chat.draftView), "Delete removes it")
            win.summary(); Qt.quit() } }
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
            win.check(r.highlighted >= 0, "hovering the ticks highlights one (" + r.highlighted + ")")
            win.railTarget = Math.max(0, r.highlighted - 1)
            const entry = win.findText(r.prompts[win.railTarget].text, r)
            win.check(entry !== null, "the hover list shows the prompts")
            win.shot("clicks-rail-hover") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = chat.transcript.rail
            win.clickItem(win.findText(r.prompts[win.railTarget].text, r), "list entry") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.check(chat.transcript.activePrompt === win.railTarget && !chat.transcript.follow, "clicking an entry jumps to it (" + chat.transcript.activePrompt + ")"); TestInput.moveTo(20, 20) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.check(chat.transcript.rail.highlighted === -1, "moving away closes the list"); TestInput.key(Qt.Key_J, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(chat.transcript.rail.keyboard, "Ctrl+J opens it for the keyboard"); TestInput.key(Qt.Key_Up, 0); TestInput.key(Qt.Key_Up, 0); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.check(chat.transcript.activePrompt === win.railTarget - 2 && !chat.transcript.rail.keyboard, "Up, Up, Enter jumps two back (" + chat.transcript.activePrompt + ")"); TestInput.key(Qt.Key_Up, Qt.AltModifier) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.check(chat.transcript.activePrompt === win.railTarget - 3, "Alt+Up: previous prompt (" + chat.transcript.activePrompt + ")"); TestInput.key(Qt.Key_Down, Qt.AltModifier) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.check(chat.transcript.activePrompt === win.railTarget - 2, "Alt+Down: next prompt (" + chat.transcript.activePrompt + ")"); win.summary(); Qt.quit() } }
    }

    // Terminal tabs: Ctrl+`, +, Ctrl+Shift+T, exit, the tab's ×, Ctrl+Shift+W.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminal"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.clickItem(win.findName("composerInput"), "composer"); TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.check(terminal.visible && terminal.tabs.length === 1 && terminal.current.running, "Ctrl+` opens a terminal"); win.clickItem(win.findName("terminalAdd"), "+") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.check(terminal.tabs.length === 2 && terminal.entry.active === 1, "+ adds a tab"); TestInput.key(Qt.Key_T, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.check(terminal.tabs.length === 3, "Ctrl+Shift+T adds another"); win.shot("clicks-terminal-tabs"); win.clickItem(terminal.current, "terminal"); TestInput.typeText("exit"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(terminal.tabs.length === 2, "typing exit closes that tab"); win.clickItem(win.findName("terminalTabClose"), "tab ×") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(terminal.tabs.length === 1, "the tab's × closes it"); win.clickItem(terminal.current, "terminal"); TestInput.key(Qt.Key_W, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(!terminal.visible, "Ctrl+Shift+W on the last tab closes the panel"); win.summary(); Qt.quit() } }
    }

    // Subagents: View on a running one, steer it by typing, Esc, Cancel in the tree.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickssubagents"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.newSession(-1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("start some subagents") }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const row = win.ancestorWith(win.findText("BuildOtter", chat.transcript), "viewRequested")
            win.check(row !== null, "BuildOtter shows under the task row")
            // Narrow: a running subagent offers Steer (which opens the same view) in place of View.
            win.clickText(win.findText("View", row) !== null ? "View" : "Steer", row) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(chat.subagentDialog.visible, "View opens its transcript"); win.clickItem(win.findName("steerField"), "steer field"); TestInput.typeText("Only run the unit tests") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.shotPopup(chat.subagentDialog, "clicks-subagent-dialog"); win.clickText("Steer", chat.subagentDialog.contentItem) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(chat.session.subagentMessage.indexOf("BuildOtter") >= 0, "Steer sends it (" + chat.session.subagentMessage + ")"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.check(!chat.subagentDialog.visible, "Esc closes the dialog"); win.shotWindow("clicks-subagent-tree")
            {
                // Each row's dot is level with its name.
                const name = win.findText("ScoutLark", chat.transcript), row = win.ancestorWith(name, "viewRequested"), dot = win.findName("subagentDot", row)
                const dy = Math.abs(dot.mapToItem(null, 0, dot.height / 2).y - name.mapToItem(null, 0, name.height / 2).y)
                win.check(dy <= 1, "a helper's dot is level with its name (" + dy.toFixed(1) + " px off)")
                const nm = win.findName("subagentName", row)
                win.check(nm.width >= Math.floor(nm.implicitWidth), "a helper's name keeps its whole width while the line has room (" + nm.width + " of " + nm.implicitWidth.toFixed(1) + ")")
            }
 win.clickText("Cancel", win.ancestorWith(win.findText("BuildOtter", chat.transcript), "viewRequested")) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { const a = chat.subagentDock.agents.find(x => x.id === "BuildOtter"); win.check(a && a.status === "aborted", "Cancel in the tree aborts it"); win.summary(); Qt.quit() } }
    }

    // Shortcut rebinding in Settings → Shortcuts, by clicks and key presses.
    function keyOf(label, cap) {
        const row = win.ancestorWith(win.findText(label, settingsDialog.contentItem), "hasPending")
        return row ? win.findText(cap, row) : null
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickskeys"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.check(settingsDialog.visible, "Ctrl+, opens Settings"); win.clickItem(win.findName("settingsNav-shortcuts", settingsDialog.contentItem), "Shortcuts") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.keyOf("Toggle sidebar", "B"), "Toggle sidebar's B") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(Keymap.recording && win.findText("Press keys…", settingsDialog.contentItem) !== null, "clicking a key starts recording"); TestInput.key(Qt.Key_K, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!palette.visible, "Ctrl+K while recording doesn't open the palette")
            win.check(win.findText("Ctrl+K is taken by “Go to a session or action”.", settingsDialog.contentItem) !== null, "a taken key asks to replace")
            win.shotPopup(settingsDialog, "clicks-keys-conflict") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("Cancel", settingsDialog.contentItem) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(Keymap.seq("toggleSidebar") === "Ctrl+B", "Cancel keeps Ctrl+B"); win.clickItem(win.keyOf("Toggle sidebar", "B"), "B again") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_X, 0) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(win.findText("X alone is for typing; add Ctrl, Alt or Meta.", settingsDialog.contentItem) !== null && Keymap.recording, "a plain key is refused, still recording"); TestInput.key(Qt.Key_1, Qt.ControlModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(win.findText("Ctrl+1 is taken by Go to tab 1.", settingsDialog.contentItem) !== null, "a reserved key is refused"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Keymap.seq("toggleSidebar") === "Ctrl+Shift+B" && !Keymap.recording, "Ctrl+Shift+B is recorded")
            win.check(JSON.parse(Workspace.setting("shortcuts")).toggleSidebar === "Ctrl+Shift+B", "and saved")
            // Replace: Files panel takes Changes panel's key.
            win.clickItem(win.keyOf("Files panel", "E"), "Files panel's E") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_G, Qt.ControlModifier | Qt.ShiftModifier) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Replace", settingsDialog.contentItem) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Keymap.seq("filesPanel") === "Ctrl+Shift+G" && Keymap.seq("changesPanel") === "", "Replace moves the key and unassigns the other")
            win.clickItem(win.keyOf("Terminal", "`"), "Terminal's key") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_Backspace, 0) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Keymap.seq("terminal") === "" && win.keyOf("Terminal", "Not set") !== null, "Backspace unassigns (shows Not set)")
            win.clickItem(win.keyOf("New session", "N"), "New session's N") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.AltModifier) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(settingsDialog, "clicks-keys-edited") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: TestInput.key(Qt.Key_Escape, 0) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(!settingsDialog.visible, "Esc closes Settings")
            win.check(sidebar.width < 220 || win.findText("Ctrl+Alt+N", sidebar) !== null, "the sidebar hint follows (Ctrl+Alt+N; hidden in a narrow sidebar)")
            win.sidebarWas = windowSettings.sidebarOpen
            TestInput.key(Qt.Key_B, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(windowSettings.sidebarOpen === win.sidebarWas, "old Ctrl+B does nothing"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(windowSettings.sidebarOpen !== win.sidebarWas, "new Ctrl+Shift+B toggles the sidebar"); TestInput.key(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickText("Reset all", settingsDialog.contentItem) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Keymap.seq("toggleSidebar") === "Ctrl+B" && Keymap.seq("changesPanel") === "Ctrl+Shift+G" && Keymap.seq("terminal") === "Ctrl+`" && Workspace.setting("shortcuts") === "", "Reset all restores the defaults")
            win.summary(); Qt.quit() } }
    }
    property bool sidebarWas: true

    // Start at login: the switch writes and removes the autostart entry;
    // --background starts hidden, and opens the window without a tray.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksautostart"
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.check(!win.visible, "--background starts without the window") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { win.check(win.visible, "no tray: the window opens anyway"); TestInput.key(Qt.Key_Comma, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("autostartSwitch", settingsDialog.contentItem), "autostart switch") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(Workspace.autostartEnabled(), "switching it on adds the entry"); testFile.load(win.argValue("--autostart=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(testFile.text.indexOf(" --background\n") > 0 && testFile.text.indexOf("Exec=\"") > 0, "the entry starts harlo with --background")
            win.shotPopup(settingsDialog, "clicks-autostart") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickItem(win.findName("autostartSwitch", settingsDialog.contentItem), "autostart switch") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(!Workspace.autostartEnabled() && Workspace.fileStamp(win.argValue("--autostart=")) === "", "switching it off removes the entry"); win.summary(); Qt.quit() } }
    }

    // The mini-chat window: open by key, chat in it, keep on top, Esc.
    property int rowsBefore: 0
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksminichat"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("fast: first message") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.rowsBefore = chat.transcript.count; TestInput.key(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(win.miniChat && win.miniChat.visible, "Ctrl+Shift+M opens the mini chat")
            win.check(win.findText("fast: first message", win.miniChat.contentItem) !== null, "it shows the active session")
            win.clickItem(win.findName("composerInput", win.miniChat.contentItem), "mini composer")
            TestInput.typeText("fast: from the mini chat")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(chat.transcript.count > win.rowsBefore && win.findText("fast: from the mini chat", chat.transcript) !== null, "a message sent there reaches the session")
            // (Its own window: taken at its next frame, so nothing changes before.)
            win.miniChat.contentItem.grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-minichat.png")) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.clickItem(win.findName("keepOnTop", win.miniChat.contentItem), "📌") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check((win.miniChat.flags & Qt.WindowStaysOnTopHint) === 0 && !win.miniChat.stayOnTop && Workspace.setting("mini_chat_on_top") === "0", "📌 turns keep-on-top off (and on by default)"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(!win.miniChat.visible, "Esc hides it"); win.summary(); Qt.quit() } }
    }

    // Project notes: per project, autosaved, todos, insert into message.
    property string notesKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksnotes"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); win.notesKey = Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(windowSettings.rightMode === "notes" && win.findText("repo", notes) !== null, "Ctrl+Shift+N opens the project's notes")
            win.clickItem(win.findName("notesEditor", notes), "notes editor")
            TestInput.typeText("Use SQLite for the cache") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.check(Workspace.projectNotes(1) === "Use SQLite for the cache", "typing autosaves"); win.clickItem(win.findName("todoField", notes), "todo field"); TestInput.typeText("write the migration"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(notes.openTodos === 1 && win.findText("Todos · 1 open", notes) !== null, "Enter adds a todo"); win.clickText("write the migration", notes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(Workspace.projectNotes(1) === "Use SQLite for the cache\n- [x] write the migration", "ticking it marks the line done (saved)")
            win.check(win.findText("Todos · all done", notes) !== null, "the todo count follows")
            win.shot("clicks-notes") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.clickText("Insert into message", notes) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(chat.composerItem.text.startsWith("Project notes:\nUse SQLite for the cache"), "Insert into message fills the composer"); chat.composerItem.text = ""; Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(win.findText("No project", notes) !== null && notes.todos.length === 0 && Workspace.projectNotes(-1) === "", "a session outside projects has its own pad")
            win.clickItem(win.findName("notesEditor", notes), "notes editor")
            TestInput.typeText("home notes") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.check(Workspace.projectNotes(-1) === "home notes", "its notes save separately"); Workspace.open(win.notesKey) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(notes.todos.length === 1 && win.findText("repo", notes) !== null, "back in the project, its notes return"); TestInput.key(Qt.Key_N, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(windowSettings.rightMode === "", "Ctrl+Shift+N closes it"); win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.clickItem(win.findName("settingsNav-schedules", settingsDialog.contentItem), "Schedules") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.check(win.findText("No schedules yet.", settingsDialog.contentItem) !== null, "the page starts empty"); win.clickText("New schedule") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("Save") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findText("Write the prompt to send.") !== null, "Save without a prompt explains why")
            win.clickItem(win.findName("schedulePrompt"), "prompt")
            TestInput.typeText("Summarize the repo")
            win.check(win.findText("Write the prompt to send.") === null, "typing clears the error")
            win.clickText("Every") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.clickItem(win.findName("scheduleEvery"), "interval"); TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("1"); win.clickText("Home folder") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("repo") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The editor is its own popup: grab the item that sits in the overlay.
            let it = win.findName("schedulePrompt")
            while (it && it.parent !== Overlay.overlay) it = it.parent
            if (it) it.grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-schedule-editor.png")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Save") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const s = win.firstSchedule(), now = Date.now() / 1000
            win.check(s && s.describe === "Every hour" && s.projectName === "repo" && s.title === "Summarize the repo", "Save adds it (" + (s ? s.describe + " · " + s.projectName : "none") + ")")
            win.check(s && Math.abs(s.nextRun - now - 3600) < 120, "next run in an hour")
            win.check(win.findText("Summarize the repo", settingsDialog.contentItem) !== null, "the card shows it")
            win.clickItem(win.findName("scheduleSwitch", settingsDialog.contentItem), "switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(!win.firstSchedule().enabled && win.firstSchedule().nextRun === null, "the switch turns it off"); win.clickItem(win.findName("scheduleSwitch", settingsDialog.contentItem), "switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(win.firstSchedule().enabled, "and on again"); win.liveBefore = win.liveCount(); win.clickText("Run now", settingsDialog.contentItem) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(win.liveCount() === win.liveBefore + 1 && Workspace.notice === "Scheduled prompt started: Summarize the repo", "Run now starts a session (" + Workspace.notice + ")")
            win.check(win.firstSchedule().lastRun > 0, "and records the run")
            win.shotPopup(settingsDialog, "clicks-schedules")
            win.liveBefore = win.liveCount()
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.check(win.liveCount() === win.liveBefore + 1, "a due schedule runs on the timer's check"); win.clickText("Edit", settingsDialog.contentItem) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.clickItem(win.findName("scheduleTitle"), "title"); TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("Morning summary"); win.clickText("Save") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.check(win.firstSchedule().title === "Morning summary" && win.findText("Morning summary", settingsDialog.contentItem) !== null, "Edit renames it"); win.clickText("Delete", settingsDialog.contentItem) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("Discard") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.check(win.firstSchedule() === null && win.findText("No schedules yet.", settingsDialog.contentItem) !== null, "Delete + Discard removes it"); win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.check(win.findName("planMeter") === null, "no meter without data"); Workspace.quotaJson = win.sampleQuota(false) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = win.findName("planMeter")
            win.check(m !== null && win.findText("80%", m) !== null, "the meter shows the fullest window (80%)")
            const logo = win.findName("planMeterLogo", m)
            win.check(logo !== null && logo.provider === "anthropic" && win.findName("planMeterClaude", m) === null, "with its provider's logo")
            // A session of the other provider runs: the meter follows it.
            Workspace.meterLast = "openai-codex" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = win.findName("planMeter")
            win.check(win.findText("30%", m) !== null && win.findName("planMeterLogo", m).provider === "openai-codex", "it shows the provider used last (30%)")
            win.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const follow = win.findName("meterFollow"), a = win.findName("meterPick-anthropic"), o = win.findName("meterPick-openai-codex")
            win.check(follow !== null && follow.marked && a !== null && a.hint === "80%" && o !== null && o.hint === "30%", "clicking it lists the providers, following the last used")
            win.shotWindow("clicks-quota-meter")
            if (a) win.clickItem(a, "anthropic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = win.findName("planMeter")
            win.check(Workspace.meterPin === "anthropic" && win.findText("80%", m) !== null, "picking one pins it (80%)")
            Workspace.meterLast = "openai-codex"
            win.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("meterPick-anthropic").marked && win.findText("80%", win.findName("planMeter")) !== null, "pinned, it stays while others are used")
            win.clickItem(win.findName("meterFollow"), "the one used last") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Workspace.meterPin === "" && win.findText("30%", win.findName("planMeter")) !== null, "and it can follow the last used again")
            win.clickItem(win.findName("planMeter"), "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("meterAll"), "All limits…") }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(settingsDialog.visible && settingsDialog.page === "usage", "All limits… opens Settings → Usage")
            win.check(win.findText("Plan limits", d) !== null && win.findText("updated 2 min ago", d) !== null, "the Plan limits section shows, and when it was checked")
            win.check(win.findText("anthropic", d) !== null && win.findText("80% used", d) !== null && win.findText("30% used", d) !== null && win.findText("max", d) !== null, "each account and window is listed")
            win.shotPopup(settingsDialog, "clicks-quota")
            Workspace.quotaJson = win.sampleQuota(true) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findText("limit reached", settingsDialog.contentItem) !== null && win.findText("100% used", settingsDialog.contentItem) !== null, "a reached limit is called out")
            win.summary(); Qt.quit() } }
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
            for (const name of win.barButtons) {
                const b = win.findName(name, c)
                const right = b ? b.mapToItem(null, b.width, 0).x : 0
                win.check(b !== null && b.height >= 26 && right <= win.width, name + " shown, ≥ 26 px, inside the window")
            }
            win.check(c.compactBar ? win.findName("modelButton", c).tip === "anthropic/claude-demo" : win.findText("Claude Demo", c) !== null, "the model's name shows (in its tooltip when narrow)")
            const modelLabel = win.findName("modelButton", c).contentItem.children[1]
            win.check(c.compactBar || !modelLabel.truncated, "the model's name isn't cut short when there's room")
            const pct = win.findName("contextPercent", c)
            win.check(pct === null || !pct.visible || /^\d+% context$/.test(pct.text), "the context bar says how full it is")
            const logo = win.findName("providerLogo", c)
            win.check(logo !== null && logo.url.startsWith("data:image/svg+xml"), "the provider's logo shows by it")
            win.check(!win.findName("sendButton", c).enabled, "Send is off with nothing typed")
            win.shot("clicks-composer") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.rowsBefore = chat.transcript.count
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.typeText("fast: hello from the bar") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(win.findName("sendButton", chat.composerItem).enabled, "typing enables Send"); win.clickItem(win.findName("sendButton", chat.composerItem), "Send") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(chat.transcript.count > win.rowsBefore && chat.composerItem.text === "", "clicking Send sends and clears the composer")
            win.clickItem(win.findName("expandButton", chat.composerItem), "expand") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const c = chat.composerItem
            win.check(c.expanded && !chat.transcript.visible && c.height > chat.height * 0.6, "expand: the composer takes the chat's height (" + Math.round(c.height) + " of " + Math.round(chat.height) + ")")
            win.check(c.inputFocused, "and keeps the cursor")
            TestInput.typeText("fast: line one")
            TestInput.key(Qt.Key_Return, Qt.ShiftModifier)
            TestInput.typeText("line two")
            win.shot("clicks-composer-expanded") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: TestInput.key(Qt.Key_Escape, 0) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!chat.composerItem.expanded && chat.transcript.visible && chat.composerItem.text === "fast: line one\nline two", "Esc goes back to the chat, text kept")
            TestInput.key(Qt.Key_X, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(chat.composerItem.expanded, "Ctrl+Shift+X expands it"); win.rowsBefore = chat.transcript.count; TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(!chat.composerItem.expanded && chat.transcript.count > win.rowsBefore, "sending goes back to the chat")
            win.approvalBefore = chat.composerItem.approvalMode
            win.check(["always-ask", "write", "yolo"].includes(win.approvalBefore), "the shield starts in a mode (" + win.approvalBefore + ")")
            win.clickItem(win.findName("permissionsButton", chat.composerItem), "shield") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const modes = ["always-ask", "write", "yolo"]
            const next = modes[(modes.indexOf(win.approvalBefore) + 1) % 3]
            win.check(chat.composerItem.approvalMode === next && Workspace.approvalMode(chat.session.key) === next, "clicking the shield switches to " + next)
            win.check(!chat.session.planMode && !chat.composerItem.planOn && (chat.composerItem.compactBar || win.findText("Build", chat.composerItem) !== null), "sessions start in Build")
            win.clickItem(win.findName("modeButton", chat.composerItem), "Build") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.session.planMode && (chat.composerItem.compactBar || win.findText("Plan", chat.composerItem) !== null) && Workspace.planMode(chat.session.key), "clicking Build switches to Plan")
            win.check(chat.composerItem.inputFocused, "the composer keeps the cursor")
            win.shot("clicks-composer-plan") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.rowsBefore = chat.transcript.count
            TestInput.typeText("fast: plan the cache")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const c = chat.composerItem, stop = win.findName("stopButton", c), send = win.findName("sendButton", c)
            win.check(stop !== null && send !== null && send.mapToItem(null, send.width, 0).x <= c.mapToItem(null, c.width, 0).x, "while it runs, Stop and Send both fit")
            win.shot("clicks-composer-busy") } }
        PauseAnimation { duration: 2200 }
        ScriptAction { script: {
            win.check(win.findText("fast: plan the cache", chat.transcript) !== null, "a plan message shows without the plan note")
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_Tab, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(!chat.session.planMode && (chat.composerItem.compactBar || win.findText("Build", chat.composerItem) !== null) && chat.composerItem.text === "", "Tab switches back to Build (no tab typed)")
            win.summary(); Qt.quit() } }
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
            win.check(!Dictation.modelReady("base"), "no speech model yet")
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.typeText("Please:")
            win.clickItem(win.findName("micButton", chat.composerItem), "mic")
            // (The local download is quick: check at once.)
            win.check(chat.composerItem.awaitingModel && Dictation.downloading === "base", "the first click downloads the model") } }
        PauseAnimation { duration: 10 }
        ScriptAction { script: {
            const c = chat.composerItem
            // (Its layout may not have run yet: check the state the bar follows.)
            win.check(c.dictationActive, "the bar shows it in place of its buttons (" + [c.dictationMine, c.awaitingModel, Dictation.state, Dictation.downloading, Dictation.error, Dictation.downloadError].join(" / ") + ")") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const c = chat.composerItem
            win.check(Dictation.modelReady("base") && Dictation.downloading === "", "the model arrives")
            win.check(Dictation.state === "recording" && c.recording, "then it listens by itself")
            win.check(win.findName("micLevel", c) !== null && c.micLevel > 0, "with a level meter")
            win.shot("clicks-dictation") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const c = chat.composerItem
            win.check(c.dictationSeconds >= 1 && win.findText("Listening 0:0" + c.dictationSeconds, c) !== null, "and a timer")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Enter inserts the text at the cursor (" + chat.composerItem.text + " / " + Dictation.error + ")")
            win.check(win.findText("Please: Add a cache to the loader.", chat.transcript) === null, "and doesn't send it")
            win.clickItem(win.findName("micButton", chat.composerItem), "mic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(Dictation.state === "recording", "with the model in, the mic listens at once"); win.clickItem(win.findName("dictationCancel", chat.composerItem), "Cancel") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Cancel drops the recording")
            TestInput.key(Qt.Key_D, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.check(Dictation.state === "recording", "Ctrl+Shift+D starts it"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Dictation.state === "idle" && chat.composerItem.text === "Please: Add a cache to the loader.", "Esc drops it")
            settingsDialog.openPage("voice") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findName("modelUse-base", d) !== null && win.findName("modelDownload-small", d) !== null, "Settings → Voice: the downloaded model is in use, the other can be downloaded")
            win.shotPopup(settingsDialog, "clicks-dictation-settings") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
    }
    // Remote instances against a throwaway local SSH server (--ssh= its
    // arguments; click-tests.sh starts it): add a host, connect, disconnect.
    SequentialAnimation {
        id: remoteSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksremote"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); settingsDialog.openPage("remote") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("addRemoteHost", settingsDialog.contentItem), "+ Add SSH connection") }
        PauseAnimation { duration: 400 }
        // From ~/.ssh/config (the test's HOME has one).
        ScriptAction { script: {
            const list = win.findName("sshConfigHosts")
            win.check(list !== null && list.count === 2 && win.findText("tester@127.0.0.1:2299") !== null && win.findText("included-box") !== null,
                      "Add starts on the hosts from ~/.ssh/config, with what they connect to (patterns left out)")
            win.shotPopup(settingsDialog, "clicks-remote-sshconfig")
            win.clickItem(win.findName("useHost-testbox"), "Use") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const h = JSON.parse(Remotes.hostsJson()).find(h => h.name === "testbox")
            win.check(h !== undefined && h.ssh === "testbox", "Use adds it under its SSH config name")
            Remotes.deleteHost("testbox")
            win.clickItem(win.findName("addRemoteHost", settingsDialog.contentItem), "+ Add SSH connection") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("remoteTabManual"), "Type it myself") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.clickItem(win.findName("remoteName"), "name")
            TestInput.typeText("box")
            win.clickItem(win.findName("remoteSsh"), "ssh command")
            TestInput.typeText("ssh " + win.argValue("--ssh="))
            win.clickItem(win.findName("saveRemoteHost"), "Save and connect") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const h = JSON.parse(Remotes.hostsJson())
            win.check(h.length === 1 && h[0].name === "box", "the host is saved")
            win.check(h.length === 1 && h[0].state === "connected" && h[0].harlo !== "", "it connects and can run Harlo (" + (h.length ? h[0].state + " " + h[0].message : "") + ")")
            win.check(win.findName("remoteHost-box", settingsDialog.contentItem) !== null, "and is listed")
            win.shotPopup(settingsDialog, "clicks-remote-settings")
            win.clickItem(win.findName("remoteConnect-box", settingsDialog.contentItem), "Disconnect") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(JSON.parse(Remotes.hostsJson())[0].state === "", "Disconnect closes it")
            win.clickItem(win.findName("remoteConnect-box", settingsDialog.contentItem), "Connect") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(JSON.parse(Remotes.hostsJson())[0].state === "connected", "Connect opens it again")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        // Stage 2: the host as the instance shown (its own projects and
        // sessions), a project there, and a Harlo session.
        ScriptAction { script: win.clickItem(win.findName("instanceButton"), "the instance button") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const st = win.findName("instanceStatus-box"), local = win.findName("instanceStatus-local")
            win.check(local && local.text === "This computer" && st && /^Connected · [0-9]+ms ping$/.test(st.text), "the instance menu: Local, and box with its ping (" + (st ? st.text : "none") + ")")
            win.shotWindow("clicks-instance-menu")
            win.clickItem(win.findName("instance-box"), "box") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(Workspace.instance === "box" && win.findName("instanceLabel").text === "box", "switched: the window shows box")
            win.check(JSON.parse(Workspace.projectsJson()).length === 0, "with box's projects (none yet)")
            win.clickItem(win.findName("addProjectButton"), "+ Project") } }
        PauseAnimation { duration: 600 }
        // (Add a project: its folder from the host's listing.)
        ScriptAction { script: win.clickItem(win.findName("addProjectChoose"), "Choose…") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const l = JSON.parse(Remotes.foldersJson || "{}")
            win.check(sidebar.remoteFolderDialog.visible && !l.loading && l.error === "" && l.dirs.length > 0 && l.display === "box:~", "it browses the host's home (" + l.display + ", " + l.dirs.length + " folders " + l.error + ")")
            // Straight to the test repository (same machine, over SSH).
            Remotes.requestFolders("ssh://box" + win.argValue("--cwd=").replace(/\/repo$/, "")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const row = win.findText("repo", sidebar.remoteFolderDialog.contentItem)
            win.check(row !== null, "folders are listed")
            win.clickItem(row, "repo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.shotPopup(sidebar.remoteFolderDialog, "clicks-remote-folders")
            win.clickItem(win.findName("remoteFolderAdd", sidebar.remoteFolderDialog.contentItem), "Add repo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("addProjectAdd"), "Add project") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo")
            win.remoteProject = p ? Number(p[0]) : -1
            win.check(p !== undefined && p[2] === "harlo", "the remote folder is a project (Harlo, harlo's own there)")
            const texts = []
            const walk = it => { for (const c of it.children) { if (c.visible && typeof c.text === "string" && c.text.indexOf("box") >= 0) texts.push(c.text); walk(c) } }
            walk(sidebar)
            win.check(texts.includes("box") && Remotes.displayPath("ssh://box" + win.argValue("--cwd=")) === "box:" + win.argValue("--cwd=").replace(Workspace.homeDir(), "~"), "the project shows its host (box)")
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 600 }
        // (click-tests.sh: the host's check says it has no bubblewrap.)
        ScriptAction { script: {
            const row = win.findName("hostProvider-harlo", settingsDialog.contentItem)
            win.check(row !== null && row.detail.indexOf("Sandbox: needs bubblewrap") >= 0, "a host without bubblewrap: Harlo's row says the sandbox needs it (" + (row ? row.detail : "none") + ")")
            win.shotPopup(settingsDialog, "clicks-remote-providers")
            settingsDialog.close()
            Workspace.newSession(win.remoteProject, "") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.waitUntil(remoteSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.log("remote session:", chat.session.harness, chat.session.status, chat.session.cwd, chat.session.modelName, Workspace.approvalMode(chat.session.key))
            win.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "a new session there runs Harlo in the remote folder")
            win.check(chat.session.modelName !== "", "Harlo answers over SSH (model " + chat.session.modelName + ")")
            win.check(Workspace.approvalMode(chat.session.key) === "always-ask", "it starts asking first")
            win.shot("clicks-remote-session") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // Local again: box's projects, sessions and tabs are out of sight.
            win.instanceKey = chat.session.key
            win.clickItem(win.findName("instanceButton"), "the instance button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("instance-local"), "Local") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const tabs = Workspace.tabs.map(t => t.split("\t")[0])
            win.check(Workspace.instance === "" && JSON.parse(Workspace.projectsJson()).every(p => !p.remote), "Local: this computer's projects only")
            // The phone's New Session: every machine's projects, whichever the window shows.
            const ph = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "projects" })))
            const onBox = (ph.projects || []).find(p => p.host === "box")
            win.check(onBox && onBox.where.startsWith("box:") && onBox.agents.indexOf("harlo") >= 0, "the phone still gets box's project, with its machine and agents (" + JSON.stringify(onBox) + ")")
            win.check(tabs.indexOf(win.instanceKey) < 0 && win.findText("box", sidebar) === null, "box's tab and sessions are out of sight (" + tabs + ")")
            // Opening box's session (a notification, say) goes back to box.
            Workspace.open(win.instanceKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const tabs = Workspace.tabs.map(t => t.split("\t")[0])
            win.check(Workspace.instance === "box" && Workspace.activeKey === win.instanceKey && tabs.indexOf(win.instanceKey) >= 0, "opening box's session shows box again, its tab there")
            win.check(Workspace.instanceOf(win.instanceKey) === "box", "a session knows its instance") } }
        PauseAnimation { duration: 2500 }
        // Stage 3: the host's saved sessions in the project.
        ScriptAction { script: {
            const row = win.findText("Remote hello", sidebar)
            win.check(row !== null, "the host's saved session shows in the project")
            win.clickItem(row, "Remote hello") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const find = it => { for (const c of it.children) { if (c.visible && c.html !== undefined && c.html.indexOf("Hi from the remote host") >= 0) return c; const f = find(c); if (f) return f } return null }
            win.check(win.findText("Remote hello", chat.transcript) !== null && find(chat.transcript) !== null, "opening it reads its transcript over SSH")
            win.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "as a Harlo session in the remote folder")
            win.shot("clicks-remote-saved") } }
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
            win.check(key !== "", "the host's session with branches is listed")
            if (key !== "") Workspace.open(key) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(chat.branchCount === 2 && win.findText("2 branches") !== null, "its branches are counted (" + chat.branchCount + ")")
            win.clickText("2 branches") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const d = chat.branchDialog
            win.check(d.visible && d.branches.length === 2 && d.current && d.current.lastUser === "Branchy second", "the dialog lists both, the other one picked (" + (d.current ? d.current.lastUser : "") + ")")
            win.check(d.previewSession.transcriptText().indexOf("ok 2") >= 0 && d.previewSession.transcriptText().indexOf("ok 3") < 0, "and previews it")
            win.clickText("Continue in a new session", d.contentItem) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const k = Workspace.sessionFile(Workspace.activeKey)
            win.check(k.indexOf("ssh://box") === 0 && k.indexOf("remote-branches") < 0 && k.endsWith(".jsonl") && win.chatText().indexOf("ok 2") >= 0 && win.chatText().indexOf("ok 3") < 0, "continuing it opens a new session, written on the host (" + k + ")")
            win.check(TestInput.readFile(k.replace("ssh://box", "")).indexOf("Branchy second") >= 0, "whose file is there") } }
        // The host's sessions are in the message search too.
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const hits = JSON.parse(Workspace.searchMessages("Branchy edited", 20))
            win.check(hits.some(h => String(h.path).indexOf("ssh://box") === 0 && String(h.path).indexOf("remote-branches.jsonl") > 0), "message search finds the host's sessions (" + hits.length + " hits)") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(git.isRepo && git.branch === "main" && git.root === "ssh://box" + win.argValue("--cwd=") && git.files.join("\n").indexOf("notes.md") >= 0,
                      "Changes shows the remote repository (" + git.root + " " + git.branch + " " + git.files.length + " " + git.message + ")")
            // Its branches, read over SSH off the UI thread.
            git.loadBranches() } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const b = git.branchesList !== "" ? JSON.parse(git.branchesList) : {}
            win.check(b.current === "main" && (b.local || []).some(x => x.name === "wt-branch"), "the host's branches are listed (" + git.branchesList.slice(0, 80) + ")")
            // A changed file on the host, side by side.
            win.clickText("notes.md", changes) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(Workspace.activeKey === "diff:0:ssh://box" + win.argValue("--cwd=") + "/notes.md" && dv.rows.length === 1 && dv.rightPane.editor.text === "notes", "the diff tab shows a file on the host (" + Workspace.activeKey + ", " + dv.rows.length + " rows)")
            Workspace.closeTab(Workspace.activeKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(terminal.visible && terminal.current.running && terminal.current.cwd === "ssh://box" + win.argValue("--cwd="), "the terminal opens on the host (" + terminal.current.cwd + ")")
            win.clickItem(terminal.current, "terminal")
            TestInput.typeText("touch remote-term-ok")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.shot("clicks-remote-terminal"); git.refresh(true) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(git.files.join("\n").indexOf("remote-term-ok") >= 0, "its shell runs in the remote folder")
            git.commitAll("remote commit") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(!git.error && git.files.length === 0, "commit all commits there (" + git.message + ")")
            // Stage 5: files on the host.
            terminal.visible && win.toggleTerminal()
            windowSettings.rightMode = "files"
            chat.session.searchFiles("READ") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(chat.session.fileMatches.indexOf("README.md") >= 0, "@ completion lists the host's files (" + chat.session.fileMatches.join(",") + ")")
            const row = win.findText("README.md", files)
            win.check(row !== null && win.findText(".git", files) === null, "the Files panel lists the remote folder")
            win.clickItem(row, "README.md") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const find = it => { for (const c of it.children) { if (c.visible && c.text === "hello\n" && c.cursorPosition !== undefined) return c; const f = find(c); if (f) return f } return null }
            win.check(Workspace.activeKey === "file:ssh://box" + win.argValue("--cwd=") + "/README.md" && find(win.contentItem) !== null, "it opens in the editor, read over SSH (" + Workspace.activeKey + ")")
            win.shot("clicks-remote-file")
            // Edited and saved: over SSH, in the background.
            chat.fileEditor.editor.text = "hello from harlo\n"
            chat.fileEditor.save() } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const readme = win.argValue("--cwd=") + "/README.md"
            win.check(TestInput.readFile(readme) === "hello from harlo\n" && !chat.fileEditor.modified, "saving writes it on the host")
            // Changed on the host (an agent there, say): no edits here, so it reloads.
            TestInput.writeFile(readme, "changed on the host\n") } }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: {
            win.check(chat.fileEditor.editor && chat.fileEditor.editor.text === "changed on the host\n", "a change on the host reloads it (" + (chat.fileEditor.editor ? JSON.stringify(chat.fileEditor.editor.text) : "") + ")")
            // With edits here, a change there is pointed out instead.
            chat.fileEditor.apply([[0, chat.fileEditor.editor.length, "my edit\n"]])
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "changed again\n") } }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: {
            win.check(chat.fileEditor.diskNotice === "changed" && chat.fileEditor.editor.text === "my edit\n", "with unsaved edits, it says the file changed there (" + chat.fileEditor.diskNotice + ")")
            win.summary(); Qt.quit() } }
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
            win.check(win.findName("notifyEnabled", d).checked && win.findName("notifyWhileFocused", d).checked && win.findName("notifyEvent-subagent", d).checked, "notifications are on by default (while focused too, like before)")
            win.check(win.findName("notifyTitle-completion", d).placeholderText === "{agent_name} is ready", "templates show their defaults")
            win.clickItem(win.findName("notifyTitle-completion", d), "completion title")
            TestInput.typeText("{session_name} is done")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(JSON.parse(Workspace.notifySettingsJson()).events.completion.title === "{session_name} is done", "editing a template saves it")
            win.shotPopup(settingsDialog, "clicks-notify-settings") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { settingsDialog.close(); Workspace.setWindowActive(true); win.notifyKeyA = Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.sendByKeys("fast: hello"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            const n = win.lastNote()
            win.check(n.kind === "completion" && n.key === win.notifyKeyA && n.title === "fast: hello is done" && n.body === "Claude Demo completed the task",
                      "a finished session you aren't viewing notifies, with the template (" + Workspace.lastNotification + ")")
            Workspace.open(win.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.sendByKeys("approve please"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const n = win.lastNote()
            win.check(n.kind === "question" && n.title === "Input needed" && n.body !== "", "a question notifies (" + n.body + ")")
            // While focused, off: quiet while harlo is focused.
            const c = JSON.parse(Workspace.notifySettingsJson()); c.whileFocused = false; Workspace.setNotifySettings(JSON.stringify(c))
            win.notifyBefore = Workspace.lastNotification
            Workspace.open(win.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.sendByKeys("fast: again"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            win.check(Workspace.lastNotification === win.notifyBefore, "with “while focused” off, nothing while harlo is focused")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.whileFocused = true; Workspace.setNotifySettings(JSON.stringify(c))
            Workspace.open(win.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.sendByKeys("fast: watched") }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            win.check(Workspace.lastNotification === win.notifyBefore, "never for the session you're looking at")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.enabled = false; Workspace.setNotifySettings(JSON.stringify(c))
            win.sendByKeys("fast: off"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            win.check(Workspace.lastNotification === win.notifyBefore, "nothing when notifications are off")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.enabled = true; Workspace.setNotifySettings(JSON.stringify(c))
            settingsDialog.openPage("notifications") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("notifyTest", settingsDialog.contentItem), "Send test notification") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.lastNote().kind === "test" && win.lastNote().title === "Test notification", "Send test notification sends one")
            win.clickItem(win.findName("quietOn", settingsDialog.contentItem), "Quiet hours") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const q = JSON.parse(Workspace.notifySettingsJson()).quiet
            win.check(q.on && q.from === 1320 && q.to === 480 && !q.asks, "Quiet hours on: 22:00 to 08:00 at first (" + JSON.stringify(q) + ")")
            win.check(win.findName("quietFrom", settingsDialog.contentItem).displayText === "22:00", "its times show")
            win.shotPopup(settingsDialog, "clicks-notify-quiet")
            // Quiet from this half hour for an hour: now.
            const now = new Date(), m = now.getHours() * 60 + now.getMinutes()
            const c = JSON.parse(Workspace.notifySettingsJson()); c.quiet.from = m - m % 30; c.quiet.to = (m - m % 30 + 60) % 1440
            Workspace.setNotifySettings(JSON.stringify(c))
            settingsDialog.close()
            win.notifyBefore = Workspace.lastNotification
            Workspace.open(win.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.sendByKeys("fast: at night"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            win.check(Workspace.lastNotification === win.notifyBefore, "quiet hours: a finished run doesn't notify")
            const c = JSON.parse(Workspace.notifySettingsJson()); c.quiet.asks = true; Workspace.setNotifySettings(JSON.stringify(c))
            Workspace.open(win.notifyKeyA) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.sendByKeys("approve please"); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(win.lastNote().kind === "question" && Workspace.lastNotification !== win.notifyBefore, "with “let questions through”, a question still does")
            win.summary(); Qt.quit() } }
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
            win.mcpProject = Workspace.projectRoots(1)[0]
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("No MCP servers here yet.", d) !== null, "no servers yet")
            win.clickItem(win.findName("mcpAdd", d), "Add MCP server") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.clickItem(win.findName("mcpName", d), "name")
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("fake")
            win.clickItem(win.findName("mcpCommand", d), "command")
            TestInput.typeText(win.argValue("--mcp-server="))
            win.clickItem(win.findName("mcpSave", d), "Create") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const l = win.mcpList("global")
            win.check(l.length === 1 && l[0].name === "fake" && l[0].harnesses.join() === "harlo", "Create saves it for Harlo (" + JSON.stringify(l.map(s => s.name + ":" + s.harnesses)) + ")")
            const st = JSON.parse(Mcp.statusJson("global")).fake
            win.check(st && st.state === "connected" && st.tools.length === 2, "and tests it: connected, 2 tools (" + JSON.stringify(st) + ")")
            const pill = win.findName("mcpStatus-fake", settingsDialog.contentItem)
            win.check(pill !== null && pill.text === "Connected · 2 tools", "the card shows the status")
            win.shotPopup(settingsDialog, "clicks-mcp-grid")
            win.clickItem(win.findName("mcpToggle-fake", settingsDialog.contentItem), "the card's switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = win.mcpList("global")
            win.check(!l[0].form.enabled && win.findName("mcpStatus-fake", settingsDialog.contentItem).text === "Disabled", "the card's switch turns it off")
            win.clickItem(win.findName("mcpToggle-fake", settingsDialog.contentItem), "the card's switch") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const st = JSON.parse(Mcp.statusJson("global")).fake
            win.check(win.mcpList("global")[0].form.enabled && st && st.state === "connected", "and on again, tested at once")
            win.clickItem(win.findName("mcpServer-fake", settingsDialog.contentItem), "fake") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("look", d) !== null && win.findText("Looks.", d) !== null, "the server's page lists its tools")
            const bar = win.findName("mcpActionBar", d)
            const flick = win.ancestorWith(win.findName("mcpName", d), "contentY")
            win.check(bar !== null && bar.parent.objectName === "settingsFooter" && bar.mapToItem(null, 0, bar.height).y <= d.mapToItem(null, 0, d.height).y + 1
                      && flick.contentHeight > flick.height, "Save / Cancel / Delete float at the bottom while the page scrolls")
            win.shotPopup(settingsDialog, "clicks-mcp-server")
            win.check(win.findName("mcpForHarlo", d) !== null && win.findName("mcpForHarlo", d).checked, "it's ticked for Harlo")
            win.clickItem(win.findName("mcpTimeout", d), "timeout")
            TestInput.typeText("45")
            win.clickItem(win.findName("mcpSave", d), "Save") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const l = win.mcpList("global")
            win.check(l.length === 1 && l[0].harnesses.join() === "harlo" && l[0].form.timeoutSecs === 45, "the timeout is saved (" + JSON.stringify(l[0]) + ")")
            settingsDialog.contentItem && win.clickItem(win.findName("mcpScope", settingsDialog.contentItem), "scope") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("repo") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("No MCP servers here yet.", d) !== null && win.findText("fake", d) === null, "the project scope has its own servers")
            win.clickItem(win.findName("mcpAdd", d), "Add") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("mcpImport", settingsDialog.contentItem), "Import JSON…") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = win.findName("mcpImportText")
            t.text = '{"mcpServers":{"one":{"command":"' + win.argValue("--mcp-server=") + '"},"web":{"type":"http","url":"http://127.0.0.1:9/mcp"}}}'
            win.clickItem(win.findName("mcpImportGo"), "Import") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            const l = win.mcpList(win.mcpProject)
            win.check(l.length === 2 && l.every(s => s.harnesses.join() === "harlo"), "importing two servers adds both to the project (" + JSON.stringify(l.map(s => s.name)) + ")")
            const st = JSON.parse(Mcp.statusJson(win.mcpProject))
            win.check(st.one && st.one.state === "connected" && st.web && st.web.state === "failed", "and tests them (" + (st.web ? st.web.error : "") + ")")
            win.clickItem(win.findName("mcpServer-web", settingsDialog.contentItem), "web") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickItem(win.findName("mcpDelete", settingsDialog.contentItem), "Delete") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("Discard") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = win.mcpList(win.mcpProject)
            win.check(l.length === 1 && l[0].name === "one", "Delete removes it")
            win.check(Workspace.fileStamp(win.mcpProject + "/.harlo/mcp.json") !== "", "project servers are in .harlo/mcp.json")
            win.summary(); Qt.quit() } }
    }
    // Phone access from the desktop: the sidebar's button, the consent
    // screen, the QR code and link, Settings → Devices. (scripts/
    // phone-e2e.sh tests a phone pairing for real.)
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksphone"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000))
            win.clickItem(win.findName("pairDeviceButton"), "phone button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(pairDeviceDialog.visible && win.findText("Turn on phone access?") !== null && !Phone.running, "the phone button asks to turn phone access on")
            win.shotPopup(pairDeviceDialog, "clicks-phone-consent") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("phoneTurnOn"), "Turn on") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const qr = win.findName("pairQr"), link = win.findName("pairLink")
            win.check(Phone.enabled && Phone.running && Phone.error === "", "Turn on starts it (" + Phone.error + ")")
            win.check(qr !== null && String(qr.source).startsWith("data:image/svg+xml") && link !== null && /^http:\/\/[0-9.]+:[0-9]+\/#p=/.test(link.text),
                      "a QR code and link appear (" + (link ? link.text.substring(0, 40) : "") + ")")
            win.shotPopup(pairDeviceDialog, "clicks-phone-qr") } }
        PauseAnimation { duration: 400 }
        // The firewall (a stand-in ufw that blocks the port): pointed out,
        // and opened with one click (pkexec asks for the password).
        ScriptAction { script: {
            win.check(win.findName("firewallNote") !== null && win.findText("This computer's firewall (ufw) blocks phones, so the link won't open on yours yet.") !== null,
                      "a firewall that blocks phones is pointed out")
            win.shotPopup(pairDeviceDialog, "clicks-phone-firewall") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("allowFirewall"), "Allow phones on this network") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            win.check(win.findName("firewallNote") === null && Phone.firewallError === "" && !Phone.firewallBusy && win.findName("pairQr") !== null,
                      "Allow opens it and the note goes (" + Phone.firewallError + ")")
            // The app tries every address: the other kind (Tailscale or this
            // network), when this computer has one, is pointed out too.
            const kinds = JSON.parse(Phone.addressesJson()).map(a => String(a.label).startsWith("Tailscale"))
            win.otherAddress = kinds.indexOf(true) >= 0 && kinds.indexOf(false) >= 0
            if (win.otherAddress) {
                win.check(win.findName("firewallOtherNote") !== null, "the firewall on the other address is pointed out too")
                win.clickItem(win.findName("allowFirewallOther"), "Allow on the other address")
            } } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: if (win.otherAddress) win.check(win.findName("firewallOtherNote") === null, "and Allow opens that one too") }
        PauseAnimation { duration: 300 }
        // From anywhere, 1: harlo's relay (relay.harlo.run, or --relay=).
        ScriptAction { script: win.clickItem(win.findName("pairAddress"), "the address picker") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Anywhere  ·  harlo relay") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findText("Use harlo's relay?") !== null && win.findName("pairQr") === null, "harlo's relay asks first")
            win.shotPopup(pairDeviceDialog, "clicks-phone-relay-consent")
            win.clickItem(win.findName("pairRelayOn"), "Use relay") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const link = win.findName("pairLink")
            win.check(Phone.relayOn && link !== null && link.text.startsWith(win.relayUnderTest + "/#p="), "the QR code points at the relay (" + (link ? link.text.substring(0, 40) : "") + ")")
            win.check(Phone.relayState === "online" && win.findName("pairRelayState") === null, "harlo is connected to the relay (" + Phone.relayState + " " + Phone.relayError + ")")
            win.shotPopup(pairDeviceDialog, "clicks-phone-relay-qr")
            win.clickItem(win.findName("pairAddress"), "the address picker") } }
        // From anywhere, 2: Tailscale Funnel (a stand-in tailscale).
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Anywhere  ·  Tailscale Funnel") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Phone.funnelAvailable && win.findText("Use Tailscale Funnel?") !== null && win.findName("pairQr") === null, "Funnel asks first")
            win.clickItem(win.findName("pairFunnelOn"), "Use Funnel") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const link = win.findName("pairLink")
            win.check(Phone.funnelOn && Phone.funnelState === "online" && link !== null && link.text.startsWith("https://desk.tail1234.ts.net:8443/#p="),
                      "the QR code points at the funnel (" + Phone.funnelState + " " + Phone.funnelError + " " + (link ? link.text.substring(0, 44) : "") + ")")
            win.shotPopup(pairDeviceDialog, "clicks-phone-funnel-qr") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { pairDeviceDialog.close(); settingsDialog.openPage("devices") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findName("phoneAccess", d).checked && win.findText("No phones paired yet.", d) !== null, "Settings → Devices: on, no phones yet")
            win.check(win.findName("relaySwitch", d).checked && win.findName("funnelSwitch", d).checked, "the relay and Funnel show as on")
            win.shot("clicks-phone-settings") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.clickItem(win.findName("funnelSwitch", settingsDialog.contentItem), "the Funnel switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(!Phone.funnelOn && Phone.funnelState === "off", "the switch turns Funnel off")
            win.clickItem(win.findName("phoneAccess", settingsDialog.contentItem), "the switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(!Phone.enabled && !Phone.running && Phone.relayState === "off", "the switch turns it off, the relay too")
            win.summary(); Qt.quit() } }
    }
    readonly property string relayUnderTest: win.argValue("--relay=") || "https://relay.harlo.run"
    // Claude sessions, against a stand-in claude (scripts/fake-claude.py,
    // --fake-claude=): a reply, an approval, a question, model / Plan /
    // permission switches (the stand-in's replies report them), stopping,
    // and the session in the sidebar, reopened with its history and resumed.
    property string claudeKey: ""
    function chatText() { return chat.session.transcriptText() }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksclaude"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.harness === "claude", "a Claude session (" + chat.session.harness + ")")
            win.check(win.chatText().indexOf("Hello from fake claude (model claude-sonnet-5, mode default)") >= 0, "it streams the reply (" + win.chatText().slice(-160) + ")")
            Workspace.renameSession(win.claudeKey, "My Claude chat") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(chat.session.title === "My Claude chat", "renamed (" + chat.session.title + ")")
            win.sendByKeys("please approve writing a file") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            // (On its tool row when that's in view; the bar otherwise.)
            win.check(chat.session.requestIsApproval && (win.findText("Allow Write to run?", chat.transcript) !== null || win.findText("Approval needed: Write", chat.requestDock) !== null), "a Write asks for approval")
            win.shotWindow("clicks-claude-approval")
            win.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Wrote hi.txt.") >= 0, "Approve lets it write")
            win.sendByKeys("approve it again") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const always = win.findName("alwaysAllowInline", chat.transcript)
            win.check(always !== null && always.ToolTip.text === "Remember: acceptEdits mode (for this session)", "it offers Always allow with Claude's suggestion (" + (always ? always.ToolTip.text : "none") + ")")
            if (always) win.clickItem(always, "Always allow") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("remembering acceptEdits") >= 0, "Always allow hands Claude its suggestion back (" + win.chatText().slice(-60) + ")")
            win.sendByKeys("plan it please") } }
        // A long plan to approve: drawn as Markdown, scrolling in its card,
        // its answers in view.
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const scroll = win.findName("requestScroll")
            const start = win.findText("Start with this plan")
            const prose = win.findAllNames("requestProse", scroll || win.contentItem).map(t => t.getText(0, t.length))
            win.check(chat.session.hasRequest && scroll !== null && scroll.scrolls && scroll.height <= Math.max(140, chat.height * 0.45) + 1, "a long plan scrolls in its card (" + (scroll ? Math.round(scroll.height) + " of " + Math.round(scroll.contentHeight) : "none") + ")")
            win.check(start !== null && start.mapToItem(null, 0, start.height).y <= win.height && win.findText("Keep planning") !== null, "its answers stay in view")
            win.check(prose.some(t => t === "Context") && !prose.some(t => t.indexOf("## ") >= 0) && prose.some(t => t.indexOf("Step 1.") >= 0 && t.indexOf("**") < 0), "it reads as Markdown: headings and a list, not ## or ** (" + prose.slice(0, 3).join(" | ") + ")")
            win.shotWindow("clicks-claude-plan")
            if (scroll) scroll.contentY = scroll.contentHeight - scroll.height } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const last = win.findAllNames("requestProse", win.findName("requestScroll")).find(t => t.getText(0, t.length) === "The last line of the plan.")
            win.check(last !== null && last !== undefined, "scrolled down, its end shows")
            win.clickText("Start with this plan") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Plan approved.") >= 0, "Start with this plan approves it (" + win.chatText().slice(-60) + ")")
            win.sendByKeys("ask me which database") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.hasRequest && chat.session.requestMethod === "ask" && win.findText("Which database?", chat.askPanel) !== null, "its question shows in the ask panel")
            win.clickText("Postgres", chat.askPanel) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Send answers", chat.askPanel) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("You picked Postgres.") >= 0, "the answer goes back (" + win.chatText().slice(-80) + ")")
            chat.session.setModel("anthropic", "claude-haiku-4-5")
            win.clickItem(win.findName("modeButton", chat.composerItem), "Plan") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.sendByKeys("what now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("(model claude-haiku-4-5, mode plan)") >= 0, "the model and Plan reach Claude (" + win.chatText().slice(-110) + ")")
            win.clickItem(win.findName("modeButton", chat.composerItem), "Build")
            win.clickItem(win.findName("permissionsButton", chat.composerItem), "the shield") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.sendByKeys("and now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("mode acceptEdits)") >= 0, "the shield's edits mode reaches Claude (" + win.chatText().slice(-80) + ")")
            win.sendByKeys("run a build in the background") } }
        // A background command's end: a notice as it comes (and the same
        // one read back from the file, below).
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            const said = 'Background command "sleep 1; echo built" completed (exit code 0)'
            win.check(win.chatText().split(said).length === 2 && win.chatText().indexOf("It finished: built.") >= 0, "a background command's end is a notice as it comes (" + win.chatText().slice(-160) + ")")
            win.check(win.findContaining("<task-notification>", chat.transcript) === null, "not a message of yours")
            win.sendByKeys("go slow please") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            win.check(chat.session.status === "busy", "a slow reply runs")
            win.clickItem(win.findName("stopButton", chat.composerItem), "Stop") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(chat.session.status === "idle", "Stop interrupts it (" + chat.session.status + ")")
            // Stop the process (the sidebar's Stop process): the session is
            // then its file.
            Workspace.closeSession(win.claudeKey) } }
        // The scanner lists it from its file; open it again.
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            // Its own transcript (under the test's Claude config folder) only.
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("/claude-config/projects/") >= 0) saved = Workspace.keyAt(i)
            win.check(saved !== "", "the session is in the sidebar (" + saved + ")")
            if (saved === "") { win.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("hello there") >= 0 && win.chatText().indexOf("You picked Postgres.") >= 0, "reopened, it shows its history")
            win.check(chat.session.title === "My Claude chat", "and its name (" + chat.session.title + ")")
            win.check(win.chatText().split('Background command "sleep 1; echo built" completed (exit code 0)').length === 2 && win.findContaining("<task-notification>", chat.transcript) === null, "the background command's notice, once, read back")
            if (chat.session.harness !== "claude") { win.check(false, "the reopened session is the Claude one"); win.summary(); Qt.quit() }
            else win.sendByKeys("one more") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Hello from fake claude") >= 0 && chat.session.status === "idle" && win.chatText().split("Hello from fake claude").length > 1, "and it carries on (resumed)")
            const hits = JSON.parse(Workspace.searchMessages("picked Postgres", 20))
            win.check(hits.some(h => String(h.path).indexOf("/claude-config/projects/") >= 0), "message search finds Claude's sessions (" + hits.length + " hits)")
            win.shot("clicks-claude")
            // Archived from the phone while open (and done): it leaves the lists.
            win.claudeKey = chat.session.key
            const r = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "archive", key: win.claudeKey, archived: true })))
            win.check(r.ok === true, "the phone archives it (" + JSON.stringify(r) + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const keys = []
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) keys.push(Workspace.keyAt(i))
            win.check(keys.indexOf(win.claudeKey) < 0 && !keys.some(k => k.indexOf("/claude-config/projects/") >= 0), "archived, the open session leaves the sidebar (" + keys + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("sessionProvider", sidebar, []).map(m => m.provider).sort()
            win.check(marks.join(",") === "claude,demo", "each session shows its provider's mark (" + marks + ")")
            const p = win.findName("projectProvider-repo", sidebar)
            // (A narrow sidebar leaves the mark out: the name keeps its room.)
            win.check(p !== null ? p.provider === "claude" : sidebar.width < Theme.px(240), "the project shows what its new sessions run (" + (p ? p.provider : "narrow: left out") + ")")
            win.check(win.findText("Demo", win.findName("sessionProviderChip")) !== null, "the header names the provider")
            const tabMarks = win.findAllNamed("tabProvider", win.contentItem, []).map(m => m.provider).sort()
            win.check(tabMarks.join(",") === "claude,demo", "and so do the tabs (" + tabMarks + ")")
            const cl = win.findAllNamed("sessionProvider", sidebar, []).find(m => m.provider === "claude")
            const at = cl.mapToItem(null, cl.width / 2, cl.height / 2)
            TestInput.moveTo(at.x, at.y) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            win.check(win.findText("Claude") !== null, "hovered, the mark says which provider")
            win.shotWindow("clicks-marks")
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("menuProvider", null, []).map(m => m.provider)
            win.check(marks.filter(m => m === "claude").length === 2 && marks.indexOf("harlo") >= 0, "the project menu shows the providers' marks (" + marks + ")")
            win.shotWindow("clicks-marks-menu")
            sidebar.projectMenu.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.clickItem(win.findName("draftHarness", chat.draftView), "the provider chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("menuProvider", null, []).map(m => m.provider)
            win.check(marks.indexOf("claude") >= 0 && marks.indexOf("harlo") >= 0, "so does New session's (" + marks + ")")
            win.shotWindow("clicks-marks-draft")
            TestInput.key(Qt.Key_Escape, 0)
            win.summary(); Qt.quit() } }
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
            win.check(d.accounts.length === 1 && a.provider === "Claude" && a.plan === "Max" && a.account === "me@example.com", "Claude's plan and account (" + JSON.stringify(a || d) + ")")
            win.check(a && a.windows.map(w => w.label).join(",") === "Session (5 h),Weekly,Weekly · Opus" && a.windows[1].usedFraction === 0.81, "its windows")
            win.check(win.findName("planMeter") !== null && win.findName("planMeterClaude") !== null, "the sidebar meter shows, with Claude's mark")
            settingsDialog.openPage("usage") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("Weekly · Opus", d) !== null && win.findText("Max", d) !== null, "Settings → Usage lists them")
            win.shotWindow("clicks-claude-usage")
            Workspace.refreshQuota() } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const d = JSON.parse(Workspace.quotaJson)
            win.check(d.error.indexOf("rate limited") >= 0 && d.accounts.length === 1 && d.accounts[0].windows.length === 3, "Claude Code can't say: the last numbers stay (" + d.error + ")")
            win.check(!TestInput.readFile(win.selftestDir + "/claude-config/.credentials.json"), "harlo never needed Claude's sign-in file")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.delegateKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.subagentDock.expanded = true; win.sendByKeys("please delegate this") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].name === "Look around" && a[0].agent === "Explore" && a[0].status === "completed", "Claude: the subagent is listed, done (" + JSON.stringify(a) + ")")
            win.check(a.length === 1 && String(a[0].sessionFile).indexOf("/subagents/agent-") > 0 && win.chatText().indexOf("The subagent found notes.md.") >= 0, "with its transcript, and the reply")
            win.check(Workspace.meterLast === "Claude", "the meter follows Claude's limits now (" + Workspace.meterLast + ")")
            win.check(win.findText("Look around", chat.subagentDock) !== null, "the dock names it by its description")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            win.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "View: its own conversation (" + t.slice(0, 120).replace(/\n/g, " | ") + ")")
            win.check(win.findName("steerField") === null && win.findText("Stop helper") === null, "no Steer or Cancel for Claude's")
            win.shotWindow("clicks-delegate-claude")
            chat.subagentDialog.close()
            Workspace.closeSession(win.delegateKey) } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("/claude-config/projects/") >= 0) saved = Workspace.keyAt(i)
            if (saved !== "") Workspace.open(saved) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(chat.session.harness === "claude" && a.length === 1 && a[0].name === "Look around" && a[0].status === "completed" && !!a[0].sessionFile, "reopened: listed from disk (" + JSON.stringify(a) + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.clickItem(win.findName("mcpAdd", settingsDialog.contentItem), "Add MCP server") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, c = win.findName("mcpForClaude", d), h = win.findName("mcpForHarlo", d)
            win.check(c !== null && c.checked && h !== null && h.checked, "a new server is for Claude and Harlo (the installed agents that are on)")
            win.shotWindow("clicks-mcp-agents")
            win.clickItem(win.findName("mcpBack", d), "back")
            const err = Mcp.save("global", "", JSON.stringify({ name: "files", enabled: true, transport: "stdio", command: ["npx", "-y", "files-mcp"], env: [["TOKEN", "t"]], url: "", headers: [], cwd: "", timeoutSecs: 20 }), JSON.stringify(["claude", "harlo"]))
            win.check(err === "", "saved (" + err + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            const hs = win.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            win.check(JSON.stringify(claude.files) === JSON.stringify({ type: "stdio", command: "npx", args: ["-y", "files-mcp"], env: { TOKEN: "t" } }), "Claude's dialect, through claude mcp (" + JSON.stringify(claude.files) + ")")
            win.check(JSON.stringify(hs.files) === JSON.stringify({ command: "npx", args: ["-y", "files-mcp"], env: { TOKEN: "t" }, timeout: 20, enabled: true }), "Harlo's dialect (" + JSON.stringify(hs.files) + ")")
            const l = win.mcpList("global")
            win.check(l.length === 1 && l[0].harnesses.join(",") === "claude,harlo" && !l[0].differs, "listed once, for both")
            win.check(Mcp.setEnabled("global", "files", false) === "", "switched off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const hs = win.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            win.check(hs.files.enabled === false && !!claude.files && win.mcpList("global")[0].differs, "Harlo's is off; Claude's stays (no switch)")
            win.check(Mcp.remove("global", "files") === "", "removed") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const hs = win.readJson(win.selftestDir + "/config/harlo-agent/mcp.json").mcpServers || {}
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            win.check(!hs.files && !claude.files && win.mcpList("global").length === 0, "gone from both")
            const project = win.argValue("--cwd=")
            win.check(Mcp.save(project, "", JSON.stringify({ name: "web", enabled: true, transport: "http", command: [], env: [], url: "https://example.com/mcp", headers: [["Authorization", "Bearer x"]], cwd: "" }), JSON.stringify(["claude", "harlo"])) === "", "a project server") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const project = win.argValue("--cwd=")
            const claude = win.readJson(project + "/.mcp.json").mcpServers || {}, hs = win.readJson(project + "/.harlo/mcp.json").mcpServers || {}
            win.check(claude.web && claude.web.type === "http" && claude.web.headers.Authorization === "Bearer x" && hs.web && hs.web.url === "https://example.com/mcp", "in the project's .mcp.json and .harlo/mcp.json")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
    }

    // Edit & resend for Claude (Stage 4): hovering a prompt
    // offers it; it forks the session before that prompt (the original
    // stays as it was) and puts the prompt back in the composer; the next
    // message goes on in the fork.
    property string forkFile: ""
    function hoverEdit(text) {
        const bubble = win.findText(text, chat.transcript)
        if (!bubble) return false
        const at = bubble.mapToItem(null, 4, 4)
        TestInput.moveTo(at.x, at.y)
        return true
    }
    SequentialAnimation {
        id: effortCheck
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("after the effort change") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("effort low") >= 0 && t.indexOf("second, edited") >= 0 && t.indexOf("second message") < 0, "a restart (effort) resumes the fork (" + t.slice(-90) + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.sendByKeys("first message") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("second message") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.forkFile = Workspace.sessionFile(Workspace.activeKey)
            win.check(win.hoverEdit("second message"), forkSteps.harness + ": the prompt is there") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Edit & resend", chat.transcript) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const now = Workspace.sessionFile(Workspace.activeKey)
            win.check(chat.composerItem.text === "second message", "the prompt is back in the composer (" + chat.composerItem.text + ")")
            win.check(win.chatText().indexOf("first message") >= 0 && win.chatText().indexOf("second message") < 0, "the transcript ends before it")
            win.check(now !== "" && now !== win.forkFile, "in a new session (" + now.split("/").pop() + ")")
            chat.composerItem.text = ""
            win.sendByKeys("second, edited") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("first message") >= 0 && t.indexOf("second, edited") >= 0 && t.indexOf("second message") < 0 && chat.session.status === "idle", "and goes on there")
            win.shot("clicks-fork-" + forkSteps.harness)
            if (forkSteps.harness === "claude") {
                win.check(TestInput.readFile(win.forkFile).indexOf("second message") >= 0, "the original is as it was")
                // Effort restarts Claude (resuming the fork).
                chat.session.setThinkingLevel("low")
                forkSteps.harness = "claude-effort"
                forkSteps.stop(); effortCheck.start()
            } else {
                win.summary(); Qt.quit()
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
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksremoteclaude"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            win.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        // (The first connection is slow when the machine is busy.)
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            win.check(JSON.parse(Remotes.hostsJson())[0].state === "connected", "connected")
            settingsDialog.openPage("remote") } }
        PauseAnimation { duration: 600 }
        // Settings → Remote Instances: the host's agents.
        ScriptAction { script: {
            const d = settingsDialog.contentItem, h = JSON.parse(Remotes.hostsJson())[0]
            win.check(win.findName("hostAgent-box-claude", d) !== null && win.findText("Claude 2.1.296", d) !== null, "the host's agents are listed, with versions")
            win.check(win.findName("hostSignIn-box-claude", d) !== null && h.claudeAuth === "no", "Claude not signed in there: Sign in")
            win.check(win.findName("hostInstall-box-claude", d) === null && win.findName("hostRecheck-box", d) !== null, "and Check again")
            win.shotWindow("clicks-remote-agents")
            settingsDialog.close()
            Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = "ssh://box" + win.argValue("--cwd="); sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("New Claude session") !== null && win.findText("New Harlo session") !== null, "a remote project offers Claude (found there) and Harlo (harlo's own)")
            sidebar.projectMenu.close()
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            win.remoteClaudeKey = Workspace.newSession(p.id, "claude") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const file = Workspace.sessionFile(Workspace.activeKey)
            win.shotWindow("clicks-remote-claude-chat")
            win.check(chat.session.harness === "claude" && win.chatText().indexOf("Hello from fake claude") >= 0, "Claude answers over SSH (" + win.chatText().slice(-80) + ")")
            win.check(file.indexOf("ssh://box" + win.selftestDir + "/claude-config/projects/") === 0, "its transcript is the host's (" + file + ")")
            // Orchestration there: the bridge on the host, harlo's socket
            // forwarded to it; the helper starts on the host too.
            win.sendByKeys("please orchestrate a helper") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const t = win.chatText(), kids = JSON.parse(Workspace.orchestratedJson(win.remoteClaudeKey))
            win.check(/Started a helper with harlo \([1-9][0-9]* tools\): live:/.test(t), "on the host, the agent finds harlo's tools (" + t.slice(-110) + ")")
            win.check(kids.length === 1 && Workspace.instanceOf(kids[0].id) === "box", "and its helper runs on box (" + JSON.stringify(kids) + ")")
            win.check(t.indexOf("The Claude session") >= 0 && t.indexOf("harlo · ") >= 0, "whose reply comes back to it")
            win.check(Workspace.sessionFile(win.remoteClaudeKey) !== "", "(still the host's session)")
            Workspace.renameSession(win.remoteClaudeKey, "Remote chat") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const file = Workspace.sessionFile(Workspace.activeKey).replace("ssh://box", "")
            win.check(TestInput.readFile(file).indexOf('"customTitle":"Remote chat"') >= 0, "the name is written there")
            win.sendByKeys("please delegate this") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].status === "completed" && String(a[0].sessionFile).indexOf("ssh://box") === 0, "a subagent there, its transcript the host's (" + (a[0] ? a[0].sessionFile : "none") + ")")
            Workspace.closeSession(win.remoteClaudeKey) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("ssh://box" + win.selftestDir + "/claude-config/projects/") === 0) saved = Workspace.keyAt(i)
            win.check(saved !== "", "the host's Claude session is in the sidebar (" + saved + ")")
            if (saved === "") { win.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(chat.session.harness === "claude" && win.chatText().indexOf("hello there") >= 0 && chat.session.title === "Remote chat", "reopened: its history and name (" + chat.session.title + ")")
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].name === "Look around" && a[0].status === "completed" && String(a[0].sessionFile).indexOf("ssh://box") === 0, "and its subagent, from the host (" + JSON.stringify(a.map(x => [x.name, x.status])) + ")")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            win.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "whose own conversation opens from there")
            chat.subagentDialog.close()
            if (chat.session.harness !== "claude") { win.summary(); Qt.quit() }
            else win.sendByKeys("one more") } }
        PauseAnimation { duration: 4000 }
        // An edit there: numbered from the host's file (read over SSH).
        ScriptAction { script: win.sendByKeys("show the tools please") }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const edit = win.findAllNames("previewCode", chat.transcript).find(v => v.signs)
            const en = edit ? edit.lines.map(l => l.n) : []
            win.check(edit && JSON.stringify(en) === JSON.stringify(["3", "", "", "4", "5", "6"]) && edit.lines[0].p.indexOf("ssh://box") === 0,
                      "an edit on the host, numbered from its file there (" + JSON.stringify(en) + ")") } }
        ScriptAction { script: {
            win.check(win.chatText().split("Hello from fake claude").length > 2 && chat.session.status === "idle", "and it carries on there (resumed)")
            win.shotWindow("clicks-remote-claude")
            // A worktree on the host: the dialog lists its branches (read in
            // the background), the project's setup runs there.
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            Workspace.setProjectSetup(p.id, "echo ok > setup-ran")
            sidebar.worktreeDialogs.openNew(p.id, p.name, "claude") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const box = sidebar.worktreeDialogs.baseBox
            win.check(box.count >= 2 && box.currentText === "main", "New worktree lists the host's branches, current first (" + box.count + " " + box.currentText + ")")
            sidebar.worktreeDialogs.newDialog.close()
            Workspace.newWorktreeSession(JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, "feature-x", "main", "claude") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const dir = win.selftestDir + "/remote-data/harlo/worktrees/"
            win.remoteClaudeKey = Workspace.activeKey
            win.check(String(chat.session.cwd).indexOf("ssh://box" + dir) === 0 && String(chat.session.cwd).endsWith("/feature-x"), "a worktree session runs in the host's worktree (" + chat.session.cwd + ")")
            const wt = String(chat.session.cwd).replace("ssh://box", "")
            win.check(TestInput.readFile(wt + "/README.md") === "hello\n", "the worktree is checked out there")
            win.check(TestInput.readFile(wt + "/setup-ran") === "ok\n", "and the project's setup ran in it")
            win.sendByKeys("hello from the worktree") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Hello from fake claude") >= 0, "Claude answers in it")
            win.remoteWorktree = String(chat.session.cwd).replace("ssh://box", "")
            Workspace.removeWorktree(win.remoteClaudeKey, true, true) } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            win.check(TestInput.readFile(win.remoteWorktree + "/README.md") === "", "removing it removes it there")
            // Settings on box: its agents, and its own MCP files.
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, row = win.findName("hostProvider-claude", d)
            win.check(Workspace.instance === "box" && row !== null && row.detail.indexOf("Version 2.1.296") === 0 && row.detail.indexOf("not signed in") > 0, "Providers shows box's agents (" + (row ? row.detail : "none") + ")")
            win.check(win.findName("provider-claude", d) === null, "not this computer's list")
            win.shotWindow("clicks-remote-providers")
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const err = Mcp.save("host:box", "", JSON.stringify({ name: "boxtools", enabled: true, transport: "stdio", command: ["box-mcp"], url: "", headers: [], env: [], cwd: "", timeoutSecs: null, exposure: "", description: "" }), JSON.stringify(["harlo", "claude"]))
            const home = win.selftestDir + "/claude-config"
            const hs = JSON.parse(TestInput.readFile(home + "/.config/harlo-agent/mcp.json") || "{}").mcpServers || {}
            const claude = JSON.parse(TestInput.readFile(home + "/.claude.json") || "{}").mcpServers || {}
            win.check(err === "" && hs.boxtools && hs.boxtools.command === "box-mcp", "MCP on box writes box's own Harlo config (" + err + ")")
            win.check(claude.boxtools !== undefined, "and box's Claude (claude mcp there)")
            const l = JSON.parse(Mcp.listJson("host:box"))
            win.check(l.servers.some(x => x.name === "boxtools" && x.harnesses.indexOf("claude") >= 0 && x.harnesses.indexOf("harlo") >= 0), "which the page lists for box")
            win.shotWindow("clicks-remote-mcp")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The host's Claude is older than the newest: offered, and
            // updated there.
            const m = win.findName("updateMessage-box/claude")
            win.check(m !== null && m.text === "Version 2.1.300 is available on box (you have 2.1.296).", "the host's agents are checked for updates (" + (m ? m.text : Updates.entriesJson) + ")")
            win.clickItem(win.findName("updateGo-box/claude"), "Update on box") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            const m = win.findName("updateMessage-box/claude")
            win.check(m !== null && m.text === "Claude on box is now 2.1.300. New sessions use it.", "and updated there (" + (m ? m.text : "none") + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.subagentDock.expanded = true; win.sendByKeys("please orchestrate a helper") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(/Started a helper with harlo \([1-9][0-9]* tools\): live:/.test(t), "the agent finds harlo's tools and starts a session (" + t.slice(-90) + ")")
            const kids = JSON.parse(Workspace.orchestratedJson(win.orchestrator))
            win.check(kids.length === 1 && kids[0].name === "Helper" && kids[0].agent === "claude", "it's the agent's subagent (" + JSON.stringify(kids) + ")")
            win.check(win.findText("Helper", chat.subagentDock) !== null, "and shows in its dock") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = win.chatText(), kids = JSON.parse(Workspace.orchestratedJson(win.orchestrator))
            win.check(kids.length === 1 && kids[0].status === "completed", "the helper finishes (" + (kids[0] ? kids[0].status : "") + ")")
            win.check(t.indexOf("The Claude session") >= 0 && t.indexOf("harlo · ") >= 0 && t.indexOf("Thanks; harlo says") >= 0 && t.indexOf("Hello from fake claude") >= 0, "and its reply comes back to the agent (" + t.slice(-120) + ")")
            const who = win.findName("subagentWho", chat)
            win.check(who !== null && who.text.indexOf("Claude Code") === 0 && who.text.indexOf("claude-") < 0, "the helper is named as elsewhere, not by its ids (" + (who ? who.text : "none") + ")")
            const st = win.findName("subagentStatus", chat)
            win.check(st === null || !st.visible, "finished needs no word: its faint dot says it")
            win.shotWindow("clicks-orchestrate")
            // A finished helper's View shows on its row's hover.
            const row = win.findName("subagentWho", chat.subagentDock)
            win.check(row !== null && win.findName("subagentView", chat.subagentDock) === null, "a finished helper's View waits for a hover")
            if (row) { const p = row.mapToItem(null, row.width / 2, row.height / 2); TestInput.moveTo(p.x, p.y) } } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("subagentView", chat.subagentDock), "View") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(win.orchestrator))
            win.check(Workspace.activeKey === kids[0].id && win.chatText().indexOf("Say hello as the helper") >= 0, "View opens the helper's session (" + Workspace.activeKey + ")")
            // Turned off: a running agent's bridge offers nothing and harlo
            // refuses; new sessions get no bridge, and there's no Delegate.
            Workspace.setOrchestrationEnabled(false)
            Workspace.open(win.orchestrator) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(win.findName("delegateButton") === null, "off: no Delegate button")
            win.sendByKeys("please orchestrate again") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("(0 tools): Orchestration is turned off in harlo") >= 0, "off: a running agent gets no tools, and calls are refused (" + t.slice(-110) + ")")
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("please orchestrate") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("No harlo tools here.") >= 0, "off: a new session has no harlo tools (" + win.chatText().slice(-60) + ")")
            win.summary(); Qt.quit() } }
    }

    // The real Claude Code (not in the click suite; spends a little):
    // one tiny turn on Haiku in --cwd.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "claudereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { chat.session.setModel("anthropic", "claude-haiku-4-5"); win.sendByKeys("Reply with exactly: OK") } }
        PauseAnimation { duration: 20000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "model:", chat.session.modelId, "context:", chat.session.contextText)
            win.log("transcript:", win.chatText())
            Qt.quit() } }
    }


    // A real subagent (not in the click suite): --harness= claude|harlo,
    // --model=provider/id; one tiny delegated task.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "delegatereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.delegateKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            win.sendByKeys("Start one subagent (your subagent/Agent tool, the general agent) with the description 'Say banana' and the task 'Reply with the single word banana.' Then tell me what it said, in one word.") } }
        PauseAnimation { duration: 75000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "model:", chat.session.modelId)
            win.log("subagents:", chat.session.subagentsJson)
            win.log("transcript:", win.chatText().replace(/\n/g, " | ").slice(-700))
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
            win.sendByKeys("Reply with exactly: apple") } }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: win.sendByKeys("Reply with exactly: banana") }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: { win.forkFile = Workspace.sessionFile(Workspace.activeKey); win.hoverEdit("Reply with exactly: banana") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Edit & resend", chat.transcript) }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            win.log("composer:", chat.composerItem.text, "| file:", win.forkFile.split("/").pop(), "->", Workspace.sessionFile(Workspace.activeKey).split("/").pop())
            win.log("after fork:", win.chatText().replace(/\n/g, " | "))
            chat.composerItem.text = ""
            win.sendByKeys("Reply with exactly: cherry") } }
        PauseAnimation { duration: 30000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "| transcript:", win.chatText().replace(/\n/g, " | ").slice(-500))
            Qt.quit() } }
    }

    // A real agent on a remote host (not in the click suite): --ssh= the
    // host, --harness=, --model=provider/id; one tiny turn there.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "remotereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")); Remotes.connectHost("box") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { win.log("host:", JSON.stringify(JSON.parse(Remotes.hostsJson())[0].versions)); Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.newSession(JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 8000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            win.sendByKeys("Reply with exactly: OK") } }
        PauseAnimation { duration: 60000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "| file:", Workspace.sessionFile(Workspace.activeKey), "| transcript:", win.chatText().replace(/\n/g, " | ").slice(-400))
            Qt.quit() } }
    }

    // Hover tips: the shared ToolTip, dressed like the pills.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickstooltip"
        PauseAnimation { duration: 900 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const b = win.findName("modeButton", chat.composerItem), p = b.mapToItem(null, b.width / 2, b.height / 2)
            // (Offscreen, controls don't follow the mouse unless told to.)
            b.hoverEnabled = true
            TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = tipStyle.tip
            win.check(t.visible && t.text.indexOf("Build: the agent makes changes") === 0, "hovering shows the tip (" + t.text + ")")
            win.check(t.leftPadding === 10 && t.contentItem && t.contentItem.font.pixelSize === Theme.px(12) && t.background && t.background.variant === "outline", "dressed like the pills")
            win.check(t.contentItem.parent !== tipStyle && t.contentItem.text === t.text, "its text is in it")
            win.shotWindow("clicks-tooltip")
            // A long one wraps instead of running across the window.
            win.findName("modeButton", chat.composerItem).ToolTip.text = "Use the task tool to run two subagents in parallel: one lists every file in this repo with its size, the other counts the lines of Rust in each crate and reports the totals." } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = tipStyle.tip
            win.check(t.visible && t.width <= Theme.px(420) && t.contentItem.lineCount >= 3 && t.height >= t.contentItem.lineCount * Theme.px(12),
                      "a long tip wraps (" + t.width + "x" + t.height + ", " + t.contentItem.lineCount + " lines)")
            const r = t.contentItem.mapToItem(null, 0, 0)
            win.check(r.x >= 0 && r.x + t.contentItem.width <= win.width, "inside the window")
            win.shotWindow("clicks-tooltip-long")
            win.summary(); Qt.quit() } }
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
            const m = win.findName("updateMessage-local/claude")
            win.check(m !== null && m.text === "Version 2.1.300 is available (you have 2.1.296).", "a newer Claude is offered (" + (m ? m.text : Updates.entriesJson) + ")")
            win.shotWindow("clicks-update-offer")
            win.clickItem(win.findName("updateDismiss-local/claude"), "Dismiss") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(win.findName("update-local/claude") === null, "Dismiss puts it away"); Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("update-local/claude") === null, "and that version isn't offered again")
            Workspace.setSetting("update_dismissed", "{}")
            Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.clickItem(win.findName("updateGo-local/claude"), "Update") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const m = win.findName("updateMessage-local/claude")
            win.check(m !== null && m.text === "Claude is now 2.1.300. New sessions use it.", "Update runs, then says what it is now (" + (m ? m.text : "none") + ")")
            win.shotWindow("clicks-update-done")
            win.clickItem(win.findName("updateOutput-local/claude"), "View output") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(windowSettings.terminalOpen, "View output opens the terminal")
            win.check(win.findName("update-local/claude") !== null, "done: it stays while the pointer is on it")
            TestInput.moveTo(20, 20) } }
        PauseAnimation { duration: 2200 }
        ScriptAction { script: {
            win.check(win.findName("update-local/claude") === null, "then goes by itself")
            Updates.checkLocal() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("update-local/claude") === null, "up to date now: nothing offered")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.clickItem(win.findText("Fix the login redirect loop", sidebar), "the session") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.siteKey = Workspace.activeKey
            const run = win.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", chat.transcript)
            if (run) win.clickItem(run, "the run")
            windowSettings.rightMode = "changes"
            git.refresh(true) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-main"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-main-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.clickText("guard.ts", changes) }
        PauseAnimation { duration: 800 }
        // (Narrow, without the sidebar: the code's lines fit.)
        ScriptAction { script: { windowSettings.rightMode = ""; if (win.width < 1000) windowSettings.sidebarOpen = false } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.shotWindow("site-diff"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.shotWindow("site-diff-light"); Workspace.chooseThemeMode("dark"); windowSettings.sidebarOpen = true } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            Workspace.chooseTheme("Midnight")
            Workspace.open(win.siteKey)
            windowSettings.rightMode = "activity" } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-midnight"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-midnight-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.chooseTheme("harlo"); windowSettings.rightMode = ""; win.openDraft(1) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.shotWindow("site-new"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.shotWindow("site-new-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.clickItem(win.findText("Where is a pallet's weight worked out?", sidebar), "the wiki lookup") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: wikiPanel.openWiki("workflows/manifests-and-weights.md", "weight-calculation-the-current-asymmetry-matters") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-wiki"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-wiki-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // (Narrow, from the question: the phone crop shows what was asked,
        // the wiki lookup and the page.)
        ScriptAction { script: {
            if (win.width >= 1000) return
            const v = chat.transcript.listView
            v.follow = false
            v.positionViewAtBeginning() } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { if (win.width < 1000) { win.shotWindow("site-wiki-top"); Workspace.chooseThemeMode("light") } } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { if (win.width < 1000) { win.shotWindow("site-wiki-top-light"); Workspace.chooseThemeMode("dark") } } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            windowSettings.rightMode = ""
            win.clickItem(win.findText("Carriers want manifests as CSV", sidebar), "the plan") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-plan"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-plan-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // (Narrow, the option cards stand one above another: the plan folded
        // to its header so both show whole, scrolled to the first, the
        // recommended one, for the plan's phone picture.)
        ScriptAction { script: {
            if (win.width >= 1000) return
            const fold = win.findName("planToggle", chat)
            if (fold) win.clickItem(fold, "fold the plan") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            if (win.width >= 1000) return
            const v = chat.transcript.listView
            const card = win.findName("optionCard", chat.transcript)
            v.follow = false
            if (card) v.contentY = Math.max(v.originY, card.mapToItem(v.contentItem, 0, 0).y - 50)
            // (The round Latest arrow would sit on the second card.)
            const latest = win.findName("latestButton", chat)
            if (latest) latest.opacity = 0 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { if (win.width < 1000) { win.shotWindow("site-plan-cards"); Workspace.chooseThemeMode("light") } } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            if (win.width >= 1000) return
            win.shotWindow("site-plan-cards-light"); Workspace.chooseThemeMode("dark")
            const latest = win.findName("latestButton", chat)
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
            win.siteKey = Workspace.newSession(api.id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(siteSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("Cache the user lookups") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const web = JSON.parse(Workspace.projectsJson()).find(p => p.path.endsWith("/webapp"))
            win.siteKey = Workspace.newSession(web.id, "claude") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(siteSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { win.sendByKeys("Review the login fix in src/auth/guard.ts"); windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        // (Named as you would, so its header doesn't say New session.)
        ScriptAction { script: Workspace.renameSession(win.siteKey, "Review the login fix") }
        // (The review finished; the Harlo agent still at work.)
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.waitUntil(siteSteps, () => chat.session.status === "idle", 45000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-hero"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-hero-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
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
        const f = win.findName("settingsPage", settingsDialog.contentItem)
        win.shotWindow("set-" + id + (part > 0 ? "-" + (part + 1) : ""))
        if (f && part < 5 && f.contentY + f.height < f.contentHeight - 8) {
            f.contentY = Math.min(f.contentHeight - f.height, f.contentY + f.height - 80)
            win.setQueue.unshift(() => win.setShotPage(id, part + 1))
        }
    }
    Timer {
        id: setTimer
        interval: 700
        repeat: true
        onTriggered: {
            const step = win.setQueue.shift()
            if (!step) { stop(); win.summary(); Qt.quit(); return }
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
                    const f = win.findName("settingsPage", settingsDialog.contentItem)
                    // (Opened after a page shot scrolled down.)
                    win.check(!f || f.contentY === 0, pg.title + " opens at its top")
                    win.setShotPage(pg.id, 0)
                })
            }
            // Narrow: the search button opens the page list over the page.
            q.push(() => {
                const b = win.findName("settingsNavSearch", settingsDialog.contentItem)
                win.check(settingsDialog.compact === (b !== null), "the nav's search button shows only when narrow")
                if (b) win.clickItem(b, "search settings")
            })
            q.push(() => {
                if (!settingsDialog.compact) return
                const nav = win.findName("settingsNav", settingsDialog.contentItem)
                win.check(settingsDialog.navOpen && nav && nav.width === 236 && win.findText("Workspace", settingsDialog.contentItem) !== null, "it opens with the pages' names")
                win.shotWindow("set-nav-open")
                win.clickItem(win.findName("settingsNav-git", settingsDialog.contentItem), "Git")
            })
            q.push(() => {
                if (!settingsDialog.compact) return
                win.check(!settingsDialog.navOpen && settingsDialog.page === "git", "picking a page closes it")
            })
            // States that say why: Harlo isn't signed in here.
            q.push(() => settingsDialog.openPage("providers"))
            q.push(() => {
                const st = win.findName("providerStatus-harlo", settingsDialog.contentItem)
                win.check(st && st.text !== "Error" && st.text !== "", "Providers says why Harlo isn't ready (" + (st ? st.text : "") + ")")
                win.check(win.findName("rowSignIn-harlo", settingsDialog.contentItem) !== null, "and offers Sign in on its row")
                // More shows the rest of a row's words, Less hides it again.
                const m = win.findMoreText(settingsDialog.contentItem)
                win.check(m !== null, "a row has More")
                if (m) {
                    const before = m.plain.length
                    m.linkActivated("more")
                    win.check(m.open && m.plain.length > before && m.text.indexOf(">Less<") > 0, "More shows the rest")
                    m.linkActivated("more")
                    win.check(!m.open && m.plain.length === before, "Less hides it")
                }
                settingsDialog.openPage("general")
            })
            q.push(() => win.check(win.findName("defaultModelSignIn", settingsDialog.contentItem) !== null, "General's Model row says where to sign in"))
            // Devices with phone access off: said once per section, not per row.
            q.push(() => settingsDialog.openPage("devices"))
            q.push(() => {
                const notes = win.findAllNames("needsPhone", settingsDialog.contentItem)
                win.check(Phone.enabled || (notes.length === 2 && win.findText("Turn on phone access first.", settingsDialog.contentItem) === null), "Devices says once per section that phone access is off (" + notes.length + ")")
                settingsDialog.openPage("voice")
            })
            // A search lands on the row it matched, lit for a moment.
            q.push(() => {
                settingsDialog.openPage("general")
                if (settingsDialog.compact) settingsDialog.navOpen = true
                const f = win.findName("settingsSearch", settingsDialog.contentItem)
                win.setSearch = f
                f.text = "sandbox"
                f.accepted()
            })
            q.push(() => {
                const pg = win.findName("settingsPage", settingsDialog.contentItem)
                const flash = win.findName("settingsSeekFlash", settingsDialog.contentItem)
                win.check(settingsDialog.page === "providers" && pg && pg.contentY > 0 && flash && flash.opacity > 0, "searching sandbox lands on its row on Providers (" + settingsDialog.page + ", " + (pg ? pg.contentY : -1) + ")")
                win.shotWindow("set-seek")
                win.setSearch.text = ""
                settingsDialog.navOpen = false
                settingsDialog.openPage("appearance")
            })
            // Glass that can't be drawn: off, with a way to the renderer.
            q.push(() => {
                const b = win.findName("glassRenderer", settingsDialog.contentItem)
                const sw = win.findName("glassSwitch", settingsDialog.contentItem)
                win.check(b === null || (!sw.enabled && !sw.checked), "Glass is off and disabled when it can't be drawn")
                win.glassLink = b !== null
                if (b) win.clickItem(b, "Renderer…")
            })
            q.push(() => {
                const pg = win.findName("settingsPage", settingsDialog.contentItem)
                if (win.glassLink) win.check(settingsDialog.page === "general" && pg.contentY > 0, "Renderer… opens General at its renderer row")
                settingsDialog.openPage("voice")
            })
            // Voice: a chosen model that isn't here isn't "in use".
            q.push(() => win.check(win.findText("Base model (in use)", settingsDialog.contentItem) === null && win.findText("Base model", settingsDialog.contentItem) !== null, "Voice doesn't call a missing model in use"))
            for (const id of ["general", "appearance", "providers"]) {
                q.push(() => { Workspace.chooseThemeMode("light"); settingsDialog.openPage(id) })
                q.push(() => { win.shotWindow("set-" + id + "-light"); Workspace.chooseThemeMode("dark") })
            }
            win.setQueue = q
            setTimer.start() } }
    }

    // Screenshots for a design critique (not a test: SCENARIOS=critiqueshots
    // KEEP=1 scripts/click-tests.sh 1280 524): a Claude session of prose
    // with headings and several prompts, its prompt list, a dozen helpers,
    // each right-hand panel, the sidebar beside it, and the new-session page.
    function critReplies() { const t = win.chatText(); return t.split("Hello from fake claude").length + t.split("What is left to do").length - 2 }
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
    function critShot(name) { win.shotWindow("crit-" + name) }
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
            ScriptAction { script: { win.sendByKeys(win.critPrompts[win.critSent]); win.critSent++ } }
            ScriptAction { script: win.waitUntil(critSteps, () => win.critReplies() >= win.critSent && chat.session.status === "idle", 30000) }
            PauseAnimation { duration: 400 }
        }
        // Prose: the first write-up from its title, then its end.
        ScriptAction { script: { chat.transcript.follow = false; win.critTop(win.findContaining("The transcript, from events to rows", chat.transcript)) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.critShot("prose-top") }
        ScriptAction { script: { const t = win.findContaining("The transcript, from events to rows", chat.transcript); win.critTopAt(t && t.getText ? t : t && t.parent, "Reading it") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.critShot("prose-end"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.critTop(win.findContaining("The transcript, from events to rows", chat.transcript)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.critShot("prose-top-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        // The prompt list (Ctrl+J), and the rail beside the session.
        ScriptAction { script: chat.transcript.rail.openKeyboard() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.critShot("rail-list"); chat.transcript.rail.close() } }
        PauseAnimation { duration: 400 }
        // The helpers, opened.
        ScriptAction { script: { chat.transcript.scrollToEnd(); const d = win.findName("subagentDock"); if (d) d.expanded = true } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.critShot("helpers")
            // A helper's task that only repeats its name isn't shown again.
            const d = win.findName("subagentDock")
            win.check(d !== null && (win.findText("Explore", d) !== null) === (win.width > 900), "helpers show their kind while their names fit beside it")
            const count = (t, r) => { let n = 0; for (const c of r.children) { if (!c.visible || c.opacity <= 0) continue; if (c.text === t) n++; n += count(t, c) } return n }
            const cut = (r) => { let n = []; for (const c of r.children) { if (!c.visible || c.opacity <= 0) continue; if (c.font && c.font.bold && c.truncated) n.push(c.text); n = n.concat(cut(c)) } return n }
            if (win.width > 900) win.check(d !== null && cut(d).length === 0, "with room, no helper's name is cut (" + (d ? cut(d).join(", ") : "") + ")")
            win.check(d !== null && count("Analyze the transcript layout", d) === 1, "a helper's name shows once, not again as its task (" + (d ? count("Analyze the transcript layout", d) : -1) + ")")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.critShot("helpers-light"); Workspace.chooseThemeMode("dark"); const d = win.findName("subagentDock"); if (d) d.expanded = false } }
        PauseAnimation { duration: 600 }
        // Each right-hand panel.
        ScriptAction { script: windowSettings.rightMode = "context" }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.critShot("right-context"); windowSettings.rightMode = "activity" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.critShot("right-activity")
            // Only filters that narrow the list; one tool throughout needs no name under each row.
            win.check(win.findName("activityFilter-all") !== null && win.findName("activityFilter-command") === null && win.findName("activityFilter-read") === null && win.findName("activityFilter-other") === null, "Activity offers no empty filter, nor one equal to all")
            win.check(win.findText("Agent", win.findName("activityList")) === null, "and its rows don't each repeat their tool")
            windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.critShot("right-changes"); windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.critShot("right-files"); windowSettings.rightMode = "notes" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.critShot("right-notes"); windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.critShot("right-wiki"); windowSettings.rightMode = "context"; Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.critShot("right-context-light"); Workspace.chooseThemeMode("dark"); windowSettings.rightMode = "" } }
        PauseAnimation { duration: 800 }
        // The new-session page.
        ScriptAction { script: win.openDraft(JSON.parse(Workspace.projectsJson()).find(p => p.path === win.argValue("--cwd=")).id) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.critShot("new"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.critShot("new-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.clickItem(win.findName("checkUpdatesNow", settingsDialog.contentItem), "Check now") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(!Updates.checking && Updates.checkStatus === "1 update found: it's offered at the bottom right of the window.", "Check now says what it found (" + Updates.checkStatus + ")")
            // (Scrolled to the row's end, its answer in the picture.)
            let f = win.findName("checkUpdatesNow", settingsDialog.contentItem)
            while (f && f.contentY === undefined) f = f.parent
            if (f) f.contentY = Math.max(0, f.contentHeight - f.height)
            win.shotPopup(settingsDialog, "clicks-selfupdate-check")
            settingsDialog.close() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const m = win.findName("updateMessage-local/harlo")
            win.check(m !== null && /^Version 9\.9\.9 is available \(you have \d+\.\d+\.\d+\)\.$/.test(m.text), "a newer harlo is offered (" + (m ? m.text : Updates.entriesJson) + ")")
            // What's new since this version: both releases, cut short, the rest a click away.
            const notes = win.findName("updateNotes-local/harlo")
            win.check(notes !== null && notes.visible && notes.text.startsWith("harlo 9.9.9\n• The stand-in release: denying a permission ends the run, and archiving an open session hides it.\n")
                      && notes.text.indexOf("harlo 9.9.8") > 0 && notes.text.endsWith("…") && notes.text.indexOf("Long ago") < 0, "it says what's new, in short (" + (notes ? JSON.stringify(notes.text) : "none") + ")")
            const more = win.findName("updateReadMore-local/harlo")
            win.check(more !== null && more.visible && JSON.parse(Updates.entriesJson).find(e => e.key === "local/harlo").notesUrl === "https://github.com/ArcticLatent/harlo/blob/v9.9.9/CHANGELOG.md", "Read more opens the full changelog")
            win.shotWindow("clicks-selfupdate-notes")
            win.clickItem(win.findName("updateGo-local/harlo"), "Update") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const m = win.findName("updateMessage-local/harlo")
            win.check(m !== null && m.text === "harlo 9.9.9 is installed. Restart harlo to use it.", "downloaded, checked and put in place (" + (m ? m.text : "none") + ")")
            win.check(TestInput.readFile(win.selftestDir + "/harlo-test.AppImage") === "harlo 9.9.9 new\n", "the AppImage is the new one")
            win.check(win.findName("updateRestart-local/harlo") !== null, "with Restart harlo")
            win.shotWindow("clicks-selfupdate") } }
        PauseAnimation { duration: 11000 }
        ScriptAction { script: {
            win.check(win.findName("update-local/harlo") !== null, "and it stays (it waits for the restart)")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView, rows = dv.rows
            win.check(Workspace.activeKey === "diff:0:" + win.argValue("--cwd=") + "/src/app.rs" && dv.visible, "a changed file opens in the diff tab (" + Workspace.activeKey + ")")
            win.check(dv.blocks.length === 4 && rows.length === 43, "four changes, the sides level (" + dv.blocks.length + " changes, " + rows.length + " rows)")
            const edited = rows.find(r => r[1] === 3), gone = rows.find(r => r[1] === 10), added = rows.filter(r => r[0] === 2 && r[5] === 1)
            win.check(edited && edited[0] === 1 && edited[5] === 1 && edited[3] === -1 && edited[8] === 6 && edited[9] === 14, "an edited line pairs up, the added part marked (" + edited + ")")
            win.check(gone && gone[0] === 1 && gone[5] === 2 && added.length === 3, "a removed line faces filler, added lines too (" + added.length + ")")
            win.check(dv.leftPane.editor.text.indexOf("line 10") >= 0 && dv.rightPane.editor.text.indexOf("line 10\n") < 0 && dv.rightPane.editor.text.indexOf("new B") >= 0, "old on the left, new on the right")
            win.check(win.findName("diffRuler") !== null && win.findName("hunk-stage-0") !== null, "the change map, and a hunk's buttons")
            win.check(dv.leftPane.width * 2 + 44 <= dv.width + 1, "the sides fit the view (" + dv.leftPane.width + " of " + dv.width + ")")
            {
                // Both sides on whole pixels (at half a pixel the text blurs).
                const lx = dv.leftPane.editor.mapToItem(null, 0, 0).x, rx = dv.rightPane.editor.mapToItem(null, 0, 0).x
                win.check(lx === Math.round(lx) && rx === Math.round(rx) && !dv.rightPane.editor.smooth, "both sides on whole pixels, drawn unfiltered (" + lx + ", " + rx + ")")
            }
            const f = dv.rightPane.flick, third = Math.max(0, Math.min(f.contentHeight - f.height, dv.blocks[0].start * dv.lineHeight - f.height / 3))
            win.check(f.height > 0 && Math.abs(f.contentY - third) < 1, "it opens at the first change, a third of the way down (" + f.contentY + ", not " + third + ")")
            win.shotWindow("clicks-diff")
            win.clickItem(win.findName("diffNext"), "next change") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(chat.diffView.rightPane.flick.contentY > 0 || chat.diffView.rightPane.flick.contentHeight <= chat.diffView.rightPane.flick.height, "next change scrolls (" + chat.diffView.rightPane.flick.contentY + ")")
            const dv = chat.diffView
            win.check(Math.abs(dv.rightPane.rowsAt() - dv.rightPane.flick.contentY) < 1, "the rows scroll with the text (" + dv.rightPane.rowsAt() + ", " + dv.rightPane.flick.contentY + ")")
            win.shotWindow("clicks-diff-next")
            win.diffCollapseClick() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(dv.rows.length === 27 && dv.folds.length === 2 && dv.folds[0].count === 14 && dv.folds[1].count === 4, "collapsed: the changes, 3 lines around each, the rest folded (" + dv.rows.length + " rows, " + dv.folds.map(f => f.count) + ")")
            win.check(dv.rightPane.editor.text.split("\n").length === 27 && dv.hunks.every(h => h[1] >= 0 && h[1] < 27), "the text and hunk buttons follow")
            win.check(Workspace.setting("diff_collapse") === "1", "and it's remembered")
            win.shotWindow("clicks-diff-collapsed")
            win.clickItem(win.findName("diffFold-r-13"), "a fold") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(dv.rows.length === 40 && dv.folds.length === 1, "a click opens a fold (" + dv.rows.length + " rows)")
            win.diffCollapseClick() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(chat.diffView.rows.length === 43 && Workspace.setting("diff_collapse") === "0", "off: every line again")
            win.clickText("README.md", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const diffTabs = Workspace.tabs.filter(t => t.startsWith("diff:"))
            win.check(diffTabs.length === 1 && Workspace.activeKey.endsWith("/README.md"), "another file replaces it in the same tab (" + diffTabs.length + " diff tabs)")
            win.check(chat.diffView.rightPane.editor.text === "hello world" && chat.diffView.leftPane.editor.text === "hello", "and shows that file")
            win.clickText("todo.txt", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const rows = chat.diffView.rows
            win.check(rows.length === 2 && rows.every(r => r[0] === 2 && r[5] === 1), "a new file: all added, nothing on the left")
            win.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.clickItem(win.findName("hunk-stage-0"), "stage the first hunk") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(git.stagedCount === 1, "the hunk is staged (" + git.stagedCount + " staged)")
            // The Staged list comes first: its app.rs.
            win.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const rows = chat.diffView.rows
            win.check(Workspace.activeKey.startsWith("diff:1:") && rows.some(r => r[5] === 1) && win.findName("hunk-unstage-0") !== null, "the staged side, with Unstage on its hunk (" + Workspace.activeKey + ")")
            // An agent changes the file: the view follows.
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello again\n")
            win.clickText("README.md", changes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello once more\n") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            win.check(chat.diffView.rightPane.editor.text === "hello once more", "it follows the file as it changes (" + chat.diffView.rightPane.editor.text + ")")
            win.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1500 }
        // Inline: from the More menu, one column.
        ScriptAction { script: win.clickItem(win.findName("diffMore"), "More") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("diffLayoutUnified"), "Always inline") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const dv = chat.diffView, u = dv.uni ? dv.uni.rows : []
            const pairs = dv.rows.filter(r => r[0] === 1 && r[5] === 1).length
            win.check(dv.unified && !dv.leftPane.visible && u.length === dv.rows.length + pairs && Workspace.setting("diff_layout") === "unified", "inline: one column, a line for each side of a change (" + u.length + " rows)")
            const gone = u.findIndex(r => r[14] === 1), added = u.findIndex(r => r[5] === 1)
            // (The staged side of app.rs: line 3 as it was, then as it is.)
            const lines = dv.rightPane.editor.text.split("\n")
            win.reviewKey = lines[gone] || ""
            win.check(gone >= 0 && added === gone + 1 && lines[gone] === "line 3" && lines[added] === "line 3 changed", "the removed line, then the added one, in the one text (" + lines[gone] + " / " + lines[added] + ")")
            win.check((win.findName("hunk-stage-0") !== null || win.findName("hunk-unstage-0") !== null) && dv.blocks.length > 0 && dv.hunks.every(h => h[1] >= 0 && h[1] < u.length), "hunk buttons and the change map follow")
            win.shotWindow("clicks-diff-inline")
            win.clickAt(win.findName("gutter-u-" + gone, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = chat.diffView.commentBox
            win.check(box.visible && box.side === "old" && box.code === win.reviewKey && box.line === 3, "a comment on a removed line, inline (" + box.side + " " + box.code + " " + box.line + ")")
            box.close()
            chat.diffView.setLayout("auto")
            Workspace.openDiffTab(win.argValue("--cwd=") + "/gone/deep/old.svg", false) } }
        ScriptAction { script: win.waitUntil(diffSteps, () => chat.diffView.info !== null && chat.diffView.path.endsWith("old.svg"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            // Its folder is gone too: the diff comes from the repository still.
            const dv = chat.diffView
            const old = (dv.unified ? dv.rightPane : dv.leftPane).editor.text
            win.check(dv.info !== null && dv.info.note === "" && dv.rows.length === 2 && old.indexOf("second") >= 0,
                      "a file deleted with its folders still shows its diff (" + (dv.info ? dv.rows.length + " rows, " + JSON.stringify(old) : "loading") + ")")
            win.summary(); Qt.quit() } }
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
            win.clickItem(win.findName("delegateButton"), "the Delegate button") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.clickItem(win.findName("delegateAdvisor"), "Second opinion…") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const agent = win.findName("delegateAgentAsk"), note = win.findName("delegateNote")
            win.check(agent && agent.currentText === "Harlo", "Second opinion: another agent by default (" + (agent ? agent.currentText : "") + ")")
            win.check(note && note.text === "is the cache right?", "the composer's text is the question")
            win.shotWindow("clicks-delegate-advisor")
            win.clickItem(win.findName("delegateStart"), "Ask") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("Get a second opinion from Harlo (agent \"harlo\", its default model)") >= 0 && t.indexOf("The question: is the cache right?") >= 0, "the agent gets harlo's instruction (" + t.slice(0, 120) + ")")
            win.check(chat.composerItem.text === "", "and the composer is empty")
            win.clickItem(win.findName("delegateButton"), "the Delegate button") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.clickItem(win.findName("delegateCommittee"), "Committee…") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const a = win.findName("delegateAgentMember1"), b = win.findName("delegateAgentMember2"), m = win.findName("delegateModelMember1")
            win.check(a && b && a.currentText === "Claude" && b.currentText === "Harlo", "Committee: this agent and another (" + (a ? a.currentText : "") + ", " + (b ? b.currentText : "") + ")")
            win.check(m && m.count > 2, "with Claude's models to pick from (" + (m ? m.count : 0) + ")")
            m.currentIndex = 1
            m.activated(1)
            win.findName("delegateNote").text = "flaky tests"
            win.clickItem(win.findName("delegateStart"), "Start committee") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(/- Claude \(agent "claude", model "[^"]+"\)/.test(t) && t.indexOf("- Harlo (agent \"harlo\"") >= 0 && t.indexOf("The problem: flaky tests") >= 0, "both members, with the chosen model, go to the agent (" + t.slice(-300) + ")")
            win.summary(); Qt.quit() } }
    }

    // The Delegate menu with real agents (not in the click suite): Claude
    // (Haiku) asks Harlo (its default model) for a second opinion.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "delegatemenureal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            chat.session.setModel("anthropic", "claude-haiku-4-5")
            chat.composerItem.text = "In Python, is `x += 1` the same as `x = x + 1` for ints? One short sentence."
            chat.composerItem.delegatePopover.show("advisor") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: {
            const p = chat.composerItem.delegatePopover
            const row = win.findName("delegateAgentAsk").parent
            row.pick("harlo")
            win.log("sending:", JSON.stringify(row.chosen()))
            p.start() } }
        PauseAnimation { duration: 100000 }
        ScriptAction { script: {
            win.log("children:", Workspace.orchestratedJson(win.orchestrator))
            win.log("asked:", chat.session.hasRequest, "| status:", chat.session.status, "| transcript:", win.chatText().replace(/\n/g, " | ").slice(-1500))
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
            win.log("graphics: api", graphicsInfo.api, "(OpenGL " + GraphicsInfo.OpenGL + ", Vulkan " + GraphicsInfo.Vulkan + ", Software " + GraphicsInfo.Software + ") drawn", win.drawnOnce,
                    "| saved", Workspace.setting("graphics"), "| notice", Workspace.notice)
            win.shot("graphics-probe") } }
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
            win.check(win.findName("graphics-auto", d) && win.findName("graphics-auto", d).selected, "Automatic is chosen at first")
            const row = win.findName("graphicsRow", d)
            win.check(row && row.detail.endsWith("Now: Software."), "it says what draws the window now (" + (row ? row.detail.slice(-30) : "") + ")")
            win.check(win.findName("graphicsRestartRow", d) === null, "no restart asked")
            win.clickItem(win.findName("graphics-vulkan", d), "Vulkan") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(Workspace.setting("graphics") === "vulkan" && win.findName("graphics-vulkan", d).selected, "Vulkan is saved for the next start")
            win.check(win.findName("graphicsRestartRow", d) !== null && win.findText("Restart harlo", d) !== null, "and a restart is offered")
            win.shotWindow("clicks-graphics")
            win.clickItem(win.findName("graphics-auto", d), "Automatic") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Workspace.setting("graphics") === "auto" && win.findName("graphicsRestartRow", settingsDialog.contentItem) === null, "back to the one it started with: no restart")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("show the tools please") }
        ScriptAction { script: win.waitUntil(toolCardSteps, () => win.chatText().indexOf("Limits now come from") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.sendByKeys("show the tools again") }
        ScriptAction { script: win.waitUntil(toolCardSteps, () => win.chatText().split("Limits now come from").length === 3 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const t = chat.transcript
            const boxes = win.findAllNames("runBox", t)
            win.check(boxes.length >= 1, "a finished run shows open, in its box (" + boxes.length + ")")
            const box = boxes[boxes.length - 1]
            const what = win.findAllNames("toolWhat", box).map(l => l.plain)
            win.check(what.indexOf("Running the full test suite") >= 0, "a command shows what it's for (" + what.join(" | ") + ")")
            win.check(what.indexOf("src/config.rs") >= 0 && what.indexOf("README.md") >= 0, "paths from the session's folder")
            const previews = win.findAllNames("toolPreview", box)
            win.check(previews.length === 2, "the write and the edit show their lines (" + previews.length + ")")
            const more = win.findAllNames("toolMoreLines", box)
            win.check(more.length === 1 && more[0].text === "↓ 5 more lines", "the write's first 6 of 11 lines, and 5 more (" + more.map(b => b.text) + ")")
            const edit = win.findAllNames("previewCode", box).find(v => v.signs)
            const en = edit ? edit.lines.map(l => l.n) : []
            win.check(edit && edit.numbers && JSON.stringify(en) === JSON.stringify(["3", "", "", "4", "5", "6"]) && win.findAllNames("codeNumber", edit).length === 4,
                      "an edit numbered where its new lines are in the file, its removed lines not (" + JSON.stringify(en) + ")")
            const results = win.findAllNames("toolResult", box).map(l => l.text)
            win.check(results.some(r => r.indexOf("11 lines") >= 0) && results.some(r => r.indexOf("+2") >= 0 && r.indexOf("−2") >= 0)
                      && results.some(r => r.indexOf("7 passed") >= 0) && results.some(r => r.indexOf("exit 101") >= 0), "what each came to (" + results.join(" | ") + ")")
            const tails = win.findAllNames("toolTail", box).map(l => l.text)
            win.check(tails.some(x => x.indexOf("7 passed") >= 0), "a command's last lines (" + tails.join(" | ") + ")")
            const code = win.findAllNames("codeView", box).reduce((all, v) => all.concat(v.lines), [])
            win.check(code.length === 7 && code[0].n === "1" && code[0].h === "hello", "a read's line, numbered (" + JSON.stringify(code[0]) + ")")
            win.check(code[1].n === "3" && code[1].mark === "match" && code[3].mark === "context" && code[3].h.indexOf("}") >= 0,
                      "grep's line numbers as a gutter, its matches tinted among their context (" + JSON.stringify(code.slice(1, 4)) + ")")
            win.check(code[4].n === "2" && code[6].n === "4" && code[4].h.indexOf("<span") >= 0 && /^    let/.test(code[4].h.replace(/<[^>]+>/g, "")),
                      "a printed file's lines numbered from where it starts, highlighted, indented (" + JSON.stringify(code.slice(4)) + ")")
            const sel = win.findAllNames("codeText", box)
            win.check(sel.length >= 2 && sel.every(e => e.selectByMouse && e.readOnly), "a listing's text can be selected")
            win.check(win.findAllNames("toolTail", box).every(l => !/^\d+[:-]|^    let/.test(l.text)), "a listing isn't shown again as plain lines")
            win.check(win.findAllNames("toolCommandRule", box).length >= 1, "a rule between a command and its listing")
            const cmds = win.findAllNames("toolCommand", box)
            const sedIn = cmds.some(c => c.text.indexOf("sed -n") >= 0)
            win.check(win.width < 700 ? cmds.length >= 3 && sedIn : cmds.length === 2 && !sedIn,
                      "the commands under what they're for; a short one on its line, in the well too when its line cuts it short (" + cmds.length + ")")
            const grepCmd = cmds.find(c => c.text.indexOf("grep -n") >= 0)
            win.check(grepCmd && grepCmd.wrapMode === Text.WrapAtWordBoundaryOrAnywhere, "a command in the well breaks at a space")
            const sedHead = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("sed -n") >= 0)
            const sedWrap = sedHead ? win.findName("codeWrap", sedHead.parent) : null
            const sedCard = sedHead ? sedHead.parent.parent.parent : null
            const sedCode = sedCard ? win.findName("codeView", sedCard) : null
            win.check(sedCode && (sedWrap !== null) === sedCode.cut && (win.width > 700 || sedCode.cut), "a call whose lines are cut offers Wrap on its line, only then (" + (sedWrap ? sedWrap.text : "none") + ")")
            win.check(win.chatText().indexOf("Running the full test suite\n") < 0 && win.findContaining("▸ cargo", t) === null, "no notice for a command in the foreground")
            // The wheel: the box first, then the session past its top.
            const f = win.findName("runBoxScroll", box)
            win.toolBoxStart = [f.contentY, parseFloat(t.debugContentY())]
            win.check(f.contentY > 0 && f.height < f.contentHeight, "taller than its box, it starts at its end (" + f.contentY + " of " + f.contentHeight + ")")
            const rail = t.rail
            win.check(!rail.visible || rail.mapToItem(t, 0, 0).x >= t.width - t.gutter, "the prompt rail sits in the gutter, never on the text (" + Math.round(rail.mapToItem(t, 0, 0).x) + " vs " + Math.round(t.width - t.gutter) + ")")
            win.shotWindow("clicks-tool-cards")
            const p = f.mapToItem(null, f.width / 2, f.height / 2)
            TestInput.wheel(p.x, p.y, 120) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = win.findAllNames("runBox", t), f = win.findName("runBoxScroll", boxes[boxes.length - 1])
            win.check(f.contentY < win.toolBoxStart[0] && Math.abs(parseFloat(t.debugContentY()) - win.toolBoxStart[1]) < 1, "the wheel scrolls the box, not the session (" + f.contentY + ", " + t.debugContentY() + ")")
        } }
        // The wheel scrolls smoothly: a notch at a time, as a hand turns it.
        SequentialAnimation {
            loops: 12
            ScriptAction { script: win.toolWheel(1) }
            PauseAnimation { duration: 250 }
        }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = win.findAllNames("runBox", t), f = win.findName("runBoxScroll", boxes[boxes.length - 1])
            win.check(f.contentY === 0 && parseFloat(t.debugContentY()) < win.toolBoxStart[1], "at the box's top, the session scrolls on (" + f.contentY + ", " + t.debugContentY() + ")")
            win.scrollIntoView(f)
            const more = win.findAllNames("toolMoreLines", boxes[boxes.length - 1])
            if (more.length) win.clickItem(more[0], "5 more lines") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript, boxes = win.findAllNames("runBox", t), box = boxes[boxes.length - 1]
            win.check(win.findAllNames("toolMoreLines", box).some(b => b.text === "↑ Fewer lines"), "opened: all of it")
            win.shotWindow("clicks-tool-cards-open")
            win.toolBoxCount = boxes.length
            win.clickItem(win.findName("runHeader", box), "the run's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const t = chat.transcript
            win.check(win.findAllNames("runBox", t).length === win.toolBoxCount - 1, "its line folds the run")
            win.check(Theme.contrastOf(Theme.textFaint, Theme.background) >= 4.49 && Theme.contrastOf(Theme.textMuted, Theme.background) > Theme.contrastOf(Theme.textFaint, Theme.background), "faint text is readable (4.5:1) and below muted, dark")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.contrastOf(Theme.textFaint, Theme.background) >= 4.49 && Theme.contrastOf(Theme.textFaint, Theme.sidebar) >= 4.49, "faint text is readable (4.5:1), light")
            win.shotWindow("clicks-tool-cards-light")
            Workspace.chooseThemeMode("dark")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.claudeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("show the listings please") }
        ScriptAction { script: win.waitUntil(listingSteps, () => win.chatText().indexOf("Here are the listings") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const box = win.listingBox()
            const code = win.findAllNames("codeView", box).reduce((all, v) => all.concat(v.lines), [])
            win.check(code.length === 33, "cat -n, the read, grep, sed, head and cat show 6, 6, 6, 6, 6 and 3 lines (" + code.length + ")")
            win.check(code[0].n === "1" && code[0].h.indexOf("<span") >= 0 && code[5].n === "6", "cat -n: its numbers, TypeScript highlighted (" + JSON.stringify(code[0]) + ")")
            win.check(code[6].n === "1" && code[9].h.indexOf("<span") >= 0, "a read: its first lines at once, QML highlighted (" + JSON.stringify(code[9]) + ")")
            win.check(code[12].mark === "file" && code[12].h.indexOf("src/main.rs") >= 0 && code[13].n === "3" && code[15].mark === "gap" && code[16].mark === "file" && code[16].h.indexOf("src/app.ts") >= 0 && code[17].h.indexOf("<span") >= 0,
                      "grep in several files: each file's lines under its name, in its language (" + JSON.stringify(code.slice(12, 18).map(l => [l.n, l.mark])) + ")")
            win.check(code[18].n === "38" && code[22].h.indexOf("file_print") >= 0, "sed: numbered from 38")
            win.check(code[24].n === "1" && code[24].h.indexOf("<span") >= 0 && code[30].n === "1" && code[30].h.indexOf("<span") < 0, "TOML highlighted; an unknown file numbered, plain (" + JSON.stringify([code[24], code[30]]) + ")")
            win.check(win.findAllNames("toolTail", box).length === 0, "no listing as plain last lines")
            win.check(win.width < 1000 || win.findAllNames("codeMoreRight", box).length >= 1, "a line longer than the well: its edge fades, more to the side")
            const blocks = win.findAllNames("replyCodeView", chat.transcript)
            win.check(blocks.length === 3 && blocks[0].count === 4 && blocks[0].numbers && blocks[1].count === 1, "a reply's code blocks: numbered (" + blocks.map(b => b.count) + ")")
            win.check(["TypeScript", "Shell", "TOML"].every(n => win.findText(n, chat.transcript) !== null), "code blocks name their language")
            win.listingAt("cat -n")
            win.shotWindow("listings-catn-read") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { win.listingAt("Find every use"); win.shotWindow("listings-grep-sed") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { win.listingAt("head -16"); win.shotWindow("listings-head-cat") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Code and text start on whole pixels (at a fraction, text jumps a pixel as things around it repaint).
            const xs = win.findAllNames("codeText", chat.transcript).map(e => e.mapToItem(null, 0, 0).x)
            win.check(xs.length > 0 && xs.every(x => Math.abs(x - Math.round(x)) < 0.01), "code starts on whole pixels (" + xs.join(", ") + ")")
            const a = win.findContaining("Run it with", chat.transcript)
            win.check(a !== null, "the reply's code blocks")
            if (a) win.scrollIntoView(a)
            win.shotWindow("listings-reply-code")
            // Lines cut at the right: that box offers Wrap (only when cut).
            const ts = win.findAllNames("replyCode", chat.transcript).find(b => win.findText("TypeScript", b) !== null)
            const wrapBtn = ts ? win.findName("codeWrap", ts) : null
            const tsCode = ts ? win.findName("replyCodeView", ts) : null
            win.check(tsCode && (wrapBtn !== null) === tsCode.cut && (win.width > 1000 || tsCode.cut) && (!wrapBtn || wrapBtn.text === "Wrap"), "a code box whose lines are cut offers Wrap, one that fits doesn't (" + (wrapBtn ? wrapBtn.text : "none") + ")")
            win.wrapBox = wrapBtn ? ts : null
            if (wrapBtn) win.clickItem(wrapBtn, "Wrap") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            if (win.wrapBox) {
                const ts = win.wrapBox, code = ts ? win.findName("replyCodeView", ts) : null, btn = ts ? win.findName("codeWrap", ts) : null
                win.check(code && code.wrap && !code.cut && btn && btn.text === "No wrap" && !Theme.wrapCode, "Wrap wraps that box only, and says No wrap (" + (btn ? btn.text : "none") + ")")
                win.shotWindow("listings-reply-wrapped")
                if (btn) win.clickItem(btn, "No wrap")
            }
            win.listingAt("qml/Badge.qml", 60) } }
        // (A press while the list still moves only stops it: a moment first.)
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // (Its line, not its path: the path opens the file.)
            win.clickItem(win.findAllNames("toolResult", win.listingBox()).find(l => l.text === "13 lines"), "the read's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const box = win.listingBox()
            const code = win.findAllNames("codeView", box).reduce((all, v) => all.concat(v.lines), [])
            const qml = code.filter(l => /Item|QtQuick/.test(l.h))
            win.check(code.length === 40 && qml.length >= 2 && qml[0].h.indexOf("<span") >= 0, "the read, opened: its 13 lines numbered, QML highlighted (" + code.length + ")")
            win.listingAt("qml/Badge.qml")
            win.shotWindow("listings-read-open")
            Workspace.chooseLook("wrap", "true") } }
        // Word wrap on: a long line takes two rows, the next line's number
        // where that line starts.
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const sed = win.findAllNames("codeView", win.listingBox()).find(v => v.lines.length && v.lines[0].n === "38")
            const one = sed.lineHeight(0), long = sed.lineHeight(4)
            win.check(Theme.wrapCode && long > one * 1.5 && Math.abs(sed.lineTop(5) - (sed.lineTop(4) + long)) < 1, "wrapped: the long line takes " + Math.round(long / one) + " rows, 43 below it (" + [one, long, sed.lineTop(4), sed.lineTop(5)] + ")")
            // Its second row hangs under its indentation and 2 more.
            const ed = win.findName("codeText", sed), s4 = sed.starts[4], y0 = ed.positionToRectangle(s4).y
            let at = s4
            while (at < ed.length && ed.positionToRectangle(at).y <= y0 + 1) at++
            const x2 = ed.positionToRectangle(at).x, want = ed.positionToRectangle(s4 + 6).x
            win.check(x2 > 0 && Math.abs(x2 - want) < 2, "…its next row under its indentation and 2 (" + [x2, want] + ")")
            const blocks = win.findAllNames("replyCodeView", chat.transcript)
            win.check(blocks.length === 3 && blocks[0].count === 4 && blocks[2].count === 2, "a reply's code blocks, wrapped: still numbered (" + blocks.map(b => b.count) + ")")
            win.listingAt("Look at how")
            win.shotWindow("listings-wrap")
            Workspace.chooseLook("wrap", "null")
            Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.listingAt("cat -n"); win.shotWindow("listings-catn-read-light") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: { win.listingAt("Find every use"); win.shotWindow("listings-grep-sed-light") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const a = win.findContaining("Run it with", chat.transcript)
            if (a) win.scrollIntoView(a)
            win.shotWindow("listings-reply-code-light")
            Workspace.chooseThemeMode("dark")
            // Copy: on hover only, a reply's as a tool's; a tool's in its
            // call's line, never over its code.
            TestInput.moveTo(4, 4)
            const block = win.findAllNames("replyCode", chat.transcript)[0]
            win.check(block && win.findAllNames("copyCode", block).length === 0, "a reply's Copy: hidden until hovered")
            if (block) {
                const p = block.mapToItem(null, block.width / 2, block.height / 2)
                TestInput.moveTo(p.x, p.y)
                win.check(win.findAllNames("copyCode", block).length === 1, "…and shown on hover")
            }
            TestInput.moveTo(4, 4)
            win.check(win.findAllNames("copyCode", win.listingBox()).every(c => !c.visible), "a tool's Copy: hidden until hovered ("
                      + win.findAllNames("copyCode", win.listingBox()).map(c => [c.copied, c.activeFocus, win.activeFocusItem ? win.activeFocusItem.objectName + ":" + win.activeFocusItem.focusReason : "-"]) + ")")
            win.listingAt("Look at how")
            const sedCard = win.findAllNames("toolWhat", win.listingBox()).find(l => l.plain.indexOf("Look at how") >= 0)
            const at = sedCard.mapToItem(null, sedCard.width / 2, sedCard.height / 2)
            win.hoverLineWidth = sedCard.width
            TestInput.moveTo(at.x, at.y + 30) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const box = win.listingBox()
            const copy = win.findAllNames("copyCode", box).find(c => c.visible)
            const head = copy ? (function up(i) { return !i || i.objectName === "toolHead" ? i : up(i.parent) })(copy.parent) : null
            win.check(copy && head, "hovered: its Copy, in the call's line (" + (copy ? copy.parent : "none") + ")")
            const sedWhat = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Look at how") >= 0)
            win.check(sedWhat && sedWhat.width === win.hoverLineWidth, "its Copy keeps its place: nothing in the line moves on hover (" + (sedWhat ? sedWhat.width : -1) + " vs " + win.hoverLineWidth + ")")
            const sed = win.findAllNames("codeView", box).find(v => v.lines.length && v.lines[0].n === "38")
            win.check(copy && sed && copy.mapToItem(null, 0, copy.height).y <= sed.mapToItem(null, 0, 0).y, "the Copy sits above the code")
            win.shotWindow("listings-copy-hover")
            // The Latest pill: on the composer's top edge, centred on the column.
            const latest = win.findName("latestButton", chat)
            const lp = latest ? latest.mapToItem(null, latest.width / 2, latest.height / 2) : null
            const cp = chat.composerItem.mapToItem(null, chat.composerItem.width / 2, 0)
            win.check(latest && Math.abs(lp.y - cp.y) < 2 && Math.abs(lp.x - cp.x) < 2, "Latest: on the composer's edge, centred (" + (lp ? [lp.x, lp.y, cp.x, cp.y] : "none") + ")")
            // Paths a listing names: on a remote session's host.
            const t = chat.transcript
            win.check(t.fullPath("src/a.rs", "", "ssh://box/home/u/p") === "ssh://box/home/u/p/src/a.rs" && t.fullPath("/etc/hosts", "", "ssh://box/home/u/p") === "ssh://box/etc/hosts"
                      && t.fullPath("x.rs", "cd sub && cat -n x.rs", "/home/u/p") === "/home/u/p/sub/x.rs" && t.fullPath("/a/b", "", "/home/u/p") === "/a/b",
                      "paths: on the session's host, from a command's cd (" + [t.fullPath("src/a.rs", "", "ssh://box/home/u/p"), t.fullPath("/etc/hosts", "", "ssh://box/home/u/p")] + ")")
            const grepHead = win.findAllNames("codeView", box).reduce((all, v) => all.concat(v.lines), []).find(l => l.mark === "file")
            win.check(grepHead && grepHead.h.indexOf("harlo-open:3:") >= 0 && grepHead.h.indexOf("↗") >= 0, "a grep file's name: a link, marked ↗")
            // A line number: opens the file at that line.
            const n38 = win.findAllNames("codeNumber", sed).find(l => l.text === "38")
            win.check(n38 && sed.lines[0].p.endsWith("/src/listing.rs"), "sed's numbers: links to src/listing.rs (" + (sed ? sed.lines[0].p : "") + ")")
            if (n38) win.clickItem(n38, "line 38") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            const line = ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1
            win.check(chat.filePath.endsWith("/src/listing.rs") && line === 38, "line 38: the file opens at it (" + chat.filePath + ", line " + line + ")")
            const top = ed.contentY, lh = ed.lineHeight
            win.check(top > 0 && top < 37 * lh - 2 * lh, "…with lines above it in view (" + Math.round(top / lh) + " lines scrolled)")
            win.check(ed.arrivalLine === 37 && ed.arrivalGlow > 0, "…tinted where you arrived (line " + (ed.arrivalLine + 1) + ")")
            win.shotWindow("listings-open-at-line")
            Workspace.open(chat.session.key) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            // The keyboard: Tab reaches a run's line (not each call); Down
            // goes into its steps, Enter opens one, Escape goes back.
            const box = win.listingBox(), runHead = win.findName("runHeader", box)
            const heads = win.findAllNames("toolHead", box)
            win.check(runHead && runHead.activeFocusOnTab && heads.every(h => !h.activeFocusOnTab), "Tab: the run's line, not each call")
            // (From the composer, by keys only.)
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.key(Qt.Key_Up, Qt.ControlModifier | Qt.ShiftModifier)
            win.check(win.activeFocusItem === runHead, "Ctrl+Shift+Up: from the composer to the run's line (" + (win.activeFocusItem ? win.activeFocusItem.objectName : "") + ")")
            win.check(runHead.Accessible.name.indexOf("Run: ") === 0, "the run's line has a name for screen readers (" + runHead.Accessible.name + ")")
            TestInput.key(Qt.Key_Down, 0)
            const first = win.activeFocusItem
            win.check(first && first.objectName === "toolHead" && win.findAllNames("toolWhat", first)[0].plain.indexOf("cat -n") >= 0, "Down: its first step")
            for (let k = 0; k < 4; k++) TestInput.key(Qt.Key_Down, 0)
            const head = win.activeFocusItem
            win.keyHead = head
            win.check(head && win.findAllNames("toolWhat", head)[0].plain.indexOf("head -16") >= 0, "Down ×4: its fifth (" + (head ? head.objectName : "") + ")")
            TestInput.key(Qt.Key_Return, 0)
            TestInput.key(Qt.Key_C, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const head = win.keyHead
            win.check(head && head.activeFocus && head.parent.open, "Enter opens it, its line ringed (" + (head ? [head.activeFocus, head.parent.open] : "") + ")")
            win.check(win.findAllNames("copyCode", head).some(c => c.copied), "Ctrl+C copies its listing")
            win.listingAt("head -16")
            win.shotWindow("listings-keyboard")
            TestInput.key(Qt.Key_Escape, 0)
            win.check(win.activeFocusItem && win.activeFocusItem.objectName === "runHeader", "Escape: back to the run's line")
            TestInput.key(Qt.Key_Escape, 0)
            win.check(win.activeFocusItem && win.activeFocusItem.objectName === "composerInput", "Escape again: the composer")
            // Typing on a run goes to the composer.
            TestInput.key(Qt.Key_Up, Qt.ControlModifier | Qt.ShiftModifier)
            TestInput.typeText("hi")
            win.check(chat.composerItem.text === "hi" && win.activeFocusItem.objectName === "composerInput", "typing on a run: into the composer (" + chat.composerItem.text + ")")
            TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0)
            win.sendByKeys("read five files") } }
        ScriptAction { script: win.waitUntil(listingSteps, () => win.chatText().indexOf("Read them all") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const box = win.listingBox()
            const shown = win.findAllNames("codeView", box).filter(v => v.visible && v.lines.length)
            win.check(shown.length === 3 && shown[0].lines[0].h.indexOf("c.rs") >= 0, "five reads: the last 3 show their lines (" + shown.map(v => v.lines.length && v.lines[0].h.replace(/<[^>]+>/g, "")) + ")")
            win.listingAt("a.rs")
            const aRead = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("a.rs") >= 0)
            const ap = aRead.mapToItem(null, 4, aRead.height / 2)
            TestInput.moveTo(ap.x, ap.y)
            win.check(win.findAllNames("showLines", box).length === 1 && !win.findAllNames("copyCode", box).some(c => c.visible), "an older read, hovered: Show lines, no Copy")
            win.shotWindow("listings-five-reads")
            win.sendByKeys("read a long file") } }
        ScriptAction { script: win.waitUntil(listingSteps, () => win.chatText().indexOf("the long one") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            chat.transcript.follow = false
            const r = win.findAllNames("toolResult", chat.transcript).find(l => l.text.indexOf("1200 lines") >= 0)
            win.check(r !== undefined, "the long read")
            const more = win.findAllNames("toolMoreLines", chat.transcript).find(l => l.text.indexOf("1194") >= 0)
            win.check(more !== undefined, "its preview: 6 lines, ↓ 1194 more (the whole file's count)")
            win.scrollIntoView(r)
            const t0 = Date.now()
            win.clickItem(r, "the long read's line")
            win.perfOpen = Date.now() - t0 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const v = win.findAllNames("codeView", chat.transcript).find(c => c.lines.length === 400)
            win.check(v !== undefined, "opened: 400 lines (" + win.findAllNames("codeView", chat.transcript).map(c => c.lines.length) + ")")
            const lf = win.findName("runBoxScroll", win.listingBox())
            win.check(lf && lf.contentY === 0, "a run of one call, opened: its top in view (" + (lf ? lf.contentY : "none") + ")")
            if (v) {
                win.check(v.lines[0].n === "1" && v.lines[399].n === "400", "…from its first line")
                // The numbers and tints placed again (as on a resize): ten times.
                const t0 = Date.now()
                for (let k = 0; k < 10; k++) v.laidOut++
                console.log("PERF relayout x10 ms", Date.now() - t0, "open ms", win.perfOpen)
            }
            const beyond = win.findAllNames("toolBeyond", chat.transcript)[0], go = win.findAllNames("toolBeyondOpen", chat.transcript)[0]
            win.check(beyond && beyond.text === "800 more lines" && go && go.text === "Open at line 401", "past 400: said, with a way on (" + (beyond ? beyond.text : "none") + " · " + (go ? go.text : "") + ")")
            if (go) { win.scrollIntoView(go); win.shotWindow("listings-past-400"); win.clickItem(go, "Open at line 401") } } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            const line = ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1
            win.check(chat.filePath.endsWith("/src/long.rs") && line === 401, "Open at line 401: the file there (" + chat.filePath + ", " + line + ")")
            Workspace.open(chat.session.key) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.sendByKeys("search a big tree") }
        ScriptAction { script: win.waitUntil(listingSteps, () => win.chatText().indexOf("Searched it") >= 0 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            chat.transcript.follow = false
            const box = win.listingBox()
            const card = what => { const w = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf(what) >= 0); return w ? w.parent.parent.parent : null }
            const grep = card("Search the whole tree"), nl = card("Number the log")
            win.check(grep && grep.code && grep.code.lines[0].n === "3" && !grep.code.earlier, "400 matches: kept from the first (" + (grep && grep.code ? grep.code.lines[0].n : "none") + ")")
            const all = grep ? chat.session.codeText("Bash", grep.command, "", chat.session.callOutput(grep.rowKey)).split("\n").length : 0
            win.check(all === 400, "Copy: all 400 matches, not just what's kept (" + all + ")")
            const nlLast = nl && nl.code ? nl.code.lines[nl.code.lines.length - 1].n : "none"
            const up = win.findAllNames("toolEarlierLines", box)
            win.check(nl && nl.code && nl.code.earlier && nlLast === "1200" && up.length === 1 && /^↑ \d+ of 1194 earlier lines$/.test(up[0].text) && win.findAllNames("toolEarlier", box).length === 0,
                      "a long log, kept to its end: its last lines, the earlier ones above them (" + nlLast + " · " + (up[0] ? up[0].text : "none") + ")")
            win.listingAt("Search the whole tree")
            const w = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Search the whole tree") >= 0)
            if (w) win.clickItem(w, "the search's line") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = win.listingBox()
            const beyond = win.findAllNames("toolBeyond", box)[0]
            win.check(beyond && beyond.text === "More lines not shown here" && win.findAllNames("toolBeyondOpen", box).length === 0, "opened: more past these, no file jump for search results (" + (beyond ? beyond.text : "none") + ")")
            win.listingAt("Number the log")
            win.shotWindow("listings-search-log")
            const up = win.findAllNames("toolEarlierLines", box)[0]
            const nlw = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Number the log") >= 0)
            const lv = nlw ? win.findAllNames("codeView", nlw.parent.parent.parent)[0] : null
            win.holdY = lv ? lv.mapToItem(null, 0, lv.height).y : -1
            if (up) win.clickItem(up, "the log's earlier lines") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = win.listingBox()
            const w = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf("Number the log") >= 0)
            const nl = w ? w.parent.parent.parent : null
            const lines = nl && nl.code ? nl.code.lines : []
            win.check(lines.length > 6 && lines[lines.length - 1].n === "1200" && Number(lines[0].n) > 1 && win.findAllNames("toolEarlier", box).length === 1
                      && win.findAllNames("toolMoreLines", box).some(b => b.text === "↑ Fewer lines") && win.findAllNames("toolEarlierLines", box).length === 0,
                      "opened, the log's kept lines to its end, and earlier lines not here said (" + (lines[0] ? lines[0].n : "none") + "–" + (lines.length ? lines[lines.length - 1].n : "") + ")")
            const lv = nl ? win.findAllNames("codeView", nl)[0] : null
            const y = lv ? lv.mapToItem(null, 0, lv.height).y : -2
            win.check(win.holdY > 0 && Math.abs(y - win.holdY) < 2, "opening the earlier lines keeps the last line where it was (" + Math.round(win.holdY) + " → " + Math.round(y) + ")")
            win.listingAt("Number the log")
            win.shotWindow("listings-search-log-open")
            win.summary(); Qt.quit() } }
    }
    property real perfOpen: 0
    property real holdY: 0
    property var harloJobCard: null
    property string harloRightWas: ""
    property var wrapBox: null

    property var keyHead: null
    function listingBox() {
        const boxes = win.findAllNames("runBox", chat.transcript)
        return boxes[boxes.length - 1]
    }
    // Brings a card of the last run to its box's top, and the box into view.
    function listingAt(what, below) {
        const box = listingBox(), f = win.findName("runBoxScroll", box)
        const w = win.findAllNames("toolWhat", box).find(l => l.plain.indexOf(what) >= 0)
        if (!w) { win.check(false, "find the card " + what); return }
        // (As a hand would: the session stops following its end, and the
        // box stays where it's put instead of going back to its end.)
        chat.transcript.follow = false
        win.scrollIntoView(f)
        f.pinned = false
        f.contentY = Math.max(0, Math.min(f.contentHeight - f.height, w.mapToItem(f.contentItem, 0, 0).y - (below || 8)))
    }
    property var toolBoxStart: []
    // Turns the wheel up over the last run box, `n` notches.
    function toolWheel(n) {
        const boxes = win.findAllNames("runBox", chat.transcript), f = win.findName("runBoxScroll", boxes[boxes.length - 1])
        const p = f.mapToItem(null, f.width / 2, f.height / 2)
        for (let k = 0; k < n; k++) TestInput.wheel(p.x, p.y, 120)
    }
    property int toolBoxCount: 0

    // Settings → Orchestration: the skills earlier versions installed are
    // gone (a user's own of the same name kept), and the page's switches.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksorchestration"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            const h = win.selftestDir + "/home"
            win.check(TestInput.readFile(h + "/.claude/skills/harlo-advisor/SKILL.md") === "" && TestInput.readFile(h + "/.agents/skills/harlo/SKILL.md") === "", "the skills harlo installed are removed at start")
            win.check(TestInput.readFile(h + "/.agents/skills/harlo-handoff/SKILL.md") === "mine\n", "the user's own skill of the same name kept")
            settingsDialog.openPage("orchestration") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findName("orchSelfStart", d) !== null && win.findName("orchReviewSteps", d) !== null && win.findText("Skills", d) === null, "the page: the switch and how helpers start, no skills")
            win.shotWindow("clicks-orchestration")
            win.findName("orchSelfStart", d).activated(0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(Workspace.setting("orchestration_on_their_own") === "asked" && win.findName("orchSelfStart", d).currentIndex === 0, "Only when I ask is saved and shown")
            win.clickItem(win.findName("orchEnabled", d), "Orchestration's switch, off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(!Workspace.orchestrationOn && Workspace.setting("orchestration") === "0" && !(win.findName("orchSelfStart", d) || {}).visible, "off: saved, and the page shows only the switch")
            win.shotWindow("clicks-orchestration-off")
            win.clickItem(win.findName("orchEnabled", d), "Orchestration's switch, on") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Workspace.orchestrationOn && Workspace.setting("orchestration") === "1", "on again")
            win.summary()
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
        ScriptAction { script: { win.log("host:", JSON.parse(Remotes.hostsJson())[0].state); Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: { chat.session.setModel("anthropic", "claude-haiku-4-5"); win.sendByKeys("Call the list_agents tool of the harlo MCP server, then reply with just the agent ids it returned, comma-separated.") } }
        PauseAnimation { duration: 45000 }
        ScriptAction { script: {
            win.log("asked:", chat.session.hasRequest, "| status:", chat.session.status, "| transcript:", win.chatText().replace(/\n/g, " | ").slice(-600))
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
        ScriptAction { script: { win.orchestrator = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, win.argValue("--harness=")) } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: {
            const m = win.argValue("--model=").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            win.sendByKeys("Call the list_agents tool of the harlo MCP server, then reply with just the agent ids it returned, comma-separated.") } }
        PauseAnimation { duration: 20000 }
        ScriptAction { script: { win.log("asked:", chat.session.hasRequest, chat.session.requestTitle); if (chat.session.hasRequest) chat.session.answerValue("Approve") } }
        PauseAnimation { duration: 25000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "| transcript:", win.chatText().replace(/\n/g, " | ").slice(-500))
            Qt.quit() } }
    }

    // Settings → Providers: each provider's state, its
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
            const d = settingsDialog.contentItem, st = id => { const l = win.findName("providerStatus-" + id, d); return l ? l.text : "(none)" }
            win.check(st("claude") === "Available · 17 models", "Claude: available, 17 models (" + st("claude") + ")")
            win.check(st("harlo") !== "(none)" && st("harlo") !== "Not installed", "Harlo: harlo's own (" + st("harlo") + ")")
            win.clickItem(win.findName("provider-claude", d), "the Claude row") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("Version 2.1.296", d) !== null && win.findText("Opus 4.8", d) !== null, "its details: version and models")
            win.shotWindow("clicks-providers")
            win.clickItem(win.findName("providerSwitch-harlo", d), "Harlo's switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const ids = JSON.parse(Workspace.chatProvidersJson()).map(p => p.id)
            const st = win.findName("providerStatus-harlo", settingsDialog.contentItem)
            win.check(st && st.text === "Disabled" && ids.indexOf("harlo") < 0, "turned off: Disabled, and not offered (" + ids + ")")
            win.check(ids.indexOf("claude") >= 0, "Claude is offered (" + ids + ")")
            settingsDialog.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            // No default set: it shows (and starts) the first one on and
            // installed, Claude here, as harlo does.
            win.check(chat.draftView.effectiveHarness === "claude" && Workspace.defaultAgent() === "claude", "with no default it shows the agent harlo starts (" + chat.draftView.effectiveHarness + ")")
            win.clickItem(win.findName("draftHarness", chat.draftView), "the provider chip") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("Claude") !== null && win.findText("Harlo") === null, "New session offers what's on")
            TestInput.key(Qt.Key_Escape, 0)
            Workspace.setProviderEnabled("harlo", true)
            win.summary(); Qt.quit() } }
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
            const chip = win.findName("draftModel")
            win.check(chip !== null && chip.text === "Default model", "a model chip: the agent's default")
            win.clickItem(chip, "the model chip") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.check(chat.draftView.modelChoices.length === 17, "its 17 models to pick (" + chat.draftView.modelChoices.length + ")") }
        ScriptAction { script: {
            win.clickItem(win.findName("draftModelFilter"), "the filter")
            TestInput.typeText("haiku") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const haiku = chat.draftView.modelChoices.find(m => m.name.toLowerCase().indexOf("haiku") >= 0)
            win.check(haiku !== undefined, "a Haiku to pick")
            win.shotWindow("clicks-draft-model")
            win.clickItem(haiku ? win.findName("draftModel-" + haiku.id) : null, "Haiku") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = chat.draftView.model
            win.check(m !== "" && win.findName("draftModel").text === chat.draftView.modelName, "picked: the chip says it (" + m + ")")
            win.clickItem(win.findName("draftInput"), "the message box")
            TestInput.typeText("hello")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const haiku = chat.draftView.modelChoices.find(m => m.name.toLowerCase().indexOf("haiku") >= 0)
            win.check(chat.session.harness === "claude" && haiku && win.chatText().indexOf("(model " + haiku.id + ",") >= 0, "the session starts with it (" + win.chatText().slice(-120) + ")")
            win.summary(); Qt.quit() } }
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
            win.check(Workspace.setting("setup_done") === "", "not set up yet")
            win.maybeShowSetup() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(setupGuide.visible && setupGuide.step === 0, "a first start shows the guide")
            win.check(["claude", "harlo"].every(id => win.findName("setupAgent-" + id) !== null), "a card for each agent")
            win.check(["claude", "harlo"].every(id => win.findName("setupInstall-" + id) === null), "no Install for an installed one")
            win.clickItem(win.findName("setupDefault-claude"), "Claude by default") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(Workspace.setting("default_harness") === "claude", "the default agent is saved")
            const combo = win.findName("setupModel"), p = JSON.parse(Workspace.providersJson()).find(x => x.id === "claude")
            const i = p.models.findIndex(n => n.toLowerCase().indexOf("haiku") >= 0)
            win.check(combo !== null && combo.visible && combo.count === p.modelIds.length + 1 && i >= 0, "its models to pick (" + (combo ? combo.count : 0) + ")")
            win.setupModel = p.modelIds[i]
            combo.activated(i + 1) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Workspace.setting("default_model_claude") === win.setupModel, "the model is saved (" + Workspace.setting("default_model_claude") + ")")
            win.shotPopup(setupGuide, "clicks-setup-agents") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("setupNext"), "Next") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(setupGuide.step === 1 && ["github", "gitlab", "forgejo"].every(s => win.findName("gitHost-" + s) !== null), "git: a card for each service")
            const gh = JSON.parse(Setup.gitStatus("github"))
            win.check(gh.state === "ready" && gh.account === "", "GitHub: not signed in here (" + JSON.stringify(gh) + ")")
            win.check(win.findName("gitHost-github").chosen, "GitHub by default")
            win.clickItem(win.findName("gitHostUse-gitlab"), "GitLab by default")
            const server = win.findName("gitHostServer-forgejo")
            server.text = "https://Git.Example.org/"
            server.editingFinished() } }
        PauseAnimation { duration: 2000 }
        // (The services checked: on a busy computer that takes a while.)
        ScriptAction { script: win.waitUntil(setupSteps, () => win.gitHostsChecked(setupGuide.contentItem), 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Workspace.setting("git_host") === "gitlab" && win.findName("gitHost-gitlab").chosen, "the default service is saved")
            win.check(JSON.parse(Setup.gitStatus("forgejo")).host === "git.example.org", "Forgejo's server, as a host (" + Setup.gitStatus("forgejo") + ")")
            win.shotPopup(setupGuide, "clicks-setup-git") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("setupNext"), "Next") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(setupGuide.step === 2, "the theme page")
            win.clickItem(win.findName("setupScheme-light"), "Light")
            win.clickItem(win.findName("setupTheme-Dracula"), "Dracula") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(Workspace.themeMode === "light" && Workspace.themeFamily === "Dracula", "the theme is chosen (" + Workspace.themeFamily + ", " + Workspace.themeMode + ")")
            win.shotPopup(setupGuide, "clicks-setup-theme")
            win.check(win.findName("setupNext").text === "Finish", "the last page finishes") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("setupNext"), "Finish") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(!setupGuide.visible && Workspace.setting("setup_done") === "1", "Finish closes it and marks harlo set up")
            win.maybeShowSetup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!setupGuide.visible, "not again on the next start")
            win.openDraft(-1)
            chat.draftView.harness = "claude" } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(chat.draftView.model === win.setupModel, "a new session starts with that model (" + chat.draftView.model + ")")
            // A harlo set up before the guide (it has projects): not shown, marked set up.
            Workspace.setSetting("setup_done", "")
            Workspace.addProject(win.argValue("--cwd="))
            win.maybeShowSetup() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!setupGuide.visible && Workspace.setting("setup_done") === "1", "a harlo with projects doesn't get it")
            settingsDialog.openPage("git") } }
        PauseAnimation { duration: 1500 }
        // (The services checked: on a busy computer that takes a while.)
        ScriptAction { script: win.waitUntil(setupSteps, () => win.gitHostsChecked(settingsDialog.contentItem), 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findName("gitHost-gitlab", settingsDialog.contentItem) !== null, "Settings → Git has the services")
            win.shotPopup(settingsDialog, "clicks-setup-settings-git") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: settingsDialog.openPage("general") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("openSetupGuide"), "Settings → Open the setup guide") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(setupGuide.visible && !settingsDialog.visible && setupGuide.step === 0, "Settings opens it again, from the start")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(!setupGuide.visible, "Escape closes it")
            win.summary()
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
            win.reviewKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.clickText("app.rs", changes) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.reviewRow = dv.rows.findIndex(r => r[5] === 1)
            win.clickAt(win.findName("gutter-r-" + win.reviewRow, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = chat.diffView.commentBox
            win.check(box !== null && box.visible && box.side === "new" && box.code !== "", "the comment box, with the line's code (" + (box ? box.code : "") + ")")
            win.clickItem(win.findName("commentText", box.contentItem), "the comment")
            TestInput.typeText("Use a clearer name here") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("commentSave", chat.diffView.commentBox.contentItem), "Comment") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(dv.comments.length === 1 && dv.comments[0].path === "src/app.rs" && dv.comments[0].text === "Use a clearer name here", "the comment is kept (" + JSON.stringify(dv.comments) + ")")
            const old = dv.rows.findIndex(r => r[0] === 1)
            win.clickAt(win.findName("gutter-l-" + old, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.clickItem(win.findName("commentText", chat.diffView.commentBox.contentItem), "the comment")
            TestInput.typeText("Keep this check")
            TestInput.key(Qt.Key_Return, Qt.ControlModifier) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const dv = chat.diffView, bar = win.findName("reviewBar", dv)
            win.check(dv.comments.length === 2 && dv.comments.some(c => c.side === "old"), "Ctrl+Enter keeps the second (on a removed line)")
            win.check(bar !== null && bar.visible && dv.canSend, "the bar: 2 comments, ready to send")
            const summary = win.findName("reviewSummary", bar), who = win.findName("reviewFor", bar)
            // (Narrow, who they're for is cut first: it may have no room.)
            win.check(summary !== null && (summary.text === "2 comments on 1 file" || (chat.diffView.width < 460 && summary.text === "2 comments")) && (who === null || who.text.indexOf("for ") === 0), "it says how many, on how many files, for which session (" + (summary ? summary.text + " " + (who ? who.text : "-") : "") + ")")
            // Each saved comment shows under its line.
            win.check(win.findName("diffNote-0", dv) !== null && win.findName("diffNote-1", dv) !== null && dv.rows.filter(r => r[0] === 4).length === 6, "the comments show under their lines (" + dv.rows.filter(r => r[0] === 4).length + " rows kept for them)")
            win.shot("clicks-review") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickItem(win.findName("reviewSend", chat.diffView), "Send to the agent") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const text = win.chatText()
            win.check(Workspace.activeKey === win.reviewKey, "back in the session (" + Workspace.activeKey + ")")
            win.check(text.indexOf("Review comments") >= 0 && text.indexOf("src/app.rs:") >= 0 && text.indexOf("Use a clearer name here") >= 0 && text.indexOf("Keep this check") >= 0 && text.indexOf("a line you removed") >= 0,
                      "the agent gets them as one message (" + text.slice(0, 600) + ")")
            win.check(Object.keys(chat.reviewStore.all).length === 0, "and they're gone from the diff")
            // Off in Settings → Git: a diff takes no comments.
            Workspace.setSetting("review_comments", "0")
            win.clickText("app.rs", changes) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(chat.diffView.visible && chat.diffView.review === null, "comments off: the diff takes none")
            win.summary()
            Qt.quit() } }
    }
    property int reviewRow: -1
    /// The diff's "hide unchanged lines": its button, or (a tight toolbar)
    /// the More menu's item.
    function diffCollapseClick() {
        const b = win.findName("diffCollapse")
        if (b) { win.clickItem(b, "Hide unchanged lines"); return }
        win.clickItem(win.findName("diffMore"), "More")
        diffMenuTimer.restart()
    }
    Timer { id: diffMenuTimer; interval: 350; onTriggered: win.clickItem(win.findName("diffMenuCollapse"), "Hide unchanged lines (menu)") }
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
            win.undoKey = Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            chat.composerItem.text = "Rewrite the README"
            chat.composerItem.focusInput()
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const undo = JSON.parse(chat.session.undoJson || "{}")
            win.check(Object.keys(undo).length === 1, "the message has a snapshot to undo to (" + chat.session.undoJson + ")")
            Workspace.openFileTab(win.argValue("--cwd=") + "/README.md") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { chat.fileEditor.typeText("The agent was here.\n"); chat.fileEditor.save() } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.open(win.undoKey); windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(git.files.some(f => f.startsWith("README.md\t")), "README.md is changed (" + git.files + ")")
            const key = Object.keys(JSON.parse(chat.session.undoJson || "{}"))[0]
            const i = chat.session.indexOfKey(key)
            win.shot("clicks-undo-button")
            const latest = win.findName("undoLatest")
            win.check(i >= 0 && chat.transcript.latestUndo === key && latest !== null && latest.visible && latest.text === "Undo these changes (1 file)",
                      "under the reply: Undo these changes (1 file) (" + (latest ? latest.text : "none") + ")")
            chat.undoDialog.show(key) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson || "{}")
            win.check(plan.state === "ready" && plan.restore.indexOf("README.md") >= 0 && plan.remove.length === 0, "it would bring README.md back (" + chat.session.undoPlanJson + ")")
            win.check(chat.undoDialog.title === "Undo this turn", "the last turn: Undo this turn")
            win.shotPopup(chat.undoDialog, "clicks-undo-dialog") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("undoConfirm", chat.undoDialog.footer), "Undo") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const plan = JSON.parse(chat.session.undoPlanJson || "{}")
            win.check(plan.state === "done" && plan.message.indexOf("Undone") === 0, "undone (" + plan.message + ")")
            win.check(!git.files.some(f => f.startsWith("README.md\t")) && git.files.some(f => f.startsWith("notes.md\t")), "README.md is back; your notes.md stays (" + git.files + ")")
            win.shotPopup(chat.undoDialog, "clicks-undo-done") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.transcript.latestUndo === "", "undone: nothing left to undo, the button goes")
            win.shot("clicks-undo-after") } }
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
            const earlier = win.findName("undoTurn-" + second)
            win.check(first !== undefined && earlier !== null && earlier.visible && earlier.turn === first, "the earlier turn ends with Undo from here (" + chat.session.undoJson + ")")
            // Off in Settings → Git: no Undo, old snapshots neither.
            Workspace.setSetting("undo_snapshots", "0")
            chat.session.loadUndo() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(chat.session.undoJson === "", "Undo off: no message offers it (" + chat.session.undoJson + ")")
            win.summary()
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
            win.check(req.state === "ready" && req.service === "github" && req.repo === "octo/app" && req.head === "fix/sidebar-scroll" && req.base === "main", "the branch can have a pull request on GitHub (" + git.requestJson + ")")
            win.check(req.url === "" && req.canCreate, "none yet; gh can create it")
            win.check(req.title === "Sidebar scroll" && req.body === "- Keep the sidebar's place\n- Test it with 200 sessions", "title and description from its commits (" + req.title + " / " + req.body + ")")
            win.clickItem(win.findName("requestButton"), "Create pull request") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const d = changes.requestDialog
            win.check(d.visible && win.findName("requestTitle", d.contentItem).text === "Sidebar scroll" && win.findName("requestBase", d.contentItem).text === "main", "the form, filled in")
            win.shotPopup(d, "clicks-request-form") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("requestCreate", changes.requestDialog.footer), "Create") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const req = JSON.parse(git.requestJson || "{}")
            win.check(!changes.requestDialog.visible, "created: the form closes")
            win.check(req.url === "https://github.com/octo/app/pull/7" && req.number === 7, "the panel has it (" + git.requestJson + ")")
            win.check(git.message === "Opened the pull request #7.", "and says so (" + git.message + ")")
            const button = win.findName("requestButton")
            win.check(button !== null && button.text === "Open", "its button opens it now")
            const checks = req.checks || {}
            win.check(checks.state === "pending" && win.findName("checksSummary").text === "Checks running · 1 of 2 done", "its checks show, still running (" + JSON.stringify(checks) + ")")
            win.shot("clicks-request-panel") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The lint check fails; harlo (watching) says so.
            TestInput.writeFile(win.argValue("--cwd=") + "/../gh-checks.json", JSON.stringify([
                { name: "build", bucket: "pass", link: "https://github.com/octo/app/actions/runs/41/job/1" },
                { name: "lint", bucket: "fail", link: "https://github.com/octo/app/actions/runs/42/job/2" }]))
            Workspace.tickSchedules() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const n = win.lastNote()
            win.check(n.kind === "ci" && n.title === "Checks failed: Pull request #7" && n.body.startsWith("1 of 2 checks failed: lint"), "harlo notifies when the checks finish (" + Workspace.lastNotification + ")")
            git.loadRequest()
            // (Looked up again, as when the panel opens: the row stays.)
            const row = win.findName("requestRow")
            win.check(JSON.parse(git.requestJson).state === "loading" && row !== null && row.visible, "while it's looked up again the request row stays (" + JSON.parse(git.requestJson).state + ")") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("checksSummary").text === "Failed: lint", "the panel shows what failed")
            win.shot("clicks-request-failed") } }
        // (A shot is taken at the next frame: nothing changes before it.)
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.clickItem(win.findName("ciSend"), "Send to the agent") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const prompts = JSON.parse(chat.session.userPrompts())
            const last = prompts.length ? prompts[prompts.length - 1].text : ""
            win.check(last.startsWith("The CI checks on pull request #7 (https://github.com/octo/app/pull/7) failed: lint.") && win.chatText().indexOf("error: lint failed") > 0,
                      "Send to the agent sends what failed, with its log (" + last + ")")
            win.summary(); Qt.quit() } }
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
            const turn = win.findName("turnAvatar")
            // (A narrow chat drops the turns' avatars.)
            win.narrowChat = turn === null
            win.check(Theme.avatar === "classic" && (win.narrowChat || String(turn.source).endsWith("penguin-classic.png")), "the classic penguin by default, on your turn (" + (turn ? turn.source : "narrow: no turn avatar") + ")")
            win.clickItem(win.findName("avatarButton"), "your avatar") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findName("avatar-glasses") !== null && win.findName("avatar-scarf") !== null, "the picker shows the penguins")
            win.shotWindow("clicks-avatar-picker")
            win.clickItem(win.findName("avatar-glasses"), "the glasses penguin") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const turn = win.findName("turnAvatar"), rail = win.findName("avatarButton")
            win.check(Theme.avatar === "glasses" && JSON.parse(Workspace.look).avatar === "glasses", "the pick is saved (" + Workspace.look + ")")
            win.check(win.narrowChat || String(turn.source).endsWith("penguin-glasses.png"), "and shows on your turn")
            win.check(win.findName("avatar-glasses") === null || !win.findName("avatar-glasses").visible, "the picker closes")
            win.shotWindow("clicks-avatar")
            win.summary()
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
            win.check(Workspace.themeFamily === "harlo", "harlo's own theme by default (" + Workspace.themeFamily + ")")
            win.check(win.findName("scheme-system") !== null && win.findName("theme-Dracula") !== null, "the scheme and theme cards show")
            win.shotWindow("clicks-appearance-top")
            win.clickItem(win.findName("scheme-light"), "Light") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Workspace.themeMode === "light" && Theme.isLight, "Light switches to light (" + Workspace.themeMode + ")")
            win.clickItem(win.findName("theme-Dracula"), "Dracula") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Workspace.themeFamily === "Dracula", "a theme card picks the theme (" + Workspace.themeFamily + ")")
            win.shotWindow("clicks-appearance-dracula-light")
            win.clickItem(win.findName("scheme-dark"), "Dark") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("theme-Midnight"), "Midnight") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Workspace.themeFamily === "Midnight" && String(Theme.accent) === "#b986fd", "Midnight: navy and purple (" + Workspace.themeFamily + ", " + Theme.accent + ")")
            settingsDialog.close()
            win.shotWindow("clicks-appearance-midnight") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            Workspace.chooseTheme("harlo")
            settingsDialog.openPage("appearance") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const before = String(Theme.hairline)
            Workspace.chooseLook("contrast", "160")
            win.check(Math.abs(Theme.contrast - 1.6) < 0.001 && String(Theme.hairline) !== before, "contrast spreads the colours (" + Theme.contrast + ")") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("chatWidth-wide"), "Wide") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.chatMaxWidth === 1180, "Wide widens the chat (" + Theme.chatMaxWidth + ")")
            win.clickItem(win.findName("diff-blue-orange"), "Blue & orange") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.diffBlueOrange && Qt.colorEqual(Theme.diffAdd, "#5b9bf0"), "diffs in blue and orange (" + Theme.diffAdd + ")")
            win.scrollIntoView(win.findName("contrastSlider"))
            win.shotWindow("clicks-appearance-interface")
            win.findName("fontSize-mono").activated(6) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.mpx(13) === 16, "the code font's size (" + Theme.mpx(13) + " px)")
            win.findName("fontSize-ui").activated(4) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.px(14) === 16 && Theme.mpx(13) === 16, "the interface's size, the code's kept (" + Theme.px(14) + ", " + Theme.mpx(13) + ")")
            win.clickItem(win.findName("wrapSwitch"), "Word wrap") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Theme.wrapCode, "word wrap on")
            Workspace.chooseLook("motion", "0")
            win.check(Theme.motion === 0, "animations off")
            win.scrollIntoView(win.findName("wrapSwitch"))
            win.shotWindow("clicks-appearance-typography") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            for (const k of ["contrast", "chatWidth", "diff", "monoScale", "wrap", "motion"]) Workspace.chooseLook(k, "null")
            Workspace.chooseTextScale(1)
            win.check(Workspace.look === "{}" && Theme.contrast === 1, "back to the defaults (" + Workspace.look + ")")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
    }

    // Accounts (Settings → Providers): signing Claude in from harlo, against
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
        ScriptAction { script: win.clickItem(win.findName("provider-claude"), "the Claude row") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const none = win.findName("accountsNone-claude")
            const claude = JSON.parse(Workspace.providersJson()).find(p => p.id === "claude")
            win.check(none !== null, "Claude: not signed in (" + Accounts.status("claude") + ")")
            win.check(claude.modelCount === 0 && claude.error.indexOf("Not signed in") === 0, "so no Claude models are offered (" + claude.modelCount + ", " + claude.error + ")")
            win.clickItem(win.findName("signIn-claude"), "Sign in…") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(signInDialog.visible && win.findText("Claude account") !== null && win.findText("Anthropic Console") !== null, "the dialog offers a Claude account or the Console")
            win.clickItem(win.findName("signInMethod-claudeai-claudeai"), "Sign in (Claude account)") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const f = signInDialog.flow
            const link = f && f.steps.find(s => s.kind === "link")
            win.check(link && link.url === "https://claude.com/cai/oauth/authorize?code=true&test=1" && link.open === false, "Claude Code's own page, as a link (Claude Code opens it) (" + (link ? link.url : "none") + ")")
            win.check(f && f.ask && f.ask.kind === "code" && f.ask.optional === true, "a box for the page's code, if it shows one")
            win.shotWindow("clicks-accounts-claude")
            win.clickItem(win.findName("signInAnswer"), "the code box")
            TestInput.typeText("good-code")
            win.clickItem(win.findName("signInSend"), "Continue") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const f = signInDialog.flow, st = JSON.parse(Accounts.status("claude"))
            win.check(f && f.state === "done", "signed in (" + (f ? f.state + " " + f.message : "") + ")")
            win.check(st.accounts.length === 1 && st.accounts[0].label === "me@example.com" && st.accounts[0].detail === "Claude Max", "the account shows: me@example.com · Claude Max (" + JSON.stringify(st.accounts) + ")")
            win.clickItem(win.findName("signInClose"), "Done") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const claude = JSON.parse(Workspace.providersJson()).find(p => p.id === "claude")
            win.check(claude.modelCount === 17 && claude.error === "", "signed in: its 17 models are offered (" + claude.modelCount + ")")
            win.clickItem(win.findName("signOut-claude-claude"), "Sign out") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(JSON.parse(Accounts.status("claude")).accounts.length === 0 && win.findName("accountsNone-claude") !== null, "signed out")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
    }

    // iPhone notifications (Settings → Devices): through harlo's relay, and
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
            win.check(win.findText("iPhone notifications", d) !== null && win.findName("pushTest", d).enabled, "Settings → Devices has iPhone notifications")
            win.check(win.findText("Your own APNs key", d) === null && win.findName("relayAddress", d) === null, "no key or relay address to set")
            win.check(JSON.parse(Phone.devicesJson())[0].push === true, "the phone that allowed them is known")
            win.check(JSON.parse(Phone.pushJson()).relay === "https://relay.harlo.run", "they go through harlo's relay (" + JSON.parse(Phone.pushJson()).relay + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("pushTest", settingsDialog.contentItem), "Send a test notification") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(Phone.pushNote === "Sent to 1 phone.", "the test notification goes to the phone (" + Phone.pushNote + ")")
            win.shotWindow("clicks-push")
            win.summary(); Qt.quit() } }
    }

    // After piqt's data moved over (not in the click suite): what harlo sees.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "migratecheck"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const projects = JSON.parse(Workspace.projectsJson())
            win.log("projects:", projects.length, projects.map(p => p.name).join(", "), "| sessions in sidebar:", win.sidebarCount(), "| theme:", Workspace.themeFamily, Workspace.themeMode)
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
            const icon = win.findName("projectIcon-repo"), other = win.findName("projectIcon-Other sessions")
            win.check(icon !== null && icon.icon === "" && other !== null, "a new project, and Other sessions, show the folder")
            TestInput.moveTo(icon.mapToItem(null, 4, 4).x, icon.mapToItem(null, 4, 4).y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findName("projectIcon-repo") === null && win.findText("▾") !== null, "hovered, the arrow takes the icon's place")
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Project settings…") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(settingsDialog.page === "projects" && win.findName("projectName", d).text === "repo", "Project settings opens on the project")
            win.clickItem(win.findName("projectIconPick-rocket", d), "the rocket") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("projectIcon-repo").icon === "rocket", "the sidebar shows the rocket")
            win.clickItem(win.findName("projectColor-keyword", settingsDialog.contentItem), "a colour") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("projectIcon-repo").color === "keyword", "and its colour")
            win.clickItem(win.findName("projectName", settingsDialog.contentItem), "the name")
            TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("Rocket app"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findText("Rocket app") !== null && win.findName("projectIcon-Rocket app") !== null, "renaming shows in the sidebar")
            win.shotWindow("clicks-projects")
            win.clickItem(win.findName("projectIconPick-none", settingsDialog.contentItem), "the folder") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("projectIcon-Rocket app").icon === "", "the folder again")
            settingsDialog.close()
            // A second project (the test's own folder): it goes first.
            Workspace.addProject(win.selftestDir) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const names = () => JSON.parse(Workspace.projectsJson()).map(p => p.name)
            const second = win.selftestDir.split("/").filter(s => s).pop()
            win.check(names()[0] === second && names()[1] === "Rocket app", "a new project goes first (" + names().join(", ") + ")")
            // A session in the lower project doesn't move it up.
            Workspace.newSession(JSON.parse(Workspace.projectsJson())[1].id, "demo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const second = win.selftestDir.split("/").filter(s => s).pop()
            const top = win.findName("projectTitle-" + second), low = win.findName("projectTitle-Rocket app")
            win.check(top !== null && low !== null && top.mapToItem(null, 0, 0).y < low.mapToItem(null, 0, 0).y, "a new session in the lower project leaves it where it is")
            // Dragged by its header above the first: it goes first.
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            TestInput.moveTo(from.x, from.y); TestInput.press(from.x, from.y, true)
            for (let i = 1; i <= 4; i++) TestInput.moveTo(from.x, from.y - 3 * i) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Lifted but not past another project's header: no line.
            const line = win.findName("projectDropLine"), ghost = win.findName("projectDragGhost")
            win.check(ghost !== null && ghost.visible && (line === null || !line.visible), "lifted where it would stay, no line shows")
            const low = win.findName("projectDragGhost"), top = win.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop())
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            win.dragPointerY = to.y
            for (let i = 1; i <= 12; i++) TestInput.moveTo(from.x, from.y + (to.y - from.y) * i / 12) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const line = win.findName("projectDropLine"), ghost = win.findName("projectDragGhost")
            const first = win.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop())
            const ly = line ? line.mapToItem(null, 0, line.height / 2).y : -1, fy = first.mapToItem(null, 0, 0).y
            win.check(line !== null && line.visible && ly < fy + 6 && ly > fy - 20, "past the first project's header, the line shows just above it (" + Math.round(ly) + " over " + Math.round(fy) + ")")
            win.check(line !== null && line.y >= 0 && line.y + line.height <= line.parent.height, "and inside the list, at its very top too (" + (line ? line.y.toFixed(1) : "") + ")")
            win.check(ghost !== null && ghost.visible && win.findText("Rocket app", ghost) !== null, "and a copy of its header follows the pointer")
            const below = first.mapToItem(null, 0, first.height).y, gy = ghost ? ghost.mapToItem(null, 0, 0).y : -1
            win.check(line !== null && ghost !== null && line.z > ghost.z && gy >= below && gy - win.dragPointerY <= 36, "the copy hangs just under the name it lands above, close to the pointer, the line drawn over it (" + Math.round(gy) + " under " + Math.round(below) + ", pointer " + Math.round(win.dragPointerY) + ")")
            // Nothing under the pointer lights up as if it took the drop.
            const lit = []
            const walk = item => { for (const c of item.children) { if (c.hovered === true && c.objectName !== "projectDragGhost") lit.push(c.objectName || String(c)); walk(c) } }
            walk(sidebar)
            win.check(lit.length === 0, "rows don't light up under the dragged project (" + lit.join(", ") + ")")
            const rows = win.findAllNames("sessionRow", sidebar).filter(r => r.visible)
            win.check(rows.length > 0 && rows.every(r => r.opacity < 0.6), "the sessions fade while it's dragged: only the headers take it (" + rows.map(r => r.opacity).join(", ") + ")")
            // (findName skips what's faded out: its title isn't to be found.)
            win.check(win.findName("projectTitle-Rocket app") === null && win.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop()) !== null, "its place is left empty")
            win.shotWindow("clicks-projects-drag")
            // A session starting mid-drag (the rows made again) doesn't drop it.
            win.sessionsBefore = win.sidebarCount()
            Workspace.newSession(-1, "demo") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const ghost = win.findName("projectDragGhost")
            win.check(ghost !== null && ghost.visible && win.sidebarCount() === win.sessionsBefore, "a session starting mid-drag leaves the rows as they are, still lifted")
            const top = win.findName("projectTitle-" + win.selftestDir.split("/").filter(s => s).pop()), to = top.mapToItem(null, 10, -6)
            TestInput.press(to.x, to.y, false) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.sidebarCount() === win.sessionsBefore + 1, "dropped, the rows catch up (" + win.sidebarCount() + ")")
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            win.check(names[0] === "Rocket app", "dropped above, it's first (" + names.join(", ") + ")")
            const line = win.findName("projectDropLine"), ghost = win.findName("projectDragGhost")
            win.check((line === null || !line.visible) && (ghost === null || !ghost.visible), "and the line and the copy go")
            // Its menu: Move down, and Move up is off at the top.
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.place = Workspace.projectPlace(p.id)
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = sidebar.projectMenu.contentItem, up = win.findName("projectMoveUp", m), down = win.findName("projectMoveDown", m)
            win.check(up !== null && !up.enabled && down !== null && down.enabled, "at the top, Move up is off")
            win.clickText("Move down") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            win.check(names[1] === "Rocket app", "Move down moves it a place (" + names.join(", ") + ")")
            // Esc calls a drag off: it stays where it was.
            const low = win.findName("projectTitle-Rocket app"), top = win.findName("projectTitle-" + names[0])
            const from = low.mapToItem(null, 10, low.height / 2), to = top.mapToItem(null, 10, -6)
            TestInput.moveTo(from.x, from.y); TestInput.press(from.x, from.y, true)
            for (let i = 1; i <= 12; i++) TestInput.moveTo(from.x, from.y + (to.y - from.y) * i / 12) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const line = win.findName("projectDropLine")
            win.check(line !== null && line.visible, "dragged again, the line shows")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const ghost = win.findName("projectDragGhost"), line = win.findName("projectDropLine")
            win.check((ghost === null || !ghost.visible) && (line === null || !line.visible), "Esc calls the drag off")
            const top = win.findName("projectTitle-" + JSON.parse(Workspace.projectsJson())[0].name), to = top.mapToItem(null, 10, -6)
            TestInput.press(to.x, to.y, false) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            win.check(names[1] === "Rocket app", "and letting go then leaves it where it was (" + names.join(", ") + ")")
            // From the keyboard: its header focused, Alt+Shift+↑ moves it up.
            let h = win.findName("projectTitle-Rocket app")
            while (h && h.projectId === undefined) h = h.parent
            win.check(h !== null && h.focusPolicy === Qt.TabFocus && h.Accessible.name.startsWith("Rocket app, project"), "a project's header takes the keys from Tab, named for a screen reader (" + (h ? h.Accessible.name : "") + ")")
            h.forceActiveFocus(Qt.TabFocusReason)
            TestInput.key(Qt.Key_Up, Qt.AltModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            const f = win.activeFocusItem
            win.check(names[0] === "Rocket app" && f && f.projectId !== undefined && f.title === "Rocket app" && f.background.border.width === 2, "Alt+Shift+↑ moves it up, and its header keeps the keys, ringed (" + names.join(", ") + "; " + (f ? f.title : "none") + ")")
            win.shotWindow("clicks-projects-keys")
            TestInput.key(Qt.Key_F10, Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(sidebar.projectMenu.opened && sidebar.projectMenu.name === "Rocket app", "Shift+F10 opens its menu")
            sidebar.projectMenu.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // The move's notice offers Undo, and Ctrl+Z (not typing) takes it.
            const undo = win.findName("noticeUndo")
            win.check(undo !== null && Workspace.notice.startsWith("Rocket app moved above "), "a move's notice offers Undo (" + Workspace.notice + ")")
            win.shotWindow("clicks-projects-undo")
            const f = win.activeFocusItem
            win.check(f && f.cursorPosition === undefined, "the keys aren't in a text field (" + f + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const names = JSON.parse(Workspace.projectsJson()).map(p => p.name)
            win.check(names[1] === "Rocket app" && Workspace.notice === "" && win.findName("noticeUndo") === null, "Ctrl+Z puts it back, and the notice goes (" + names.join(", ") + ")")
            win.summary(); Qt.quit() } }
    }

    // Pair a device sized to its content (its first screen once spilled out
    // of a too-short dialog), and in a very short window: it fits, and its
    // content scrolls.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksphoneshort"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.width = Number(win.argValue("--winwidth=") || 900); win.height = 700; settingsDialog.openPage("devices") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("pairDeviceSettings", settingsDialog.contentItem), "Pair a device…") }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const d = pairDeviceDialog, scroller = win.findName("pairScroller")
            win.check(d.visible && scroller !== null && scroller.height >= scroller.contentHeight - 1 && scroller.contentHeight > 150,
                      "the dialog is as tall as its content (" + (scroller ? Math.round(scroller.height) + " for " + Math.round(scroller.contentHeight) : "") + ")")
            win.shotPopup(d, "clicks-phone-sized") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.height = 260 }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = pairDeviceDialog, scroller = win.findName("pairScroller")
            const top = d.background.parent.mapToItem(null, 0, 0).y
            win.check(top >= 0 && top + d.height <= win.height + 1, "in a short window it fits (" + Math.round(top) + "+" + Math.round(d.height) + " of " + win.height + ")")
            win.check(scroller.contentHeight > scroller.height + 1, "and its content scrolls (" + Math.round(scroller.contentHeight) + " in " + Math.round(scroller.height) + ")")
            win.shotPopup(d, "clicks-phone-short") } }
        PauseAnimation { duration: 400 }
        // A port of its own (the running harlo may have 7676).
        ScriptAction { script: { Phone.setAccessPort(20000 + Math.floor(Math.random() * 12000)); win.clickItem(win.findName("phoneTurnOn"), "Turn on (scrolled into view)") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(Phone.enabled, "a button below the fold is scrolled to and clicked")
            pairDeviceDialog.close(); settingsDialog.close(); win.height = 700 } }
        // Opened again with phone access already on: the code is there at
        // once, for this network (it once needed another address picked
        // and back).
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickItem(win.findName("pairDeviceButton"), "phone button") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const qr = win.findName("pairQr"), link = win.findName("pairLink")
            win.check(qr !== null && String(qr.source).startsWith("data:image/svg+xml") && link !== null && /^http:\/\/[0-9.]+:[0-9]+\/#p=/.test(link.text),
                      "reopened: the QR code shows straight away (" + (link ? link.text.substring(0, 32) : "none") + ")")
            win.summary(); Qt.quit() } }
    }
    // Runs of tool calls sit under a summary line that folds them (a saved session that
    // scripts/click-tests.sh writes); session rows are one line.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksruns"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            const row = win.findText("Fix the login redirect loop", sidebar)
            let d = row
            while (d && d.rowKey === undefined) d = d.parent
            win.check(d && d.height <= 46 && d.detail.endsWith("/repo") && d.ToolTip.text === d.detail, "the session row is one line, its folder in the hover tip (" + (d ? d.height + " " + d.detail : "none") + ")")
            win.clickItem(row, "the session") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const t = chat.transcript
            const run = win.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t)
            win.check(run !== null, "the second run's summary")
            win.check(win.findContaining("npm test -- auth", t) !== null && win.findAllNames("runBox", t).length > 0, "a finished run starts open, in its box")
            const whats = win.findAllNames("toolWhat", t)
            const npm = whats.find(l => l.plain === "npm test -- auth")
            let card = npm
            while (card && card.echoed === undefined) card = card.parent
            win.check(card && card.echoed && !card.outInWell, "one line of output that the line already says ('48 passed') isn't shown again")
            const guard = whats.find(l => l.plain === "src/auth/guard.ts")
            win.check(guard && guard.font.family === Theme.mono && npm.font.family === Theme.mono, "paths and commands in code type")
            win.shotWindow("clicks-runs-open")
            win.clickItem(run, "the summary") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = chat.transcript
            win.check(win.findContaining("npm test -- auth", t) === null, "clicking it folds the run")
            win.check(t.debugFoldedRuns() === 1, "only that run")
            win.shotWindow("clicks-runs-folded")
            win.clickItem(win.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t), "the summary again") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findContaining("npm test -- auth", chat.transcript) !== null, "and again opens it")
            chat.transcript.positionAt(0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = chat.transcript
            const read = win.findAllNames("codeView", t).find(v => v.lines.some(l => l.h.indexOf("guard") >= 0))
            win.check(read && read.lines[0].n === "1" && read.lines[0].p.endsWith("/src/auth/guard.ts") && win.findAllNames("codeNumber", read).length === 1,
                      "Harlo's plain read numbered from its first line, a link to the file (" + (read ? JSON.stringify(read.lines[0].n) : "none") + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.log("glass:", Theme.glass, "backdrop:", Theme.backdrop !== null); win.shotWindow("glass-menu") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { sidebar.sessionMenu.close(); pairDeviceDialog.open() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.shotWindow("glass-dialog") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { pairDeviceDialog.close(); settingsDialog.openPage("appearance") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.shotWindow("glass-settings") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("fontPick-Interface font", settingsDialog.contentItem), "font list") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.shotWindow("glass-settings-combo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { settingsDialog.close(); Workspace.chooseGlass(false, Workspace.glassOpacity) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: sidebar.sessionMenu.popup(win.contentItem, 520, 260) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shotWindow("glass-off-menu") }
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
            win.log("phone running:", Phone.running, Phone.error, "relay:", Phone.relayState, Phone.relayError)
            testFile.path = win.selftestDir + "/pair-url.txt"
            testFile.save(JSON.parse(Phone.pairingJson(win.argValue("--relay=") ? "relay" : "127.0.0.1")).url) } }
        PauseAnimation { duration: Number(win.argValue("--stay=") || 60) * 1000 }
        ScriptAction { script: { win.log("devices:", Phone.devicesJson()); Qt.quit() } }
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
            win.blockedCount = addresses.length
            win.check(Phone.running && addresses.length > 0, "phone access runs (" + addresses.map(a => a.label).join(", ") + ")")
            win.check(addresses.every(a => win.findName("phoneFirewall-" + a.address) !== null), "a notice for each blocked address")
            const tail = addresses.find(a => String(a.label).startsWith("Tailscale"))
            if (tail) win.check(win.findText("Your phone can't reach harlo over Tailscale") !== null, "the Tailscale one says so")
            // (The notices themselves: they sit above the page.)
            win.grabItem(win.findName("phoneFirewallToast"), "clicks-phone-firewall-toast")
            win.clickItem(win.findName("phoneFirewallAllow"), "Allow") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const left = JSON.parse(Phone.blockedJson()).length
            win.check(left === win.blockedCount - 1 && Phone.firewallError === "", "Allow opens that address (" + left + " left, " + Phone.firewallError + ")")
            win.summary(); Qt.quit() } }
    }
    property int blockedCount: 0
    property bool otherAddress: false
    // The real thing, read-only (not in the click suite): plan limits via harlo.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "quotareal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: Workspace.refreshQuota() }
        PauseAnimation { duration: 25000 }
        ScriptAction { script: { const d = JSON.parse(Workspace.quotaJson); win.log("accounts:", d.accounts.length, "error:", d.error, "first:", d.accounts.length ? d.accounts[0].provider + " " + d.accounts[0].plan + " " + d.accounts[0].windows.map(w => w.label + " " + Math.round(w.usedFraction * 100) + "%").join(", ") : "-"); Qt.quit() } }
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
        ScriptAction { script: { win.check(git.branch === "main" && changes.changedCount === 1, "starts on main with notes.md untracked"); win.clickItem(win.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const p = changes.branchPicker, x = p.background.parent.mapToItem(null, 0, 0).x
            win.check(p.opened, "chip click opens the picker")
            win.check(x >= 0 && x + p.width <= win.width, "picker inside the window (" + Math.round(x) + "–" + Math.round(x + p.width) + " of " + win.width + ")")
            win.check(p.items.some(i => i.name === "wt-branch"), "picker lists the worktree branch")
            TestInput.typeText("feature-x") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.check(changes.branchPicker.items[0].kind === "create", "typing offers Create branch"); win.shotPopup(changes.branchPicker, "clicks-create"); TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.check(git.branch === "feature-x" && changes.changedCount === 1, "Enter creates feature-x and keeps notes.md (" + git.branch + ", " + changes.changedCount + ")") }
        // B: commit notes.md on feature-x
        ScriptAction { script: { win.clickItem(win.findName("commitMessage"), "commit message"); TestInput.typeText("add notes") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Commit all") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: win.check(changes.changedCount === 0, "commit leaves nothing to commit (" + git.message + ")") }
        // C: clean switches, no dialog
        ScriptAction { script: win.clickItem(win.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("main", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(git.branch === "main" && !changes.switchDialog.visible, "clean switch to main, no dialog"); win.check(Workspace.fileStamp(win.argValue("--cwd=") + "/notes.md") === "", "notes.md not on main") } }
        ScriptAction { script: win.clickItem(win.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("feature-x", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(git.branch === "feature-x", "back on feature-x"); win.check(Workspace.fileStamp(win.argValue("--cwd=") + "/notes.md") !== "", "notes.md back") ; testFile.load(win.argValue("--cwd=") + "/README.md") } }
        PauseAnimation { duration: 500 }
        // D: change README, switch bringing it along
        ScriptAction { script: { testFile.save(testFile.text + "world\n"); git.refresh(true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.check(changes.changedCount === 1, "README.md modified"); win.clickItem(win.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("main", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.check(changes.switchDialog.visible && changes.switchDialog.title === "Switch to main?", "dirty switch asks first"); win.shotPopup(changes.switchDialog, "clicks-switch-dialog"); } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Bring changes along") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.check(git.branch === "main" && changes.changedCount === 1 && !changes.switchDialog.visible, "Bring changes along: on main, README still modified") }
        // E: stash and switch, then pop
        ScriptAction { script: win.clickItem(win.findName("branchChip"), "branch chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("feature-x", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickText("Stash and switch") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { win.check(git.branch === "feature-x" && changes.changedCount === 0 && git.stashCount === 1, "Stash and switch: on feature-x, clean, 1 stash (" + git.stashCount + ")"); win.clickItem(win.findName("stashButton"), "stash button") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { const e = changes.stashesDialog.entries; win.check(changes.stashesDialog.visible && e.length === 1 && e[0].message === "harlo: before switching to feature-x" && e[0].branch === "main", "stash list shows it (" + JSON.stringify(e.map(x => x.message + " on " + x.branch)) + ")"); win.shotPopup(changes.stashesDialog, "clicks-stashes"); } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Pop") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: win.check(git.stashCount === 0 && changes.changedCount === 1 && changes.stashesDialog.entries.length === 0, "Pop restores README and drops the stash") }
        // F: stash by hand with a message, then drop
        ScriptAction { script: { win.clickItem(win.findName("stashMessage"), "stash message"); TestInput.typeText("try later") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Stash 1 file") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { const e = changes.stashesDialog.entries; win.check(git.stashCount === 1 && e.length === 1 && e[0].message === "try later" && changes.changedCount === 0, "manual stash 'try later'"); win.clickText("Drop") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickText("Discard") }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { win.check(git.stashCount === 0 && changes.stashesDialog.entries.length === 0, "Drop + Discard removes it"); win.clickText("Close") } }
        PauseAnimation { duration: 500 }
        // G: a branch git refuses (checked out in another worktree)
        ScriptAction { script: { win.check(!changes.stashesDialog.visible, "Close closes the stashes dialog"); win.clickItem(win.findName("branchChip"), "branch chip") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("wt-branch", changes.branchPicker.contentItem) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(git.branch === "feature-x" && git.error && git.message !== "", "git's refusal shows as an error, branch unchanged (" + git.message.split("\n")[0] + ")"); win.shot("clicks-error") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("checks:", win.checksPassed, "passed,", win.checksFailed, "failed"); Qt.quit() } }
    }

    // Branch switching and stashes against a scratch repo (`--cwd`, dirty, with a `feature` branch).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "gitbranches"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("start: branch", git.branch, "changed", changes.changedCount, "stashes", git.stashCount); changes.branchPicker.open() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { const x = changes.branchPicker.background.parent.mapToItem(null, 0, 0).x; win.log("picker: left", Math.round(x), "right", Math.round(x + changes.branchPicker.width), "window", win.width) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { const p = changes.branchPicker; win.log("picker items:", p.items.map(i => i.kind + ":" + i.name + (i.current ? "*" : "")).join(" | ")); win.shotPopup(p, "branch-picker") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: changes.branchPicker.query = "fea" }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.log("filtered:", changes.branchPicker.items.map(i => i.kind + ":" + i.name).join(" | ")); win.shotPopup(changes.branchPicker, "branch-picker-search") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.branchPicker.pick(changes.branchPicker.items.findIndex(i => i.name === "feature")) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("dialog open:", changes.switchDialog.visible, "dirty:", changes.switchDialog.dirty); win.shotPopup(changes.switchDialog, "branch-switch-dirty") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.switchDialog.accept() }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after stash+switch: branch", git.branch, "changed", changes.changedCount, "stashes", git.stashCount, "msg:", git.message); win.shot("branch-switched") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.stashesDialog.open() }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("stash entries:", JSON.stringify(changes.stashesDialog.entries)); win.shotPopup(changes.stashesDialog, "stashes") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: git.stashApply(0, true) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after pop: changed", changes.changedCount, "stashes", git.stashCount, "entries", changes.stashesDialog.entries.length, "msg:", git.message); win.shotPopup(changes.stashesDialog, "stashes-popped") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: git.stashPush("selftest stash") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after push: changed", changes.changedCount, "stashes", git.stashCount, "entries", JSON.stringify(changes.stashesDialog.entries)); changes.stashesDialog.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: changes.requestSwitch("topic/new-idea", false, true) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("after create: branch", git.branch, "msg:", git.message); changes.branchPicker.open() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("picker after:", changes.branchPicker.items.map(i => i.name + (i.current ? "*" : "")).join(" | ")); changes.branchPicker.close() } }
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
                win.wtKey = Workspace.activeKey
                win.log("notice:", Workspace.notice, "| error:", Workspace.noticeError, "| active:", win.wtKey, "| cwd:", chat.session.cwd)
                win.log("info:", Workspace.worktreeInfo(win.wtKey))
                let rows = []
                for (let i = 0; Workspace.keyAt(i) !== ""; i++) rows.push(Workspace.keyAt(i))
                win.log("sidebar keys:", rows.join(" , "))
                win.shot("worktree-created")
                Workspace.mergeWorktree(win.wtKey)
            }
        }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { win.log("after merge:", Workspace.notice, "| error:", Workspace.noticeError); Workspace.removeWorktree(win.wtKey, true, false) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: win.log("after remove:", Workspace.notice, "| error:", Workspace.noticeError, "| active:", Workspace.activeKey) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Live resize with the changes panel open: shrink, then grow.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "resize"
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("start", win.width, "side", sidebar.width, "chat", chat.width, "changes", changes.width, "x", changes.x); win.width = 853 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("853:", "side", sidebar.width, "chat", chat.width, "changes", changes.width, "right edge", changes.x + changes.width); win.shot("resize-853") } }
        ScriptAction { script: win.width = 1100 }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("1100:", "side", sidebar.width, "chat", chat.width, "changes", changes.width, "right edge", changes.x + changes.width); win.shot("resize-1100") } }
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
        ScriptAction { script: { pasteProbe.paste(); win.log("pasted:", JSON.stringify(pasteProbe.text)) } }
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
        ScriptAction { script: { win.log("loaded, follow:", chat.transcript.follow, "atEnd:", chat.transcript.atEnd); chat.transcript.scrollBy(-3000) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.log("after 1st scroll up: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd); chat.transcript.scrollBy(-20000) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.log("after 2nd scroll up: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd); win.shot("scrolled-up") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.log("1.5 s later: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd) }
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
        ScriptAction { script: { win.log("opened: tabs", terminal.tabs.length, "running", terminal.current.running); win.newTerminalTab() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.log("after Ctrl+Shift+T: tabs", terminal.tabs.length, "active", terminal.entry.active, "titles", terminal.tabs.map(t => terminal.tabTitle(t)).join(" | ")); terminal.current.session.paste("cd /tmp"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { win.newTerminalTab() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.log("three tabs:", terminal.tabs.length, "active", terminal.entry.active); win.shot("termtabs-three") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { terminal.current.session.paste("exit"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("after exit: tabs", terminal.tabs.length, "active", terminal.entry.active); terminal.activate(0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.log("activated 0: active", terminal.entry.active, "focused", terminal.terminalFocused); win.closeTerminalTab() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("after Ctrl+Shift+W: tabs", terminal.tabs.length, "title", terminal.tabTitle(terminal.current)); win.shot("termtabs-closed") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("other folder:", terminal.folder === Workspace.homeDir(), "tabs", terminal.tabs.length, "folders", Object.keys(terminal.folders).length) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { terminal.current.session.paste("exit"); terminal.current.session.key(Qt.Key_Return, 0, "\r") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("last tab exit: tabs", terminal.tabs.length, "running", terminal.current.running, "exitText", terminal.current.session.exitText); win.shot("termtabs-last-exit") } }
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
        ScriptAction { script: { const s = chat.session; win.log("request:", s.requestMethod, "approval:", s.requestIsApproval, "row:", s.approvalRow, "index:", s.indexOfKey(s.approvalRow)); win.shot("approval-inline") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Approve") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { const s = chat.session; win.log("after approve: hasRequest", s.hasRequest, "row:", JSON.stringify(s.approvalRow), "status", s.status); win.shot("approval-approved") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.submit("fast: approve again", "") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollBy(-3000); win.log("second row:", chat.session.approvalRow) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shot("approval-scrolled") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Deny") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.log("after deny: hasRequest", chat.session.hasRequest, "status", chat.session.status) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shot("approval-denied") }
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
        ScriptAction { script: { const t = chat.transcript; win.log("prompts:", t.prompts.length, "active:", t.activePrompt, "rail visible:", t.rail.visible, "start:", t.rail.start, "first:", JSON.stringify(t.prompts[0])); win.shot("rail-idle") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.openPromptRail() }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("keyboard:", chat.transcript.rail.keyboard, "highlighted:", chat.transcript.rail.highlighted); win.shot("rail-keyboard") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.moveTo(12) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shot("rail-scrubbed") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.select(12) }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { const t = chat.transcript; win.log("after jump to 12: active", t.activePrompt, "follow", t.follow, "keyboard", t.rail.keyboard, "start", t.rail.start); win.shot("rail-jumped") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.stepPrompt(-1) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("Alt+Up: active", chat.transcript.activePrompt); chat.transcript.stepPrompt(1) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("Alt+Down: active", chat.transcript.activePrompt); chat.transcript.scrollBy(300) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.log("partway in: active", chat.transcript.activePrompt); chat.transcript.stepPrompt(-1) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("Alt+Up from partway: active", chat.transcript.activePrompt); chat.transcript.rail.moveTo(59) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("end of list start:", chat.transcript.rail.start); win.shot("rail-end") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { chat.transcript.rail.close(); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.transcript.rail.moveTo(30) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shot("rail-light") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.chooseThemeMode("dark"); win.width = 700 } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.log("narrow: rail width", chat.transcript.rail.width, "gutter", chat.transcript.gutter); win.shot("rail-narrow") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Opens a saved session (`--open=<file>`) at its top, for a look.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "opensaved"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("rows:", chat.transcript.count); chat.transcript.positionAt(Number(win.argValue("--row=") || "0")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.shot("opensaved") }
        PauseAnimation { duration: 600 }
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
            if (++win.wheelSteps >= 15) stop()
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "scrollsaved"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("opened rows:", chat.transcript.count, "follow:", chat.transcript.follow, "atEnd:", chat.transcript.atEnd); wheelSim.start() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.log("after first wheel scroll: follow", chat.transcript.follow, "atEnd", chat.transcript.atEnd) }
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
            const j = win.jitter
            const now = chat.transcript.probe()
            if (j.last && j.last[0] === now[0] && now[0] >= 0) {
                // `last` was taken right after the previous scroll, so any
                // movement since then is a jump nobody asked for.
                const moved = now[1] - j.last[1]
                const extra = Math.abs(moved)
                if (extra > 0.5) { j.bad++; j.worst = Math.max(j.worst, extra) }
                if (extra > 0.5 && j.bad < 10) win.log("jump at step", j.steps, "row", now[0], "moved", moved.toFixed(2), chat.transcript.debugContentY())
            }
            chat.transcript.scrollBy(-40)
            j.last = chat.transcript.probe()
            if (++j.steps >= 400) { stop(); win.log("jitter: steps", j.steps, "jumps", j.bad, "worst", j.worst.toFixed(2), "px"); Qt.quit() }
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "jitter"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: { win.log("rows:", chat.transcript.count, "overscroll after open:", chat.transcript.overscroll().toFixed(1), "px"); jitterSim.start() } }
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
                win.log("opened 3:", win.tabState(), "| titles:", Workspace.tabs.map(t => t.split("\t")[1].substring(0, 24)).join(" / "))
            }
        }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.nextTab(1); win.log("next (wraps):", win.tabState()); Workspace.activateTab(1); win.log("tab 2:", win.tabState()) } }
        ScriptAction { script: { Workspace.closeTab(Workspace.activeKey); win.log("closed active:", win.tabState()) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.shot("tabs"); palette.openWith(""); palette.items.length; } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.log("palette empty query:", palette.items.length, "items, first:", palette.items[0] ? palette.items[0].kind + " " + palette.items[0].title.substring(0, 30) : "-") } }
        ScriptAction { script: { palette.openWith("rock"); win.log("palette 'rock':", palette.items.length, "first:", palette.items[0] ? palette.items[0].kind + " " + palette.items[0].title.substring(0, 40) : "-") } }
        ScriptAction { script: { palette.openWith(">"); win.log("palette '>':", palette.items.length, "actions:", palette.items.slice(0, 4).map(i => i.title).join(" | ")) } }
        ScriptAction { script: palette.openWith("@main") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("palette '@main':", palette.items.length, "first:", palette.items[0] ? palette.items[0].title : "-"); palette.openWith("houdini") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.log("palette 'houdini':", palette.items.length, "visible:", palette.visible); palette.grab(win.selftestDir + "/palette.png") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { palette.close(); shortcutsDialog.open(); win.log("shortcuts visible:", shortcutsDialog.visible, "rows:", shortcutsDialog.rowCount) } }
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
                win.log("context:", r ? ("messages " + r.messages + ", user " + r.user + ", assistant " + r.assistant + ", tokens " + r.contextTokens + ", cost " + r.cost.toFixed(2) + ", rows " + r.rows.length + ", model " + r.model) : "none")
                win.shot("panel-context")
                const more = win.findName("contextShowMore")
                win.log("show more:", more ? more.visible + " " + more.contentItem.text + ", " + more.remaining + " hidden" : "none")
                // The whole list, the row under it included (it's below the fold).
                if (more) more.parent.grabToImage(r => r.saveToFile(win.selftestDir + "/panel-context-list.png"))
            }
        }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const more = win.findName("contextShowMore"); if (more && more.visible) win.clickItem(more, "show more") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { const more = win.findName("contextShowMore"); win.log("after show more:", more ? more.visible + " " + more.contentItem.text + ", " + more.remaining + " hidden" : "none"); win.shot("panel-context-more") } }
        PauseAnimation { duration: 600 }
        ScriptAction {
            script: {
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.newSession(1, "demo")
                windowSettings.rightMode = "files"
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("files: folder", files.browser.folder, "rows", files.browser.rowCount(files.browser.index(-1, -1))); files.browser.toggle(0); Workspace.openFileTab(files.browser.folder + "/src/app.rs") } }
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                win.log("file tab:", Workspace.activeKey.split("/").pop(), "state", chat.fileEditor.state, "language", chat.fileEditor.language, "tabs", Workspace.tabs.length)
                chat.fileEditor.typeText("// edited in harlo\n")
                win.log("dirty:", chat.fileDrafts[chat.filePath] !== undefined)
                win.shot("file-tab")
            }
        }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { chat.fileEditor.save(); win.log("after save, dirty:", chat.fileDrafts[chat.filePath] !== undefined) } }
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
            Workspace.openFileTab(win.switchFiles[win.switchCount % win.switchFiles.length])
            if (win.switchCount % 7 === 3) chat.fileEditor.typeText("x")
            if (++win.switchCount >= 300) { stop(); win.log("switched", win.switchCount, "times without crashing"); Qt.quit() }
        }
    }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "fileswitch"
        PauseAnimation { duration: 800 }
        ScriptAction {
            script: {
                const dir = win.argValue("--cwd=")
                win.switchFiles = ["a.rs", "b.py", "c.md", "d.json", "e.sh"].map(f => dir + "/" + f)
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
        ScriptAction { script: { chat.requestOpenFile(win.argValue("--cwd=") + "/b.py"); win.log("after 2 opens: file tabs", win.fileTabs(), "active", win.activeName()) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { chat.fileEditor.typeText("# edit\n"); chat.requestOpenFile(win.argValue("--cwd=") + "/a.rs"); win.log("edited + open a.rs: prompt", chat.unsavedDialog.visible, "active", win.activeName()) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.unsavedDialog.discarded() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.log("after Discard: active", win.activeName(), "file tabs", win.fileTabs()); chat.fileEditor.typeText("// saved edit\n"); chat.requestOpenFile(win.argValue("--cwd=") + "/pic.png") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.log("edited + open png: prompt", chat.unsavedDialog.visible); chat.unsavedDialog.accept() } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("after Save: active", win.activeName(), "image", chat.fileEditor.imageInfo, "file tabs", win.fileTabs()); win.shot("image-tab") } }
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
                win.log("active", Workspace.activeKey, "state", chat.fileEditor.state, "chars", ed ? ed.text.length : -1, "visible", chat.fileEditor.visible)
                win.shot("restored")
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
                win.log("state", chat.fileEditor.state, "language", chat.fileEditor.language, "editor", !!ed, "chars", ed ? ed.text.length : -1, "lines", ed ? ed.lineCount : -1, "visible", chat.fileEditor.visible)
                win.shot("openfile")
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
            const m = win.mix
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
                if (!ok) { m.bad++; if (m.bad < 6) win.log("MISMATCH on", path.split("/").pop(), "state", fe.state, "message:", fe.shownMessage.substring(0, 90)) }
                mixer.settled = true
                mixer.interval = 10
                mixer.start()
                return
            }
            mixer.settled = false
            const f = m.files[Math.floor(Math.random() * m.files.length)]
            chat.requestOpenFile(f)
            m.n++
            if (m.n >= 240) { win.log("mixswitch:", m.n, "switches,", m.checks, "checks,", m.bad, "mismatches"); Qt.quit(); return }
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
                win.mix.files = ["notes.md", "README.md", "a.rs", "scene.hip", "pic.png", "b.py"].map(f => d + "/" + f)
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
    Timer { id: diskWriteTimer; interval: 300; onTriggered: win.log("  (wrote disk:", diskWriter.save(win.diskWriterText), ")") }
    function editorText() { return chat.fileEditor.editor ? chat.fileEditor.editor.text : "<none>" }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "diskchange"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: chat.requestOpenFile(win.argValue("--cwd=") + "/a.rs") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.writeDisk("// agent edit 1\nfn main() {}\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { win.log("no edits -> reloaded:", win.editorText().startsWith("// agent edit 1"), "bar:", chat.fileEditor.diskBarVisible); chat.fileEditor.typeText("// mine\n") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.writeDisk("// agent edit 2\nfn main() {}\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { win.log("with edits -> bar:", chat.fileEditor.diskBarVisible, "kept mine:", win.editorText().startsWith("// mine")); win.shot("disk-bar") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: chat.fileEditor.keepMine() }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { win.log("after keep mine -> bar:", chat.fileEditor.diskBarVisible); chat.fileEditor.save() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.writeDisk("// agent edit 3\n") }
        PauseAnimation { duration: 2600 }
        ScriptAction { script: { win.log("after save, next agent edit reloads:", win.editorText().startsWith("// agent edit 3"), "bar:", chat.fileEditor.diskBarVisible); Qt.quit() } }
    }

    // The file editor's keys: Enter keeps the indentation, brackets,
    // Tab and Shift+Tab, Backspace by a level, Ctrl+/ comments, find and
    // replace (one undo), go to line, the bracket pair; per file type.
    function edText() { return chat.fileEditor.editor ? chat.fileEditor.editor.text : "" }
    function edLines() { return win.edText().split("\n") }
    function edLine() { const ed = chat.fileEditor.editor; return ed ? ed.text.slice(0, ed.cursorPosition).split("\n").length : -1 }
    function edCol() { const ed = chat.fileEditor.editor; return ed ? ed.cursorPosition - (ed.text.lastIndexOf("\n", ed.cursorPosition - 1) + 1) : -1 }
    function edCount(what) { return win.edText().split(what).length - 1 }
    property string edBefore: ""
    SequentialAnimation {
        id: editorSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickseditor"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/editor.rs") } }
        ScriptAction { script: win.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/editor.rs"), 8000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            win.clickItem(ed, "the editor")
            // Enter after an opening brace: a level deeper.
            ed.cursorPosition = ed.text.indexOf("fn main() {") + 11
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edLines()[1] === "    " && win.edCol() === 4, "Enter after {: the new line a level in (" + JSON.stringify(win.edLines()[1]) + ", col " + win.edCol() + ")")
            TestInput.typeText("let x = 1;")
            win.check(win.edLines()[1] === "    let x = 1;", "…and you type there (" + JSON.stringify(win.edLines()[1]) + ")")
            // Between a pair: the closing one on a line of its own.
            ed.cursorPosition = ed.length
            TestInput.typeText("fn f() {}")
            TestInput.key(Qt.Key_Left, 0)
            TestInput.key(Qt.Key_Return, 0)
            const l = win.edLines(), n = win.edLine()
            win.check(l[n - 2] === "fn f() {" && l[n - 1] === "    " && l[n] === "}" && win.edCol() === 4, "Enter between {}: the } on its own line, the cursor between (" + JSON.stringify(l.slice(n - 2, n + 1)) + ")")
            // A } typed on a line of spaces: lined up with its {.
            TestInput.typeText("if x {")
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edLines()[win.edLine() - 1] === "        ", "a nested block: two levels")
            TestInput.typeText("}")
            win.check(win.edLines()[win.edLine() - 1] === "    }", "} lines up with its { (" + JSON.stringify(win.edLines()[win.edLine() - 1]) + ")")
            // Backspace in the indentation: a level at once.
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edLines()[win.edLine() - 1] === "    ", "Enter after }: the same indentation")
            TestInput.key(Qt.Key_Backspace, 0)
            win.check(win.edLines()[win.edLine() - 1] === "" && win.edCol() === 0, "Backspace: back a level, not a space (" + JSON.stringify(win.edLines()[win.edLine() - 1]) + ")")
            // Tab on selected lines: each a level in, one undo.
            win.edBefore = ed.text
            const s = ed.text.indexOf("fn add("), e = ed.text.indexOf("}\n", s) + 2
            ed.select(s, e)
            TestInput.key(Qt.Key_Tab, 0)
            win.check(ed.text.indexOf("    fn add(a: i32, b: i32) -> i32 {\n        a + b\n    }\n") >= 0, "Tab: the selected lines a level in")
            win.check(ed.selectionStart === s && ed.selectionEnd === ed.text.indexOf("    }\n", s) + 5, "…still selected (" + [ed.selectionStart, ed.selectionEnd] + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            win.check(ed.text === win.edBefore, "Ctrl+Z: undone in one step")
            ed.select(s, e)
            TestInput.key(Qt.Key_Backtab, Qt.ShiftModifier)
            win.check(ed.text.indexOf("fn add(a: i32, b: i32) -> i32 {\na + b\n}") >= 0, "Shift+Tab: a level out (" + JSON.stringify(ed.text.slice(s, s + 44)) + ")")
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            // Ctrl+/: comments the lines out at their indentation, then back.
            ed.select(s, e)
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            win.check(ed.text.indexOf("// fn add(a: i32, b: i32) -> i32 {\n//     a + b\n// }\n") >= 0, "Ctrl+/: commented out (" + JSON.stringify(ed.text.slice(s, s + 50)) + ")")
            win.check(!shortcutsDialog.visible, "…not the shortcuts sheet")
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            win.check(ed.text === win.edBefore, "Ctrl+/ again: back in")
            ed.cursorPosition = 0 } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            // Find: Ctrl+F in a file finds in the file.
            TestInput.key(Qt.Key_F, Qt.ControlModifier)
            const fe = chat.fileEditor
            win.check(fe.findOpen && win.findName("editorFindField", fe).activeFocus && !chat.findBar.open, "Ctrl+F: the file's find, not the session's")
            TestInput.typeText("total") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            const first = ed.text.indexOf("total"), second = ed.text.indexOf("total", first + 1)
            win.check(fe.matches.length === 2 && fe.currentMatch === 0 && ed.selectionStart === first && ed.selectionEnd === first + 5, "typed: both found, the first selected (" + fe.matches.length + ")")
            win.check(win.findName("editorFindCount", fe).text === "1 of 2", "1 of 2 (" + win.findName("editorFindCount", fe).text + ")")
            win.check(ed.visibleMatches() === 2, "both marked in the text (" + ed.visibleMatches() + ")")
            win.shot("editor-find")
            TestInput.key(Qt.Key_Return, 0)
            win.check(fe.currentMatch === 1 && ed.selectionStart === second, "Enter: the next")
            TestInput.key(Qt.Key_Return, Qt.ShiftModifier)
            win.check(fe.currentMatch === 0 && ed.selectionStart === first, "Shift+Enter: back")
            win.clickItem(win.findName("editorMatchCase", fe), "Aa")
            win.findName("editorFindField", fe).forceActiveFocus()
            win.findName("editorFindField", fe).selectAll()
            TestInput.typeText("TOTAL") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor
            win.check(fe.matchCase && fe.matches.length === 0 && win.findName("editorFindCount", fe).text === "None", "Aa: TOTAL isn't total (" + win.findName("editorFindCount", fe).text + ")")
            win.clickItem(win.findName("editorMatchCase", fe), "Aa")
            win.findName("editorFindField", fe).forceActiveFocus() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor
            win.check(!fe.matchCase && fe.matches.length === 2, "Aa off: found again")
            // Replace: Ctrl+H, then All.
            TestInput.key(Qt.Key_H, Qt.ControlModifier)
            win.check(fe.replaceOpen && win.findName("editorReplaceField", fe).activeFocus, "Ctrl+H: the replace line, its field focused")
            TestInput.typeText("sum") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            win.shot("editor-replace")
            win.edBefore = ed.text
            win.clickItem(win.findName("editorReplaceAll", fe), "All")
            win.check(win.edCount("total") === 0 && win.edCount("sum") === 2, "All: both replaced (" + win.edCount("sum") + ")")
            win.check(win.findName("editorFindCount", fe).text === "Replaced 2", "…and says so (" + win.findName("editorFindCount", fe).text + ")")
            ed.forceActiveFocus()
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            win.check(ed.text === win.edBefore, "Ctrl+Z: every replacement undone at once") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            win.check(fe.matches.length === 2, "the matches found again after the undo (" + fe.matches.length + ")")
            win.clickItem(win.findName("editorFindNext", fe), "next")
            win.clickItem(win.findName("editorReplaceField", fe), "the replace field")
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edCount("total") === 1 && win.edCount("sum") === 1 && fe.matches.length === 1, "Enter in Replace: just the one shown (" + win.edCount("sum") + ")")
            TestInput.key(Qt.Key_Escape, 0)
            win.check(!fe.findOpen && !win.findName("editorFind", fe) && ed.activeFocus, "Esc: find closed, back in the text")
            win.check(ed.visibleMatches() === 0, "…its marks gone") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            // Go to line: Ctrl+G, or the line count in the header.
            TestInput.key(Qt.Key_G, Qt.ControlModifier)
            win.check(fe.lineOpen && win.findName("editorLineField", fe).activeFocus, "Ctrl+G: go to line")
            win.shot("editor-goto")
            TestInput.typeText("7")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            win.check(!fe.lineOpen && win.edLine() === 7 && ed.activeFocus, "…line 7 (" + win.edLine() + ")")
            win.clickItem(win.findName("editorLineLabel", fe), "Ln")
            win.check(fe.lineOpen, "the header's Ln opens it too")
            TestInput.key(Qt.Key_Escape, 0)
            win.check(!fe.lineOpen, "Esc closes it")
            // The bracket by the cursor and its partner.
            ed.forceActiveFocus()
            const open = ed.text.indexOf("fn add(") + 6, close = ed.text.indexOf(")", open)
            ed.cursorPosition = open } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            const open = ed.text.indexOf("fn add(") + 6, close = ed.text.indexOf(")", open)
            const marks = JSON.parse(ed.bracketPair())
            win.check(marks.length === 2 && marks[0] === open && marks[1] === close, "( and its ) marked (" + marks + " vs " + [open, close] + ")")
            win.shot("editor-brackets")
            ed.cursorPosition = ed.text.indexOf("let x")
            TestInput.key(Qt.Key_S, Qt.ControlModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(chat.fileEditor.editor.bracketPair() === "[]", "no bracket by the cursor: no marks")
            win.check(TestInput.readFile(win.argValue("--cwd=") + "/src/editor.rs") === win.edText() && !chat.fileEditor.modified, "Ctrl+S saves it")
            win.check(win.composerKeys.some(k => k.group === "File editor" && k.seq === "Ctrl+/"), "the shortcuts sheet lists the editor's keys")
            // A file indented by two spaces: Enter adds two.
            Workspace.openFileTab(win.argValue("--cwd=") + "/web/app.js") } }
        ScriptAction { script: win.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/app.js"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            ed.forceActiveFocus()
            ed.cursorPosition = ed.text.indexOf("function show() {") + 17
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edLines()[1] === "  " && win.edCol() === 2, "two-space JavaScript: Enter adds two (" + JSON.stringify(win.edLines()[1]) + ")")
            TestInput.key(Qt.Key_Tab, 0)
            win.check(win.edLines()[1] === "    ", "…and Tab two more (" + JSON.stringify(win.edLines()[1]) + ")")
            ed.cursorPosition = 0
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            win.check(win.edLines()[0] === "// function show() {", "// for JavaScript (" + win.edLines()[0] + ")")
            Workspace.openFileTab(win.argValue("--cwd=") + "/tool.py") } }
        ScriptAction { script: win.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/tool.py"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            ed.forceActiveFocus()
            ed.cursorPosition = ed.text.indexOf("def main():") + 11
            TestInput.key(Qt.Key_Return, 0)
            win.check(win.edLines()[1] === "    ", "Python: a level in after a colon (" + JSON.stringify(win.edLines()[1]) + ")")
            ed.cursorPosition = ed.text.indexOf("print")
            TestInput.key(Qt.Key_Slash, Qt.ControlModifier)
            win.check(win.edText().indexOf("    # print(\"hi\")") >= 0, "# for Python, at the line's indentation")
            // Back to app.js (left with unsaved edits): they're still there,
            // and so is their undo.
            Workspace.openFileTab(win.argValue("--cwd=") + "/web/app.js") } }
        ScriptAction { script: win.waitUntil(editorSteps, () => !!chat.fileEditor.editor && chat.filePath.endsWith("/app.js"), 8000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fe = chat.fileEditor, ed = fe.editor
            win.check(fe.modified && win.edLines()[0] === "// function show() {" && win.edLines()[1] === "    ", "back to a file with unsaved edits: they're kept (" + JSON.stringify(win.edLines()[0]) + ")")
            win.check(chat.fileDrafts[chat.filePath] === true && chat.fileDrafts[win.argValue("--cwd=") + "/tool.py"] === true, "…and both files are marked unsaved")
            ed.forceActiveFocus()
            TestInput.key(Qt.Key_Z, Qt.ControlModifier)
            win.check(win.edLines()[0] === "function show() {", "…and so is their undo (" + JSON.stringify(win.edLines()[0]) + ")")
            win.summary(); Qt.quit() } }
    }

    // A big file (8000 lines): its diff from the Changes panel, then the
    // file itself, scrolled; then 100,000 lines, typed at the end. The code
    // views (drawn in Rust) show at once and colour in the background.
    property real perfT0: 0
    property real perfLast: 0
    property var perfMarks: ({})
    property var perfStalls: []
    function perfStart() { win.perfT0 = Date.now(); win.perfLast = win.perfT0; win.perfMarks = {}; win.perfStalls = []; perfPoll.start() }
    function perfLongest() { return win.perfStalls.reduce((m, s) => Math.max(m, s.ms), 0) }
    Timer {
        id: perfPoll
        interval: 5
        repeat: true
        onTriggered: {
            const now = Date.now(), t = now - win.perfT0, m = win.perfMarks
            if (now - win.perfLast > 40) win.perfStalls.push({ at: Math.round(t - (now - win.perfLast)), ms: now - win.perfLast })
            win.perfLast = now
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
        ScriptAction { script: { win.perfStart(); win.clickText("big.rs", changes) } }
        ScriptAction { script: win.waitUntil(bigFileSteps, () => win.perfMarks.jumped !== undefined, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop()
            const dv = chat.diffView, m = win.perfMarks
            console.log("PERF big diff: marks", JSON.stringify(m), "stalls", JSON.stringify(win.perfStalls))
            win.check(m.jumped !== undefined, "the diff opened at its first change (" + m.jumped + " ms)")
            win.check(m.jumped < 300 && win.perfLongest() < 200, "the window kept answering: longest hold " + win.perfLongest() + " ms (it was 5100; then 1900)")
            win.check(dv.rightPane.editor.text === dv.rightPane.text && dv.leftPane.editor.text === dv.leftPane.text && dv.rightPane.editor.lineCount >= 8000, "both sides all in (" + dv.rightPane.editor.lineCount + " lines)")
            const f = dv.rightPane.flick, y = dv.blocks[0].start * dv.lineHeight
            win.check(y >= f.contentY && y <= f.contentY + f.height, "the first change in view")
            win.shot("bigfile-diff")
            win.perfStart()
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/big.rs") } }
        ScriptAction { script: win.waitUntil(bigFileSteps, () => win.perfMarks.fileReady !== undefined, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop()
            const fe = chat.fileEditor, m = win.perfMarks
            console.log("PERF big file: marks", JSON.stringify(m), "stalls", JSON.stringify(win.perfStalls))
            win.check(m.fileReady !== undefined && m.fileShown < 150 && win.perfLongest() < 100, "the file shown in " + m.fileShown + " ms, the window answering: longest hold " + win.perfLongest() + " ms")
            win.check(fe.editor.text === TestInput.readFile(win.argValue("--cwd=") + "/src/big.rs") && !fe.modified, "all of it in, and not an edit")
            win.check(fe.language === "Rust" && !fe.editor.readOnly, "coloured, and editable once in (" + fe.language + ")")
            win.shot("bigfile-editor")
            // Scrolling through it: each frame drawn quickly, the window
            // never held.
            win.cvFrames = []
            cvScroll.n = 0
            win.perfStart()
            cvScroll.start() } }
        ScriptAction { script: win.waitUntil(bigFileSteps, () => !cvScroll.running, 10000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            perfPoll.stop()
            const f = win.cvFrames.slice(2), ed = chat.fileEditor.editor
            const avg = f.reduce((a, b) => a + b, 0) / f.length, max = Math.max.apply(null, f)
            console.log("PERF big file scroll: frames", f.length, "avg", avg.toFixed(2), "ms max", max.toFixed(2), "ms; stalls", JSON.stringify(win.perfStalls))
            win.check(ed.contentY > 5000 && max < 8, "scrolling: every frame drawn in under 8 ms (max " + max.toFixed(2) + " ms)")
            win.check(win.perfLongest() < 50, "…and the window never held (longest " + win.perfLongest() + " ms)")
            // 100,000 lines.
            win.perfStart()
            Workspace.openFileTab(win.argValue("--cwd=") + "/src/huge.rs") } }
        ScriptAction { script: win.waitUntil(bigFileSteps, () => win.perfMarks.fileReady !== undefined && chat.fileEditor.editor && chat.fileEditor.editor.path.endsWith("/huge.rs"), 15000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            perfPoll.stop()
            const fe = chat.fileEditor, ed = fe.editor, m = win.perfMarks
            console.log("PERF huge file: lines", ed.lineCount, "marks", JSON.stringify(m), "stalls", JSON.stringify(win.perfStalls))
            win.check(ed.lineCount >= 100000, "100,000 lines in (" + ed.lineCount + ")")
            win.check(m.fileShown < 250, "…shown in " + m.fileShown + " ms")
            win.check(win.perfLongest() < 150, "…the window held at most " + win.perfLongest() + " ms")
            // Typing at the very end.
            ed.forceActiveFocus()
            ed.cursorPosition = ed.length
            const t0 = Date.now()
            TestInput.typeText("// the end.")
            const ms = (Date.now() - t0) / 11
            console.log("PERF huge file typing:", ms.toFixed(2), "ms a key")
            win.check(ed.text.endsWith("// the end.") && fe.modified, "typing at the end of 100,000 lines")
            win.check(ms < 16, "…" + ms.toFixed(1) + " ms a key")
            win.check(ed.colouring, "(colouring in order hasn't got this far)") } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const ed = chat.fileEditor.editor
            win.check(ed.lineColoured(ed.lineCount - 3) && ed.lineColoured(ed.lineCount - 30), "…yet the lines on screen are coloured at once")
            win.shot("bigfile-huge-end")
            win.summary(); Qt.quit() } }
    }

    // The terminal under a flood of output: 20,000 coloured lines, then 400
    // full screens redrawn in place. The window keeps answering and the
    // screen keeps up.
    property int termFrames: 0
    property real termT0: 0
    Connections {
        target: win
        enabled: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminalspeed"
        function onFrameSwapped() { win.termFrames++ }
    }
    function termRun(command) {
        const t = terminal.current.session
        win.termFrames = 0
        win.termT0 = Date.now()
        win.perfStart()
        win.termTicks = 0
        win.termPaintMax = 0
        win.termPaints = []
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
            win.termTicks++
            const ms = terminal.current.session.paintMs
            if (ms !== win.termPaintLast) { win.termPaints.push(Math.round(ms * 10) / 10); win.termPaintLast = ms }
            win.termPaintMax = Math.max(win.termPaintMax, ms)
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
        const ms = Number(TestInput.readFile(win.argValue("--cwd=") + "/" + file).trim()) - win.termT0
        const free = Math.round(100 * win.termTicks * 5 / (Date.now() - win.termT0))
        console.log("PERF terminal " + what + ":", ms, "ms,", win.termFrames, "frames, slowest drawn in", win.termPaintMax.toFixed(2), "ms, window free", free + "%, longest hold", win.perfLongest(), "ms; stalls", JSON.stringify(win.perfStalls), "frames drawn in", JSON.stringify(win.termPaints))
        win.check(win.perfLongest() < limit, what + ": the window held at most " + win.perfLongest() + " ms (" + ms + " ms in all, " + win.termFrames + " frames; it held 120–360 ms a frame when rows were rich text)")
        win.check(win.termPaintMax < 30, what + ": every frame drawn in under 30 ms (slowest " + win.termPaintMax.toFixed(1) + ")")
    }
    SequentialAnimation {
        id: termSpeedSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksterminalspeed"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { windowSettings.terminalHeight = win.height - 120; win.clickItem(win.findName("composerInput"), "composer"); TestInput.key(Qt.Key_QuoteLeft, Qt.ControlModifier) } }
        ScriptAction { script: win.waitUntil(termSpeedSteps, () => terminal.visible && terminal.current !== null && terminal.current.running, 5000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.termRun("awk 'BEGIN{for(i=1;i<=20000;i++) printf \"\\033[3%dm%06d\\033[0m the quick brown fox jumps over the lazy dog \\033[1m%d\\033[0m\\n\", i%8, i, i*i}'; date +%s%3N > stream.done") }
        ScriptAction { script: win.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/stream.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.termReport("20,000 lines", 100, "stream.done")
            const screen = win.termScreen()
            win.check(screen.indexOf("020000thequickbrownfoxjumpsoverthelazydog400000000") >= 0, "the last line drawn whole (" + screen.slice(-120) + ")")
            win.shot("terminal-stream") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.termRun("clear; awk -v R=$LINES -v C=$COLUMNS 'BEGIN{for(f=0;f<400;f++){printf \"\\033[H\"; for(r=0;r<R-1;r++){for(c=0;c<int(C/10);c++) printf \"\\033[3%dm%-9d\\033[0m \", (r+c+f)%8, f*r+c; printf \"\\n\"}}}'; date +%s%3N > redraw.done") }
        ScriptAction { script: win.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/redraw.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.termReport("400 full screens", 100, "redraw.done"); win.shot("terminal-redraw") } }
        PauseAnimation { duration: 300 }
        // Every cell its own colours (a busy full-screen program).
        ScriptAction { script: win.termRun("clear; awk -v R=$LINES -v C=$COLUMNS 'BEGIN{for(f=0;f<200;f++){printf \"\\033[H\"; for(r=0;r<R-1;r++){for(c=0;c<C;c++) printf \"\\033[38;5;%d;48;5;%dm%c\", (r+c+f)%256, (r*3+c+f*2)%256, 33+(r+c+f)%90; printf \"\\033[0m\\n\"}}}'; date +%s%3N > cells.done") }
        ScriptAction { script: win.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/cells.done") !== "", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.termReport("200 screens of coloured cells", 100, "cells.done"); win.shot("terminal-cells") } }
        PauseAnimation { duration: 300 }
        // Boxes, blocks, styles, wide characters.
        ScriptAction { script: win.termRun("clear; printf '╭──┬──╮ ┏━━┓ ╔══╗\\n│ab│cd│ ┃  ┃ ║  ║\\n├──┼──┤ ┗━━┛ ╚══╝\\n╰──┴──╯ ▀▄█▌▐░▒▓ ▖▗▘▝▙▟\\n\\033[1mbold\\033[0m \\033[3mitalic\\033[0m \\033[4munderline\\033[0m \\033[9mstrike\\033[0m \\033[7minverse\\033[0m \\033[2mdim\\033[0m\\n漢字 テスト ✓ → é ü\\n'; date +%s%3N > glyphs.done") }
        ScriptAction { script: win.waitUntil(termSpeedSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/glyphs.done") !== "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            perfPoll.stop(); termTick.stop()
            const screen = win.termScreen()
            win.check(screen.indexOf("╭──┬──╮┏━━┓╔══╗") === 0 && screen.indexOf("漢字テスト✓→éü") > 0, "boxes and wide characters in their cells (" + screen + ")")
            win.shot("terminal-glyphs"); win.summary(); Qt.quit() } }
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
        const v = view || win.imgView()
        const top = v.mapToItem(null, v.width / 2, -2), below = v.mapToItem(null, v.width / 2, v.height + 12)
        return [TestInput.colourAt(top.x, top.y), TestInput.colourAt(below.x, below.y)]
    }
    property int imgFrames: 0
    property bool imgSawPreview: false
    property var imgStats: null
    function imgView() { return chat.fileEditor.imageViewer.view }
    function imgOpen(name) { win.imgT0 = Date.now(); win.imgSawPreview = false; Workspace.openFileTab(win.imgDir + "/" + name) }
    function imgReady(name) { const v = win.imgView(); return v.path === win.imgDir + "/" + name && v.status === "ready" }
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
    function imgThumbsReady() { const t = win.imgThumbs(); return t.length > 0 && t.every(c => c.status === "ready") }
    Connections {
        target: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksimages" ? chat.fileEditor.imageViewer.view : null
        function onFrameChanged() { win.imgFrames++ }
        function onStatusChanged() { if (win.imgView().status === "preview") win.imgSawPreview = true }
    }
    SequentialAnimation {
        id: imgSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksimages"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); Workspace.newSession(1, "demo"); if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 1000 }
        // A big one: more than a texture's side; a JPEG shows a copy at the
        // screen's size first.
        ScriptAction { script: win.imgBig = TestInput.readFile(win.imgDir + "/big.png") !== "" ? "big.png" : "big.jpg" }
        ScriptAction { script: win.imgOpen(win.imgBig) }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady(win.imgBig), 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.imgView()
            console.log("PERF image 9000x6000 " + win.imgBig + ": first on screen", v.firstMs.toFixed(1), "ms, whole", v.loadMs.toFixed(1), "ms")
            win.check(v.imageWidth === 9000 && v.imageHeight === 6000, "the big image whole: " + v.imageWidth + "×" + v.imageHeight + " (" + v.status + ")")
            if (win.imgBig === "big.jpg")
                win.check(win.imgSawPreview && v.firstMs < v.loadMs, "a copy at the screen's size first (" + v.firstMs.toFixed(0) + " ms), then the whole (" + v.loadMs.toFixed(0) + " ms)")
            win.check(v.fit && v.zoom < 0.2, "fitted to the view (" + (v.zoom * 100).toFixed(1) + "%)")
            win.imgOpen("img1.png") } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.imgView()
            const viewer = chat.fileEditor.imageViewer
            console.log("PERF image 1200x800:", v.loadMs.toFixed(1), "ms")
            win.check(v.loadMs < 1000, "a 1200×800 image on screen in " + v.loadMs.toFixed(0) + " ms")
            win.check(v.activeFocus, "the image has the keys")
            const dpr = win.imgDpr()
            const fitZoom = Math.min(1, v.width * dpr / 1200, v.height * dpr / 800)
            win.check(v.fit && Math.abs(v.zoom - fitZoom) < 1e-6, "fitted: " + v.zoom.toFixed(4) + " (" + fitZoom.toFixed(4) + ")")
            win.shotWindow("clicks-images-tab")
            const pathLabel = win.findName("editorPath", chat.fileEditor)
            const outside = win.findName("editorOpenOutside", chat.fileEditor)
            win.check(outside !== null && outside.iconName === "external-link", "opening it outside is a drawn icon")
            win.check(pathLabel && pathLabel.text === "pics/img1.png" && pathLabel.whole.endsWith("/pics/img1.png"), "the tab's path is the file's in its project, the whole on hover (" + (pathLabel ? pathLabel.text : "none") + ")")
            const tight = viewer.width < 640
            const more = win.findName("imageMore", viewer), turn = win.findName("imageTurnLeft", viewer)
            win.check(tight ? (more !== null && turn === null) : (more === null && turn !== null), (tight ? "narrow, turning and the grid go into the ⋯ menu" : "wide, the bar shows turning, no ⋯ menu") + " (more " + (more !== null) + ", turn " + (turn !== null) + ")")
            const zoom = win.findName("imageZoom", viewer)
            const barRight = zoom ? zoom.parent.mapToItem(viewer, zoom.parent.width, 0).x : -1
            win.check(barRight > 0 && barRight <= viewer.width + 0.5, "the bar fits its view (" + barRight.toFixed(0) + " of " + viewer.width.toFixed(0) + ")")
            if (tight) viewer.moreMenu.popup(more, 0, 0)
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            if (viewer.width < 640)
                win.clickItem(win.findName("imageKeys", viewer.moreMenu.contentItem), "Keyboard shortcuts")
        } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer, v = win.imgView()
            if (viewer.width < 640) {
                win.check(shortcutsDialog.visible, "⋯ → Keyboard shortcuts opens the sheet")
                shortcutsDialog.close()
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.imgView(), viewer = chat.fileEditor.imageViewer
            viewer.focusView()
            win.check(viewer.images.length === 33 && viewer.images[2] === win.imgDir + "/img1.png" && viewer.images[3] === win.imgDir + "/img2.png" && viewer.images[11] === win.imgDir + "/img10.png",
                      "the folder's 33 images in natural order (" + viewer.images.slice(0, 5).map(p => p.split("/").pop()).join(" ") + " …), not the text file")
            // Four wheel notches at a point left of the middle.
            const x = v.width * 0.35, y = v.height * 0.5
            win.imgBefore = JSON.parse(v.imagePointAt(x, y))
            win.imgZoom = v.zoom
            win.imgEdges = win.imgEdgeColours()
            const p = v.mapToItem(null, x, y)
            for (let i = 0; i < 4; i++) TestInput.wheel(p.x, p.y, 120) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = win.imgView()
            const after = JSON.parse(v.imagePointAt(v.width * 0.35, v.height * 0.5))
            const moved = Math.hypot(after[0] - win.imgBefore[0], after[1] - win.imgBefore[1])
            win.check(Math.abs(v.zoom / win.imgZoom - Math.pow(1.25, 4)) < 0.01, "four wheel notches zoom ×2.44 (" + (v.zoom / win.imgZoom).toFixed(3) + ")")
            win.check(moved < 1, "the point under the pointer stays under it (moved " + moved.toFixed(2) + " image px)")
            win.check(!v.fit, "zoomed, it no longer follows the view's size")
            TestInput.key(Qt.Key_1, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(Math.abs(v.zoom - 1) < 1e-9 && v.pixelated === false, "1: actual size (" + v.zoom + ")")
            const tl = JSON.parse(v.imagePointAt(0, 0))
            win.check(Math.abs(tl[0] - Math.round(tl[0])) < 1e-6 && Math.abs(tl[1] - Math.round(tl[1])) < 1e-6, "at actual size the pixels sit on whole screen pixels (" + tl + ")")
            TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(Math.abs(v.zoom - 1.5) < 1e-6, "+ zooms a step, to 150% (" + v.zoom + ")")
            TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(Math.abs(v.zoom - 2) < 1e-6 && v.pixelated, "the next step is 200%, pixels drawn as squares (" + v.zoom + ", " + v.pixelated + ")")
            win.shotWindow("clicks-images-zoomed")
            const edges = win.imgEdgeColours()
            win.check(edges[0] !== "" && edges[0] === win.imgEdges[0] && edges[1] === win.imgEdges[1],
                      "bigger than its view, the image is drawn only inside it: not over the bar below or what is above (" + edges + ", fitted " + win.imgEdges + ")")
            v.pan(-100000, -100000)
            const br = JSON.parse(v.imagePointAt(v.width - 0.01, v.height - 0.01))
            win.check(Math.abs(br[0] - 1200) < 0.1 && Math.abs(br[1] - 800) < 0.1, "dragged far, it stops at the image's corner (" + br + ")")
            TestInput.key(Qt.Key_0, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.imgView()
            const dpr = win.imgDpr()
            win.check(v.fit && Math.abs(v.zoom - Math.min(1, v.width * dpr / 1200, v.height * dpr / 800)) < 1e-6, "0 fits it again (" + v.zoom.toFixed(4) + ")")
            TestInput.key(Qt.Key_R, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.imgView()
            const dpr = win.imgDpr()
            win.check(v.turn === 90 && Math.abs(v.zoom - Math.min(1, v.width * dpr / 800, v.height * dpr / 1200)) < 1e-6, "R turns it right and fits it turned (" + v.turn + "°, " + v.zoom.toFixed(4) + ")")
            const c = JSON.parse(v.imagePointAt(v.width / 2, v.height / 2))
            win.check(Math.abs(c[0] - 600) < 0.6 && Math.abs(c[1] - 400) < 0.6, "turned about its middle (" + c + ")")
            TestInput.key(Qt.Key_R, Qt.ShiftModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.imgView().turn === 0, "Shift+R turns it back")
            // (img2 was decoded ahead while this one was looked at.)
            win.imgT0 = Date.now()
            TestInput.key(Qt.Key_Right, 0) } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("img2.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = win.imgView()
            console.log("PERF image next (decoded ahead):", v.loadMs.toFixed(2), "ms")
            win.check(v.loadMs < 30, "→ steps to img2, decoded ahead: on screen in " + v.loadMs.toFixed(1) + " ms")
            win.check(Workspace.activeKey === "file:" + win.imgDir + "/img2.png" && v.fit && v.turn === 0, "the file tab now shows img2, fitted")
            TestInput.key(Qt.Key_K, 0) } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(win.imgView().loadMs < 30, "K steps back, from memory (" + win.imgView().loadMs.toFixed(1) + " ms)")
            win.imgOpen("anim.gif") } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("anim.gif"), 10000) }
        ScriptAction { script: win.imgFrames = 0 }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(v.frameCount === 3 && v.playing && win.imgFrames >= 4, "a GIF plays: " + v.frameCount + " frames, " + win.imgFrames + " shown in 0.9 s")
            TestInput.key(Qt.Key_P, 0)
            win.imgFrames = 0 } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(!v.playing && win.imgFrames === 0, "P pauses it (" + win.imgFrames + " frames since)")
            TestInput.key(Qt.Key_Period, 0) } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            win.check(win.imgFrames === 1, ". shows the next frame (" + win.imgView().frame + ")")
            win.imgOpen("logo.png") } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("logo.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = win.imgView()
            const dpr = win.imgDpr()
            win.check(v.fit && Math.abs(v.zoom - Math.min(1, v.width * dpr / 400, v.height * dpr / 400)) < 1e-6, "a small image isn't enlarged to fit (" + v.zoom + ")")
            win.imgT0 = Date.now()
            win.imgStats = JSON.parse(Images.stats())
            TestInput.key(Qt.Key_G, 0) } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgThumbsReady(), 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            const shown = win.imgThumbs().length
            const st = JSON.parse(Images.stats())
            console.log("PERF image grid:", shown, "thumbnails made in", Date.now() - win.imgT0, "ms;", JSON.stringify(st))
            win.check(viewer.gridMode && viewer.grid.count === 33 && viewer.grid.activeFocus, "G shows the folder's 33 images as a grid, with the keys")
            win.check(win.imgThumbsReady(), "every thumbnail in view is drawn (" + shown + ")")
            win.check(st.diskThumbs >= shown - 1 && st.thumbsMade - win.imgStats.thumbsMade >= shown - 1, "they're kept on disk (" + st.diskThumbs + " files)")
            win.check(viewer.grid.currentIndex === 32, "the grid starts at the image shown (" + viewer.grid.currentIndex + ")")
            win.check(Math.abs(viewer.gridShape - 800 / 1200) < 0.01 && viewer.grid.cellHeight < viewer.grid.cellWidth, "its cells are shaped like most of its pictures, 3:2 (" + viewer.gridShape.toFixed(3) + ", " + viewer.grid.cellWidth + "×" + viewer.grid.cellHeight + ")")
            win.shotWindow("clicks-images-grid")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const viewer = chat.fileEditor.imageViewer
            win.check(!viewer.gridMode && win.imgView().activeFocus, "Esc goes back to the image")
            // Again, with only the disk cache.
            Images.forgetMemory()
            win.imgStats = JSON.parse(Images.stats())
            win.imgT0 = Date.now()
            TestInput.key(Qt.Key_G, 0) } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgThumbsReady(), 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const st = JSON.parse(Images.stats())
            const ms = Date.now() - win.imgT0
            console.log("PERF image grid from the disk cache:", ms, "ms;", JSON.stringify(st))
            const read = st.thumbsFromDisk - win.imgStats.thumbsFromDisk
            win.check(read >= win.imgThumbs().length - 1, "opened again, the thumbnails come from disk (" + read + " read, " + (st.thumbsMade - win.imgStats.thumbsMade) + " made: those just past the view the first time)")
            const g = chat.fileEditor.imageViewer.grid
            g.currentIndex = 0
            TestInput.key(Qt.Key_L, 0)
            TestInput.key(Qt.Key_L, 0)
            TestInput.key(Qt.Key_Return, 0) } }
        ScriptAction { script: win.waitUntil(imgSteps, () => win.imgReady("img1.png"), 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(!chat.fileEditor.imageViewer.gridMode && Workspace.activeKey === "file:" + win.imgDir + "/img1.png", "L L Enter opens the third image (" + Workspace.activeKey.split("/").pop() + ")")
            TestInput.key(Qt.Key_F, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(chat.fileEditor.imageViewer.filled && v.width === win.width, "F fills the window (" + v.width + " of " + win.width + ")")
            const back = win.findName("imageFill")
            win.check(back !== null && back.iconName === "fullscreen-exit", "and the way back shows in its bar, narrow too (" + (back ? back.iconName : "none") + ")")
            win.shotWindow("clicks-images-filled")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.imgView()
            win.check(!chat.fileEditor.imageViewer.filled && v.width < win.width && v.activeFocus, "Esc puts it back in the tab")
            windowSettings.rightMode = "files" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            // File-type icons: glyphs of the bundled font, in their type's colour.
            const icons = win.findAllNames("fileIcon", win.contentItem).filter(i => i.visible && i.text !== "")
            win.check(icons.length >= 3, "file icons in the tabs and the files panel (" + icons.length + ")")
            win.check(icons.every(i => i.fontInfo.family === "Harlo File Icons" && i.contentWidth > 6 && i.contentHeight >= 12),
                      "drawn from the bundled icon font (" + icons.map(i => i.fontInfo.family + " " + i.contentWidth.toFixed(0) + "×" + i.contentHeight.toFixed(0)).slice(0, 3).join(", ") + ")")
            const rs = Workspace.fileIcon("src/main.rs"), toml = Workspace.fileIcon("Cargo.toml"), png = Workspace.fileIcon("a.png")
            win.check(rs.startsWith("#ff7043") && rs !== toml && toml !== png && png.length >= 8, "each type its own glyph and colour (" + [rs, toml, png].map(t => t.substring(0, 7)).join(" ") + ")")
            const pale = Theme.fileTint("#ffd54f")
            const contrast = (Math.max(Theme.relativeLuminance(pale), Theme.relativeLuminance(Theme.background)) + 0.05) / (Math.min(Theme.relativeLuminance(pale), Theme.relativeLuminance(Theme.background)) + 0.05)
            win.check(contrast >= 2.9, "a pale colour still stands out from the background (" + contrast.toFixed(2) + ":1)")
            win.summary(); Qt.quit() } }
    }

    // Pictures in the chat: thumbnails for one you sent, one a tool
    // returned, a file a command wrote, a reply's Markdown images (one gone
    // from disk) and one a model made; a click opens the viewer over the
    // chat, stepping through them all; Open in tab; the composer's.
    function picTiles() { return win.findAllNames("pictureTile", chat.transcript).filter(t => win.shown(t)) }
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
            win.clickItem(win.findText("Make me a sun picture", sidebar), "the session") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.waitUntil(picSteps, () => win.picTiles().length >= 8 && win.picTiles().every(t => t.thumb.status === "ready" || t.thumb.status === "error"), 10000) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const tiles = win.picTiles()
            const names = tiles.map(t => t.modelData.split("/").pop())
            win.log("tiles:", tiles.map(t => t.modelData.split("/").pop() + "=" + t.thumb.status).join(" "), "all:", win.findAllNames("pictureTile", chat.transcript).length, "pictures:", chat.session.sessionPictures())
            win.check(tiles.length === 8, "8 thumbnails: sent, returned, written, two Markdown images, a named file, made, pasted (" + names.join(", ") + ")")
            const gone = tiles.filter(t => t.thumb.status === "error")
            win.check(gone.length === 1 && gone[0].modelData.endsWith("/art/gone.png") && win.findText("No longer on disk", gone[0]) !== null, "a picture gone from disk says so")
            win.check(tiles.filter(t => t.thumb.status === "ready").length === 7, "the rest are drawn")
            win.shotWindow("clicks-pictures-thumbs")
            win.check(win.findText("paste-1.png", chat.transcript) === null, "a pasted picture shows as itself, not its file's name")
            win.check(win.findContaining("a yellow sun", chat.transcript) !== null, "a Markdown image's words stay in the reply")
            const all = JSON.parse(chat.session.sessionPictures())
            win.picAll = all
            win.clickItem(tiles.find(t => t.modelData === all[1]), "the returned picture") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(picSteps, () => win.picViewer().view.status === "ready", 5000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = win.picViewer()
            win.check(chat.transcript.chatPictures.opened && v.view.imageWidth === 800 && v.view.activeFocus, "a click opens it over the chat (" + v.view.imageWidth + "×" + v.view.imageHeight + ", keys " + v.view.activeFocus + ")")
            win.check(v.images.length === 7 && v.index === 1, "it steps through the chat's 7 pictures, this the 2nd (" + (v.index + 1) + " / " + v.images.length + ")")
            win.check(v.view.width >= chat.transcript.width - 1, "over the whole conversation")
            win.shotWindow("clicks-pictures-viewer")
            const origin = win.findName("imageOrigin", v), more = win.findName("imageMore", v), close = win.findName("imageClose", v)
            if (v.width < 640) {
                win.check(origin === null && more !== null && close !== null && close.mapToItem(v, 0, 0).x > more.mapToItem(v, 0, 0).x, "narrow, the bar has ⋯ and close stays last, after it")
                v.moreMenu.popup(more, 0, 0)
            } else {
                win.check(origin !== null && origin.text.startsWith("From "), "the bar says where it came from (" + (origin ? origin.text : "none") + ")")
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.picViewer()
            if (v.width < 640) {
                const item = win.findName("imageOriginItem", v.moreMenu.contentItem)
                win.check(item !== null && item.text.startsWith("From "), "and where it came from heads the ⋯ menu (" + (item ? item.text : "none") + ")")
                win.shotWindow("clicks-pictures-more")
                win.clickItem(win.findText("All the chat's pictures", v.moreMenu.contentItem), "All the chat's pictures")
            }
        } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const v = win.picViewer()
            if (v.width < 640) {
                win.check(v.gridMode && v.grid.activeFocus, "the grid chosen from ⋯ opens with the keys")
                TestInput.key(Qt.Key_Escape, 0)
            }
        } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = win.picViewer()
            v.focusView()
            win.check(v.view.activeFocus, "the image has the keys again")
            win.imgEdges = win.imgEdgeColours(v.view)
            for (let i = 0; i < 8; i++) TestInput.key(Qt.Key_Plus, 0) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const v = win.picViewer()
            const edges = win.imgEdgeColours(v.view)
            win.check(v.view.zoom * 600 / win.imgDpr() > v.view.height * 2 && edges[0] !== "" && edges[0] === win.imgEdges[0] && edges[1] === win.imgEdges[1],
                      "zoomed past its view, the picture is drawn only inside it: not over the bar below or the tabs above (" + edges + ", fitted " + win.imgEdges + ", zoom " + v.view.zoom + ")")
            win.shotWindow("clicks-pictures-zoomed")
            TestInput.key(Qt.Key_0, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.picViewer()
            win.check(v.view.fit, "0 fits it again")
            TestInput.key(Qt.Key_Right, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(picSteps, () => win.picViewer().view.status === "ready", 5000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const v = win.picViewer()
            win.check(v.path.endsWith("/art/sun.png") && v.view.imageWidth === 640 && chat.transcript.chatPictures.opened, "→ steps to the next picture, the file the command wrote")
            TestInput.key(Qt.Key_G, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = win.picViewer()
            win.check(v.gridMode && v.grid.count === 7, "G: all the chat's pictures as a grid (" + v.grid.count + ")")
            win.shotWindow("clicks-pictures-grid")
            const names = win.findAllNames("imageThumbName", v.grid).map(l => l.text)
            win.check(names.length === 7 && names.indexOf("You sent · Make me a sun picture") >= 0 && names.indexOf("You sent · And this one?") >= 0 && names.indexOf("art/sun.png") >= 0 && names.every(n => !/[0-9a-f]{12}/.test(n)),
                      "the grid names each by where it came from, not its stored file's name (" + names.join(", ") + ")")
            const goneTile = win.findAllNames("imageThumb", v.grid).find(t => t.path.endsWith("/art/gone.png"))
            win.check(goneTile && goneTile.status === "error" && win.findText("No longer on disk", goneTile.parent) !== null, "in the grid too, a picture gone from disk says so")
            const sun = win.findAllNames("imageThumb", v.grid).find(t => t.path.endsWith("/art/sun.png"))
            win.check(sun && sun.bottomAligned, "the grid's pictures sit on their names")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(!win.picViewer().gridMode && chat.transcript.chatPictures.opened, "Esc leaves the grid first")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(!chat.transcript.chatPictures.opened, "Esc again closes the viewer")
            win.clickItem(win.picTiles().find(t => t.modelData === win.picAll[0]), "the sent picture") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = win.picViewer()
            win.check(v.index === 0 && v.view.imageWidth === 300, "the picture you sent opens (" + v.view.imageWidth + ")")
            TestInput.key(Qt.Key_T, 0) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            win.check(!chat.transcript.chatPictures.opened && Workspace.activeKey.startsWith("file:") && chat.fileEditor.imageViewer.view.imageWidth === 300, "T opens it in the file tab (" + Workspace.activeKey.split("/").pop() + ")")
            Workspace.open(Workspace.tabs.map(t => t.split("\t")[0]).find(k => !k.startsWith("file:")) || "")
            chat.session.addAttachment(win.argValue("--cwd=") + "/out/sun 2.png") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const thumbs = win.findAllNames("attachmentThumb", chat).filter(t => win.shown(t))
            win.check(thumbs.length === 1 && thumbs[0].status === "ready", "a picture to send shows as a thumbnail (" + thumbs.map(t => t.status) + ")")
            win.summary(); Qt.quit() } }
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
            win.cvFrames.push(v.paintMs)
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
                win.log("run:", id, "active:", Workspace.activeKey)
            }
        }
        PauseAnimation { duration: 3500 }
        ScriptAction {
            script: {
                const r = JSON.parse(Workspace.runJson(Workspace.activeKey.substring(9)))
                win.log("lanes:", r.lanes.map(l => l.letter + " " + l.model + " " + l.status + " " + l.branch + " changes=" + l.changes).join(" | "))
                win.shot("multirun")
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Workspace.keepRunLane(Workspace.activeKey.substring(9), 0) }
        PauseAnimation { duration: 3000 }
        ScriptAction {
            script: {
                const r = JSON.parse(Workspace.runJson(Workspace.activeKey.substring(9)))
                win.log("after keep:", "kept", r.kept, r.lanes.map(l => l.letter + " gone=" + l.worktreeGone).join(" | "), "notice:", Workspace.notice)
                win.shot("multirun-kept")
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
                win.log("dropdown:", win.dropDown ? "created " + (win.dropDown.toString().indexOf("Layer") >= 0 ? "layer" : "plain") + " visible " + win.dropDown.visible : "not created")
                if (win.dropDown) win.dropDown.contentItem.children[0].grabToImage(r => r.saveToFile(win.selftestDir + "/dropdown.png"))
            }
        }
        PauseAnimation { duration: 600 }
        ScriptAction { script: Qt.quit() }
    }
    Connections {
        target: Workspace
        function onRemoteCommand(command) { if (win.selftestDir !== "") win.log("remote command:", command) }
    }

    // Session branches: header button, dialog, preview, continue in a new session.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "branches"
        PauseAnimation { duration: 3000 }
        ScriptAction { script: Workspace.open(win.argValue("--open=")) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("branches:", chat.branchCount, "rows:", chat.transcript.count); win.shot("branches-header"); chat.branchDialog.show(win.argValue("--open="), "harlo", "/tmp") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("dialog:", chat.branchDialog.branches.map(b => (b.current ? "*" : "") + b.lastUser + "@" + b.divergesAt).join(" | "), "selected", chat.branchDialog.selected); win.shotPopup(chat.branchDialog, "branches-dialog") } }
        PauseAnimation { duration: 500 }
        ScriptAction {
            script: {
                const d = chat.branchDialog
                const key = Workspace.continueBranch(d.file, d.current.leaf)
                d.close()
                win.log("continued:", key.split("/").pop(), "active:", Workspace.activeKey === key)
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("new session rows:", chat.transcript.count, "branches:", chat.branchCount); win.shot("branches-continued") } }
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
            win.slowSearchFirst = searchDialog.hits.length
            win.check(searchDialog.settled && win.slowSearchFirst < 16, "typing answered at once, before the indexer finished (" + win.slowSearchFirst + " of 16)")
            win.check(slowSearchTick.worst < 300, "the window kept drawing while it searched (longest gap " + slowSearchTick.worst + " ms)")
            TestInput.typeText(" tusk") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(searchDialog.settled && searchDialog.query === "walrus tusk", "more typing answered too (" + searchDialog.hits.length + ")")
            TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0); TestInput.key(Qt.Key_Backspace, 0) } }
        PauseAnimation { duration: 8000 }
        ScriptAction { script: {
            win.check(searchDialog.hits.length === 16 && win.slowSearchFirst < 16, "the rest arrived late, as the indexer committed them (" + searchDialog.hits.length + ")")
            win.check(slowSearchTick.worst < 300, "and the window never stalled (longest gap " + slowSearchTick.worst + " ms)")
            win.shotWindow("search-slow-index")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.log("hits:", searchDialog.hits.length, "first:", searchDialog.hits.length ? searchDialog.hits[0].title + " #" + searchDialog.hits[0].index + " " + searchDialog.hits[0].snippet.slice(0, 80) : "-") }
        ScriptAction { script: win.shotPopup(searchDialog, "search") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: searchDialog.pick(0) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("opened:", Workspace.activeKey.split("/").pop(), "rows:", chat.transcript.count, "pending:", chat.pendingReveal); win.shot("search-opened") } }
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
        ScriptAction { script: win.clickText("Fix the login redirect loop", sidebar) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: shortcutsDialog.open() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("kb-shortcuts")
            const groups = win.shortcutList.concat(win.composerKeys).filter(s => !s.hidden).map(s => s.group)
            win.check(groups.every(g => /^[A-Z][a-z ]*$/.test(g)), "shortcut groups are in sentence case")
            win.check(win.shortcutList.concat(win.composerKeys).every(s => s.hidden || (s.icon && !s.glyph)), "every shortcut row has an icon, no glyph")
            const go = win.shortcutList.filter(s => s.label === "Go to a session or action")
            win.check(go.length === 2 && go[0].group === go[1].group, "Ctrl+K and Ctrl+P are one row")
            win.check(win.composerKeys.some(s => s.group === "Runs in a session"), "the run keys are listed")
            win.check(win.composerKeys.filter(s => s.group === "Image viewer").length >= 12 && win.composerKeys.some(s => s.group === "Sidebar" && s.seq === "Esc"), "the image viewer's keys and Esc for a drag are listed")
            win.check(!!win.findName("shortcutTips", shortcutsDialog.contentItem), "the footer explains > and @")
            shortcutsDialog.close(); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: shortcutsDialog.open() }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("kb-shortcuts-light")
            win.check(!!win.findName("shortcutsMore", shortcutsDialog.contentItem), "a fade says more is below")
            { const cap = win.findText("Up", shortcutsDialog.contentItem); win.check(!!cap && cap.children.some(c => c.text === "↑"), "arrow keys are shown as arrows") }
            win.clickItem(win.findText("Search all sessions", shortcutsDialog.contentItem), "the Search all sessions row") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(!shortcutsDialog.visible && searchDialog.visible, "clicking a row runs it")
            searchDialog.close(); palette.openWith("") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.shotWindow("pal-empty-light"); palette.close(); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: palette.openWith("") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("pal-empty")
            const it = palette.items
            win.check(it[0].kind === "header" && it[0].title === "Recent sessions", "empty palette starts with Recent sessions")
            win.check(it.some(i => i.kind === "header" && i.title === "Actions"), "then Actions")
            win.check(it.filter(i => i.title === "New session in webapp").length === 1, "one New session in webapp row (" + it.filter(i => i.title === "New session in webapp").length + ")")
            win.check(!it.some(i => i.title === "New demo session"), "no demo action")
            win.check(palette.current === 1, "the first session is chosen, not the heading")
            const firstAction = it.findIndex(i => i.kind === "action")
            for (let i = 1; i < firstAction - 2; i++) palette.move(1)
            win.check(palette.current === firstAction - 2, "Down walks the sessions")
            palette.move(1)
            win.check(palette.current === firstAction, "Down skips the Actions heading (" + palette.current + " vs " + firstAction + ")")
            palette.openWith("login") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.shotWindow("pal-query"); palette.openWith("set") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("pal-query-action")
            win.check(palette.marked("Settings", "set") === "<b><font color='" + Theme.rgb(Theme.accent) + "'>Set</font></b>tings", "matched letters are marked")
            win.check(palette.marked("Close tab", "set").split("<b>").length === 3, "letters in order when no word matches")
            win.check(palette.marked("Add CSV export", "set").split("<b>").length === 2, "a loose match marks only where a word starts (" + palette.marked("Add CSV export", "set") + ")")
            win.check(palette.choices[0].title === "Settings" && palette.choices[2].title === "Close tab", "whole-word matches come first (" + palette.choices.slice(0, 3).map(c => c.title).join(", ") + ")")
            const firstLoose = palette.choices.findIndex(c => c.kind === "session" && palette.strength(c.title, "set") === 0)
            win.check(firstLoose < 0 || palette.choices.slice(firstLoose).filter(c => c.kind === "session").every(c => palette.strength(c.title, "set") === 0), "strong sessions before loose ones")
            win.check(palette.choices.filter(i => i.kind === "session").every(i => i.subtitle === ""), "one project: not said on every row")
            palette.openWith(">") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("pal-actions")
            win.check(palette.items.length > 0 && !palette.items.some(i => i.kind === "header"), "> lists actions without headings")
            palette.openWith("zzqx") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.shotWindow("pal-none")
            win.check(palette.items.length === 0 && !!win.findName("paletteNone", palette.contentItem), "no matches says so")
            palette.openWith("@auth") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.shotWindow("pal-files")
            win.check(palette.choices.length === 2 && palette.choices[0].kind === "file", "@auth finds two files (" + palette.choices.length + ")")
            win.check(win.findText("Insert", palette.contentItem) === null, "file rows don't each say Insert")
            palette.close(); Workspace.open("view:stats") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.shotWindow("stats-top")
            const sv = chat.statsView, inner = win.findName("statsInner", sv)
            const right = inner.mapToItem(sv, inner.width, 0).x
            win.check(right <= sv.width - 8, "Usage fits its tab (" + Math.round(right) + " of " + sv.width + ")")
            win.check(win.findName("statsLeadValue", sv).text === "$11.69" || /^\$/.test(win.findName("statsLeadValue", sv).text), "the lead is the cost")
            win.check(/over 11 sessions/.test(win.findName("statsLeadText", sv).text), "and says what it covers")
            win.check(!/Helpers|helper/.test(win.findName("statsFigures", sv).text), "no helper count when there are none")
            win.check(!Qt.colorEqual(sv.cInput, sv.cCacheWrite) && !Qt.colorEqual(sv.cCacheRead, sv.cCacheWrite) && !Qt.colorEqual(sv.cInput, sv.cOutput), "the four token kinds have their own colours")
            win.check(!win.findText("unknown", sv), "no unknown model row")
            win.check(!!win.findName("statsRefresh", sv), "refresh is an icon button")
            win.check(!Qt.colorEqual(sv.cCount, Theme.info) && !Qt.colorEqual(sv.cCount, Theme.primary), "counts aren't drawn in the input blue")
            win.check(!!win.findName("statsModelLegend", sv), "the models' colours have a legend")
            win.check(win.findText("Output", sv) !== null && win.findText("Output (with reasoning)", sv) === null, "Output is called one thing")
            if (sv.width > 500) { const pr = win.findName("statsProject", sv); win.check(pr.y === 0, "the project picker sits beside the periods (y " + pr.y + ")") }
            chat.statsView.scrollToEnd() } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.shotWindow("stats-bottom"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.shotWindow("stats-bottom-light"); Workspace.chooseThemeMode("dark"); settingsDialog.openPage("usage") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.shotWindow("usage-settings")
            const sd = settingsDialog.contentItem
            win.check(!!win.findName("limitsRefresh", sd) && !!win.findName("statsRefresh", sd) && win.findText("Refresh", sd) === null, "one refresh style on the page")
            win.check(!!win.findText("Every session over the period you pick, read from their files; the spending limits above count today only.", sd), "Activity says it isn't today's spending")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
    }

    // Screenshots of the diff viewer and the session search, for a design
    // review (not a test: SCENARIOS=diffshots KEEP=1 scripts/click-tests.sh 1280).
    function diffShot(name) { win.shotWindow("diff-" + name) }
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
        ScriptAction { script: win.clickText("Fix the login redirect loop", sidebar) }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: { windowSettings.rightMode = "changes"; git.refresh(true) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.diffShot("changes"); win.clickText("session.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const dv = chat.diffView
            // Beside the sidebar and Changes, the sides don't fit: inline.
            win.check(dv.unified === !dv.splitFits && (win.width > 1500 || dv.unified), "inline when the sides don't fit (" + dv.width + " px, inline " + dv.unified + ")")
            win.check(win.findName("hunk-stage-0", dv) !== null && win.findName("hunk-discard-0", dv) !== null, "each change has Stage and Discard buttons")
            win.diffShot("file")
            win.clickItem(win.findName("diffNext"), "next change") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.diffShot("next"); win.diffCollapseClick() } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            win.diffShot("collapsed")
            const dv = chat.diffView, u = dv.shownRows
            win.reviewRow = u.findIndex(r => r[5] === 1 && r[14] !== 1)
            win.clickAt(win.findName((dv.unified ? "gutter-u-" : "gutter-r-") + win.reviewRow, dv)) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.clickItem(win.findName("commentText", chat.diffView.commentBox.contentItem), "the comment")
            TestInput.typeText("Should this also check the cookie's secure flag? It only matters behind a proxy that ends TLS.") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.diffShot("comment-box"); win.clickItem(win.findName("commentSave", chat.diffView.commentBox.contentItem), "Comment") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(win.findName("diffNote-0", dv) !== null, "the saved comment shows under its line")
            win.diffShot("review-bar")
            // Wide (no sidebar, no panel): side by side.
            windowSettings.sidebarOpen = false
            windowSettings.rightMode = "" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const dv = chat.diffView
            win.check(dv.unified === !dv.splitFits, "side by side when it fits (" + dv.width + " px, inline " + dv.unified + ")")
            win.diffShot("wide")
            win.clickItem(win.findName("diffMore"), "More") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.diffShot("more-menu"); TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { windowSettings.sidebarOpen = true; windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.diffShot("rail"); win.clickText("return-to.test.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.diffShot("new-file"); win.clickText("guard.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.diffShot("small"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.diffShot("small-light"); win.clickText("session.ts", changes) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.diffShot("collapsed-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 900 }
        // The sidebar's filter, and from it the messages.
        ScriptAction { script: { win.clickItem(win.findName("sessionFilter"), "Filter sessions"); TestInput.typeText("cookie") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const row = win.findName("filterSearchMessages")
            win.check(row !== null && row.visible, "filtering offers the messages search")
            win.shotWindow("search-filter")
            win.clickItem(row, "Search messages for") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            win.check(searchDialog.visible && searchDialog.query === "cookie" && searchDialog.hits.length > 0, "it opens the search with those words (" + searchDialog.hits.length + " hits)")
            const f = win.findName("sessionFilter"); f.text = ""
            win.diffSearch("") } }
        PauseAnimation { duration: 600 }
        // Search: empty (recent sessions), results, none, then a hit opened.
        ScriptAction { script: {
            win.check(searchDialog.recents.length === 4 && win.findName("searchHint") !== null, "empty, it says what it searches and offers recent sessions (" + searchDialog.recents.length + ")")
            win.shotWindow("search-empty")
            win.diffSearch("returnTo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const h = searchDialog.hits
            win.check(h.length > 0 && h.every(x => x.snippet.indexOf("`") < 0), "snippets without markdown")
            win.shotWindow("search-results")
            searchDialog.current = Math.min(2, h.length - 1) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.shotWindow("search-chosen"); win.diffSearch("kubernetes") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.shotWindow("search-none"); Workspace.chooseThemeMode("light"); win.diffSearch("returnTo") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { win.shotWindow("search-results-light"); Workspace.chooseThemeMode("dark") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const i = searchDialog.hits.findIndex(h => (h.title || "").indexOf("security review") >= 0 && h.snippet.indexOf("evil") >= 0)
            searchDialog.pick(Math.max(0, i)) } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: {
            win.check(chat.transcript.wordMarks.length > 0, "the matched words are marked where it lands (" + chat.transcript.wordMarks.length + ")")
            win.shotWindow("search-opened")
            chat.openFind()
            TestInput.typeText("cookie") } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            const fb = chat.findBar
            win.check(fb.visible && fb.hits.length === 3 && fb.current >= 0 && chat.transcript.wordMarks.length > 0, "find in the session: every message with the word, the one shown marked (" + fb.hits.length + ")")
            win.shotWindow("find")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            win.shotWindow("find-next")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(!chat.findBar.visible && chat.transcript.wordMarks.length === 0, "Esc closes it and clears the marks")
            win.summary(); Qt.quit() } }
    }

    // Todos and queue editing (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "extras"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("make a plan with todos", "") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { const d = chat.todoDock; win.log("todos:", d.done + "/" + d.tasks.length, "current:", d.current ? d.current.content : "-"); d.expanded = true } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.shot("todos"); chat.submit("queue demo", "") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("queue:", chat.session.queued.join(" | ")); chat.session.promoteQueued(1) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.log("after promote:", chat.session.queued.join(" | ")); chat.session.removeQueued(0) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.log("after remove:", chat.session.queued.join(" | ")) }
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
        ScriptAction { script: { win.log("dock:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" "), "running", chat.subagentDock.running); win.shot("subagents-running") } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { win.log("later:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" ")); chat.subagentDialog.show(chat.subagentDock.agents[1], true) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("dialog rows:", chat.subagentDialog.contentItem ? "open" : "-", "running", chat.subagentDialog.running); chat.subagentDialog.background.parent.grabToImage(r => r.saveToFile(win.selftestDir + "/subagent-dialog.png")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { chat.session.steerSubagent("BuildOtter", "Only run the unit tests"); chat.session.cancelSubagent("BuildOtter") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("after cancel:", chat.subagentDock.agents.map(a => a.id + "=" + a.status).join(" "), "msg", chat.session.subagentMessage); chat.subagentDialog.close() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shot("subagents-done") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // An open image reloads when the file changes, and notes a deletion.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "imagereload"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.openFileTab(win.argValue("--image=")) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.log("before:", chat.fileEditor.imageInfo) }
        PauseAnimation { duration: 4500 }
        ScriptAction { script: { win.log("after change:", chat.fileEditor.imageInfo, "version", chat.fileEditor.imageVersion); win.shot("image-changed") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: { win.log("after delete: gone", chat.fileEditor.imageGone); win.shot("image-gone") } }
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
                win.log("terminal: running", t.running, "cols", t.cols, "rows", t.rows, "cwd", t.session.cwd.split("/").pop(), "focus", t.activeFocus)
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
        ScriptAction { script: win.shot("terminal") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Workspace.chooseThemeMode("light") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.shot("terminal-light") }
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
        ScriptAction { script: { win.log("after exit: running", terminal.current.running, "text", terminal.current.session.exitText); win.shot("terminal-exited") } }
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
        ScriptAction { script: { win.log("changes:", git.unstagedCount, "harness:", changes.harness); git.generateMessage(win.argValue("--fake-harlo="), "", false); win.log("generating:", git.generating) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.shot("commitmsg-writing") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("message:", JSON.stringify(changes.commitText), "generating:", git.generating, "error:", git.error, git.message); win.shot("commitmsg-done") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // New-session page: empty state, project/branch, starter, send, worktree.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "draft"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("empty: draft visible", chat.draftView.visible, "tabs", Workspace.tabs.length); win.shot("draft-empty") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")); win.openDraft(1) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { const d = chat.draftView; win.log("project:", d.project ? d.project.name : "-", "branch:", d.currentBranch, "default:", d.atDefaultBranch, "tab:", Workspace.activeKey); win.shot("draft-project") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { const d = chat.draftView; d.starter = "explore"; d.text = "Focus on the build setup"; d.harness = "demo" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shot("draft-starter") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.draftView.send() }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("sent: active", Workspace.activeKey, "tabs", Workspace.tabs.map(t => t.split("\t")[1]).join(" | "), "cwd", chat.session.cwd.split("/").pop()); win.shot("draft-sent") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.openDraft(1); const d = chat.draftView; d.harness = "demo"; d.useWorktree = true; d.worktreeName = "harlo/draft-test"; d.text = "hello from a worktree" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.shot("draft-worktree"); chat.draftView.send() } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { win.log("worktree: active", Workspace.activeKey, "cwd", chat.session.cwd, "notice", Workspace.notice); win.shot("draft-worktree-sent") } }
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
        ScriptAction { script: { win.log("request:", chat.session.requestMethod, "questions:", JSON.parse(chat.session.requestJson).questions.length, "complete:", chat.askPanel.complete); win.shot("ask-open") } }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                const p = chat.askPanel
                p.toggle(0, "Postgres")
                p.previewText = "DATABASE_URL=postgres://localhost/harlo"
                p.toggle(1, "Search"); p.toggle(1, "Export")
                p.setCustom(1, "Offline mode")
                win.log("answers:", JSON.stringify(p.answers), "complete:", p.complete)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shot("ask-answered") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.askPanel.submit() }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("after send: hasRequest", chat.session.hasRequest); chat.submit("now choose one", "") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("select request:", chat.session.requestMethod, chat.session.requestOptions.join("|")); win.shot("ask-select") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.answerValue("Merge") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("after select: hasRequest", chat.session.hasRequest); win.shot("ask-done") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Fonts: the desktop's by default; a chosen interface and code font.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "fonts"
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                win.log("system ui:", Workspace.systemFont(false), "| system mono:", Workspace.systemFont(true),
                        "| families:", Workspace.fontFamilies(false).length, "fixed:", Workspace.fontFamilies(true).length,
                        "| Theme:", Theme.uiFont, "/", Theme.mono, "| app font:", Qt.application.font.family)
                Workspace.newSession(-1, "demo")
            }
        }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: chat.submit("stream a demo reply fast", "") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.shot("fonts-system") }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                Workspace.chooseFont("ui", win.argValue("--ui-font="))
                Workspace.chooseFont("mono", win.argValue("--mono-font="))
                win.log("chosen:", Theme.uiFont, "/", Theme.mono, "| app font:", Qt.application.font.family)
            }
        }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.shot("fonts-chosen") }
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
        ScriptAction { script: { win.log("settings open:", settingsDialog.visible, "page:", settingsDialog.page, "theme:", Workspace.themeFamily, Workspace.themeMode, Workspace.textScale); settingsDialog.grab(win.selftestDir + "/settings-general.png") } }
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
        ScriptAction { script: { win.log("search 'font':", settingsDialog.shownPages.map(p => p.id).join(","), "page:", settingsDialog.page); settingsDialog.grab(win.selftestDir + "/settings-search.png") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { settingsDialog.query = ""; settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.log("closed:", !settingsDialog.visible, "tabs:", win.tabState()); Qt.quit() } }
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
                win.log("rows:", rows.join(" | "))
                win.shot("dirty")
            }
        }
        PauseAnimation { duration: 300 }
        ScriptAction {
            script: {
                const repo = Workspace.sessionRepo("live:1")
                win.log("repo of live:1:", repo, "ignored:", Workspace.isRepoIgnored(repo))
                Workspace.setRepoIgnored(repo, true)
            }
        }
        PauseAnimation { duration: 1200 }
        ScriptAction {
            script: {
                win.shot("dirty-ignored")
                const m = sidebar.sessionMenu
                m.key = "live:1"; m.sessionTitle = "New session"; m.live = true; m.branch = ""
                m.repo = Workspace.sessionRepo("live:1"); m.repoIgnored = Workspace.isRepoIgnored(m.repo)
                m.popup(sidebar, 120, 140)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.shotPopup(sidebar.sessionMenu, "dirty-menu"); sidebar.sessionMenu.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { if (win.argValue("--unignore=") === "1") Workspace.setRepoIgnored(Workspace.sessionRepo("live:1"), false) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.shot("dirty-after") }
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
                win.log("added:", results.join(" "), "| attachments:", chat.session.attachments.length)
            }
        }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shot("attach-composer") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.submit("Please review these files.", "") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("after send: attachments", chat.session.attachments.length); win.shot("attach-sent") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Themed menus and dialogs.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "menus"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { sidebar.sessionMenu.sessionTitle = "A session"; sidebar.sessionMenu.live = false; sidebar.sessionMenu.popup(sidebar, 120, 140) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.sessionMenu, "menu-session") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.sessionMenu.close(); sidebar.projectMenu.harness = "harlo"; sidebar.projectMenu.popup(sidebar, 120, 110) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.projectMenu, "menu-project") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.projectMenu.close(); sidebar.renameDialog.ask("x", "create a file hello.py that prints hello") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.renameDialog, "dialog-rename") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.renameDialog.close(); sidebar.trashDialog.label = "create a file hello.py that prints hello"; sidebar.trashDialog.open() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.trashDialog, "dialog-trash") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.trashDialog.close(); sidebar.openNewWorktree(-1, "houdini-mcp", "harlo") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.worktreeDialogs.newDialog, "dialog-worktree") }
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
        ScriptAction { script: win.shotPopup(sidebar.worktreeDialogs.finishDialog, "dialog-finish") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { sidebar.worktreeDialogs.finishDialog.close(); sidebar.worktreeDialogs.openSetup(-1, "houdini-mcp") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shotPopup(sidebar.worktreeDialogs.setupDialog, "dialog-setup") }
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
        ScriptAction { script: win.shot("fileicons-files") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: windowSettings.rightMode = "changes" }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.shot("fileicons-changes") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: Qt.quit() }
    }

    // Bottom-left buttons: usage stats, settings placeholder, shortcuts dialog.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "stats"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.open("view:stats") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("stats tab:", win.tabState(), "covered:", chat.covered); win.shot("stats") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.statsView.scrollToEnd() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.shot("stats-bottom") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.width = 760; win.height = 900 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.log("narrow: win", win.width, "sidebar", sidebar.width, "chat", chat.width, "panel", rightPanel.visible ? rightPanel.width : 0); win.shot("stats-narrow") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.width = 1400; win.height = 900 } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { shortcutsDialog.open(); win.log("shortcuts rows:", shortcutsDialog.rowCount) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { shortcutsDialog.grab(win.selftestDir + "/shortcuts.png"); win.shot("shortcuts-window") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: Qt.quit() }
    }

    // Real sessions: sidebar from disk, then open the newest session read-only
    // (no harness process is started; nothing is sent).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "sidebar"
        PauseAnimation { duration: 2500 }
        ScriptAction { script: win.log("sidebar rows:", win.sidebarCount(), "sessions:", Workspace.sessionCount, "projects:", Workspace.projectCount) }
        ScriptAction { script: win.shot("sidebar") }
        ScriptAction {
            script: {
                const explicit = win.argValue("--open=")
                if (explicit !== "") { win.log("opening", explicit); Workspace.open(explicit); return }
                for (let i = 0; i < 500; i++) {
                    const k = Workspace.keyAt(i)
                    if (k === "") break
                    if (k.startsWith("/")) { win.log("opening", k); Workspace.open(k); break }
                }
            }
        }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: win.log("opened:", chat.session.key, "status:", chat.session.status, "title:", chat.session.title, "rows:", chat.transcript.count) }
        ScriptAction { script: win.shot("saved-session") }
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            // (Background commands run without asking in yolo.)
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(chat.session.harness === "harlo" && chat.session.modelName === "Fake Coder", "a Harlo session on the scripted model (" + chat.session.modelName + ")")
            win.sendByKeys("please ask me something") } }
        PauseAnimation { duration: 300 }
        // The ask card.
        ScriptAction { script: win.waitUntil(harloSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.session.requestMethod === "ask" && win.findText("Which colour should the button be?", chat.askPanel) !== null, "Harlo's question shows in the question card")
            win.shotWindow("clicks-harlo-ask")
            win.clickText("Blue", chat.askPanel) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Send answers", chat.askPanel) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSteps, () => win.chatText().indexOf("Done:") >= 0, 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const asked = win.modelRequests().filter(b => (b.messages || []).some(m => m.role === "tool" && JSON.stringify(m.content).indexOf("Blue") >= 0))
            win.check(!chat.session.hasRequest && asked.length > 0, "the answer goes back to the model (" + win.chatText().slice(-120) + ")")
            const askRes = win.findAllNames("toolResult", chat.transcript).map(l => l.text)
            win.check(askRes.indexOf("Blue") >= 0, "the question's line shows what was answered (" + askRes.join(" | ") + ")")
            win.sendByKeys("/btw which colour did I pick?") } }
        PauseAnimation { duration: 300 }
        // The side question: a card above the composer, not in the conversation.
        ScriptAction { script: win.waitUntil(harloSteps, () => chat.session.asideJson !== "" && JSON.parse(chat.session.asideJson).done, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const card = win.findName("asideCard")
            const a = chat.session.asideJson !== "" ? JSON.parse(chat.session.asideJson) : {}
            win.check(card !== null && (a.text || "").indexOf("Aside: which colour did I pick?") === 0, "/btw answers in the card above the composer (" + JSON.stringify(a).slice(0, 160) + ")")
            win.check(win.chatText().indexOf("Aside:") < 0, "and not in the conversation")
            const foot = win.findText("A side question: not part of the conversation, and not kept.")
            win.check(foot && !foot.truncated && foot.mapToItem(null, foot.contentWidth, 0).x <= win.findName("asideCard").mapToItem(null, win.findName("asideCard").width, 0).x + 1, "the aside's footnote stays inside its card")
            win.shotWindow("clicks-harlo-btw")
            win.clickItem(win.findName("asideDismiss"), "dismiss the aside") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("asideCard") === null, "dismissing it hides the card")
            win.sendByKeys("quote 2>&1; and <b>bold</b> here") } }
        PauseAnimation { duration: 300 }
        // A reply quoting a shell redirect: its Context row shows it as
        // text (unescaped, "&1;" read as markup and Qt warned).
        ScriptAction { script: win.waitUntil(harloSteps, () => win.chatText().indexOf("ok: quote") >= 0, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.harloRightWas = windowSettings.rightMode; windowSettings.rightMode = "context" } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const rows = win.findAllNames("contextRowLabel", win.contentItem).map(l => l.text)
            win.check(rows.some(t => t.indexOf("ok: quote 2>&amp;1; and &lt;b>bold&lt;/b> here") >= 0), "a reply's Context row shows its 2>&1; and <b> as text (" + rows.slice(-2).join(" | ").slice(0, 200) + ")")
            windowSettings.rightMode = win.harloRightWas
            win.sendByKeys("run the build in the background") } }
        PauseAnimation { duration: 300 }
        // A background job: the turn ends at once; its end starts a run.
        ScriptAction { script: win.waitUntil(harloSteps, () => win.chatText().indexOf("The background job ended") >= 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("Started in the background as j1") >= 0, "the command starts as a job (" + t.slice(-300) + ")")
            win.check(t.indexOf("The background job ended: built-ok") >= 0, "its end comes as a notice, with its output, and starts a run by itself")
            win.check(win.findContaining("The background command finished", chat.transcript) !== null && win.findContaining("read or grep it", chat.transcript) === null, "the notice shows in the conversation, in plain words")
            win.shotWindow("clicks-harlo-job")
            // Its one line, cut short to the well: a click shows all of it.
            const w = win.findAllNames("toolWhat", chat.transcript).find(l => l.plain.indexOf("echo built-ok") >= 0)
            let card = w
            while (card && card.cutLines === undefined) card = card.parent
            win.harloJobCard = card
            win.check(card && card.cutLines > 0 && card.canOpen, "a line cut short to its well opens the call (" + (card ? card.cutLines : "none") + ")")
            if (w) win.clickItem(w, "the job's line") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const card = win.harloJobCard
            win.check(card && card.open && win.findAllNames("toolTail", card).length === 0 && win.findContaining("j1.log", card) !== null, "opened: all of it, the log's path too")
            win.shotWindow("clicks-harlo-job-open")
            win.sendByKeys("now reset hard") } }
        PauseAnimation { duration: 300 }
        // A destructive command asks, even in yolo, without Always allow.
        ScriptAction { script: win.waitUntil(harloSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(Workspace.approvalMode(win.harloKey) === "yolo" && chat.session.requestIsApproval, "git reset --hard asks in yolo")
            win.check(win.findContaining("can't be undone", chat.transcript) !== null, "and says why")
            win.check(win.findAllNames("approveInline", chat.transcript).some(b => b.text === "Run anyway"), "its confirm names what it does, not Approve")
            win.check(win.findName("alwaysAllowInline", chat.transcript) === null && win.findText("Always allow") === null, "with no Always allow")
            const q = win.findAllNames("approvalQuestion", chat.transcript)[0]
            const grid = q ? q.parent : null
            win.check(q && !q.truncated && (win.width < 700 ? grid.columns === 1 && q.lineCount === 1 : grid.columns === 2), "the question beside its answers, or over them when narrow (" + (grid ? grid.columns : "none") + ")")
            win.shotWindow("clicks-harlo-destructive")
            win.clickText("Deny", chat.transcript) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSteps, () => !chat.session.hasRequest && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findAllNames("toolMark", chat.transcript).some(m => m.name === "close"), "Deny: it isn't run (" + win.chatText().slice(-160) + ")")
            // A file too big to read whole: its outline, each row its line.
            const big = ["// The ledger.", "", "pub struct Ledger {", "    id: u32,", "}", "", "impl Ledger {", "    pub fn post(&self) {}", "}"]
            for (let i = big.length; i < 2099; i++) big.push("// filler " + i)
            big.push("fn last() {}")
            TestInput.writeFile(win.argValue("--cwd=") + "/big.rs", big.join("\n") + "\n")
            win.sendByKeys("read the big file") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSteps, () => win.chatText().indexOf("Its outline follows") >= 0 && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.shotWindow("clicks-harlo-outline")
            const views = win.findAllNames("codeView", chat.transcript)
            const code = views.length ? views[views.length - 1].lines : []
            win.check(JSON.stringify(code.map(l => l.n)) === JSON.stringify(["3", "7", "8", "2100"]) && code[1].h.indexOf("Ledger") >= 0, "a big file's read: its outline, each definition at its own line (" + JSON.stringify(code.map(l => l.n)) + ")")
            win.check(win.findAllNames("toolResult", chat.transcript).some(l => l.text === "2100 lines, outline"), "its card says it's the outline of 2100 lines")
            win.summary(); Qt.quit() } }
    }

    // The wiki with Harlo and Claude Code: with Wiki on for a project that
    // has one, its search and read tools (no others) and a few lines on
    // when to use them reach both through harlo's tools, and OpenWiki
    // answers them from the repository's openwiki/; harlo starts OpenWiki
    // with its telemetry off unless allowed; with Wiki off they're gone.
    property int wikiProject: -1
    property int wikiDone: 0
    property int wikiRequests: 0
    function doneCount() { return win.chatText().split("Done:").length - 1 }
    function sendWiki(text) { win.wikiDone = win.doneCount(); win.sendByKeys(text) }
    function wikiAnswered() { return win.doneCount() > win.wikiDone && chat.session.status === "idle" }
    // What the last tool said, as the model got it.
    function lastToolResult() {
        const reqs = win.modelRequests()
        const msgs = (reqs[reqs.length - 1] || {}).messages || []
        const tools = msgs.filter(m => m.role === "tool")
        const last = tools[tools.length - 1]
        return last ? (typeof last.content === "string" ? last.content : JSON.stringify(last.content)) : ""
    }
    function wikiEnvLog() { return TestInput.readFile(win.argValue("--cwd=") + "/../openwiki-env.log") || "" }
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
            win.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            // (Wiki left as it comes: on, the project having a wiki.)
            win.check(Workspace.setting("wiki_" + win.wikiProject) === "", "the Wiki setting not set")
            win.harloKey = Workspace.newSession(win.wikiProject, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendWiki("search the wiki for how are failed jobs retried") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.wikiAnswered(), 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const first = win.modelRequests()[0] || {}
            const names = (first.tools || []).map(t => t.function.name)
            const wiki = names.filter(n => n.indexOf("openwiki") >= 0)
            win.check(wiki.length === 2 && wiki.some(n => n.endsWith("openwiki_search")) && wiki.some(n => n.endsWith("openwiki_read")), "Wiki on (as it comes): search and read reach Harlo, and no other wiki tool (" + wiki.join(", ") + ")")
            const tool = (first.tools || []).find(t => t.function.name.endsWith("openwiki_search"))
            win.check(tool && JSON.stringify(tool.function.parameters).indexOf("root") < 0, "without root: harlo fills in the repository")
            const said = JSON.stringify((first.messages || []).filter(m => m.role === "system"))
            win.check(said.indexOf("This project has a wiki") >= 0 && said.indexOf("check it in the code") >= 0, "and a few lines on when to use them")
            const t = win.lastToolResult()
            win.check(t.indexOf("openwiki/architecture/jobs.md") >= 0 && t.indexOf("retried three times") >= 0, "OpenWiki finds the section (" + t.slice(0, 240) + ")")
            win.check(win.findText("Search the wiki", chat.transcript) !== null, "its card says what it did")
            const env = win.wikiEnvLog()
            win.check(env.indexOf("start\nOPENWIKI_TELEMETRY_DISABLED=1") === 0 || (env.indexOf("start") === 0 && env.indexOf("OPENWIKI_TELEMETRY_DISABLED=1") > 0 && env.indexOf("DO_NOT_TRACK=1") > 0), "OpenWiki starts with its telemetry off (" + env.replace(/\n/g, " | ") + ")")
            win.sendWiki("read the wiki at openwiki/architecture/jobs.md#retry-control") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.wikiAnswered(), 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const t = win.lastToolResult()
            win.check(t.indexOf("Retry control") >= 0 && t.indexOf("three times") >= 0, "openwiki_read gives the section (" + t.slice(0, 240) + ")")
            win.check(win.wikiEnvLog().split("start").length - 1 === 1, "one OpenWiki serves both calls")
            win.shotWindow("clicks-harlo-wiki")
            // Claude Code, through the same tools.
            win.claudeKey = Workspace.newSession(win.wikiProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("wiki search failed jobs retried") }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.chatText().indexOf("Wiki tools:") >= 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("Wiki tools: openwiki_search, openwiki_read; wiki told; found: openwiki/architecture/jobs.md") >= 0, "Claude Code gets them and the lines too, and finds the page (" + t.slice(t.indexOf("Wiki tools:"), t.indexOf("Wiki tools:") + 200) + ")")
            // Telemetry allowed: OpenWiki starts again without the switch.
            Workspace.setSetting("openwiki_telemetry", "1")
            Workspace.open(win.harloKey) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.sendWiki("search the wiki for backoff") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.wikiAnswered(), 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const runs = win.wikiEnvLog().split("start")
            win.check(runs.length - 1 === 2 && runs[2].indexOf("OPENWIKI_TELEMETRY_DISABLED") < 0, "allowed, OpenWiki starts again with its telemetry on (" + win.wikiEnvLog().replace(/\n/g, " | ") + ")")
            Workspace.setSetting("openwiki_telemetry", "0")
            // Wiki off: a new session gets neither the tools nor the lines.
            Workspace.setSetting("wiki_" + win.wikiProject, "0")
            win.wikiRequests = win.modelRequests().length
            win.harloKey = Workspace.newSession(win.wikiProject, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendWiki("hello") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.modelRequests().length > win.wikiRequests, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const next = win.modelRequests()[win.wikiRequests] || {}
            const names = (next.tools || []).map(t => t.function.name)
            win.check(next.tools && names.every(n => n.indexOf("openwiki") < 0), "Wiki off: no wiki tools in a new session (" + names.join(", ") + ")")
            win.check(JSON.stringify((next.messages || []).filter(m => m.role === "system")).indexOf("This project has a wiki") < 0, "nor the lines")
            win.claudeKey = Workspace.newSession(win.wikiProject, "claude") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("wiki search failed jobs") }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => win.chatText().indexOf("Wiki tools:") >= 0, 20000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Wiki tools: none; wiki not told") >= 0, "nor for Claude Code (" + win.chatText().slice(-120) + ")")
            // Creating the wiki: a session of its own runs OpenWiki's steps
            // with the writing tools, which only it gets.
            Workspace.setSetting("default_harness", "harlo")
            // (Written with the project's model for Harlo.)
            Workspace.setSetting("project_model_" + win.wikiProject + "_harlo", "fake/fake-helper")
            win.wikiRequests = win.modelRequests().length
            win.wikiKeyBefore = Workspace.activeKey
            win.check(Workspace.startWikiRun("project:" + win.wikiProject, "create") === "", "Create starts") } }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => Workspace.activeKey !== win.wikiKeyBefore && chat.session.key === Workspace.activeKey, 20000) }
        ScriptAction { script: win.waitUntil(harloWikiSteps, () => /Wiki: (complete|no update|the page|unexpected)/.test(win.chatText()) && chat.session.status === "idle", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = win.modelRequests().slice(win.wikiRequests)
            const first = reqs[0] || {}
            const names = (first.tools || []).map(t => t.function.name)
            const asked = JSON.stringify(first.messages || [])
            win.check(["openwiki_begin", "openwiki_submit_plan", "openwiki_next_page", "openwiki_submit_page", "openwiki_finish"].every(n => names.some(m => m.endsWith(n))), "the wiki session gets OpenWiki's writing tools (" + names.filter(n => n.indexOf("openwiki") >= 0).join(", ") + ")")
            win.check(asked.indexOf("Create this repository's wiki with OpenWiki") >= 0 && asked.indexOf("Required sequence") >= 0, "and OpenWiki's own steps as its prompt")
            win.check(chat.session.title === "Wiki: create", "titled Wiki: create (" + chat.session.title + ")")
            win.check(reqs.length > 0 && reqs.every(r => r.model === "fake-helper"), "with the project's model (" + reqs.map(r => r.model).filter((m, i, a) => a.indexOf(m) === i).join(", ") + ")")
            win.check(Workspace.approvalMode(Workspace.activeKey) === "write", "its writes go ahead (the write mode); commands still ask (" + Workspace.approvalMode(Workspace.activeKey) + ")")
            const said = win.chatText()
            win.check(JSON.parse(chat.session.userPrompts()).length === 0 && win.findText("Wiki: create. The agent writes this project's wiki from its code, a page at a time.", chat.transcript) !== null,
                      "it opens with harlo's line, not OpenWiki's text as your message")
            win.check(win.findText("a new wiki", chat.transcript) !== null && win.findText("1 page planned", chat.transcript) !== null,
                      "its steps say what they did (a new wiki, 1 page planned), not a run's id")
            win.check(win.findText("none left", chat.transcript) !== null && win.findText("type: overview", chat.transcript) === null,
                      "the next page step says what it got (none left at the end), and the page's write shows its text, not its YAML header")
            win.check(said.indexOf("Wiki: complete") >= 0, "the run finishes (" + said.slice(said.lastIndexOf("Wiki:"), said.lastIndexOf("Wiki:") + 300) + ")")
            const cwd = win.argValue("--cwd=")
            win.check((TestInput.readFile(cwd + "/openwiki/quickstart.md") || "").indexOf("greets the reader") >= 0, "the page is written")
            win.check((TestInput.readFile(cwd + "/openwiki/.last-update.json") || "").indexOf("gitHead") >= 0, "OpenWiki records the update")
            win.check((TestInput.readFile(cwd + "/AGENTS.md") || "").indexOf("OPENWIKI:START") >= 0, "and its block in AGENTS.md")
            win.check(TestInput.readFile(cwd + "/.github/workflows/openwiki-update.yml") === "" && TestInput.listDir(cwd + "/.github") === "", "but not OpenWiki's daily update workflow")
            win.shotWindow("clicks-harlo-wiki-create")
            // Done, and left: on the project's Wiki shelf, folded.
            Workspace.open(win.wikiKeyBefore) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const shelf = win.findName("shelfRow-wikishelf:" + win.wikiProject)
            win.check(shelf !== null && shelf.collapsed && shelf.title === "Wiki" && shelf.count === 1, "the finished run goes on the Wiki shelf (" + (shelf ? shelf.title + " · " + shelf.count : "none") + ")")
            win.check(win.findName("shelfRow-shelf:" + win.wikiProject) === null, "not on Settled")
            if (shelf) win.clickItem(shelf, "the Wiki shelf") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const rows = win.findAllNames("sessionRow", win.contentItem)
            win.check(rows.some(r => r.title === "Wiki: create" && r.settled !== ""), "opened, it shows the run (" + rows.map(r => r.title + (r.settled ? " [" + r.settled + "]" : "")).join(", ") + ")")
            win.summary(); Qt.quit() } }
    }
    property string wikiKeyBefore: ""

    // The Wiki panel: a project's pages by folder, each read with its
    // diagram and code, links between pages followed (and Back), a link to
    // code opening the file, a search, the graph whose boxes open pages,
    // how fresh it is, an interrupted run's Continue; and a project with
    // none, whose Create runs a wiki session and then shows its pages.
    property int wikiProject2: -1
    /// Clicks the words of a link in a page's text.
    function clickLink(edit, words) {
        if (!edit) { win.check(false, "find the text with " + words); return }
        win.scrollIntoView(edit)
        const i = edit.getText(0, edit.length).indexOf(words)
        const r = edit.positionToRectangle(i + 2)
        const p = edit.mapToItem(null, r.x + 2, r.y + r.height / 2)
        TestInput.click(p.x, p.y)
    }
    function wikiProse(words) { return win.findAllNames("wikiProse", wikiPanel).find(t => t.getText(0, t.length).indexOf(words) >= 0) || null }
    function wikiRows() { return win.findAllNames("wikiPageRow", wikiPanel).map(r => r.modelData.title) }
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
            win.wikiProject = ps.find(p => p.folder === "repo").id
            win.wikiProject2 = ps.find(p => p.folder === "repo2").id
            win.harloKey = Workspace.newSession(win.wikiProject, "harlo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("rail-wiki"), "the rail's Wiki") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.wikiRows().length === 3, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(windowSettings.rightMode === "wiki", "the rail opens the Wiki panel")
            TestInput.key(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(windowSettings.rightMode === "", "Ctrl+Shift+K closes it")
            TestInput.key(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.wikiRows().length === 3, 10000) }
        PauseAnimation { duration: 300 }
        // (Larger text: the freshness line breaks between its phrases, or a
        // phrase between its words, never inside a word.)
        ScriptAction { script: { win.wikiScale = Workspace.textScale; Workspace.chooseTextScale(1.25) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const f = win.findName("wikiFreshness", wikiPanel)
            const labs = f ? f.children.filter(c => c.lineCount !== undefined) : []
            win.check(labs.length === 2 && labs.every(l => l.lineCount === 1 || l.implicitWidth > f.width),
                      "at 125% the freshness line keeps its words whole (" + labs.map(l => l.lineCount).join(",") + " lines, " + Math.round(f ? f.width : 0) + "px)")
            Workspace.chooseTextScale(win.wikiScale) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(windowSettings.rightMode === "wiki" && win.findName("wikiSearch", wikiPanel).activeFocus, "and opens it again, its search ready to type in")
            win.check(win.shortcutList.some(x => x.id === "wikiPanel" && x.seq === "Ctrl+Shift+K" && x.group === "Panels"), "listed with the other panels' shortcuts")
            win.check(JSON.stringify(win.wikiRows()) === JSON.stringify(["Quickstart", "Jobs", "Releasing"]), "its pages, the quickstart first (" + JSON.stringify(win.wikiRows()) + ")")
            win.check(win.findText("Architecture", wikiPanel) !== null && win.findText("Workflows", wikiPanel) !== null, "grouped by folder")
            const fresh = win.findName("wikiFreshness", wikiPanel)
            win.check(fresh && fresh.text.replace(/\u00a0/g, " ") === "Updated just now · 1 commit since", "how fresh it is, against the code (" + (fresh ? fresh.text.replace(/\u00a0/g, " ") : "") + ")")
            const run = win.findName("wikiRun", wikiPanel)
            win.check(run && run.text === "Update" && run.variant === "primary", "Update, the thing to do when it's behind")
            win.check(win.findName("wikiSwitch", wikiPanel).checked && win.findText("Agents search it, in sessions started from now on.", wikiPanel) !== null, "and whether agents use it (on unless switched off)")
            win.shotWindow("clicks-wiki-pages")
            // (Off the rail first: its button's tooltip can sit over the row.)
            TestInput.moveTo(win.width / 2, win.height / 2) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: win.clickItem(win.findAllNames("wikiPageRow", wikiPanel)[0], "the Quickstart row") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "quickstart.md" && win.wikiProse("job runner") !== null, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const meta = win.findName("wikiPageMeta", wikiPanel)
            win.check(meta && meta.text === "Overview", "a page says what kind it is (" + (meta ? meta.text : "") + ")")
            win.check(win.findText("Where to start.", wikiPanel) !== null, "and its summary")
            const title = win.findName("wikiPageTitle", wikiPanel), lede = win.findName("wikiPageLede", wikiPanel)
            win.check(title && title.text === "Quickstart" && lede && title.y < lede.y && lede.y < meta.y
                      && win.findAllNames("wikiProse", wikiPanel).every(t => !t.visible || t.text.indexOf(">Quickstart<") < 0),
                      "its title first, then its summary and kind, the title not again in its text")
            win.clickLink(win.wikiProse("job runner"), "job runner") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md", 10000) }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.findAllNames("wikiDiagram", wikiPanel).some(d => d.ready), 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(true, "a link to another page opens it in the panel")
            const back = win.findName("wikiBack", wikiPanel)
            win.check(back && back.text === "← Quickstart", "with Back to the page before (" + (back ? back.text : "") + ")")
            win.check(win.findAllNames("wikiDiagram", wikiPanel).some(d => d.ready), "its diagram drawn as a picture")
            const code = win.findAllNames("wikiCode", wikiPanel)
            win.check(code.length === 1 && code[0].children[0].getText(0, code[0].children[0].length).indexOf("run-jobs --retries 3") >= 0, "its code in a code well")
            const codeText = code.length === 1 ? code[0].children[0] : null
            win.check(codeText && codeText.contentWidth <= codeText.width + 1 && codeText.lineCount > 1,
                      "its long line wrapped in the well, not cut (" + (codeText ? Math.round(codeText.contentWidth) + " in " + Math.round(codeText.width) + ", " + codeText.lineCount + " lines" : "") + ")")
            win.check(win.findName("wikiPageMeta", wikiPanel).text === "Architecture", "and its kind")
            win.shotWindow("clicks-wiki-reader")
            win.clickLink(win.wikiProse("starts in"), "README.md") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => Workspace.activeKey.endsWith("/repo/README.md"), 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Workspace.activeKey === "file:" + win.argValue("--cwd=") + "/README.md", "a link to code opens the file (" + Workspace.activeKey + ")")
            Workspace.open(win.harloKey) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(wikiPanel.openPage === "architecture/jobs.md", "and back in the session the page is still open")
            win.clickItem(win.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "quickstart.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(win.findName("wikiBack", wikiPanel).text === "← Pages", "Back returns to it, then to the pages")
            win.clickItem(win.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.wikiRows().length === 3, "the pages again")
            win.clickItem(win.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.typeText("failed job retried")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => !wikiPanel.searching && win.findAllNames("wikiHit", wikiPanel).length > 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const hits = win.findAllNames("wikiHit", wikiPanel)
            win.check(hits[0].modelData.page.endsWith("architecture/jobs.md") && hits[0].modelData.section === "retry-control" && !wikiPanel.results.plain,
                      "OpenWiki's search finds the section (" + JSON.stringify(hits[0].modelData) + ")")
            win.check(win.findText("Retry control", wikiPanel) !== null, "shown by its heading")
            const snip = win.findName("wikiHitSnippet", hits[0])
            win.check(snip && snip.visible && /<b><font[^>]*>(failed|job|retried)<\/font><\/b>/i.test(snip.text)
                      && hits.every(h => !/-->|\| *-{3}/.test(h.modelData.snippet)),
                      "its text says why it matched, the query's words marked, no diagram or table syntax (" + (snip ? snip.text : "-") + ")")
            const fill = wikiPanel.marked("How a job is retried", "how is a job retried")
            win.check(fill.split("<b>").length === 3 && fill.indexOf(">job<") > 0 && fill.indexOf(">retried<") > 0 && wikiPanel.marked("How it works", "how").split("<b>").length === 2,
                      "filler words left unmarked, unless the query is only those (" + fill + ")")
            win.shotWindow("clicks-wiki-search")
            win.clickItem(hits[0], "the first result") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findName("wikiBack", wikiPanel).text === "← Results", "a result opens its page, with Back to the results")
            win.clickItem(win.findName("wikiBack", wikiPanel), "Back") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findAllNames("wikiHit", wikiPanel).length > 0, "the results again")
            win.clickItem(win.findName("wikiSearch", wikiPanel), "the search field")
            TestInput.key(Qt.Key_Escape, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.wikiRows().length === 3 && win.findName("wikiSearch", wikiPanel).text === "", "Escape clears the search")
            win.clickItem(win.findName("wikiViewGraph", wikiPanel), "Graph") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.findAllNames("wikiGraphBox", wikiPanel).length === 3, 20000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findAllNames("wikiGraphBox", wikiPanel).length === 3, "the graph: a box per page, to click")
            win.check(wikiPanel.graphSource.indexOf("p0 --> p1") >= 0 && wikiPanel.graphSource.indexOf("p2 --> p1") >= 0, "and an arrow per link (" + wikiPanel.graphSource.replace(/\n/g, " ") + ")")
            win.shotWindow("clicks-wiki-graph")
            const box = win.findAllNames("wikiGraphBox", wikiPanel).find(b => b.page.path === "workflows/release.md")
            win.clickItem(box, "the Releasing box") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "workflows/release.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const meta = win.findName("wikiPageMeta", wikiPanel)
            win.check(meta.text.indexOf("Workflow · checked against the code ") === 0, "a box opens its page, which says when it was checked (" + meta.text + ")")
            // A link to a section of another page: opened at it.
            win.clickLink(win.wikiProse("the jobs"), "the jobs") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => wikiPanel.openPage === "architecture/jobs.md" && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findName("wikiBack", wikiPanel).text === "← Releasing", "a link to a section opens its page")
            // A run that stopped part way: Continue.
            const cwd = win.argValue("--cwd=")
            TestInput.writeFile(cwd + "/openwiki/.run.json", JSON.stringify({ mode: "update", phase: "generating", startedAt: new Date().toISOString(),
                plan: { pages: [{ path: "/openwiki/quickstart.md", status: "complete" }, { path: "/openwiki/a.md", status: "pending" }, { path: "/openwiki/b.md", status: "pending" }] } }))
            wikiPanel.load() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const fresh = win.findName("wikiFreshness", wikiPanel)
            win.check(fresh && fresh.text.replace(/\u00a0/g, " ") === "A run stopped part way (1 of 3 pages).", "a run that stopped part way says so (" + (fresh ? fresh.text.replace(/\u00a0/g, " ") : "") + ")")
            win.check(win.findName("wikiRun", wikiPanel).text === "Continue", "with Continue")
            TestInput.writeFile(win.argValue("--cwd=") + "/openwiki/.run.json", "")   // (unreadable: no run)
            // A project without a wiki: what it is, and Create.
            win.harloKey = Workspace.newSession(win.wikiProject2, "harlo") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.findName("wikiCreate", wikiPanel) !== null, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findText("No wiki yet", wikiPanel) !== null && win.findName("wikiCreate", wikiPanel).enabled, "a project without one: what a wiki is, and Create")
            win.check(wikiPanel.openPage === "" && win.findName("wikiSearch", wikiPanel) === null, "another repository starts afresh")
            win.shotWindow("clicks-wiki-empty")
            win.wikiKeyBefore = Workspace.activeKey
            win.clickItem(win.findName("wikiCreate", wikiPanel), "Create wiki") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => Workspace.activeKey !== win.wikiKeyBefore && chat.session.key === Workspace.activeKey, 20000) }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => /Wiki: (complete|no update|the page|unexpected)/.test(win.chatText()) && chat.session.status === "idle", 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiPanelSteps, () => win.wikiRows().length === 1, 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.session.title === "Wiki: create" && win.chatText().indexOf("Wiki: complete") >= 0, "Create runs a wiki session (" + chat.session.title + ")")
            win.check(Workspace.setting("wiki_" + win.wikiProject2) === "1", "and turns the wiki on for the project's agents")
            win.check(JSON.stringify(win.wikiRows()) === JSON.stringify(["Quickstart"]), "its page shows in the panel (" + JSON.stringify(win.wikiRows()) + ")")
            const fresh = win.findName("wikiFreshness", wikiPanel)
            win.check(fresh && fresh.text.replace(/\u00a0/g, " ") === "Updated just now · up to date", "up to date (" + (fresh ? fresh.text.replace(/\u00a0/g, " ") : "") + ")")
            win.check(win.findName("wikiRun", wikiPanel).variant === "outline", "Update is quiet then")
            win.check(win.findText("Agents search it, in sessions started from now on.", wikiPanel) !== null, "and agents use it")
            win.shotWindow("clicks-wiki-created")
            win.clickItem(win.findName("wikiSwitch", wikiPanel), "the panel's Wiki switch") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Workspace.setting("wiki_" + win.wikiProject2) === "0" && win.findText("Agents don't use it: switched off for this project.", wikiPanel) !== null, "switched off, it says so")
            win.shotWindow("clicks-wiki-off")
            settingsDialog.projectId = win.wikiProject2
            settingsDialog.openPage("projects") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            // (Settings → Projects: its Wiki row, scrolled to.)
            const sw = win.findName("projectWiki", settingsDialog.contentItem)
            const f = win.findName("settingsPage", settingsDialog.contentItem)
            if (sw && f) f.contentY = Math.max(0, Math.min(sw.mapToItem(f.contentItem, 0, 0).y - 160, f.contentHeight - f.height))
            win.check(sw !== null && !sw.checked, "Settings shows it off too") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.shotWindow("clicks-wiki-settings")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
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
    // the quickstart first, and how far behind the code it is; every page
    // opens; every link to a page goes to one, every link to code to a file
    // and lines that are there, every diagram draws; the graph has a box
    // per page; OpenWiki's search finds the pages that answer each of
    // wiki-queries.json; and a Harlo session searches and reads it.
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
            win.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("wiki_" + win.wikiProject, "1")
            win.harloKey = Workspace.newSession(win.wikiProject, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo")
            windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => wikiPanel.pages.length > 0, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pages = wikiPanel.pages
            win.demoPages = pages.map(p => p.path)
            win.check(pages.length >= 3, "the wiki has pages (" + pages.length + ": " + win.demoPages.join(", ") + ")")
            win.check(pages[0].path === "quickstart.md", "the quickstart first (" + pages[0].path + ")")
            win.check(pages.every(p => p.title !== ""), "every page has a title")
            const behind = Number(win.argValue("--wiki-behind=") || "-1")
            const fresh = win.findName("wikiFreshness", wikiPanel)
            if (behind >= 0)
                win.check(fresh && fresh.text.replace(/\u00a0/g, " ").endsWith(behind === 0 ? "· up to date" : "· " + behind + (behind === 1 ? " commit" : " commits") + " since"), "how far behind the code (" + (fresh ? fresh.text.replace(/\u00a0/g, " ") : "") + ")")
            win.demoQueries = JSON.parse(TestInput.readFile(win.argValue("--wiki-queries=")) || "[]")
            win.shotWindow("wiki-demo-pages")
            // Every page, opened in the panel and checked.
            win.walk(win.demoPages, p => wikiPanel.openWiki(p, "", false),
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
                         if (!ok || !v || v.error || (v.blocks || []).length === 0) { win.demoProblems.opened.push(p + ": " + (v ? v.error || "no text" : "didn't open")); return }
                         const dir = p.split("/").slice(0, -1)
                         for (const b of v.blocks) {
                             if (b.kind === "mermaid") {
                                 win.demoDiagrams++
                                 const d = JSON.parse(Diagrams.diagram(b.code, Theme.isLight, "{}"))
                                 if (d.state !== "ready") win.demoProblems.diagrams.push(p + ": " + (d.error || d.state))
                             }
                             // (No matchAll in Qt's JavaScript.)
                             const hrefs = /href="([^"]+)"/g
                             for (let m = hrefs.exec(String(b.html || "")); m !== null; m = hrefs.exec(String(b.html || ""))) {
                                 const href = m[1].replace(/&amp;/g, "&")
                                 if (href.startsWith("repo://")) {
                                     win.demoCode++
                                     const c = /^repo:\/\/([^#]+)(?:#L(\d+)(?:-L(\d+))?)?/.exec(href)
                                     const text = c ? win.demoFile(decodeURIComponent(c[1])) : ""
                                     const last = c && c[3] ? Number(c[3]) : c && c[2] ? Number(c[2]) : 1
                                     if (text === "" || text.split("\n").length < last) win.demoProblems.code.push(p + " → " + href)
                                 } else if (/\.md(#|$)/.test(href) && !/^[a-z]+:/.test(href)) {
                                     win.demoLinks++
                                     const parts = href.startsWith("/") ? [] : dir.slice()
                                     for (const s of href.split("#")[0].split("/")) { if (s === "..") parts.pop(); else if (s !== "." && s !== "") parts.push(s) }
                                     const to = parts.join("/").replace(/^openwiki\//, "")
                                     if (win.demoPages.indexOf(to) < 0) win.demoProblems.links.push(p + " → " + href)
                                 }
                             }
                         }
                         // (Its sources: files and the lines its claims cite.)
                         for (const src of v.sources || []) {
                             win.demoCode++
                             const text = win.demoFile(src.path)
                             const last = Math.max(1, ...(src.ranges || []).map(r => r[1]))
                             if (text === "" || text.split("\n").length < last) win.demoProblems.code.push(p + " → source " + src.path + " L" + last)
                         }
                         // (Shown under the page, a link each.)
                         if ((v.sources || []).length && win.findAllNames("wikiSource", wikiPanel).length !== v.sources.length)
                             win.demoProblems.code.push(p + ": " + win.findAllNames("wikiSource", wikiPanel).length + " of " + v.sources.length + " sources shown")
                         if (!win.demoBest || v.blocks.length > 6) win.demoBest = p
                     }, 30000) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => walker.done, 600000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const pr = win.demoProblems
            win.check(pr.opened.length === 0, "every page opens (" + win.demoPages.length + " read" + (pr.opened.length ? "; " + pr.opened.join("; ") : "") + ")")
            win.check(pr.links.length === 0, "every link to a page goes to one (" + win.demoLinks + " links" + (pr.links.length ? "; broken: " + pr.links.join("; ") : "") + ")")
            win.check(win.demoCode > 0 && pr.code.length === 0, "every link to code (and source) goes to a file and lines that are there (" + win.demoCode + " links" + (pr.code.length ? "; broken: " + pr.code.join("; ") : "") + ")")
            win.check(pr.diagrams.length === 0, "every diagram draws (" + win.demoDiagrams + (pr.diagrams.length ? "; failed: " + pr.diagrams.join("; ") : "") + ")")
            wikiPanel.openWiki(win.demoBest, "", false) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => wikiPanel.openPage === win.demoBest && wikiPanel.loadingPage === "", 10000) }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.shotWindow("wiki-demo-reader")
            // (A flowchart drawn across shows down when that fits larger.)
            const drawn = win.findAllNames("wikiDiagram", wikiPanel).filter(d => d.ready)
            win.check(drawn.every(d => !d.down || d.down.state !== "ready" || d.fit + 0.001 >= Math.max(0.7, Math.min(1, d.width / Math.max(1, d.across.width)))),
                      "its diagrams shown the way they fit larger (" + drawn.map(d => d.downward ? "down" : "across").join(", ") + ")")
            // (Its end: the files and lines it comes from.)
            const reader = win.findName("wikiReader", wikiPanel)
            if (reader) reader.contentY = Math.max(0, reader.contentHeight - reader.height) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.shotWindow("wiki-demo-sources")
            const edge = win.findName("wikiBackEdge", wikiPanel)
            win.check(edge && Qt.colorEqual(edge.color, Theme.hairline), "scrolled, the page has an edge under Back")
            wikiPanel.closePage()
            wikiPanel.mode = "graph" } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => win.findAllNames("wikiGraphBox", wikiPanel).length > 0 || (wikiPanel.graphPic && wikiPanel.graphPic.state === "failed"), 30000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const boxes = win.findAllNames("wikiGraphBox", wikiPanel).length
            win.check(boxes === wikiPanel.graphPages.length, "the graph: a box per page (" + boxes + " of " + wikiPanel.graphPages.length + (wikiPanel.graphPic && wikiPanel.graphPic.error ? "; " + wikiPanel.graphPic.error : "") + ")")
            const cap = win.findName("wikiGraphCaption", wikiPanel)
            win.check(!cap || !cap.visible || cap.columns === (wikiPanel.width < 360 ? 1 : 2), "its zoom beside the caption, or under it in a narrow panel")
            const gbox = win.findName("wikiGraph", wikiPanel), gpic = win.findName("wikiGraphPicture", wikiPanel), zoom = win.findName("wikiGraphZoom", wikiPanel)
            win.check(gpic && zoom && zoom.text === Math.round(gpic.fit * 100) + "%", "its zoom says the size it's drawn at (" + (zoom ? zoom.text : "-") + ", fit " + (gpic ? gpic.fit.toFixed(2) : "-") + ")")
            const capText = cap ? cap.children[0].text : ""
            win.check(!!gbox && gbox.wider === /scroll sideways/.test(capText) && !!win.findName("wikiGraphFull", wikiPanel),
                      "wider than the panel, its caption says to scroll or open it full size, with a button for that (" + (gbox ? gbox.wider : "-") + ")")
            const last = wikiPanel.graphPages.findIndex(p => p.path === wikiPanel.lastRead)
            const qi = last >= 0 ? last : wikiPanel.graphPages.findIndex(p => p.path === "quickstart.md")
            const qbox = win.findAllNames("wikiGraphBox", wikiPanel).find(b => b.index === qi)
            const qx = qbox ? qbox.mapToItem(gbox, 0, 0).x : -1
            win.check(last >= 0 && qbox && qbox.lastRead && win.findAllNames("wikiGraphBox", wikiPanel).filter(b => b.lastRead).length === 1,
                      "the page read last ringed in it (" + wikiPanel.lastRead + ")")
            win.check(qi < 0 || (qbox && qx >= 0 && qx + qbox.width <= gbox.width + 1), "it opens with the page read last in view (" + Math.round(qx) + "+" + (qbox ? Math.round(qbox.width) : "-") + " in " + (gbox ? Math.round(gbox.width) + ", scrolled " + Math.round(gbox.contentX) + " of " + Math.round(gbox.contentWidth) : "-") + ")")
            win.shotWindow("wiki-demo-graph")
            wikiPanel.mode = "pages"
            // Each search, as the panel asks it of OpenWiki.
            win.walk(win.demoQueries, q => wikiPanel.search(q.q), q => !wikiPanel.searching && wikiPanel.query === q.q,
                     (q, ok) => {
                         const hits = ok && wikiPanel.results ? wikiPanel.results.hits.map(h => h.page.replace(/^openwiki\//, "")) : []
                         const plain = !!(wikiPanel.results && wikiPanel.results.plain)
                         const good = hits.length > 0 && !plain && (q.pages.length === 0 || q.pages.some(p => hits.slice(0, 3).indexOf(p) >= 0))
                         if (!good) win.demoProblems.search.push("“" + q.q + "” → " + (plain ? "(plain) " : "") + (hits.slice(0, 3).join(", ") || "nothing"))
                         // (No hit reads as a flattened code block.)
                         const code = ok && wikiPanel.results ? wikiPanel.results.hits.find(h => /(^|\s)(python|rust|sh|bash|js|ts)\s+\S*[(@{=]/.test(h.snippet)) : null
                         if (code) win.demoProblems.search.push("“" + q.q + "” shows code: " + code.snippet.slice(0, 60))
                     }, 30000) } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => walker.done, 300000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(win.demoProblems.search.length === 0, "OpenWiki's search finds the page for each question (" + win.demoQueries.length + (win.demoProblems.search.length ? "; missed: " + win.demoProblems.search.join("; ") : "") + ")")
            const snips = wikiPanel.results ? wikiPanel.results.hits.map(h => h.snippet).join(" | ") : ""
            win.check(/\b(place|amend|pay|refund)_order\b/.test(snips) && !/\b(place|amend|pay|refund) order\b/.test(snips),
                      "its hits keep the code's names (place_order, not \"place order\") (" + snips.slice(0, 160) + ")")
            win.shotWindow("wiki-demo-search")
            wikiPanel.search("") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendWiki("search the wiki for " + (win.demoQueries[0] ? win.demoQueries[0].q : "orders")) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => win.wikiAnswered(), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const t = win.lastToolResult()
            const found = /openwiki\/([\w\/.-]+\.md)#([\w-]*)/.exec(t)
            win.check(found !== null, "a Harlo session searches it (" + t.slice(0, 200) + ")")
            win.demoBest = found ? found[1] + "#" + found[2] : "quickstart.md"
            win.sendWiki("read the wiki at openwiki/" + win.demoBest) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiDemoSteps, () => win.wikiAnswered(), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const t = win.lastToolResult()
            win.check(t.length > 80 && !/error|not found|Invalid/i.test(t.slice(0, 120)), "and reads what it found (" + win.demoBest + ": " + t.slice(0, 160) + ")")
            win.shotWindow("wiki-demo-session")
            win.summary(); Qt.quit() } }
    }

    // Merged work changing code the wiki cites: an offer in the corner to
    // update it; Not now holds until main moves again; Update starts the
    // update's session.
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
            win.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("project_model_" + win.wikiProject + "_harlo", "fake/fake-helper")
            Workspace.checkWikiOffers() } }
        ScriptAction { script: win.waitUntil(wikiOfferSteps, () => win.findName("wikiOffer-" + win.wikiProject) !== null, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const toast = win.findName("wikiOffer-" + win.wikiProject)
            const text = toast ? win.findName("wikiOfferText", toast).text : ""
            win.check(text.indexOf("2 commits on main since its last update change a.rs.") === 0, "an offer: main's commits since the update, and only the cited file they change (" + text + ")")
            win.check(win.findText("Merged work changes what repo's wiki describes", toast) !== null, "naming the project")
            win.shotWindow("clicks-harlo-wiki-offer")
            win.clickItem(win.findName("wikiOfferLater", toast), "Not now") } }
        ScriptAction { script: win.waitUntil(wikiOfferSteps, () => win.findName("wikiOffer-" + win.wikiProject) === null, 5000) }
        ScriptAction { script: {
            win.check(win.findName("wikiOffer-" + win.wikiProject) === null, "Not now: it goes")
            Workspace.checkWikiOffers() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("wikiOffer-" + win.wikiProject) === null, "and doesn't come back while main stays where it is")
            // (As if main had moved since.)
            Workspace.setSetting("wiki_seen_" + win.wikiProject, "")
            Workspace.checkWikiOffers() } }
        ScriptAction { script: win.waitUntil(wikiOfferSteps, () => win.findName("wikiOffer-" + win.wikiProject) !== null, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.wikiKeyBefore = Workspace.activeKey
            win.clickItem(win.findName("wikiOfferUpdate", win.findName("wikiOffer-" + win.wikiProject)), "Update the wiki") } }
        ScriptAction { script: win.waitUntil(wikiOfferSteps, () => Workspace.activeKey !== win.wikiKeyBefore && chat.session.key === Workspace.activeKey && chat.session.title === "Wiki: update", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.session.title === "Wiki: update", "Update starts the update's session (" + chat.session.title + ")")
            win.check(win.findName("wikiOffer-" + win.wikiProject) === null, "and the offer goes")
            win.summary(); Qt.quit() } }
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
            win.wikiProject = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setSetting("wiki_" + win.wikiProject, "1")
            Workspace.setSetting("default_harness", win.argValue("--agent=") || "harlo")
            if (win.argValue("--model="))
                Workspace.setSetting("project_model_" + win.wikiProject + "_" + (win.argValue("--agent=") || "harlo"), win.argValue("--model="))
            const mode = win.argValue("--wiki-mode=")
            if (mode === "lookup") {
                win.harloKey = Workspace.newSession(win.wikiProject, win.argValue("--agent=") || "harlo")
                Workspace.setApprovalMode(win.harloKey, "yolo")
            } else {
                win.wikiKeyBefore = Workspace.activeKey
                const err = Workspace.startWikiRun("project:" + win.wikiProject, mode)
                if (err !== "") { win.log("not started:", err); Qt.quit() }
            } } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.waitUntil(wikiRealSteps, () => win.harloKey !== "" ? chat.session.modelName !== "" || chat.session.harness === "claude"
                                                                         : Workspace.activeKey !== win.wikiKeyBefore && Workspace.activeKey !== "", 120000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.log("session", chat.session.key, chat.session.harness, chat.session.modelName, chat.session.title)
            if (win.harloKey === "") {
                win.harloKey = Workspace.activeKey
                Workspace.setApprovalMode(win.harloKey, "yolo")
            } else {
                chat.session.send(TestInput.readFile(win.argValue("--prompt-file=")).trim(), "normal")
            }
            win.realStarted = false; win.realQuiet = 0; win.realText = win.chatText().length; win.realBegin = Date.now() } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.waitUntil(wikiRealSteps, () => {
            if (chat.session.key !== win.harloKey) Workspace.open(win.harloKey)
            const st = JSON.parse(Workspace.wikiJson("project:" + win.wikiProject))
            const busy = chat.session.status !== "idle" || st.running
            // (Started: busy, or a reply already there, or idle for 20 s
            // since: done before it was looked at.)
            if (busy || win.chatText().length > win.realText + 40 || Date.now() - win.realBegin > 20000) win.realStarted = true
            if (busy || !win.realStarted || chat.session.hasRequest) { win.realQuiet = 0; return false }
            if (!win.realQuiet) win.realQuiet = Date.now()
            return Date.now() - win.realQuiet > 15000
        }, 3 * 3600 * 1000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const st = JSON.parse(Workspace.wikiJson("project:" + win.wikiProject))
            TestInput.writeFile(win.selftestDir + "/wiki-real.json", JSON.stringify({
                mode: win.argValue("--wiki-mode="), agent: chat.session.harness, model: chat.session.modelName, title: chat.session.title,
                status: chat.session.status, request: chat.session.hasRequest, wiki: st, chat: win.chatText().slice(-6000) }, null, 1))
            win.log("done", chat.session.status, st.pages.length, "pages")
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

    // The wiki on another machine (the test's SSH server; the demo project
    // there): the bundled OpenWiki goes into the host's cache once, its
    // server starts there over SSH, the panel lists, reads and searches the
    // pages there, and a Harlo session there searches and reads them.
    function hostOpenWiki() {
        const cache = win.selftestDir + "/sshd/home/.cache/harlo"
        return TestInput.listDir(cache).split("\n").filter(n => n.startsWith("openwiki-")).map(n => cache + "/" + n)
    }
    SequentialAnimation {
        id: wikiRemoteSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlowikiremote"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            if (win.argValue("--theme-mode=")) Workspace.chooseThemeMode(win.argValue("--theme-mode="))
            Workspace.setProviderEnabled("claude", false)
            win.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => JSON.parse(Remotes.hostsJson())[0].state === "connected", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(win.hostOpenWiki().length === 0, "no OpenWiki on box yet")
            Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo")
            win.wikiProject = p ? Number(p[0]) : -1
            win.check(Workspace.instance === "box" && win.wikiProject >= 0 && p[2] === "harlo", "the demo project on box, with Harlo there")
            Workspace.setSetting("wiki_" + win.wikiProject, "1")
            win.harloKey = Workspace.newSession(win.wikiProject, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo")
            windowSettings.rightMode = "wiki" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => wikiPanel.pages.length > 0 && !wikiPanel.st.loading, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pages = wikiPanel.pages, fresh = win.findName("wikiFreshness", wikiPanel)
            win.check(wikiPanel.st.host === "box" && wikiPanel.st.root === "ssh://box" + win.argValue("--cwd="), "the panel reads the wiki on box (" + wikiPanel.st.root + ")")
            win.check(pages.length === 6 && pages[0].path === "quickstart.md", "its pages, the quickstart first (" + pages.map(p => p.path).join(", ") + ")")
            win.check(fresh && fresh.text.replace(/\u00a0/g, " ").endsWith("· 1 commit since"), "how far behind box's code (" + (fresh ? fresh.text.replace(/\u00a0/g, " ") : "") + ")")
            win.check(win.findText("repo on box", wikiPanel) !== null, "its head names the host")
            win.shotWindow("wiki-remote-pages")
            wikiPanel.openWiki("architecture/orders.md", "refunds", false) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => wikiPanel.openPage === "architecture/orders.md" && wikiPanel.loadingPage === "" && wikiPanel.pageView
                                              && (wikiPanel.pageView.blocks || []).filter(b => b.kind === "mermaid").every(b => JSON.parse(Diagrams.diagram(b.code, Theme.isLight, "{}")).state !== "drawing"), 20000) }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            const v = wikiPanel.pageView, blocks = v.blocks || []
            win.check(!v.error && v.title === "Orders" && blocks.length > 5, "a page read from box (" + (v.error || v.title + ", " + blocks.length + " blocks") + ")")
            const mer = blocks.filter(b => b.kind === "mermaid")
            const drawn = mer.length ? JSON.parse(Diagrams.diagram(mer[0].code, Theme.isLight, "{}")) : {}
            win.check(mer.length === 1 && drawn.state === "ready" && win.findAllNames("wikiDiagram", wikiPanel).length >= 1, "its diagram drawn (" + mer.length + ", " + drawn.state + " " + (drawn.error || "") + ", " + win.findAllNames("wikiDiagram", wikiPanel).length + " shown)")
            win.check(blocks.some(b => String(b.html || "").indexOf("repo://shipyard/orders/service.py#L") >= 0), "its links to box's code")
            win.shotWindow("wiki-remote-reader")
            wikiPanel.closePage()
            wikiPanel.search("assorted pallet weight") } }
        PauseAnimation { duration: 300 }
        // (The first search puts OpenWiki on box: some 250 MB over SSH.)
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => !wikiPanel.searching && wikiPanel.query === "assorted pallet weight", 150000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const r = wikiPanel.results || {}, hits = (r.hits || []).map(h => h.page.replace(/^openwiki\//, ""))
            win.check(!r.plain && hits[0] === "architecture/manifest.md", "OpenWiki on box finds the page (" + (r.plain ? "plain text search: " : "") + hits.slice(0, 3).join(", ") + (r.error ? " " + r.error : "") + ")")
            const ow = win.hostOpenWiki()
            win.check(ow.length === 1 && TestInput.listDir(ow[0] + "/bin").split("\n").indexOf("node") >= 0 && TestInput.readFile(ow[0] + "/VERSION") !== "", "put in box's cache, once (" + ow.join(", ") + ")")
            win.check(TestInput.listDir(win.selftestDir + "/data/harlo").split("\n").every(n => !n.startsWith("openwiki")), "nothing of it in this computer's data")
            win.shotWindow("wiki-remote-search")
            wikiPanel.search("") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => chat.session.modelName !== "", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(chat.session.harness === "harlo" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "a Harlo session on box (" + chat.session.cwd + ")")
            win.sendWiki("search the wiki for how to add a new report") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => win.wikiAnswered(), 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const t = win.lastToolResult()
            const found = /openwiki\/([\w\/.-]+\.md)#([\w-]*)/.exec(t)
            win.check(found !== null && found[1] === "workflows/reports.md", "the session searches the wiki on box (" + t.slice(0, 200) + ")")
            win.demoBest = found ? found[1] + "#" + found[2] : "quickstart.md"
            win.sendWiki("read the wiki at openwiki/" + win.demoBest) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(wikiRemoteSteps, () => win.wikiAnswered(), 60000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const t = win.lastToolResult()
            win.check(t.length > 80 && t.indexOf("register") >= 0 && !/error|not found|Invalid/i.test(t.slice(0, 120)), "and reads it (" + win.demoBest + ": " + t.slice(0, 160) + ")")
            win.check(win.hostOpenWiki().length === 1, "the same OpenWiki on box served both")
            win.shotWindow("wiki-remote-session")
            win.summary(); Qt.quit() } }
    }

    // A diagram that doesn't parse: harlo tells the agent what's wrong
    // when its turn ends and it sends the diagram again, fixed, drawn as a
    // picture; one sent broken again isn't asked about a second time.
    function harloNotes() {
        const reqs = win.modelRequests()
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
        ScriptAction { script: win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(diagramSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("draw a broken diagram") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(diagramSteps, () => win.chatText().indexOf("Again:") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const notes = win.harloNotes()
            const said = notes.length ? JSON.stringify(notes[0].content) : ""
            win.check(notes.length === 1 && /line \d+: /.test(said), "harlo tells the agent what's wrong in its diagram (" + said.slice(0, 260) + ")")
            const pics = win.findAllNames("diagram", chat.transcript)
            win.check(pics.length >= 1, "the diagram sent again is drawn (" + pics.length + " pictures)")
            win.check(win.modelRequests().length === 2, "and harlo asks no more (" + win.modelRequests().length + " requests)")
            win.shotWindow("clicks-harlo-diagram")
            win.clickItem(pics[pics.length - 1], "the diagram") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            win.check(v && v.opened && v.pictures.length === 1 && v.title === "Diagram" && v.fit > 1 && win.findName("viewerZoom") !== null,
                      "clicking the diagram opens it full size in harlo, a small one enlarged (" + (v ? v.fit.toFixed(2) : "") + ")")
            win.shotWindow("clicks-harlo-diagram-viewer")
            v.close()
            win.diagramKey2 = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(diagramSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("draw a stubborn diagram") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(diagramSteps, () => win.chatText().indexOf("Again:") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const reqs = win.modelRequests()
            win.check(reqs.length === 4, "broken again: harlo asks only once (" + reqs.length + " requests)")
            win.summary(); Qt.quit() } }
    }

    // Options and mockups in a reply: a comparison drawn as cards (its code
    // a click away), mockups the agent wrote shown as pictures with the one
    // before them, taken again when it edits the file, and a mockup of a
    // file that isn't there sent back to the agent.
    property string mockUrl: ""
    function visualRows() { return win.findAllNames("replyVisual", chat.transcript) }
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const said = JSON.stringify(((win.modelRequests()[0] || {}).messages || []).filter(m => m.role === "system"))
            win.sendByKeys("compare the approaches") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => win.chatText().indexOf("I'd match by key") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const said = JSON.stringify(((win.modelRequests()[0] || {}).messages || []).filter(m => m.role === "system"))
            win.check(said.indexOf("```options") >= 0 && said.indexOf("```mockup") >= 0, "the agent is told about options and mockups")
            const cards = win.findAllNames("optionCard", chat.transcript)
            const marked = win.findAllNames("optionRecommended", chat.transcript)
            win.check(cards.length === 2 && marked.length === 1, "the options are drawn as cards, the recommended one marked (" + cards.length + ", " + marked.length + ")")
            win.check(win.findText("nothing to restore", chat.transcript) !== null && win.findText("the list still redraws", chat.transcript) !== null, "with their pros and cons")
            const about = win.findAllNames("optionAbout", chat.transcript).find(l => l.text.indexOf("key") >= 0)
            win.check(about && about.textFormat === Text.RichText && about.text.indexOf("<code>key</code>") >= 0 && about.text.indexOf("`") < 0,
                      "a card's Markdown is drawn: `key` as code, no backticks (" + (about ? about.text.replace(/^<style>.*<\/style>/, "") : "none") + ")")
            const plainOne = win.findAllNames("optionAbout", chat.transcript).find(l => l.text === "Smaller change.")
            win.check(plainOne && plainOne.textFormat === Text.PlainText, "a line with no marks stays plain text")
            win.shotWindow("clicks-harlo-options")
            win.clickText("Code", chat.transcript) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findAllNames("optionCard", chat.transcript).length === 0 && win.findText("Cards", chat.transcript) !== null, "Code shows the block's text, Cards the cards again")
            win.clickText("Cards", chat.transcript) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // Waiting for your answer: a Choose on each card; one sends its name.
            const chooses = win.findAllNames("optionChoose", chat.transcript).filter(b => b.visible)
            win.check(chooses.length === 2 && chooses.some(b => b.variant === "primary"), "a Choose button on each card, the recommended one filled (" + chooses.length + ")")
            const [p0, p1] = chooses.map(b => b.mapToItem(null, 0, 0))
            win.check(chooses.length === 2 && (Math.abs(p0.x - p1.x) < 1 || Math.abs(p0.y - p1.y) < 1),
                      "side by side, the buttons line up (" + (p0 ? p0.y.toFixed(0) : "") + ", " + (p1 ? p1.y.toFixed(0) : "") + ")")
            win.clickItem(chooses.find(b => b.Accessible.name === "Choose Match rows by key"), "Choose on the recommended card") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => win.chatText().indexOf("ok: Match rows by key") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const sent = ((win.modelRequests().slice(-1)[0] || {}).messages || []).filter(m => m.role === "user").slice(-1)[0]
            const said = sent ? (typeof sent.content === "string" ? sent.content : JSON.stringify(sent.content)) : ""
            win.check(said.indexOf("Match rows by key") >= 0, "the choice's name is sent as your answer (" + said.substring(0, 60) + ")")
            const chosen = win.findAllNames("optionChosen", chat.transcript).filter(l => l.visible)
            const cards = win.findAllNames("optionCard", chat.transcript)
            win.check(chosen.length === 1 && win.findAllNames("optionChoose", chat.transcript).every(b => !b.visible), "the card is marked Chosen, the buttons gone")
            win.check(cards.filter(c => c.opacity < 0.9).length === 1, "the other card steps back")
            win.sendByKeys("mock up the settings") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => win.chatText().indexOf("That's the change") >= 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => { const p = win.findName("mockupPicture"); return p !== null && p.visible }, 15000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pic = win.findName("mockupPicture")
            win.check(pic && pic.visible && pic.width <= 642 && Math.abs((pic.width - 2) / (pic.height - 2) - 1.6) < 0.02, "the mockup is shown as its picture, in its window's proportions, fitted to the column (" + (pic ? pic.width + "x" + pic.height : "none") + ")")
            const cap = win.findName("mockupCaption")
            const ba = win.findName("mockupBeforeAfter")
            win.check(cap && cap.text === "Settings with Sync now" && ba && ba.visible && ba.current === 1, "with its title, After chosen of Before | After (" + (cap ? cap.text : "") + ")")
            win.mockUrl = pic.children[0].source.toString()
            win.shotWindow("clicks-harlo-mockup") } }
        // (Both pictures taken: a real browser takes a moment.)
        ScriptAction { script: win.waitUntil(visualSteps, () => { const m = win.findName("mockupView"); return m && m.mock && m.mock.before && m.mock.before.state === "ready" && m.mock.after.state === "ready" }, 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.clickItem(win.findName("mockupPicture"), "the mockup's picture") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            const frame = win.findName("viewerPictureFrame")
            win.check(v && v.opened && v.pictures.length === 2 && v.pic.label === "After" && frame && frame.width > 0,
                      "clicking it opens it full size in harlo, After shown, Before beside it (" + (v ? v.pictures.length : "none") + "; " + (() => { const m = win.findName("mockupView"); return m && m.mock ? (m.mock.before ? m.mock.before.state : "-") + "/" + m.mock.after.state : "" })() + ")")
            win.shotWindow("clicks-harlo-mockup-viewer")
            // (Shown whole at its own size, there's nothing to switch.)
            if (v.fit < 1) win.clickItem(win.findName("viewerZoom"), "100%")
            else win.check(win.findName("viewerZoom") === null, "no 100% when it's shown at its own size") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            win.check(Math.abs(v.scaleNow - 1) < 0.001 && Math.abs(win.findName("viewerPictureFrame").width - v.pic.width - 2) <= 1, "100% shows it at its own size (" + v.scaleNow + ", " + win.findName("viewerPictureFrame").width + " for " + v.pic.width + ")")
            win.clickItem(win.findName("viewerPictureBefore"), "Before in the viewer") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const v = chat.transcript.pictureViewer
            win.check(v.pic.label === "Before" && v.pic.url !== win.mockUrl, "Before | After switches the picture there too")
            v.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("mockupBefore"), "Before") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(visualSteps, () => { const p = win.findName("mockupPicture"); return p !== null && p.visible }, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pic = win.findName("mockupPicture")
            win.check(win.findName("mockupBeforeAfter").current === 0 && pic.children[0].source.toString() !== win.mockUrl, "Before shows how it is now")
            win.clickItem(win.findName("mockupAfter"), "After")
            win.sendByKeys("change the mockup") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => { const p = win.findName("mockupPicture"); return p !== null && p.visible && p.children[0].source.toString() !== win.mockUrl && win.findName("mockupBeforeAfter").current === 1 }, 15000) }
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            win.check(true, "the agent's edit of the file shows in the picture")
            win.sendByKeys("mock up a missing file") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(visualSteps, () => win.chatText().split("That's the change").length >= 3 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const notes = win.modelRequests().reduce((all, b) => all.concat(b.messages || []), []).filter(m => m.role === "user" && JSON.stringify(m.content).indexOf("a mockup names `mockups/none.html`, which isn't there") >= 0)
            win.check(notes.length >= 1, "a mockup of a file that isn't there goes back to the agent, which shows the right one")
            win.summary(); Qt.quit() } }
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            // (The helper works in its parent's mode: writes without asking.)
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(splitSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("split it please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(splitSteps, () => chat.session.hasRequest, 10000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("Split into helpers?") !== null || win.findContaining("Split into helpers", chat.requestDock) !== null, "Harlo offers the split")
            win.shotWindow("clicks-split-offer")
            win.clickText("Start helpers", chat.requestDock) } }
        PauseAnimation { duration: 300 }
        // The helper starts in its worktree, writes, reports; the parent
        // gets its report and brings the work over.
        ScriptAction { script: win.waitUntil(splitSteps, () => TestInput.readFile(win.argValue("--cwd=") + "/helper.txt") !== "", 40000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            win.check(kids.length === 1, "one helper started (" + JSON.stringify(kids) + ")")
            const t = win.chatText()
            win.check(t.indexOf("finished and reported: succeeded") >= 0, "its report comes back to Harlo")
            win.check(TestInput.readFile(win.argValue("--cwd=") + "/helper.txt") === "from the helper\n", "bring_over brings helper.txt to the project")
            win.check(git.unstagedCount + git.stagedCount > 0 || t.indexOf("helper.txt") >= 0, "as a change, nothing committed (" + t.slice(-200) + ")")
            // The helper isn't offered splits of its own part; its parent is.
            const reqs = win.modelRequests()
            const firstUser = r => { const m = (r.messages || []).find(m => m.role === "user"); return m ? JSON.stringify(m.content) : "" }
            const offers = r => (r.tools || []).some(x => x["function"].name.endsWith("propose_helpers"))
            const said = r => JSON.stringify((r.messages || []).filter(m => m.role === "system"))
            const helperReqs = reqs.filter(r => firstUser(r).indexOf("You're a helper") >= 0)
            const parentReqs = reqs.filter(r => firstUser(r) === JSON.stringify("split it please"))
            win.check(helperReqs.length > 0 && !helperReqs.some(offers) && !helperReqs.some(r => said(r).indexOf("propose_helpers") >= 0),
                      "the helper isn't offered propose_helpers, nor told of it (" + helperReqs.length + " requests)")
            win.check(parentReqs.length > 0 && parentReqs.every(offers), "its parent is")
            win.shotWindow("clicks-split-brought") } }
        PauseAnimation { duration: 200 }
        // The report shows as harlo's, not yours: not on the prompt rail, no
        // Edit & resend; Open helper shows the helper.
        ScriptAction { script: {
            // (Among harlo's rows: the report.)
            const rows = []
            const walk = it => { if (!it) return; if (it.objectName === "harloMessage" && it.visible) rows.push(it); for (const c of it.children || []) walk(c); if (it.contentItem && it.contentItem !== it) walk(it.contentItem) }
            walk(chat)
            const msg = rows.find(r => / finished/.test(win.findName("harloMessageText", r).text)) || null
            const text = msg ? win.findName("harloMessageText", msg) : null
            win.check(msg !== null && msg.objectName === "harloMessage" && msg.visible && text.text.indexOf("[harlo]") < 0,
                      "the helper's report shows as harlo's (" + (text ? text.text.slice(0, 120) : "none") + ")")
            win.check(text !== null && !/live:|report_done|\(local:|session "/.test(text.text), "it says what happened plainly, no session keys or tool names (" + (text ? text.text : "") + ")")
            const toggle = msg ? win.findName("harloSentToggle", msg) : null
            win.check(toggle !== null && toggle.visible, "what harlo sent is a click away")
            if (toggle) toggle.clicked()
            const sent = msg ? win.findName("harloSent", msg) : null
            win.check(sent !== null && sent.visible && sent.text.indexOf("finished and reported") >= 0, "a click shows it")
            if (toggle) toggle.clicked()
            const prompts = JSON.parse(chat.session.userPrompts())
            win.check(prompts.length === 1 && prompts[0].text === "split it please", "the prompt rail has only yours (" + JSON.stringify(prompts) + ")")
            win.check(msg !== null && win.findText("Edit & resend", msg.parent) === null, "no Edit & resend on it")
            const open = msg ? win.findName("openHelper", msg) : null
            win.check(open !== null && open.visible, "it offers Open helper")
            win.clickItem(open, "Open helper") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            const helper = kids.length ? kids[0].openKey : ""
            win.check(helper !== "" && Workspace.activeKey === helper, "Open helper shows the helper (" + Workspace.activeKey + ", " + JSON.stringify(kids) + ")")
            win.shotWindow("clicks-split-helper") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(liveSplitSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.log("model", chat.session.modelName)
            win.sendByKeys("Create two independent Python modules with tests: bmi.py (bmi(weight_kg, height_m) rounded to one decimal, and category(value) giving underweight, normal, overweight or obese by the WHO cut-offs) and roman.py (to_roman(n) and from_roman(text) for 1 to 3999, ValueError on anything else). Each gets its own pytest file." + (win.argValue("--split=") ? " Split it into two helpers, one per module." : "")) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: win.waitUntil(liveSplitSteps, () => chat.session.hasRequest || chat.session.status === "idle", 600000) }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            // (Asked for helpers, it starts them itself; otherwise it offers.)
            const offered = chat.session.hasRequest && win.findContaining("Split into helpers", chat.requestDock) !== null
            const started = JSON.parse(Workspace.orchestratedJson(win.harloKey)).length > 0
            win.check(offered || started, "Harlo offers the split, or starts the helpers asked for")
            win.shotWindow("live-split-offer")
            if (offered) win.clickText("Start helpers", chat.requestDock)
            if (offered || started) return
            win.log("chat", win.chatText().slice(-3000))
            win.summary(); Qt.quit() } }
        PauseAnimation { duration: 5000 }
        // Both helpers report, and the main session is done with them.
        ScriptAction { script: win.waitUntil(liveSplitSteps, () => {
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            const reports = win.chatText().split("finished and reported").length - 1
            return kids.length > 0 && reports >= kids.length && chat.session.status === "idle" && !chat.session.hasRequest
        }, 1500000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            win.log("helpers", JSON.stringify(kids))
            win.check(kids.length >= 2, "two helpers (" + kids.length + ")")
            win.check(TestInput.readFile(cwd + "/bmi.py") !== "" && TestInput.readFile(cwd + "/roman.py") !== "", "both modules are in the project")
            win.log("chat", win.chatText().slice(-3000))
            win.shotWindow("live-split-done")
            win.summary(); Qt.quit() } }
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
            Workspace.setProviderEnabled("claude", false)
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const project = JSON.parse(Workspace.projectsJson())[0].id
            // (--wiki=on|off: the project's wiki's reading tools, on or off.)
            if (win.argValue("--wiki=") !== "") Workspace.setSetting("wiki_" + project, win.argValue("--wiki=") === "on" ? "1" : "0")
            win.harloKey = Workspace.newSession(project, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(benchSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/"))
            if (win.argValue("--thinking=")) chat.session.setThinkingLevel(win.argValue("--thinking=")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const task = TestInput.readFile(win.argValue("--prompt-file=")).trim()
            win.log("model", chat.session.modelName)
            chat.session.send(task, "normal") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.waitUntil(benchSteps, () => {
            // (A split it offers: accepted, as the task asks for one.)
            if (chat.session.hasRequest && win.findContaining("Split into helpers", chat.requestDock) !== null) {
                win.clickText("Start helpers", chat.requestDock)
                win.benchQuiet = 0
                return false
            }
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            const done = chat.session.status === "idle" && !chat.session.hasRequest && kids.every(k => ["completed", "failed", "ended"].includes(k.status))
            if (!done) { win.benchQuiet = 0; return false }
            if (!win.benchQuiet) win.benchQuiet = Date.now()
            return Date.now() - win.benchQuiet > 15000
        }, 3 * 3600 * 1000) }
        // (A pause between: a wait followed at once by another action
        // doesn't hold the animation.)
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            TestInput.writeFile(win.selftestDir + "/bench.json", JSON.stringify({ helpers: kids, status: chat.session.status, chat: win.chatText().slice(-4000) }, null, 1))
            win.log("helpers", kids.length)
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(livePlanSteps, () => chat.session.modelName !== "", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = (win.argValue("--model=") || "codex/gpt-6.1-sol").split("/")
            chat.session.setModel(m[0], m.slice(1).join("/")) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("Make the shared plan (plan_set) for this, with exactly two steps, and stop there: don't do the steps yourself. Step 1: bmi.py (bmi(weight_kg, height_m) rounded to one decimal, and category(value) giving underweight, normal, overweight or obese by the WHO cut-offs) with test_bmi.py. Step 2: roman.py (to_roman(n) and from_roman(text) for 1 to 3999, ValueError on anything else) with test_roman.py. Both use pytest.") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: win.waitUntil(livePlanSteps, () => (JSON.parse(Workspace.planJson(win.harloKey)).steps || []).length >= 2 && chat.session.status === "idle", 300000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.log("plan", Workspace.planJson(win.harloKey))
            win.clickItem(win.findName("planRun"), "Run with helpers") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("planRunTeam"), "Helpers: as this session") }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.waitUntil(livePlanSteps, () => { const p = JSON.parse(Workspace.planJson(win.harloKey)); return !p.run || (p.run.paused || "") !== "" }, 1800000) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.planJson(win.harloKey))
            win.log("plan", JSON.stringify(p))
            win.log("helpers", Workspace.orchestratedJson(win.harloKey))
            win.check(!p.run && (p.steps || []).every(s => s.status === "done"), "the plan ran to the end (" + (p.run ? p.run.paused : "") + ")")
            win.check((p.steps || []).every(s => (s.summary || "").indexOf("Reviewed:") >= 0), "each step was reviewed")
            win.shotWindow("live-plan-done")
            win.summary(); Qt.quit() } }
    }

    // A plan run reviews each step: Settings → Orchestration has it on; a
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
            const sw = win.findName("orchReviewSteps", settingsDialog.contentItem)
            win.check(sw !== null && sw.checked && win.findText("Review each step", settingsDialog.contentItem) !== null, "Settings → Orchestration: Review each step, on by default")
            win.shotWindow("clicks-review-setting")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("make a plan please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => (JSON.parse(Workspace.planJson(win.harloKey)).steps || []).length === 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 600 }
        // The step to another model: chosen on the plan card.
        // (Its buttons show on hover.)
        ScriptAction { script: { const t = win.findText("Write helper.txt", chat.planDock), p = t.mapToItem(null, t.width / 2, t.height / 2); TestInput.moveTo(p.x, p.y) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("planStepHelper-1"), "Who does it") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findText("Who does this step") !== null, "the step's helper is chosen in a popover")
            const box = win.findName("delegateModelHelper")
            const i = box ? box.model.indexOf("Fake Helper") : -1
            win.check(i > 0, "its models are offered (" + (box ? box.model : "none") + ")")
            if (i > 0) { box.currentIndex = i; box.activated(i) } } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: win.clickItem(win.findName("delegateStart"), "Choose") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const step = JSON.parse(Workspace.planJson(win.harloKey)).steps[0]
            win.check(step.agent === "harlo" && step.model === "fake/fake-helper", "the step keeps who does it (" + step.agent + ", " + step.model + ")")
            const who = win.findName("planStepWho-1")
            win.check(who !== null && who.visible && who.text === "Done by Harlo · Fake Helper", "the card says so (" + (who ? who.text : "none") + ")")
            win.shotWindow("clicks-review-step-helper") } }
        PauseAnimation { duration: 200 }
        ScriptAction { script: win.clickItem(win.findName("planRun"), "Run with helpers") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickItem(win.findName("planRunTeam"), "Helpers: as this session") }
        // Its helper writes helper.txt and reports; a review starts.
        ScriptAction { script: win.waitUntil(planReviewSteps, () => { const r = JSON.parse(Workspace.planJson(win.harloKey)).run; return !!r && Object.keys(r.reviewing || {}).length > 0 }, 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            win.check(win.findContaining("being reviewed") !== null && win.findName("planOpenReview-1") !== null, "the step shows it's being reviewed, with Open review")
            // The sidebar's rows fit (a helper's too), at any width.
            const titles = win.findAllNames("sessionTitle", sidebar).filter(t => t.visible)
            const fits = t => Array.from(t.parent.children).filter(c => c.visible).every(c => c.x + c.width <= t.parent.width + 1)
            win.check(titles.length >= 3 && titles.every(t => t.width >= 24 && fits(t)), "the sidebar's rows fit, each title with room (" + titles.map(t => Math.round(t.width)).join(", ") + ")")
            const planWhat = win.findAllNames("toolWhat", chat.transcript).find(l => l.plain === "Write helper.txt")
            win.check(planWhat && planWhat.font.family !== Theme.mono, "a plan's step reads as prose, not code")
            win.check(win.findText("Plan", chat.transcript) !== null && win.findText("Plan set", chat.transcript) === null, "the plan tool is named 'Plan'")
            win.shotWindow("clicks-review-running") } }
        // The review finds a problem; the helper fixes it; a second review
        // passes and the plan finishes.
        ScriptAction { script: win.waitUntil(planReviewSteps, () => { const p = JSON.parse(Workspace.planJson(win.harloKey)); return !p.run && (p.steps || []).every(s => s.status === "done") }, 60000) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const cwd = win.argValue("--cwd=")
            const p = JSON.parse(Workspace.planJson(win.harloKey))
            const step = (p.steps || [])[0] || {}
            win.check(step.status === "done" && step.summary.indexOf("Fixed") >= 0 && step.summary.indexOf("Reviewed: Looks right") >= 0, "the step is done, with its review (" + step.summary + ")")
            win.check(TestInput.readFile(cwd + "/helper.txt") === "from the helper\nsecond line\n", "the helper fixed what the review found")
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            const reviews = kids.filter(k => k.name.indexOf("Review: Step 1") === 0)
            win.check(reviews.length === 2 && reviews.every(k => k.status === "completed"), "two reviews, both completed (" + JSON.stringify(kids.map(k => [k.name, k.status])) + ")")
            const reqs = win.modelRequests()
            const first = r => { const m = (r.messages || []).find(m => m.role === "user"); return m ? JSON.stringify(m.content) : "" }
            const reviewReqs = reqs.filter(r => first(r).indexOf("You review one step") >= 0)
            win.check(reviewReqs.length > 0 && reviewReqs.every(r => first(r).indexOf("git diff ") >= 0), "the reviewer is told where the step's changes are")
            const fixes = reqs.filter(r => (r.messages || []).some(m => m.role === "user" && JSON.stringify(m.content).indexOf("A review of your step found problems") >= 0))
            win.check(fixes.length > 0 && fixes.some(r => JSON.stringify(r.messages).indexOf("helper.txt:1 needs a second line") >= 0), "the problems went back to the step's helper")
            // Each helper's model and what it used; the run's total.
            // (The step's helper on the model chosen for it; its reviews on the run's.)
            const stepKid = kids.find(k => k.step === 1)
            win.check(stepKid && stepKid.model === "fake/fake-helper" && reviews.every(k => k.model === "fake/fake-coder"), "the step's helper ran on the model chosen for it, its reviews on the run's (" + JSON.stringify(kids.map(k => [k.name, k.model])) + ")")
            win.check(kids.length === 3 && kids.every(k => k.used && k.used.tokens > 0), "each helper shows what it used (" + JSON.stringify(kids.map(k => k.used)) + ")")
            const total = kids.reduce((t, k) => t + k.used.tokens, 0)
            win.check(win.findContaining("Helpers · all done · " + (total / 1000).toFixed(1) + "k tokens") !== null, "and the Subagents box")
            win.shotWindow("clicks-review-done")
            // A finished plan folds to "All done" (a reopened session too).
            win.check(!chat.planDock.expanded && win.findText("All done", chat.planDock) !== null, "the finished plan folds to All done")
            win.clickItem(win.findName("planToggle", chat.planDock), "the plan's fold") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(chat.planDock.expanded && win.findText("Write helper.txt", chat.planDock) !== null, "opened by hand, it shows its steps")
            chat.planDock.settleFold()
            win.check(chat.planDock.expanded, "and stays open for that session")
            win.clickItem(win.findName("planToggle", chat.planDock), "fold it again") } }
        // The project's Team: its helpers' agent, and its reviewer on
        // another model (the next run's reviews use it).
        PauseAnimation { duration: 300 }
        ScriptAction { script: settingsDialog.openProject(JSON.parse(Workspace.projectsJson())[0].id) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const box = win.findName("teamAgent", settingsDialog.contentItem)
            win.check(box !== null && box.model[box.currentIndex] === "Same as the session", "Team: the helpers are like the session at first (" + (box ? box.model : "none") + ")")
            win.check(win.findName("teamPreset", settingsDialog.contentItem) === null, "no ready-made teams (they repeated the rows below)")
            const i = box ? box.model.indexOf("Harlo") : -1
            win.check(i > 0, "Harlo is one of them")
            if (i > 0) { box.currentIndex = i; box.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pid = JSON.parse(Workspace.projectsJson())[0].id
            win.check(JSON.parse(Workspace.setting("team_helpers_" + pid)).agent === "harlo", "choosing it sets the team (" + Workspace.setting("team_helpers_" + pid) + ")")
            const rev = win.findName("teamReviewer", settingsDialog.contentItem)
            const i = rev ? rev.model.indexOf("Harlo") : -1
            win.check(rev !== null && rev.model[rev.currentIndex] === "The helpers' agent" && i > 0, "Reviews by: the helpers' agent, or another (" + (rev ? rev.model : "none") + ")")
            if (i > 0) { rev.currentIndex = i; rev.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const m = win.findName("teamReviewerModel", settingsDialog.contentItem)
            const i = m ? m.model.indexOf("Fake Helper") : -1
            win.check(i > 0, "the reviewer's model is offered (" + (m ? m.model : "none") + ")")
            if (i > 0) { m.currentIndex = i; m.activated(i) } } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const pid = JSON.parse(Workspace.projectsJson())[0].id
            const v = JSON.parse(Workspace.setting("team_helpers_" + pid))
            win.check(v.reviewer && v.reviewer.agent === "harlo" && v.reviewer.model === "fake/fake-helper", "the team's reviewer is saved (" + JSON.stringify(v) + ")")
            win.scrollIntoView(win.findName("teamReviewerModel", settingsDialog.contentItem))
            win.shotWindow("clicks-review-team")
            settingsDialog.close() } }
        // Problems that stay: the run pauses for you.
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("make a bad plan please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => (JSON.parse(Workspace.planJson(win.harloKey)).steps || []).length === 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 600 }
        // This run from the phone, with the team's helpers; its helper asks
        // before writing (the session asks first: its helpers start so),
        // and the phone approves.
        ScriptAction { script: {
            const call = m => JSON.parse(Workspace.phoneTestCall(JSON.stringify(m)))
            const plan = call({ op: "plan", key: win.harloKey })
            win.check(plan.team && plan.team.agent === "harlo" && plan.team.team === true && plan.team.reviewer && plan.team.reviewer.model === "fake/fake-helper",
                      "the phone's plan says who the team's helpers are (" + JSON.stringify(plan.team) + ")")
            Workspace.setApprovalMode(win.harloKey, "always-ask")
            const run = call({ op: "planRun", key: win.harloKey, agent: "" })
            win.check(run.run && run.run.agent === "harlo" && !run.error, "the phone runs it with the team's helpers (" + JSON.stringify(run.run || run) + ")") } }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "helpers", key: win.harloKey }))).helpers.some(h => h.asks && h.asks.approval), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const call = m => JSON.parse(Workspace.phoneTestCall(JSON.stringify(m)))
            const h = call({ op: "helpers", key: win.harloKey }).helpers.find(h => h.asks && h.asks.approval)
            win.check(h.step === 1 && h.model === "fake/fake-coder" && h.asks.options.indexOf("Approve") >= 0, "the phone sees the helper's model and its request (" + JSON.stringify([h.model, h.asks]) + ")")
            win.check(!!call({ op: "answerHelper", key: win.harloKey, helper: "live:none", id: h.asks.id, value: "Approve" }).error, "a session that isn't its helper can't be answered")
            const after = call({ op: "answerHelper", key: win.harloKey, helper: h.id, id: h.asks.id, value: "Approve" })
            win.check(Array.isArray(after.helpers) && !after.helpers.some(x => x.id === h.id && x.asks && x.asks.id === h.asks.id), "the phone approves it (" + JSON.stringify((after.helpers || []).map(x => x.asks)) + ")")
            win.approvedHelper = h.id
            Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const h = JSON.parse(Workspace.phoneTestCall(JSON.stringify({ op: "helpers", key: win.harloKey }))).helpers.find(x => x.id === win.approvedHelper)
            win.check(h && !h.asks && h.status !== "waiting", "the helper went on (" + (h ? h.status : "gone") + ")") } }
        ScriptAction { script: win.waitUntil(planReviewSteps, () => { const r = JSON.parse(Workspace.planJson(win.harloKey)).run; return !!r && (r.paused || "") !== "" }, 60000) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.planJson(win.harloKey))
            const step = (p.steps || [])[0] || {}
            win.check(p.run && p.run.paused.indexOf("its review still found problems after a round of fixes") >= 0 && p.run.paused.indexOf("helper.txt:1") >= 0, "problems after a round of fixes pause the run (" + (p.run ? p.run.paused : "no run") + ")")
            win.check(step.status === "pending" && step.summary.indexOf("Not done: its review found problems") === 0, "and the step waits again (" + step.status + ": " + step.summary + ")")
            win.check(win.findText("· paused") !== null && win.findText("Run again") !== null, "the plan card says paused, with Run again")
            win.check(win.findName("planPaused").text.indexOf("Paused at step 1.") === 0 && win.findName("planPaused").text.indexOf("helper.txt:1") < 0, "the pause names the step and what to do; the problem is said once, at the step (" + win.findName("planPaused").text + ")")
            const kids = JSON.parse(Workspace.orchestratedJson(win.harloKey))
            const reviews = kids.filter(k => k.review !== null && k.review !== undefined)
            win.check(reviews.length === 2 && reviews.every(k => k.model === "fake/fake-helper") && kids.filter(k => k.step === 1).every(k => k.model === "fake/fake-coder"),
                      "the team's reviewer reviews, on its model; the helper is the team's (" + JSON.stringify(kids.map(k => [k.name, k.model])) + ")")
            win.shotWindow("clicks-review-paused")
            win.summary(); Qt.quit() } }
    }

    // Settings → Providers → Harlo: the subagents' model and the model to go
    // on with, typed in; a new session's subagent runs on the first, and a
    // server that stays busy hands the turn to the second.
    SequentialAnimation {
        id: harloSettingsSteps
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksharlosettings"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.addProject(win.argValue("--cwd="))
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 600 }
        // Code intelligence: each language's server, found where it's
        // usually installed too (a stand-in gopls in the home's go/bin).
        ScriptAction { script: win.waitUntil(harloSettingsSteps, () => Accounts.languageServers !== "" && JSON.parse(Accounts.languageServers).state === "ready", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            const list = JSON.parse(Accounts.languageServers).list || []
            const go = list.find(l => l.language === "Go") || {}
            win.check(list.length === 5 && list.every(l => l.state === "ready" || l.install !== ""), "Code intelligence lists five languages, with how to install the missing (" + list.map(l => l.language + ":" + l.state).join(", ") + ")")
            win.check(go.state === "ready" && go.path.endsWith("/home/go/bin/gopls"), "a server outside the PATH is found where it's usually installed (" + go.state + " " + go.path + ")")
            const rows = win.findName("harloLanguageServers", settingsDialog.contentItem)
            win.check(rows && rows.visible && rows.children.length >= 6, "and shown under it (" + (rows ? rows.children.length : "none") + ")")
            win.clickItem(win.findName("harloLanguageServersCheck", settingsDialog.contentItem), "Check again") } }
        PauseAnimation { duration: 100 }
        ScriptAction { script: win.waitUntil(harloSettingsSteps, () => JSON.parse(Accounts.languageServers).state === "ready", 20000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            win.check((JSON.parse(Accounts.languageServers).list || []).length === 5, "Check again asks again")
            win.scrollIntoView(win.findName("harloLanguageServers", settingsDialog.contentItem))
            win.shotWindow("clicks-harlo-language-servers")
            // Picked from Harlo's models and the budget's choices.
            const pick = (name, text) => {
                const box = win.findName(name, settingsDialog.contentItem)
                const i = box ? box.model.findIndex(t => t.indexOf(text) >= 0) : -1
                win.check(i > 0, name + " offers " + text + " (" + (box ? box.model.join(", ") : "none") + ")")
                if (i > 0) { box.currentIndex = i; box.activated(i) }
            }
            pick("harloTaskModel", "Fake Helper")
            pick("harloFallback", "Go on with Fake Helper")
            pick("harloHelperBudget", "500k tokens") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(Workspace.setting("harlo_task_model") === "fake/fake-helper" && Workspace.setting("harlo_fallback") === "fake/fake-helper" && Workspace.setting("harlo_helper_budget") === "500k",
                      "the three are saved as picked (" + [Workspace.setting("harlo_task_model"), Workspace.setting("harlo_fallback"), Workspace.setting("harlo_helper_budget")].join(", ") + ")")
            win.shotWindow("clicks-harlo-settings")
            settingsDialog.openPage("git") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const box = win.findName("commitModel", settingsDialog.contentItem)
            const i = box ? box.model.indexOf("Fake Helper") : -1
            win.check(box && box.model[box.currentIndex] === "Harlo's default" && i > 0, "Git: the commit messages' model is picked from Harlo's (" + (box ? box.model.join(", ") : "none") + ")")
            if (i > 0) { box.currentIndex = i; box.activated(i) }
            win.check(Workspace.setting("commit_model") === "fake/fake-helper", "and saved as its id (" + Workspace.setting("commit_model") + ")")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSettingsSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("use a subagent please") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSettingsSteps, () => win.chatText().indexOf("Done:") >= 0, 15000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            // (A subagent's: its first message is the task.)
            const first = b => JSON.stringify(((b.messages || []).find(m => m.role === "user") || {}).content || "")
            const sub = win.modelRequests().filter(b => first(b).indexOf("SUBTASK") >= 0)
            win.check(sub.length > 0 && sub.every(b => b.model === "fake-helper"), "the subagent runs on the subagents' model (" + sub.map(b => b.model).join() + ")")
            win.check(win.modelRequests().filter(b => first(b).indexOf("use a subagent") >= 0).every(b => b.model === "fake-coder"), "the session stays on its own")
            win.sendByKeys("be busy now") } }
        PauseAnimation { duration: 300 }
        // (Retries take 1 + 2 + 4 s.)
        ScriptAction { script: win.waitUntil(harloSettingsSteps, () => win.chatText().indexOf("[fake-helper]") >= 0 && chat.session.status === "idle", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("ok: be busy now [fake-helper]") >= 0, "a server that stays busy: it goes on with the model set (" + t.slice(-200) + ")")
            win.check(chat.session.modelName === "Fake Helper", "and says so: the session's model is now that one (" + chat.session.modelName + ")")
            win.shotWindow("clicks-harlo-fallback")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => (JSON.parse(Mcp.statusJson("global")).locked || {}).state === "needs_auth", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const pill = win.findName("mcpStatus-locked", settingsDialog.contentItem)
            win.check(pill !== null && pill.text === "Needs sign-in", "a server that wants a sign-in says so (" + (pill ? pill.text : "none") + ")")
            win.clickItem(win.findName("mcpServer-locked", settingsDialog.contentItem), "locked") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const b = win.findName("mcpSignIn", settingsDialog.contentItem)
            win.check(b !== null, "its page has Sign in (for Harlo)")
            win.shotPopup(settingsDialog, "clicks-mcp-signin")
            win.clickItem(b, "Sign in") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => signInDialog.flow !== null && signInDialog.flow.state !== "running", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(signInDialog.flow !== null && signInDialog.flow.state === "done", "the sign-in finishes in the browser (" + (signInDialog.flow ? signInDialog.flow.state + " " + signInDialog.flow.message : "none") + ")")
            win.shotWindow("clicks-mcp-signed-in")
            signInDialog.close() } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => (JSON.parse(Mcp.statusJson("global")).locked || {}).state === "connected", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const st = JSON.parse(Mcp.statusJson("global")).locked || {}
            win.check(st.state === "connected" && (st.tools || []).length === 1, "then it's tested again: connected, its tool listed (" + JSON.stringify(st) + ")")
            win.check(TestInput.readFile(win.selftestDir + "/config/harlo-agent/mcp-auth.json").indexOf("/mcp") >= 0, "the sign-in is kept in Harlo's mcp-auth.json")
            win.clickItem(win.findName("mcpBack", settingsDialog.contentItem), "All servers") } }
        PauseAnimation { duration: 400 }
        // The project's own server (the repo's .harlo/mcp.json).
        ScriptAction { script: win.clickItem(win.findName("mcpScope", settingsDialog.contentItem), "scope") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.clickText("repo") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findName("mcpServer-dice", d) !== null && win.findText("not allowed for Harlo", d) !== null, "a project's own server: not allowed for Harlo")
            win.shotPopup(settingsDialog, "clicks-mcp-project")
            win.clickItem(win.findName("mcpServer-dice", d), "dice") } }
        PauseAnimation { duration: 500 }
        // Its tool marked as only looking (the server doesn't say).
        // (Pressed, not clicked: scrolled into view, it's under the floating
        // Save bar.)
        ScriptAction { script: win.findName("mcpTest", settingsDialog.contentItem).clicked() }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => { const c = win.findName("mcpLooks-roll", settingsDialog.contentItem); return c !== null && c.visible }, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, c = win.findName("mcpLooks-roll", d)
            win.check(c !== null && c.visible && c.enabled && !c.checked, "a tested tool can be marked as only looking, for Harlo (unmarked: the server doesn't say)")
            win.check(win.findName("mcpLooksHint", d) !== null && win.findName("mcpLooksHint", d).visible, "the page says what marking does")
            c.click()
            const f = win.ancestorWith(c, "contentHeight")
            if (f) f.contentY = Math.max(0, f.contentHeight - f.height) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const c = win.findName("mcpLooks-roll", settingsDialog.contentItem)
            win.check(c.checked, "ticked")
            win.shotPopup(settingsDialog, "clicks-mcp-looks") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickItem(win.findName("mcpSave", settingsDialog.contentItem), "Save") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const file = TestInput.readFile(win.argValue("--cwd=") + "/.harlo/mcp.json")
            win.check(JSON.stringify((JSON.parse(file).mcpServers.dice || {}).readOnly) === JSON.stringify(["roll"]), "saved in Harlo's entry as readOnly (" + file.replace(/\s+/g, " ") + ")")
            win.check(win.findText("not allowed for Harlo", settingsDialog.contentItem) === null, "saved here, it's allowed")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("what time is it? mcp clock") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => win.chatText().indexOf("Done:") >= 0, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Done: It's 3 pm in Tokyo") >= 0, "Harlo calls the signed-in server's tool (" + win.chatText().slice(-160) + ")")
            win.sendByKeys("roll one: mcp roll") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloMcpSteps, () => win.chatText().indexOf("You rolled") >= 0 || chat.session.status === "idle" && win.chatText().split("Done:").length > 2, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Done: You rolled 4") >= 0, "and the project's server, now allowed (" + win.chatText().slice(-160) + ")")
            win.shotWindow("clicks-mcp-harlo")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: win.waitUntil(catalogSteps, () => catalogSteps.offered().loading === false && catalogSteps.offered().latest === "2.1.7" && catalogSteps.offered().stars > 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogSteps.offered()
            const card = win.findName("mcpCatalog-context-mode", d), sw = win.findName("mcpCatalogToggle-context-mode", d), u = win.findName("mcpCatalogUpdate-context-mode", d)
            win.check(card !== null && card.visible, "Ready to install lists context-mode")
            win.check(sw !== null && !sw.checked && sw.enabled, "off at first, and it can be turned on")
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.7 is the newest", "not installed, and its newest release (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.check(u === null || !u.visible, "no Update before it's installed")
            const repo = win.findName("mcpCatalogRepo-context-mode", d), stars = win.findName("mcpCatalogStars-context-mode", d)
            win.check(repo !== null && repo.visible && repo.text === "mksglu/context-mode" && repo.url === "https://github.com/mksglu/context-mode", "its repository, a link to it (" + (repo ? repo.text + " " + repo.url : "none") + ")")
            win.check(stars !== null && stars.visible && stars.text === "12k" && stars.ToolTip.text === "12,345 stars on GitHub", "its stars (" + (stars ? stars.text + ", " + stars.ToolTip.text : "none") + ")")
            win.check(b.version === "" && b.node !== "" && b.nodeError === "", "nothing downloaded yet; a Node found (" + JSON.stringify(b) + ")")
            win.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") === "" && catalogSteps.entry() === null, "nothing in harlo's folder or Harlo's servers yet")
            win.shotPopup(settingsDialog, "clicks-catalog-off")
            win.clickItem(sw, "turn on") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => (catalogSteps.offered().agents || []).length > 0 || (catalogSteps.offered().error || "") !== "", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = catalogSteps.offered(), e = catalogSteps.entry() || {}
            win.check(JSON.stringify(b.agents) === JSON.stringify(["harlo"]) && b.error === "" && b.version === "2.1.7", "downloaded and on for Harlo (" + JSON.stringify(b) + ")")
            const data = win.selftestDir + "/data/harlo/mcp/context-mode"
            win.check(JSON.parse(TestInput.readFile(data + "/2.1.7/package.json") || "{}").version === "2.1.7", "in harlo's folder")
            win.check(e.command && e.command.startsWith("/") && e.command.endsWith("/node"), "the entry runs Node by its path (" + e.command + ")")
            win.check(JSON.stringify(e.args) === JSON.stringify([data + "/current/server.mjs", "--agent", "claude-code"]), "harlo's copy, through current (" + JSON.stringify(e.args) + ")")
            win.check(JSON.stringify(e.hiddenTools) === JSON.stringify(["ctx_upgrade", "ctx_doctor"]) && e.enabled === true, "its own updating hidden from Harlo (" + JSON.stringify(e) + ")") } }
        ScriptAction { script: win.waitUntil(catalogSteps, () => (JSON.parse(Mcp.statusJson("global"))["context-mode"] || {}).state === "connected", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const pill = win.findName("mcpCatalogStatus-context-mode", d)
            win.check(pill.text === "Connected · 9 tools", "tested: connected (" + pill.text + ")")
            win.check(win.findName("mcpServer-context-mode", d) === null, "not listed again under Your servers")
            win.shotPopup(settingsDialog, "clicks-catalog-on")
            // (Looking again finds the newer release.)
            win.clickItem(win.findName("mcpRefresh", d), "refresh") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => catalogSteps.offered().latest === "2.1.8", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, u = win.findName("mcpCatalogUpdate-context-mode", d)
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.7 · 2.1.8 is out" && u !== null && u.visible, "a newer release: Update (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.clickItem(u, "Update") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => catalogSteps.offered().busy === "" && (catalogSteps.offered().message !== "" || catalogSteps.offered().error !== ""), 30000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogSteps.offered()
            win.check(b.message === "context-mode is updated: 2.1.7 → 2.1.8. New sessions use it." && b.version === "2.1.8", "updated from the release (" + b.message + b.error + ")")
            const pkg = TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") || "{}"
            win.check(JSON.parse(pkg).version === "2.1.8", "current is the new one")
            win.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/2.1.7/package.json") !== "", "the one before stays, for sessions still running it")
            const u = win.findName("mcpCatalogUpdate-context-mode", d)
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.8 · up to date" && (u === null || !u.visible), "up to date: no Update (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.check(win.findName("mcpCatalogNote-context-mode", d).text === b.message, "it says so")
            win.shotPopup(settingsDialog, "clicks-catalog-updated")
            settingsDialog.close() } }
        PauseAnimation { duration: 400 }
        // A Harlo session uses it.
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: win.sendByKeys("how much was kept out? mcp ctx_stats") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => win.chatText().indexOf("Done:") >= 0, 40000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Done: ctx_stats:") >= 0, "Harlo calls context-mode's tool (" + win.chatText().slice(-200) + ")")
            const req = win.modelRequests().slice(-1)[0] || {}
            const names = (req.tools || []).map(t => t.function.name)
            win.check(names.includes("mcp__context-mode__ctx_execute") && !names.includes("mcp__context-mode__ctx_upgrade") && !names.includes("mcp__context-mode__ctx_doctor"), "offered its tools but the two hidden (" + names.filter(n => n.indexOf("context") >= 0).join() + ")")
            const system = ((req.messages || [])[0] || {}).content || ""
            win.check(system.indexOf('<mcp_server name="context-mode">') >= 0 && system.indexOf("ctx_execute") >= 0, "and its instructions")
            win.shotWindow("clicks-catalog-session")
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.clickItem(win.findName("mcpCatalogToggle-context-mode", settingsDialog.contentItem), "turn off") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogSteps, () => catalogSteps.offered().busy === "" && (catalogSteps.offered().agents || []).length === 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(catalogSteps.entry() === null, "off: its entry is gone")
            win.check(win.findName("mcpCatalogStatus-context-mode", d).text === "Off" && win.findName("mcpCatalogFor-context-mode-harlo", d).checked, "Off, and Harlo stays ticked for next time")
            win.summary(); Qt.quit() } }
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
            win.check(Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh=")) === "", "the host is saved")
            Remotes.connectHost("box") } }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => JSON.parse(Remotes.hostsJson())[0].state === "connected", 30000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: { Workspace.switchInstance("box"); settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().loading === false && catalogHostSteps.offered().latest === "2.1.7", 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogHostSteps.offered()
            const card = win.findName("mcpCatalog-context-mode", d), sw = win.findName("mcpCatalogToggle-context-mode", d)
            win.check(card !== null && card.visible && sw !== null && !sw.checked && sw.enabled, "box's global servers offer it too, off")
            win.check(win.findName("mcpCatalogLede", d).text.indexOf("sets it up on box for the agents you tick") > 0, "it says it's set up on box (" + win.findName("mcpCatalogLede", d).text + ")")
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.7 is the newest", "not installed there (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.check(b.node !== "" && b.nodeError === "" && (b.error || "") === "", "box's Node found (" + JSON.stringify(b) + ")")
            win.check(TestInput.readFile(catalogHostSteps.copies + "/current/package.json") === "" && catalogHostSteps.entry() === null, "nothing on box yet")
            win.shotPopup(settingsDialog, "clicks-catalog-host-off")
            win.clickItem(sw, "turn on") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && ((catalogHostSteps.offered().agents || []).length > 0 || (catalogHostSteps.offered().error || "") !== ""), 60000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = catalogHostSteps.offered(), e = catalogHostSteps.entry() || {}, c = catalogHostSteps.copies
            win.check(JSON.stringify(b.agents) === JSON.stringify(["harlo"]) && b.error === "" && b.version === "2.1.7", "downloaded and on for Harlo on box (" + JSON.stringify(b) + ")")
            win.check(JSON.parse(TestInput.readFile(c + "/current/package.json") || "{}").version === "2.1.7" && TestInput.readFile(c + "/current/server.mjs") !== "", "copied to box, its current link there")
            win.check(TestInput.readFile(win.selftestDir + "/data/harlo/mcp/context-mode/current/package.json") === "", "not installed on this computer")
            win.check(e.command && e.command.startsWith("/") && e.command.endsWith("/node"), "box's entry runs box's Node by its path (" + e.command + ")")
            win.check(JSON.stringify(e.args) === JSON.stringify([c + "/current/server.mjs", "--agent", "claude-code"]), "box's copy, through current (" + JSON.stringify(e.args) + ")")
            win.check(JSON.stringify(e.hiddenTools) === JSON.stringify(["ctx_upgrade", "ctx_doctor"]) && e.enabled === true, "its own updating hidden from Harlo there") } }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => (JSON.parse(Mcp.statusJson("host:box"))["context-mode"] || {}).state === "connected", 40000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            const pill = win.findName("mcpCatalogStatus-context-mode", d)
            win.check(pill !== null && pill.text === "Connected · 9 tools", "tested on box: connected (" + (pill ? pill.text : "") + " " + Mcp.statusJson("host:box").slice(0, 600) + ")")
            win.check(win.findName("mcpServer-context-mode", d) === null, "not listed again under Your servers")
            win.shotPopup(settingsDialog, "clicks-catalog-host-on")
            win.clickItem(win.findName("mcpRefresh", d), "refresh") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().latest === "2.1.8" && catalogHostSteps.offered().version === "2.1.7", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, u = win.findName("mcpCatalogUpdate-context-mode", d)
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.7 · 2.1.8 is out" && u !== null && u.visible, "a newer release: Update (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.clickItem(u, "Update") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && ((catalogHostSteps.offered().message || "") !== "" || (catalogHostSteps.offered().error || "") !== ""), 60000) }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, b = catalogHostSteps.offered(), c = catalogHostSteps.copies
            win.check(b.message === "context-mode is updated on box: 2.1.7 → 2.1.8. New sessions use it." && b.version === "2.1.8", "updated on box (" + b.message + b.error + ")")
            win.check(JSON.parse(TestInput.readFile(c + "/current/package.json") || "{}").version === "2.1.8", "box's current is the new one")
            win.check(TestInput.readFile(c + "/2.1.7/package.json") !== "", "the one before stays there")
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Version 2.1.8 · up to date", "up to date")
            win.shotPopup(settingsDialog, "clicks-catalog-host-updated")
            win.clickItem(win.findName("mcpCatalogToggle-context-mode", d), "turn off") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered().busy === "" && (catalogHostSteps.offered().agents || []).length === 0, 30000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(catalogHostSteps.entry() === null && TestInput.readFile(catalogHostSteps.copies + "/current/package.json") !== "", "off: box's entry is gone, its copy stays")
            win.check(win.findName("mcpCatalogStatus-context-mode", d).text === "Off", "Off")
            Workspace.switchInstance("") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.waitUntil(catalogHostSteps, () => catalogHostSteps.offered("global").loading === false, 20000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const b = catalogHostSteps.offered("global"), d = settingsDialog.contentItem
            win.check(b.version === "" && (b.agents || []).length === 0, "this computer's own: not installed here (" + JSON.stringify(b) + ")")
            win.check(win.findName("mcpCatalogVersion-context-mode", d).text === "Not installed · version 2.1.8 is the newest", "and its card says so (" + win.findName("mcpCatalogVersion-context-mode", d).text + ")")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSkillSteps, () => chat.session.modelName !== "" && chat.session.commands.length > 0, 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(chat.session.commands.some(c => c.indexOf("skill:greet") >= 0), "the command list offers skill:greet (" + chat.session.commands.filter(c => c.indexOf("skill") >= 0).join() + ")")
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.typeText("/skill:gr") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            win.check(chat.composerItem.completionVisible && chat.composerItem.completionItems.some(c => JSON.stringify(c).indexOf("skill:greet") >= 0), "typing / suggests it")
            win.shotWindow("clicks-harlo-skill-completion")
            // (The rest typed out: the suggestion isn't picked.)
            TestInput.key(Qt.Key_A, Qt.ControlModifier)
            TestInput.typeText("/skill:greet the team")
            TestInput.key(Qt.Key_Escape, 0)
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSkillSteps, () => win.modelRequests().length > 0 && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const b = win.modelRequests()[0] || {}
            const sys = JSON.stringify((b.messages || []).filter(m => m.role === "system")), user = JSON.stringify((b.messages || []).filter(m => m.role === "user").slice(-1))
            win.check(sys.indexOf("greet") >= 0 && sys.indexOf("Greets the user warmly.") >= 0, "Harlo's instructions list the skill with its description")
            win.check(user.indexOf("<skill name=\\\"greet\\\"") >= 0 && user.indexOf("Say hello in three languages.") >= 0 && user.indexOf("the team") >= 0, "/skill:greet sends the skill with the prompt (" + user.slice(0, 200) + ")")
            win.sendByKeys("use the reviewer please") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSkillSteps, () => win.chatText().split("Done:").length > 1 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const first = b => JSON.stringify(((b.messages || []).find(m => m.role === "user") || {}).content || "")
            const sub = win.modelRequests().find(b => first(b).indexOf("SUBTASK review it") >= 0)
            const tools = sub ? (sub.tools || []).map(t => t.function.name) : []
            win.check(sub !== undefined && JSON.stringify(sub.messages).indexOf("REVIEWER-PROFILE") >= 0, "a task names Claude's agent profile: its instructions are the subagent's")
            win.check(tools.includes("read") && tools.includes("grep") && !tools.includes("write") && !tools.includes("bash"), "with its tools only (" + tools.join() + ")")
            // A change to commit, and the message Harlo writes for it.
            TestInput.writeFile(win.argValue("--cwd=") + "/README.md", "hello\nmore\n")
            windowSettings.rightMode = "changes" } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.clickItem(win.findName("commitWrite"), "Write the message") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSkillSteps, () => !git.generating, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(changes.commitText.indexOf("[fake-coder]") >= 0 && !git.error, "Harlo writes the commit message (" + JSON.stringify(changes.commitText).slice(0, 120) + " " + git.message + ")")
            win.shotWindow("clicks-harlo-commit")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo"); Workspace.setApprovalMode(win.harloKey, "yolo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSessionSteps, () => chat.session.modelName !== "" && JSON.parse(Workspace.sessionMentions(chat.session.cwd, chat.session.key, "")).length > 0, 15000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            const all = JSON.parse(Workspace.sessionMentions(chat.session.cwd, chat.session.key, ""))
            win.check(all.length === 1 && all[0].mention === "@session:a1b2c3d4", "the project's past session is offered, not this one (" + JSON.stringify(all).slice(0, 200) + ")")
            win.clickItem(win.findName("composerInput"), "composer")
            TestInput.typeText("@sandbox") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            const items = chat.composerItem.completionItems
            win.check(chat.composerItem.completionVisible && items.length > 0 && items[0].kind === "session" && items[0].name.indexOf("sandbox commands") >= 0,
                      "typing @ and its words lists the session first (" + JSON.stringify(items.slice(0, 2)) + ")")
            win.shotWindow("clicks-harlo-session-mention")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findName("composerInput").text.indexOf("@session:a1b2c3d4 ") === 0, "picking it writes its mention (" + JSON.stringify(win.findName("composerInput").text) + ")")
            TestInput.typeText("what did we pick?")
            TestInput.key(Qt.Key_Return, 0) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSessionSteps, () => win.modelRequests().length > 0 && chat.session.status === "idle", 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const user = JSON.stringify((win.modelRequests()[0].messages || []).filter(m => m.role === "user").slice(-1))
            win.check(user.indexOf("PICKED-BWRAP") >= 0 && user.indexOf("Session \\\"How should we sandbox commands?\\\"") >= 0 && user.indexOf("what did we pick?") >= 0,
                      "its messages go with the message (" + user.slice(0, 300) + ")")
            win.check(win.chatText().indexOf("what did we pick?") >= 0 && win.chatText().indexOf("PICKED-BWRAP") < 0, "the transcript shows the message, not the excerpt")
            win.check(chat.session.title.indexOf("@session:") < 0 && chat.session.title.indexOf("what did we pick?") >= 0, "the title leaves the mention out (" + chat.session.title + ")")
            win.shotWindow("clicks-harlo-session-sent")
            win.sendByKeys("search past sessions for bubblewrap") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSessionSteps, () => win.chatText().split("Done:").length > 1 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = win.modelRequests(), last = reqs[reqs.length - 1]
            const result = JSON.stringify((last.messages || []).filter(m => m.role === "tool").slice(-1))
            win.check(result.indexOf("@session:a1b2c3d4") >= 0 && result.indexOf("**bubblewrap**") >= 0, "list_sessions with a query finds it, with the passage (" + result.slice(0, 300) + ")")
            win.sendByKeys("read past session @session:a1b2c3d4") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSessionSteps, () => win.chatText().split("Done:").length > 2 && chat.session.status === "idle", 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const reqs = win.modelRequests(), last = reqs[reqs.length - 1]
            const result = JSON.stringify((last.messages || []).filter(m => m.role === "tool").slice(-1))
            win.check(result.indexOf("closed") >= 0 && result.indexOf("Assistant: PICKED-BWRAP") >= 0, "read_session reads the past one (" + result.slice(0, 300) + ")")
            win.shotWindow("clicks-harlo-session-read")
            win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(codexLimitSteps, () => chat.session.modelName !== "", 10000) }
        PauseAnimation { duration: 50 }
        ScriptAction { script: {
            win.check(chat.session.modelName === "Fake Codex" && win.findName("planMeter") === null, "a Harlo session on the Codex sign-in; no meter yet (" + chat.session.modelName + ")")
            win.sendByKeys("hello") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(codexLimitSteps, () => win.chatText().indexOf("hi from codex") >= 0 && win.findName("planMeter") !== null, 15000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const m = win.findName("planMeter"), q = Workspace.quotaJson
            win.check(win.chatText().indexOf("hi from codex") >= 0, "it answers")
            win.check(m !== null && win.findText("41%", m) !== null && win.findName("planMeterLogo", m).provider === "openai-codex", "the meter shows the plan's fullest window from the reply (41%) under ChatGPT's logo")
            win.check(q.indexOf("tester@example.com") >= 0 && q.indexOf("13") >= 0, "both windows, for the signed-in account")
            win.shotWindow("clicks-codex-meter")
            win.clickItem(m, "plan meter") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const o = win.findName("meterPick-openai-codex")
            win.check(o !== null && o.hint === "41%", "its menu lists ChatGPT (41%)")
            win.summary(); Qt.quit() } }
    }
}
