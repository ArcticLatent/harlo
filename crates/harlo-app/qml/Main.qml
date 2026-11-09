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
                    SpecsPanel {
                        anchors.fill: parent
                        // (On the new-session page: the project picked there.)
                        sessionKey: chat.isDraft ? "project:" + chat.draftView.projectId : chat.session.key
                        canSend: !chat.isDraft && chat.session.key !== ""
                        drafting: chat.isDraft && chat.draftView.projectId >= 0
                        // Starts the session with it, as the button says.
                        onDraftRequested: text => {
                            chat.draftView.input.text = text
                            chat.draftView.send()
                        }
                        visible: rightPanel.shownMode === "specs"
                        onCloseRequested: windowSettings.rightMode = ""
                        onOpenRequested: path => chat.requestOpenFile(path)
                        onSendRequested: text => {
                            chat.session.send(text, chat.session.status === "busy" ? "followUp" : "normal")
                            Workspace.open(chat.session.key)
                        }
                    }
                    FilesPanel {
                        id: files
                        anchors.fill: parent
                        folder: chat.session.cwd
                        visible: rightPanel.shownMode === "files"
                        onCloseRequested: windowSettings.rightMode = ""
                        onOpenRequested: path => chat.requestOpenFile(path)
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
        const nav = "Navigation & Commands", sess = "Session Management", ui = "Interface", panels = "Panels"
        const list = [
            { id: "palette", seq: Keymap.seq("palette"), label: "Command palette", group: nav, glyph: "⌕", run: () => palette.openWith("") },
            { id: "goToSession", seq: Keymap.seq("goToSession"), label: "Go to session", group: nav, glyph: "↗", run: () => palette.openWith("") },
            { id: "searchAll", seq: Keymap.seq("searchAll"), label: "Search all sessions", group: nav, glyph: "⌕", run: () => searchDialog.openSearch() },
            { id: "actions", seq: Keymap.seq("actions"), label: "Run an action", group: nav, glyph: "›", run: () => palette.openWith(">") },
            { id: "jumpPrompt", seq: Keymap.seq("jumpPrompt"), label: "Jump to a prompt", group: nav, glyph: "≡", run: () => { if (chat.transcript.visible) chat.transcript.openPromptRail() } },
            { id: "prevPrompt", seq: Keymap.seq("prevPrompt"), label: "Previous prompt", group: nav, glyph: "↑", run: () => { if (chat.transcript.visible) chat.transcript.stepPrompt(-1) } },
            { id: "focusRuns", seq: Keymap.seq("focusRuns"), label: "Go to the session's runs", group: nav, glyph: "⤒", run: () => { if (chat.transcript.visible) chat.transcript.focusLastRun() } },
            { id: "nextPrompt", seq: Keymap.seq("nextPrompt"), label: "Next prompt", group: nav, glyph: "↓", run: () => { if (chat.transcript.visible) chat.transcript.stepPrompt(1) } },
            { id: "nextTab", seq: Keymap.seq("nextTab"), label: "Next tab", group: nav, glyph: "→", run: () => Workspace.nextTab(1) },
            { id: "nextTabAlt", seq: Keymap.seq("nextTabAlt"), label: "Next tab", group: nav, glyph: "→", run: () => Workspace.nextTab(1) },
            { id: "prevTab", seq: Keymap.seq("prevTab"), label: "Previous tab", group: nav, glyph: "←", run: () => Workspace.nextTab(-1) },
            { id: "prevTabAlt", seq: Keymap.seq("prevTabAlt"), label: "Previous tab", group: nav, glyph: "←", run: () => Workspace.nextTab(-1) },
            { id: "newSession", seq: Keymap.seq("newSession"), label: "New session", group: sess, glyph: "+", run: () => win.newSessionHere() },
            { id: "closeTab", seq: Keymap.seq("closeTab"), label: "Close tab", group: sess, glyph: "×", run: () => chat.requestCloseTab(Workspace.activeKey) },
            { id: "toggleSidebar", seq: Keymap.seq("toggleSidebar"), label: "Toggle sidebar", group: ui, glyph: "◧", run: () => win.toggleSidebar() },
            { id: "settings", seq: Keymap.seq("settings"), label: "Settings", group: ui, glyph: "⚙", run: () => settingsDialog.open() },
            { id: "shortcuts", seq: Keymap.seq("shortcuts"), label: "Keyboard shortcuts", group: ui, glyph: "?", run: () => shortcutsDialog.open() },
            // (Closing the window keeps harlo in the tray: this quits.)
            { id: "quit", seq: Keymap.seq("quit"), label: "Quit harlo", group: ui, glyph: "⏻", run: () => Qt.quit() },
            { seq: "Ctrl+Alt+L", label: "Layout readout", hidden: true, run: () => { layoutInfo.visible = !layoutInfo.visible; console.log("layout:", layoutInfo.text) } },
            { id: "contextPanel", seq: Keymap.seq("contextPanel"), label: "Context panel", group: panels, glyph: "◔", run: () => win.togglePanel("context") },
            { id: "activityPanel", seq: Keymap.seq("activityPanel"), label: "Activity panel", group: panels, glyph: "≡", run: () => win.togglePanel("activity") },
            { id: "changesPanel", seq: Keymap.seq("changesPanel"), label: "Changes panel", group: panels, glyph: "⎇", run: () => win.togglePanel("changes") },
            { id: "filesPanel", seq: Keymap.seq("filesPanel"), label: "Files panel", group: panels, glyph: "▤", run: () => win.togglePanel("files") },
            { id: "notesPanel", seq: Keymap.seq("notesPanel"), label: "Notes panel", group: panels, glyph: "✎", run: () => win.togglePanel("notes") },
            { id: "terminal", seq: Keymap.seq("terminal"), label: "Terminal", group: panels, glyph: ">", run: () => win.toggleTerminal() },
            { id: "newTerminalTab", seq: Keymap.seq("newTerminalTab"), label: "New terminal tab", group: panels, glyph: "+", run: () => win.newTerminalTab() },
            { id: "closeTerminalTab", seq: Keymap.seq("closeTerminalTab"), label: "Close terminal tab (in the terminal)", group: panels, glyph: "×", run: () => win.closeTerminalTab() },
            { id: "miniChat", seq: Keymap.seq("miniChat"), label: "Mini chat window", group: ui, glyph: "▭", run: () => win.toggleMiniChat() },
            { id: "expandComposer", seq: Keymap.seq("expandComposer"), label: "Expand the composer", group: "Composer", glyph: "⤢", run: () => { if (chat.composerItem.visible) chat.composerItem.toggleExpanded() } },
            { id: "dictate", seq: Keymap.seq("dictate"), label: "Dictate (again: insert the text)", group: "Composer", glyph: "🎙", run: () => { if (chat.composerItem.visible) chat.composerItem.toggleDictation() } },
        ]
        list.push({ seq: "Ctrl+1…9", label: "Go to tab 1–9", group: nav, glyph: "#" })
        for (let i = 1; i <= 9; i++)
            list.push({ seq: "Ctrl+" + i, label: "Go to tab " + i, hidden: true, run: () => Workspace.activateTab(i - 1) })
        return list
    }
    // Keys handled by the composer; listed in the shortcuts dialog only.
    readonly property var composerKeys: [
        { seq: "Enter", label: "Send", group: "Composer", glyph: "↵" },
        { seq: "Shift+Enter", label: "New line", group: "Composer", glyph: "¶" },
        { seq: "Ctrl+Enter", label: "Steer the running agent", group: "Composer", glyph: "⇢" },
        { seq: "Esc", label: "Stop the run", group: "Composer", glyph: "■" },
        { seq: "Esc", label: "Leave the expanded composer", group: "Composer", glyph: "⤡" },
        { seq: "↑", label: "Previous prompts (empty composer)", group: "Composer", glyph: "↕" },
        { seq: "↓", label: "Previous prompts (empty composer)", group: "Composer", glyph: "↕" },
        { seq: "Tab", label: "Build / Plan", group: "Composer", glyph: "⇥" },
        { seq: "/", label: "Commands", group: "Composer", glyph: "/" },
        { seq: "@", label: "Mention a file", group: "Composer", glyph: "@" },
        // In a run (reached with "Go to the session's runs", or a click).
        { seq: "↓ / ↑", label: "Into a run, then step by step", group: "Session", glyph: "↕" },
        { seq: "Enter", label: "Open or fold the run or step", group: "Session", glyph: "↵" },
        { seq: "Esc", label: "Back to the run, then to the composer", group: "Session", glyph: "⤺" },
        { seq: "Ctrl+C", label: "Copy a call's listing", group: "Session", glyph: "⧉" },
        { seq: "Tab", label: "Next run", group: "Session", glyph: "⇥" },
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
            acts.push({ title: "New session in " + here.name, hint: Keymap.seq("newSession"), run: () => win.openDraft(here.id) })
            acts.push({ title: "New worktree session in " + here.name + "…", run: () => sidebar.openNewWorktree(here.id, here.name, here.harness) })
        }
        acts.push({ title: "New session in home folder", run: () => win.openDraft(-1) })
        for (const p of projects)
            if (p.id !== pid)
                acts.push({ title: "New worktree session in " + p.name + "…", run: () => sidebar.openNewWorktree(p.id, p.name, p.harness) })
        acts.push({ title: "Add project…", run: () => sidebar.openAddProject() })
        acts.push({ title: "Changes panel", hint: Keymap.seq("changesPanel"), run: () => win.togglePanel("changes") })
        acts.push({ title: "Context panel", hint: Keymap.seq("contextPanel"), run: () => win.togglePanel("context") })
        acts.push({ title: "Activity panel: what the agent ran and touched", hint: Keymap.seq("activityPanel"), run: () => win.togglePanel("activity") })
        acts.push({ title: "Files panel", hint: Keymap.seq("filesPanel"), run: () => win.togglePanel("files") })
        acts.push({ title: "Notes panel", hint: Keymap.seq("notesPanel"), run: () => win.togglePanel("notes") })
        if (chat.session.key !== "") {
            acts.push({ title: "Export session as Markdown…", run: () => win.exportMarkdown(chat.session.key, chat.session.title) })
            acts.push({ title: "Copy session as Markdown", run: () => Workspace.copyMarkdown(chat.session.key) })
        }
        acts.push({ title: "Terminal", hint: Keymap.seq("terminal"), run: () => win.toggleTerminal() })
        acts.push({ title: "Toggle sidebar", hint: Keymap.seq("toggleSidebar"), run: () => win.toggleSidebar() })
        if (chat.hasSession) {
            acts.push({ title: "Close tab", hint: Keymap.seq("closeTab"), run: () => chat.requestCloseTab(Workspace.activeKey) })
            if (chat.session.status === "busy")
                acts.push({ title: "Stop the current run", hint: "Esc", run: () => chat.session.abort() })
            if (Workspace.activeKey.startsWith("live:") && chat.session.status !== "offline")
                acts.push({ title: "Stop this session's agent process", run: () => Workspace.closeSession(Workspace.activeKey) })
            if (Workspace.inWorktree(Workspace.activeKey))
                acts.push({ title: "Worktree: merge or remove…", run: () => sidebar.openWorktree(Workspace.activeKey) })
        }
        acts.push({ title: "New demo session", run: () => Workspace.newSession(-1, "demo") })
        acts.push({ title: "Search all sessions", hint: Keymap.seq("searchAll"), run: () => searchDialog.openSearch() })
        acts.push({ title: "Drop-down window", run: () => win.toggleDropDown() })
        acts.push({ title: "Mini chat window", hint: Keymap.seq("miniChat"), run: () => win.toggleMiniChat() })
        for (const r of JSON.parse(Workspace.runsJson()).slice(0, 8))
            acts.push({ title: "Run: " + r.title + (r.kept !== null && r.kept !== undefined ? " (kept)" : ""), run: () => Workspace.open("view:run:" + r.id) })
        acts.push({ title: "Usage stats", run: () => Workspace.open("view:stats") })
        acts.push({ title: "Settings", hint: Keymap.seq("settings"), run: () => settingsDialog.open() })
        acts.push({ title: "Setup guide…", run: () => win.openSetupGuide() })
        acts.push({ title: "Keyboard shortcuts", hint: Keymap.seq("shortcuts"), run: () => shortcutsDialog.open() })
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
    PhoneFirewallToast {
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
    function shot(name) {
        const path = selftestDir + "/" + name + ".png"
        page.grabToImage(r => { r.saveToFile(path); console.log("selftest: saved", path) })
    }
    /// The whole window, popups and their shadows included.
    function shotWindow(name) {
        const path = selftestDir + "/" + name + ".png"
        console.log("selftest:", TestInput.grabWindow(path) ? "saved" : "grab failed", path)
    }
    // A popup (menu, dialog): its item is the background's parent.
    function shotPopup(popup, name) {
        const path = selftestDir + "/" + name + ".png"
        const ok = popup.background.parent.grabToImage(r => { r.saveToFile(path); console.log("selftest: saved", path) })
        if (!ok) console.log("selftest: grab failed", name)
    }
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
            win.shot("clicks-spend-banner")
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
            win.shot("clicks-rail-hover")
            win.clickItem(entry, "list entry") } }
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
        ScriptAction { script: { win.check(!chat.subagentDialog.visible, "Esc closes the dialog"); win.shotWindow("clicks-subagent-tree"); win.clickText("Cancel", win.ancestorWith(win.findText("BuildOtter", chat.transcript), "viewRequested")) } }
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
            win.check(win.findText("Ctrl+K is Command palette.", settingsDialog.contentItem) !== null, "a taken key asks to replace")
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
            win.miniChat.contentItem.grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-minichat.png"))
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
            win.shot("clicks-notes")
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
            win.shot("clicks-composer")
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
            win.shot("clicks-composer-plan")
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
            win.shot("clicks-remote-session")
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
            win.shot("clicks-remote-saved")
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
            chat.fileEditor.editor.text = "my edit\n"
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
            win.shot("clicks-phone-settings")
            win.clickItem(win.findName("funnelSwitch", d), "the Funnel switch") } }
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
            win.check(p !== null && p.provider === "claude", "the project shows what its new sessions run")
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
            win.clickItem(win.findName("subagentView", chat.subagentDock), "View") } }
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
    // Midnight theme with the Activity panel, and the new-session page. Each
    // dark, then light (name-light) in the same state, for the site's themes.
    property string siteKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "siteshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            Workspace.chooseThemeMode("dark")
            win.width = 1440
            win.height = 900
            const base = win.argValue("--cwd=").replace(/\/repo$/, "")
            Workspace.addProject(base + "/webapp")
            Workspace.addProject(base + "/api")
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
        ScriptAction { script: windowSettings.rightMode = "" }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.shotWindow("site-diff"); Workspace.chooseThemeMode("light") } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { win.shotWindow("site-diff-light"); Workspace.chooseThemeMode("dark") } }
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
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdiff"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
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
            win.shotWindow("clicks-diff")
            win.clickItem(win.findName("diffNext"), "next change") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(chat.diffView.rightPane.flick.contentY > 0 || chat.diffView.rightPane.flick.contentHeight <= chat.diffView.rightPane.flick.height, "next change scrolls (" + chat.diffView.rightPane.flick.contentY + ")")
            const dv = chat.diffView
            win.check(Math.abs(dv.rightPane.rowsAt() - dv.rightPane.flick.contentY) < 1, "the rows scroll with the text (" + dv.rightPane.rowsAt() + ", " + dv.rightPane.flick.contentY + ")")
            win.shotWindow("clicks-diff-next")
            win.clickItem(win.findName("diffCollapse"), "Collapse unchanged regions") } }
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
            win.clickItem(win.findName("diffCollapse"), "Collapse unchanged regions, off") } }
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
            win.check(win.findAllNames("toolCommand", box).length === 2, "the commands under what they're for (the other is on its line)")
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
            const a = win.findContaining("Run it with", chat.transcript)
            win.check(a !== null, "the reply's code blocks")
            if (a) win.scrollIntoView(a)
            win.shotWindow("listings-reply-code")
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
            TestInput.moveTo(at.x, at.y + 30) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const box = win.listingBox()
            const copy = win.findAllNames("copyCode", box).find(c => c.visible)
            const head = copy ? (function up(i) { return !i || i.objectName === "toolHead" ? i : up(i.parent) })(copy.parent) : null
            win.check(copy && head, "hovered: its Copy, in the call's line (" + (copy ? copy.parent : "none") + ")")
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
            const top = chat.fileEditor.editor.parent.parent.contentY, lh = chat.fileEditor.editor.contentHeight / chat.fileEditor.editor.lineCount
            win.check(top > 0 && top < 37 * lh - 2 * lh, "…with lines above it in view (" + Math.round(top / lh) + " lines scrolled)")
            const band = win.findName("arrivalLine", chat.fileEditor)
            win.check(band && Math.abs(band.y - ed.cursorRectangle.y) < 1, "…tinted where you arrived")
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
            win.check(nl && nl.code && nl.code.earlier && nlLast === "1200" && up.length === 1 && /^↑ \d+ earlier lines$/.test(up[0].text) && win.findAllNames("toolEarlier", box).length === 0,
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
            win.listingAt("Number the log")
            win.shotWindow("listings-search-log-open")
            win.summary(); Qt.quit() } }
    }
    property real perfOpen: 0

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
            const summary = win.findName("reviewSummary", bar)
            win.check(summary !== null && summary.text.indexOf("2 comments on 1 file · for ") === 0, "it says how many, on how many files, for which session (" + (summary ? summary.text : "") + ")")
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
            win.shot("clicks-undo-after")
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
            git.loadRequest() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findName("checksSummary").text === "Failed: lint", "the panel shows what failed")
            win.shot("clicks-request-failed")
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
            Phone.setAccessPort(Number(win.argValue("--phone-port=") || 7676)); Phone.setAccess(true); Workspace.newSession(-1, "demo")
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
            win.findName("phoneFirewallToast").grabToImage(r => r.saveToFile(win.selftestDir + "/clicks-phone-firewall-toast.png"))
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

    // Full-text search over sessions: index, query, open at the message.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "search"
        PauseAnimation { duration: 4000 }
        ScriptAction {
            script: {
                searchDialog.openSearch()
                searchDialog.query = win.argValue("--query=")
                searchDialog.run()
                win.log("hits:", searchDialog.hits.length, "first:", searchDialog.hits.length ? searchDialog.hits[0].title + " #" + searchDialog.hits[0].index + " " + searchDialog.hits[0].snippet.slice(0, 80) : "-")
            }
        }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shotPopup(searchDialog, "search") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: searchDialog.pick(0) }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.log("opened:", Workspace.activeKey.split("/").pop(), "rows:", chat.transcript.count, "pending:", chat.pendingReveal); win.shot("search-opened") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
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
            win.shotWindow("clicks-harlo-btw")
            win.clickItem(win.findName("asideDismiss"), "dismiss the aside") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findName("asideCard") === null, "dismissing it hides the card")
            win.sendByKeys("run the build in the background") } }
        PauseAnimation { duration: 300 }
        // A background job: the turn ends at once; its end starts a run.
        ScriptAction { script: win.waitUntil(harloSteps, () => win.chatText().indexOf("The background job ended") >= 0, 20000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("Started in the background as j1") >= 0, "the command starts as a job (" + t.slice(-300) + ")")
            win.check(t.indexOf("The background job ended: built-ok") >= 0, "its end comes as a notice, with its output, and starts a run by itself")
            win.check(win.findContaining("Background job j1", chat.transcript) !== null, "the notice shows in the conversation")
            win.shotWindow("clicks-harlo-job")
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
            win.shotWindow("clicks-harlo-destructive")
            win.clickText("Deny", chat.transcript) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.waitUntil(harloSteps, () => !chat.session.hasRequest && chat.session.status === "idle", 10000) }
        PauseAnimation { duration: 300 }
        ScriptAction { script: {
            win.check(win.findAllNames("toolMark", chat.transcript).some(m => m.name === "close"), "Deny: it isn't run (" + win.chatText().slice(-160) + ")")
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
            win.harloKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "harlo")
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
            win.shotWindow("clicks-review-done") } }
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
        ScriptAction { script: win.clickItem(win.findName("mcpSave", settingsDialog.contentItem), "Save") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
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
