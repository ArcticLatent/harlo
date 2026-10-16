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
    palette.highlightedText: Theme.onPrimary
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
    // Settings → General: keep running in the tray instead of quitting.
    onClosing: close => {
        if (Workspace.setting("close_to_tray") === "1" && trayLoader.item && trayLoader.item.available) {
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
    }
    // Plan limits (omp usage): soon after starting, then every 10 minutes.
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
            if (Workspace.worktreeInfo(Workspace.activeKey) !== "")
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
        onPicked: (path, index) => {
            Workspace.open(path)
            chat.revealMessage(index)
        }
    }
    PairDeviceDialog { id: pairDeviceDialog }
    SignInDialog {
        id: signInDialog
        onClosed: if (!settingsDialog.visible) Accounts.release()
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
    // At startup (once things settle), then twice a day.
    Timer {
        interval: 6000
        running: win.selftestDir === "" && win.checkUpdates
        onTriggered: { Updates.checkLocal(); interval = 12 * 3600 * 1000; restart() }
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
        onClosed: if (!signInDialog.visible) Accounts.release()
        onTrayToggled: on => win.setTrayEnabled(on)
        onPromptRailToggled: on => chat.transcript.promptRail = on
        onRunOnHostRequested: (folder, command) => {
            settingsDialog.close()
            if (!windowSettings.terminalOpen) win.toggleTerminal()
            Qt.callLater(() => terminal.runIn(folder, command))
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
    // Approvals: Approve on the tool row; omp's goal-tool wording, the
    // waiting footer and the bar; Deny in the bar.
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
            win.check(win.findText("Allow bash to run?", chat.transcript) !== null, "the bash row asks 'Allow bash to run?'")
            win.check(win.findText("Waiting for your approval", chat.transcript) !== null, "footer says 'Waiting for your approval'")
            win.check(win.findText("Approval needed: bash", chat.requestDock) !== null, "the bar names the tool")
            win.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: { win.check(!chat.session.hasRequest && win.findText("Waiting for your approval", chat.transcript) === null, "Approve on the row runs it"); win.sendByKeys("fast: approve goal") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.findText("Let omp mark the goal complete?", chat.transcript) !== null, "goal approval reads 'Let omp mark the goal complete?'")
            win.check(win.findText("Approval needed: omp wants to update its goal", chat.requestDock) !== null, "the bar explains the goal approval")
            win.shot("clicks-approval-goal") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.clickText("Deny", chat.requestDock) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.check(!chat.session.hasRequest && win.findText("✗", chat.transcript) !== null, "Deny in the bar fails the call"); win.summary(); Qt.quit() } }
    }

    // Goal from the New Session page: toggle, budget warning, Enter, then
    // Pause / Resume / Drop in the dock.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksgoal"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.openDraft(-1); chat.draftView.harness = "demo"; if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.clickItem(win.findName("draftInput"), "draft input"); TestInput.typeText("Make the parser handle nested lists"); win.clickItem(win.findName("draftGoalButton", chat.draftView), "Goal") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(chat.draftView.goalOn, "Goal toggle turns goal mode on"); win.clickItem(win.findName("goalBudget"), "budget field"); TestInput.typeText("100") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(win.findText("100 tokens: too small for one turn", chat.draftView) !== null, "budget 100 warns 'too small for one turn'"); TestInput.key(Qt.Key_A, Qt.ControlModifier); TestInput.typeText("50k") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.check(win.findText("50,000 tokens", chat.draftView) !== null, "budget 50k reads '50,000 tokens'"); win.shot("clicks-goal-draft") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: TestInput.key(Qt.Key_Return, 0) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { const g = chat.session.goalJson ? JSON.parse(chat.session.goalJson) : {}; win.check(g.status === "active" && g.tokenBudget === 50000 && g.objective === "Make the parser handle nested lists", "Enter starts the session as a 50k goal"); win.clickText("Pause", chat.goalDock) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(JSON.parse(chat.session.goalJson).status === "paused", "Pause pauses"); win.clickText("Resume", chat.goalDock) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(JSON.parse(chat.session.goalJson).status === "active", "Resume resumes"); win.clickText("Drop", chat.goalDock) } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickText("Discard") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.check(chat.session.goalJson === "" && !chat.goalDock.visible, "Drop + Discard removes the goal"); win.summary(); Qt.quit() } }
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
        ScriptAction { script: { win.check((win.miniChat.flags & Qt.WindowStaysOnTopHint) === 0 && !win.miniChat.onTop && Workspace.setting("mini_chat_on_top") === "0", "📌 turns keep-on-top off (and on by default)"); TestInput.key(Qt.Key_Escape, 0) } }
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
    // inside the window, the provider's logo by the model, Ask first and
    // Send by clicks.
    // A narrow chat folds goal, btw and Ask first into ⋯.
    readonly property var barButtons: ["attachButton", "permissionsButton", "modeButton", "modelButton", "micButton", "sendButton", "expandButton"]
        .concat(chat.composerItem.compactBar ? ["moreButton"] : ["goalButton", "btwButton", "askFirstButton"])
    function clickAskFirst() {
        if (chat.composerItem.compactBar) {
            win.clickItem(win.findName("moreButton", chat.composerItem), "⋯")
            win.clickLater = "Ask first"
        } else {
            win.clickItem(win.findName("askFirstButton", chat.composerItem), "Ask first")
        }
    }
    // Clicked after a pause (menu items appear once the menu has opened).
    property string clickLater: ""
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
            const logo = win.findName("providerLogo", c)
            win.check(logo !== null && logo.url.startsWith("data:image/svg+xml"), "the provider's logo shows by it")
            win.check(!win.findName("sendButton", c).enabled, "Send is off with nothing typed")
            win.shot("clicks-composer")
            win.clickAskFirst() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: if (win.clickLater !== "") { win.clickText(win.clickLater); win.clickLater = "" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(chat.session.askFirst && (chat.composerItem.compactBar || win.findName("askFirstButton", chat.composerItem).on), "clicking Ask first turns it on")
            win.clickAskFirst() } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: if (win.clickLater !== "") { win.clickText(win.clickLater); win.clickLater = "" } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(!chat.session.askFirst, "clicking again turns it off")
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
            win.check(["always-ask", "write", "yolo"].includes(win.approvalBefore), "the shield starts at omp's configured mode (" + win.approvalBefore + ")")
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
            win.check(h.length === 1 && h[0].state === "connected" && h[0].omp !== "", "it connects and finds omp (" + (h.length ? h[0].state + " " + h[0].message : "") + ")")
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
        // sessions), a project there, and an omp session.
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
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const l = JSON.parse(Remotes.foldersJson)
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
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = Workspace.projectList().map(l => l.split("\t")).find(p => p[1] === "repo")
            win.remoteProject = p ? Number(p[0]) : -1
            win.check(p !== undefined && p[2] === "omp", "the remote folder is a project (omp, found there)")
            const texts = []
            const walk = it => { for (const c of it.children) { if (c.visible && typeof c.text === "string" && c.text.indexOf("box") >= 0) texts.push(c.text); walk(c) } }
            walk(sidebar)
            win.check(texts.includes("box") && Remotes.displayPath("ssh://box" + win.argValue("--cwd=")) === "box:" + win.argValue("--cwd=").replace(Workspace.homeDir(), "~"), "the project shows its host (box)")
            Workspace.newSession(win.remoteProject, "") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            win.log("remote session:", chat.session.harness, chat.session.status, chat.session.cwd, chat.session.modelName, Workspace.approvalMode(chat.session.key))
            win.check(chat.session.harness === "omp" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "a new session there runs omp in the remote folder")
            win.check(chat.session.modelName !== "", "omp answers over SSH (model " + chat.session.modelName + ")")
            win.check(["always-ask", "write", "yolo"].includes(Workspace.approvalMode(chat.session.key)), "its approval mode comes from that machine's config")
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
            win.check(onBox && onBox.where.startsWith("box:") && onBox.agents.indexOf("omp") >= 0, "the phone still gets box's project, with its machine and agents (" + JSON.stringify(onBox) + ")")
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
            win.check(chat.session.harness === "pi" && chat.session.cwd === "ssh://box" + win.argValue("--cwd="), "as a pi session in the remote folder")
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
            // Only omp and pi here (clicksmcpagents covers Claude and OpenCode).
            for (const id of ["claude", "opencode"]) Workspace.setProviderEnabled(id, false)
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
            win.check(l.length === 1 && l[0].name === "fake" && l[0].harnesses.join() === "omp,pi", "Create saves it for omp and pi (" + JSON.stringify(l.map(s => s.name + ":" + s.harnesses)) + ")")
            const st = JSON.parse(Mcp.statusJson("global")).fake
            win.check(st && st.state === "connected" && st.tools.length === 2, "and tests it: connected, 2 tools (" + JSON.stringify(st) + ")")
            const pill = win.findName("mcpStatus-fake", settingsDialog.contentItem)
            win.check(pill !== null && pill.text === "Connected · 2 tools", "the card shows the status")
            win.shotPopup(settingsDialog, "clicks-mcp-grid")
            win.clickItem(win.findName("mcpToggle-fake", settingsDialog.contentItem), "the card's switch") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const l = win.mcpList("global")
            win.check(!l[0].form.enabled && win.findName("mcpStatus-fake", settingsDialog.contentItem).text === "Disabled", "the card's switch turns it off (in both files)")
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
            win.clickItem(win.findName("mcpForPi", d), "untick pi") } }
        // (Unticking pi reflows the page: click the timeout where it is now.)
        PauseAnimation { duration: 200 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.clickItem(win.findName("mcpTimeout", d), "timeout")
            TestInput.typeText("45")
            win.clickItem(win.findName("mcpSave", d), "Save") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const l = win.mcpList("global")
            win.check(l.length === 1 && l[0].harnesses.join() === "omp" && l[0].form.timeoutSecs === 45, "unticking pi removes it from pi's file; the timeout is kept (" + JSON.stringify(l[0]) + ")")
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
            win.check(l.length === 2 && l.every(s => s.harnesses.join() === "omp,pi"), "importing two servers adds both to the project (" + JSON.stringify(l.map(s => s.name)) + ")")
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
            win.check(Workspace.fileStamp(win.mcpProject + "/.omp/mcp.json") !== "" && Workspace.fileStamp(win.mcpProject + "/.pi/mcp.json") !== "", "project servers are in .omp/mcp.json and .pi/mcp.json")
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
            win.check(chat.session.requestIsApproval && win.findText("Approval needed: Write", chat.requestDock) !== null, "a Write asks for approval")
            win.shot("clicks-claude-approval")
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
            if (chat.session.harness !== "claude") { win.check(false, "the reopened session is the Claude one"); win.summary(); Qt.quit() }
            else win.sendByKeys("one more") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("Hello from fake claude") >= 0 && chat.session.status === "idle" && win.chatText().split("Hello from fake claude").length > 1, "and it carries on (resumed)")
            const hits = JSON.parse(Workspace.searchMessages("picked Postgres", 20))
            win.check(hits.some(h => String(h.path).indexOf("/claude-config/projects/") >= 0), "message search finds Claude's sessions (" + hits.length + " hits)")
            win.shot("clicks-claude")
            win.summary(); Qt.quit() } }
    }

    // OpenCode sessions, against a stand-in server (scripts/fake-opencode.py,
    // --fake-opencode=): a reply, a permission, a form, model / variant /
    // Plan (agent) switches (the stand-in's replies report them), stopping,
    // and the session in the sidebar, reopened with its history and resumed.
    property string openCodeKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksopencode"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { win.openCodeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "opencode") } }
        PauseAnimation { duration: 2500 }
        // Named before it exists in OpenCode: created with the name.
        ScriptAction { script: { Workspace.renameSession(win.openCodeKey, "My OpenCode chat"); win.sendByKeys("hello there") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.harness === "opencode", "an OpenCode session (" + chat.session.harness + ")")
            win.check(win.chatText().indexOf("Hello from fake opencode (model fakeai/fake-small, variant default, agent build)") >= 0, "it streams the reply (" + win.chatText().slice(-160) + ")")
            win.check(chat.session.modelId.indexOf("fake-small") >= 0, "the model OpenCode chose shows (" + chat.session.modelId + ")")
            win.check(chat.session.contextText.indexOf("1k/128k") >= 0, "and its context use (" + chat.session.contextText + ")")
            win.sendByKeys("please approve listing the folder") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.requestIsApproval && win.findText("Approval needed: shell", chat.requestDock) !== null, "a shell call asks for permission") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { win.shotWindow("clicks-opencode-approval"); win.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Listed it.") >= 0 && win.chatText().indexOf("notes.txt") >= 0, "Approve lets it run")
            win.sendByKeys("approve it again") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const always = win.findName("alwaysAllowInline", chat.transcript)
            win.check(always !== null && always.ToolTip.text === "Remember: ls *", "it offers Always allow, naming what it remembers (" + (always ? always.ToolTip.text : "none") + ")")
            if (always) win.clickItem(always, "Always allow") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Listed it, and may from now on.") >= 0, "Always allow reaches OpenCode as always")
            win.sendByKeys("ask me which database") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.hasRequest && chat.session.requestMethod === "ask" && win.findText("Which database?", chat.askPanel) !== null, "its form shows in the ask panel")
            win.clickText("Postgres", chat.askPanel) } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: win.clickText("Send answers", chat.askPanel) }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            // The label goes back as the field's value.
            win.check(win.chatText().indexOf("You picked pg.") >= 0, "the answer goes back (" + win.chatText().slice(-80) + ")")
            chat.session.setModel("fakeai", "fake-large")
            win.clickItem(win.findName("modeButton", chat.composerItem), "Plan") } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.sendByKeys("what now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("(model fakeai/fake-large, variant default, agent plan)") >= 0, "the model and Plan reach OpenCode (" + win.chatText().slice(-110) + ")")
            win.clickItem(win.findName("modeButton", chat.composerItem), "Build")
            chat.session.setModel("fakeai", "fake-small") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { chat.session.loadThinkingLevels() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(chat.session.thinkingLevels.length === 2 && chat.session.thinkingLevels[1] === "high", "its variants are the thinking levels (" + chat.session.thinkingLevels + ")")
            chat.session.setThinkingLevel("high") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.sendByKeys("and now") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("(model fakeai/fake-small, variant high, agent build)") >= 0, "the variant and Build reach OpenCode (" + win.chatText().slice(-110) + ")")
            // The shield: edits go through, commands still ask.
            win.clickItem(win.findName("permissionsButton", chat.composerItem), "the shield") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Workspace.approvalMode(win.openCodeKey) === "write", "the shield is on edits (" + Workspace.approvalMode(win.openCodeKey) + ")")
            win.sendByKeys("please rewrite the notes") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Rewrote it.") >= 0, "an edit goes through without asking")
            win.sendByKeys("approve one more listing") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(chat.session.requestIsApproval, "a command still asks")
            win.clickText("Deny", chat.transcript) } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.clickItem(win.findName("permissionsButton", chat.composerItem), "the shield") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(Workspace.approvalMode(win.openCodeKey) === "yolo", "the shield is on auto")
            win.sendByKeys("approve the listing too") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().split("Listed it.").length >= 3, "on auto, commands go through too")
            win.sendByKeys("go slow please") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(chat.session.status === "busy", "a slow reply runs")
            win.clickItem(win.findName("stopButton", chat.composerItem), "Stop") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.check(chat.session.status === "idle", "Stop interrupts it (" + chat.session.status + ")")
            Workspace.closeSession(win.openCodeKey) } }
        // The scanner lists it from OpenCode's database; open it again.
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            // Its own session (in the test's data folder) only.
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf(win.selftestDir + "/data/opencode/sessions/ses_") === 0) saved = Workspace.keyAt(i)
            win.check(saved !== "", "the session is in the sidebar (" + saved + ")")
            if (saved === "") { win.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(win.chatText().indexOf("hello there") >= 0 && win.chatText().indexOf("You picked pg.") >= 0, "reopened, it shows its history")
            win.check(chat.session.title === "My OpenCode chat", "and the name it was given before it began (" + chat.session.title + ")")
            if (chat.session.harness !== "opencode") { win.check(false, "the reopened session is the OpenCode one"); win.summary(); Qt.quit() }
            else win.sendByKeys("one more") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(win.chatText().split("Hello from fake opencode").length > 4 && chat.session.status === "idle", "and it carries on (resumed)")
            win.shot("clicks-opencode")
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
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
            Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const id = JSON.parse(Workspace.projectsJson())[0].id
            Workspace.setProjectHarness(id, "claude")
            Workspace.newSession(id, "opencode") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "claude") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("sessionProvider", sidebar, []).map(m => m.provider).sort()
            win.check(marks.join(",") === "claude,demo,opencode", "each session shows its provider's mark (" + marks + ")")
            const p = win.findName("projectProvider-repo", sidebar)
            win.check(p !== null && p.provider === "claude", "the project shows what its new sessions run")
            win.check(win.findText("Demo", win.findName("sessionProviderChip")) !== null, "the header names the provider")
            const tabMarks = win.findAllNamed("tabProvider", win.contentItem, []).map(m => m.provider).sort()
            win.check(tabMarks.join(",") === "claude,demo,opencode", "and so do the tabs (" + tabMarks + ")")
            const oc = win.findAllNamed("sessionProvider", sidebar, []).find(m => m.provider === "opencode")
            const at = oc.mapToItem(null, oc.width / 2, oc.height / 2)
            TestInput.moveTo(at.x, at.y) } }
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            win.check(win.findText("OpenCode") !== null, "hovered, the mark says which provider")
            win.shotWindow("clicks-marks")
            const p = JSON.parse(Workspace.projectsJson())[0]
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = p.path; sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("menuProvider", null, []).map(m => m.provider)
            win.check(marks.filter(m => m === "claude").length === 2 && marks.indexOf("opencode") >= 0, "the project menu shows the providers' marks (" + marks + ")")
            win.shotWindow("clicks-marks-menu")
            sidebar.projectMenu.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: win.clickItem(win.findName("draftHarness", chat.draftView), "the provider chip") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const marks = win.findAllNamed("menuProvider", null, []).map(m => m.provider)
            win.check(marks.indexOf("claude") >= 0 && marks.indexOf("opencode") >= 0 && marks.indexOf("pi") >= 0, "so does New session's (" + marks + ")")
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
            Workspace.setSetting("omp_path", win.selftestDir + "/missing-omp")
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

    // Subagents of Claude and OpenCode sessions (Stage 4), against the
    // stand-ins: "delegate" starts one; the dock lists it by its
    // description, its transcript opens, a reopened session lists it from
    // disk / OpenCode's database; an OpenCode subagent can be cancelled.
    function subagentsOf() { try { return JSON.parse(chat.session.subagentsJson || "[]") } catch (e) { return [] } }
    property string delegateKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksdelegate"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
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
            win.check(win.findName("steerField") === null && win.findText("Cancel subagent") === null, "no Steer or Cancel for Claude's")
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
            win.delegateKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "opencode") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: win.sendByKeys("please delegate this") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].name === "Look around" && a[0].agent === "explore" && a[0].status === "completed" && String(a[0].childSession).indexOf("ses_") === 0, "OpenCode: the subagent is listed, done (" + JSON.stringify(a) + ")")
            win.check(win.chatText().indexOf("The subagent found notes.md.") >= 0, "and the reply")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            win.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "View: its session (" + t.slice(0, 120).replace(/\n/g, " | ") + ")")
            chat.subagentDialog.close()
            win.sendByKeys("delegate slowly please") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 2 && a[1].status === "running", "a slow one runs (" + (a[1] ? a[1].status : "none") + ")")
            chat.subagentDock.expanded = true
            const c = win.findName("subagentCancel", chat.subagentDock)
            win.check(c !== null, "with Cancel")
            if (c) win.clickItem(c, "Cancel") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 2 && a[1].status === "aborted" && win.chatText().indexOf("The subagent was stopped.") >= 0, "Cancel stops it (" + (a[1] ? a[1].status : "none") + ")")
            win.shotWindow("clicks-delegate-opencode")
            Workspace.closeSession(win.delegateKey) } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf(win.selftestDir + "/data/opencode/sessions/ses_") === 0) saved = Workspace.keyAt(i)
            win.check(saved !== "", "the OpenCode session (not its subagents) is in the sidebar")
            if (saved !== "") Workspace.open(saved) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(chat.session.harness === "opencode" && a.length === 2 && a[0].status === "completed" && a[1].status === "aborted" && a.every(x => x.id.indexOf("call_") === 0), "reopened: listed from OpenCode's database, linked to their calls (" + JSON.stringify(a.map(x => [x.id, x.status])) + ")")
            win.summary(); Qt.quit() } }
    }

    // MCP servers for Claude and OpenCode (Stage 4): a new server is for
    // the installed agents; it lands in each one's own dialect (Claude's
    // global ones through `claude mcp`, the stand-in here); OpenCode's
    // switch works, Claude has none; removing reaches both; a project gets
    // .mcp.json and opencode.json; a commented opencode.jsonc is read, not
    // rewritten.
    function readJson(path) { try { return JSON.parse(TestInput.readFile(path) || "{}") } catch (e) { return {} } }
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksmcpagents"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
            for (const id of ["omp", "pi"]) Workspace.setProviderEnabled(id, false)
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickItem(win.findName("mcpAdd", settingsDialog.contentItem), "Add MCP server") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, c = win.findName("mcpForClaude", d), o = win.findName("mcpForOpenCode", d)
            win.check(c !== null && c.checked && o !== null && o.checked && win.findName("mcpForOmp", d) === null, "a new server is for Claude and OpenCode (the installed agents that are on)")
            win.shotWindow("clicks-mcp-agents")
            win.clickItem(win.findName("mcpBack", d), "back")
            const err = Mcp.save("global", "", JSON.stringify({ name: "files", enabled: true, transport: "stdio", command: ["npx", "-y", "files-mcp"], env: [["TOKEN", "t"]], url: "", headers: [], cwd: "", timeoutSecs: 20 }), JSON.stringify(["claude", "opencode"]))
            win.check(err === "", "saved (" + err + ")") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            const oc = (win.readJson(win.selftestDir + "/config/opencode/opencode.json").mcp || {})
            win.check(JSON.stringify(claude.files) === JSON.stringify({ type: "stdio", command: "npx", args: ["-y", "files-mcp"], env: { TOKEN: "t" } }), "Claude's dialect, through claude mcp (" + JSON.stringify(claude.files) + ")")
            win.check(JSON.stringify(oc.files) === JSON.stringify({ type: "local", command: ["npx", "-y", "files-mcp"], environment: { TOKEN: "t" }, timeout: 20000, enabled: true }), "OpenCode's dialect (" + JSON.stringify(oc.files) + ")")
            const l = win.mcpList("global")
            win.check(l.length === 1 && l[0].harnesses.join(",") === "claude,opencode" && !l[0].differs, "listed once, for both")
            win.check(Mcp.setEnabled("global", "files", false) === "", "switched off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const oc = win.readJson(win.selftestDir + "/config/opencode/opencode.json").mcp || {}
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            win.check(oc.files.enabled === false && !!claude.files && win.mcpList("global")[0].differs, "OpenCode's is off; Claude's stays (no switch)")
            win.check(Mcp.remove("global", "files") === "", "removed") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const oc = win.readJson(win.selftestDir + "/config/opencode/opencode.json").mcp || {}
            const claude = win.readJson(win.selftestDir + "/claude-config/.claude.json").mcpServers || {}
            win.check(!oc.files && !claude.files && win.mcpList("global").length === 0, "gone from both")
            const project = win.argValue("--cwd=")
            win.check(Mcp.save(project, "", JSON.stringify({ name: "web", enabled: true, transport: "http", command: [], env: [], url: "https://example.com/mcp", headers: [["Authorization", "Bearer x"]], cwd: "" }), JSON.stringify(["claude", "opencode"])) === "", "a project server") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const project = win.argValue("--cwd=")
            const claude = win.readJson(project + "/.mcp.json").mcpServers || {}, oc = win.readJson(project + "/opencode.json").mcp || {}
            win.check(claude.web && claude.web.type === "http" && claude.web.headers.Authorization === "Bearer x" && oc.web && oc.web.type === "remote" && oc.web.url === "https://example.com/mcp", "in the project's .mcp.json and opencode.json")
            const wt = win.selftestDir + "/wt", l = win.mcpList(wt)
            win.check(l.length === 1 && l[0].name === "commented" && l[0].harnesses.join(",") === "opencode", "a commented opencode.jsonc is read")
            const err = Mcp.save(wt, "commented", JSON.stringify(Object.assign({}, l[0].form, { url: "https://other/mcp" })), JSON.stringify(["opencode"]))
            win.check(err.indexOf("has comments") >= 0 && TestInput.readFile(wt + "/opencode.jsonc").indexOf("// keep me") >= 0, "and not rewritten (" + err + ")")
            settingsDialog.close()
            win.summary(); Qt.quit() } }
    }

    // Edit & resend for Claude and OpenCode (Stage 4): hovering a prompt
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
            forkSteps.harness = "opencode"
            forkSteps.restart() } }
    }
    SequentialAnimation {
        id: forkSteps
        property string harness: "claude"
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksfork"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
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
            const install = win.findName("hostInstall-box-opencode", d)
            win.check(install !== null && install.command === "curl -fsSL https://opencode.ai/install | bash", "a missing one offers its installer (" + (install ? install.command : "none") + ")")
            win.check(win.findName("hostSignIn-box-claude", d) !== null && h.claudeAuth === "no", "Claude not signed in there: Sign in")
            win.check(win.findName("hostInstall-box-claude", d) === null && win.findName("hostRecheck-box", d) !== null, "and Check again")
            win.shotWindow("clicks-remote-agents")
            // Install runs in the background: the page stays.
            win.clickItem(install, "Install OpenCode") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(settingsDialog.visible && win.findName("hostInstalling-box-opencode", d) !== null && !windowSettings.terminalOpen, "installing: a progress bar, the page stays, no terminal")
            win.shotWindow("clicks-remote-installing") } }
        PauseAnimation { duration: 3500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, m = win.findName("hostInstallMessage-box-opencode", d)
            win.check(m && m.text === "OpenCode is installed on box.", "done: it says so (" + (m ? m.text : "none") + ")")
            win.clickItem(win.findName("hostInstall-box-pi", d), "Install Pi") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, m = win.findName("hostInstallMessage-box-pi", d)
            win.check(m && m.text === "Installing Pi on box failed: error: no space left on device", "a failure says why (" + (m ? m.text : "none") + ")")
            win.check(Remotes.installOutputCommand("box", "pi").indexOf("tail -n +1 ") === 0, "and its output can be shown")
            win.shotWindow("clicks-remote-install-failed")
            win.clickItem(win.findName("hostInstallOutput-box-pi", d), "View output") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(!settingsDialog.visible && windowSettings.terminalOpen, "View output: the terminal shows it")
            win.toggleTerminal()
            Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = "ssh://box" + win.argValue("--cwd="); sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("New Claude session") !== null && win.findText("New Oh My Pi session") !== null && win.findText("New Pi session") === null && win.findText("New OpenCode session") === null, "a remote project offers the agents its host has (Claude, omp; not pi, missing there)")
            sidebar.projectMenu.close()
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            win.remoteClaudeKey = Workspace.newSession(p.id, "claude") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const file = Workspace.sessionFile(Workspace.activeKey)
            win.check(chat.session.harness === "claude" && win.chatText().indexOf("Hello from fake claude") >= 0, "Claude answers over SSH (" + win.chatText().slice(-80) + ")")
            win.check(file.indexOf("ssh://box" + win.selftestDir + "/claude-config/projects/") === 0, "its transcript is the host's (" + file + ")")
            // Orchestration there: the bridge on the host, harlo's socket
            // forwarded to it; the helper starts on the host too.
            win.sendByKeys("please orchestrate a helper") } }
        PauseAnimation { duration: 6000 }
        ScriptAction { script: {
            const t = win.chatText(), kids = JSON.parse(Workspace.orchestratedJson(win.remoteClaudeKey))
            win.check(t.indexOf("Started a helper with harlo (6 tools): live:") >= 0, "on the host, the agent finds harlo's tools (" + t.slice(-110) + ")")
            win.check(kids.length === 1 && Workspace.instanceOf(kids[0].id) === "box", "and its helper runs on box (" + JSON.stringify(kids) + ")")
            win.check(t.indexOf("[harlo] The Claude session") >= 0, "whose reply comes back to it")
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
            win.check(win.findName("hostProviderInstall-opencode", d) !== null && win.findName("provider-claude", d) === null, "with Install for the missing ones, not this computer's list")
            win.shotWindow("clicks-remote-providers")
            settingsDialog.openPage("mcp") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const err = Mcp.save("host:box", "", JSON.stringify({ name: "boxtools", enabled: true, transport: "stdio", command: ["box-mcp"], url: "", headers: [], env: [], cwd: "", timeoutSecs: null, exposure: "", description: "" }), JSON.stringify(["omp", "claude"]))
            const home = win.selftestDir + "/claude-config"
            const omp = JSON.parse(TestInput.readFile(home + "/.omp/agent/mcp.json") || "{}").mcpServers || {}
            const claude = JSON.parse(TestInput.readFile(home + "/.claude.json") || "{}").mcpServers || {}
            win.check(err === "" && omp.boxtools && omp.boxtools.command === "box-mcp", "MCP on box writes box's own omp config (" + err + ")")
            win.check(claude.boxtools !== undefined, "and box's Claude (claude mcp there)")
            const l = JSON.parse(Mcp.listJson("host:box"))
            win.check(l.servers.some(x => x.name === "boxtools" && x.harnesses.indexOf("claude") >= 0 && x.harnesses.indexOf("omp") >= 0), "which the page lists for box")
            // Orchestration on box: its skills and omp's bridge, over SSH.
            settingsDialog.openPage("orchestration") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: win.clickItem(win.findName("orchSkill-harlo-advisor", settingsDialog.contentItem), "harlo-advisor on box") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const home = win.selftestDir + "/claude-config"
            win.check(TestInput.readFile(home + "/.agents/skills/harlo-advisor/SKILL.md").indexOf("name: harlo-advisor") >= 0 && TestInput.readFile(home + "/.claude/skills/harlo-advisor/.harlo-skill") !== "", "a skill installs on box (both folders, marked)")
            win.check(win.findName("orchSkill-harlo-advisor", settingsDialog.contentItem).checked, "and shows as installed there")
            win.clickItem(win.findName("orchBridge-omp", settingsDialog.contentItem), "omp's bridge on box") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const c = JSON.parse(TestInput.readFile(win.selftestDir + "/claude-config/.omp/agent/mcp.json") || "{}").mcpServers || {}
            win.check(c.harlo && c.harlo.command === "python3" && String(c.harlo.args[0]).endsWith("/harlo_mcp_bridge.py") && String(c.harlo.args[1]).endsWith(".sock"), "omp on box gets the bridge there (" + JSON.stringify(c.harlo) + ")")
            win.shotWindow("clicks-remote-orchestration")
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

    // OpenCode on a remote host: its server there, reached through a port
    // forwarded over SSH (the stand-in, its data in the test's folder): a
    // reply, an approval, a name; the host's OpenCode sessions listed from
    // that server, reopened with their history and resumed.
    property string remoteOpenCodeKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksremoteopencode"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Remotes.saveHost("", "box", "ssh " + win.argValue("--ssh="))
            Remotes.connectHost("box") } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const h = JSON.parse(Remotes.hostsJson())[0]
            win.check(h.state === "connected" && h.opencode !== "" && h.versions.opencode === "2.0.25", "the host has OpenCode (" + JSON.stringify(h.versions) + ")")
            Workspace.addProject("ssh://box" + win.argValue("--cwd=")) } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            const p = JSON.parse(Workspace.projectsJson()).find(p => p.remote)
            sidebar.projectMenu.projectId = p.id; sidebar.projectMenu.path = "ssh://box" + win.argValue("--cwd="); sidebar.projectMenu.name = p.name; sidebar.projectMenu.harness = p.harness
            sidebar.projectMenu.popup(sidebar, 120, 160) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("New OpenCode session") !== null, "a remote project offers OpenCode")
            sidebar.projectMenu.close()
            win.remoteOpenCodeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson()).find(p => p.remote).id, "opencode") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: win.sendByKeys("hello there") }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            win.check(chat.session.harness === "opencode" && win.chatText().indexOf("Hello from fake opencode") >= 0, "OpenCode answers from the host (" + win.chatText().slice(-80) + ")")
            win.check(Workspace.sessionFile(Workspace.activeKey).indexOf("ssh://box/opencode-sessions/ses_") === 0, "the session is the host's (" + Workspace.sessionFile(Workspace.activeKey) + ")")
            win.sendByKeys("please approve listing") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(chat.session.requestIsApproval, "its permission comes over")
            win.clickText("Approve", chat.transcript) } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            win.check(!chat.session.hasRequest && win.chatText().indexOf("Listed it.") >= 0, "and the answer goes back")
            Workspace.renameSession(win.remoteOpenCodeKey, "Remote OpenCode")
            win.sendByKeys("please delegate this") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].status === "completed" && String(a[0].sessionFile).indexOf("ssh://box/opencode-sessions/ses_") === 0, "a subagent there (" + (a[0] ? a[0].status + " " + a[0].sessionFile : "none") + ")")
            if (a.length) chat.subagentDialog.show(a[0], false) } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const t = chat.subagentDialog.transcriptSession.transcriptText()
            win.check(t.indexOf("List the files") >= 0 && t.indexOf("Found notes.md.") >= 0, "its session opens from the host's server")
            chat.subagentDialog.close() } }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: Workspace.closeSession(win.remoteOpenCodeKey) }
        PauseAnimation { duration: 7000 }
        ScriptAction { script: {
            let saved = ""
            for (let i = 0; Workspace.keyAt(i) !== ""; i++) if (Workspace.keyAt(i).indexOf("ssh://box/opencode-sessions/ses_") === 0) saved = Workspace.keyAt(i)
            win.check(saved !== "", "the host's OpenCode session is in the sidebar (" + saved + ")")
            if (saved === "") { win.summary(); Qt.quit() }
            else Workspace.open(saved) } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            win.check(chat.session.harness === "opencode" && win.chatText().indexOf("hello there") >= 0 && win.chatText().indexOf("Listed it.") >= 0 && chat.session.title === "Remote OpenCode", "reopened: its history and name, from the host (" + chat.session.title + ")")
            const a = win.subagentsOf()
            win.check(a.length === 1 && a[0].status === "completed" && a[0].id.indexOf("call_") === 0, "and its subagent, linked to its call (" + JSON.stringify(a.map(x => [x.id, x.status])) + ")")
            if (chat.session.harness !== "opencode") { win.summary(); Qt.quit() }
            else win.sendByKeys("one more") } }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: {
            win.check(win.chatText().split("Hello from fake opencode").length > 3 && chat.session.status === "idle", "and it carries on there (resumed)")
            win.shotWindow("clicks-remote-opencode")
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
            win.check(t.indexOf("Started a helper with harlo (6 tools): live:") >= 0, "the agent finds harlo's tools and starts a session (" + t.slice(-90) + ")")
            const kids = JSON.parse(Workspace.orchestratedJson(win.orchestrator))
            win.check(kids.length === 1 && kids[0].name === "Helper" && kids[0].agent === "claude", "it's the agent's subagent (" + JSON.stringify(kids) + ")")
            win.check(win.findText("Helper", chat.subagentDock) !== null, "and shows in its dock") } }
        PauseAnimation { duration: 3000 }
        ScriptAction { script: {
            const t = win.chatText(), kids = JSON.parse(Workspace.orchestratedJson(win.orchestrator))
            win.check(kids.length === 1 && kids[0].status === "completed", "the helper finishes (" + (kids[0] ? kids[0].status : "") + ")")
            win.check(t.indexOf("[harlo] The Claude session") >= 0 && t.indexOf("Thanks; harlo says") >= 0 && t.indexOf("Hello from fake claude") >= 0, "and its reply comes back to the agent (" + t.slice(-120) + ")")
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

    // The real OpenCode (not in the click suite): one tiny turn on a free
    // model in --cwd.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "opencodereal"
        PauseAnimation { duration: 800 }
        ScriptAction { script: { Workspace.addProject(win.argValue("--cwd=")) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.openCodeKey = Workspace.newSession(JSON.parse(Workspace.projectsJson())[0].id, "opencode") } }
        PauseAnimation { duration: 8000 }
        ScriptAction { script: { chat.session.setModel("opencode", "ling-3.1-flash-free"); win.sendByKeys("Reply with exactly: OK") } }
        PauseAnimation { duration: 60000 }
        ScriptAction { script: {
            win.log("status:", chat.session.status, "model:", chat.session.modelId, "context:", chat.session.contextText)
            win.log("transcript:", win.chatText().replace(/\n/g, " | "))
            Qt.quit() } }
    }

    // A real subagent (not in the click suite): --harness= claude|opencode,
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
            for (const id of ["opencode", "pi", "omp"]) Workspace.setSetting(id + "_path", win.selftestDir + "/missing-" + id)
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
    // Midnight theme with the Activity panel, and the new-session page.
    property string siteKey: ""
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "siteshots"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: {
            win.width = 1440
            win.height = 900
            const base = win.argValue("--cwd=").replace(/\/repo$/, "")
            Workspace.addProject(base + "/webapp")
            Workspace.addProject(base + "/api")
            Workspace.saveTemplate(JSON.stringify({ name: "Review a pull request", harness: "claude", approval: "always-ask", text: "Review the open pull request: correctness first, then tests." }))
            Workspace.saveTemplate(JSON.stringify({ name: "Fix the failing tests", harness: "omp", approval: "write", text: "Run the tests and fix what fails." })) } }
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
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-main") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: win.clickText("guard.ts", changes) }
        PauseAnimation { duration: 800 }
        ScriptAction { script: windowSettings.rightMode = "" }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.shotWindow("site-diff") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            Workspace.chooseTheme("Midnight")
            Workspace.open(win.siteKey)
            windowSettings.rightMode = "activity" } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { chat.transcript.scrollToEnd(); win.shotWindow("site-midnight") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { Workspace.chooseTheme("harlo"); windowSettings.rightMode = ""; win.openDraft(1) } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: win.shotWindow("site-new") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: { win.summary(); Qt.quit() } }
    }

    // harlo's own update (an AppImage here): offered from its release,
    // downloaded, checked against SHA256SUMS, put in place; then Restart.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksselfupdate"
        PauseAnimation { duration: 900 }
        ScriptAction { script: { if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth=")); Updates.checkLocal() } }
        PauseAnimation { duration: 2500 }
        ScriptAction { script: {
            const m = win.findName("updateMessage-local/harlo")
            win.check(m !== null && m.text === "Version 9.9.9 is available (you have 1.0.0).", "a newer harlo is offered (" + (m ? m.text : Updates.entriesJson) + ")")
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
            Workspace.setSetting("opencode_path", win.argValue("--fake-opencode="))
            for (const id of ["pi", "omp"]) Workspace.setSetting(id + "_path", win.selftestDir + "/missing-" + id)
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
            win.check(agent && agent.currentText === "OpenCode", "Second opinion: another agent by default (" + (agent ? agent.currentText : "") + ")")
            win.check(note && note.text === "is the cache right?", "the composer's text is the question")
            win.shotWindow("clicks-delegate-advisor")
            win.clickItem(win.findName("delegateStart"), "Ask") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(t.indexOf("Get a second opinion from OpenCode (agent \"opencode\", its default model)") >= 0 && t.indexOf("The question: is the cache right?") >= 0, "the agent gets harlo's instruction (" + t.slice(0, 120) + ")")
            win.check(chat.composerItem.text === "", "and the composer is empty")
            win.clickItem(win.findName("delegateButton"), "the Delegate button") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.clickItem(win.findName("delegateCommittee"), "Committee…") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            const a = win.findName("delegateAgentMember1"), b = win.findName("delegateAgentMember2"), m = win.findName("delegateModelMember1")
            win.check(a && b && a.currentText === "Claude" && b.currentText === "OpenCode", "Committee: this agent and another (" + (a ? a.currentText : "") + ", " + (b ? b.currentText : "") + ")")
            win.check(m && m.count > 2, "with Claude's models to pick from (" + (m ? m.count : 0) + ")")
            m.currentIndex = 1
            m.activated(1)
            win.findName("delegateNote").text = "flaky tests"
            win.clickItem(win.findName("delegateStart"), "Start committee") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const t = win.chatText()
            win.check(/- Claude \(agent "claude", model "[^"]+"\)/.test(t) && t.indexOf("- OpenCode (agent \"opencode\"") >= 0 && t.indexOf("The problem: flaky tests") >= 0, "both members, with the chosen model, go to the agent")
            win.summary(); Qt.quit() } }
    }

    // The Delegate menu with real agents (not in the click suite): Claude
    // (Haiku) asks OpenCode (a free model) for a second opinion.
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
            row.pick("opencode")
            row.model = "opencode/ling-3.1-flash-free"
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

    // Settings → Orchestration: skills installed into both skill folders
    // (and removed), a user's own skill of the same name left alone, and
    // the bridge in pi's MCP config (another server kept; omp's own
    // "harlo" left alone).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksorchestration"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            settingsDialog.openPage("orchestration") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findName("orchSkill-harlo", d) !== null && !win.findName("orchSkill-harlo", d).checked, "the skills, none installed")
            win.shotWindow("clicks-orchestration")
            win.clickItem(win.findName("orchSkill-harlo-advisor", d), "harlo-advisor's switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const h = win.selftestDir + "/home", f = TestInput.readFile(h + "/.agents/skills/harlo-advisor/SKILL.md")
            win.check(f.indexOf("name: harlo-advisor") >= 0 && TestInput.readFile(h + "/.claude/skills/harlo-advisor/SKILL.md") === f, "installed in both skill folders")
            win.check(TestInput.readFile(h + "/.claude/skills/harlo-advisor/.harlo-skill") !== "", "marked as harlo's")
            win.clickItem(win.findName("orchSkillsAll", settingsDialog.contentItem), "All skills") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, h = win.selftestDir + "/home", p = win.findName("orchProblem", d)
            win.check(TestInput.readFile(h + "/.claude/skills/harlo-committee/SKILL.md").indexOf("name: harlo-committee") >= 0, "All skills: the others installed")
            win.check(TestInput.readFile(h + "/.agents/skills/harlo-handoff/SKILL.md") === "mine\n" && TestInput.readFile(h + "/.claude/skills/harlo-handoff/SKILL.md") === "", "the user's own harlo-handoff left alone (and not half-installed)")
            win.check(p && p.visible && p.text.indexOf("harlo-handoff") >= 0, "and said so (" + (p ? p.text : "") + ")")
            win.check(!win.findName("orchSkillsAll", d).checked, "so not all skills are on")
            win.clickItem(win.findName("orchSkill-harlo-advisor", d), "harlo-advisor's switch, off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const h = win.selftestDir + "/home"
            win.check(TestInput.readFile(h + "/.agents/skills/harlo-advisor/SKILL.md") === "" && TestInput.readFile(h + "/.claude/skills/harlo-advisor/SKILL.md") === "", "removed from both folders")
            win.check(TestInput.readFile(h + "/.agents/skills/harlo-handoff/SKILL.md") === "mine\n", "the user's skill still there")
            win.clickItem(win.findName("orchBridge-pi", settingsDialog.contentItem), "pi's bridge switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const c = JSON.parse(TestInput.readFile(win.selftestDir + "/home/.pi/agent/mcp.json")).mcpServers
            win.check(c.harlo && c.harlo.args.indexOf("--mcp-bridge") >= 0 && c.harlo.args.indexOf("--socket") >= 0 && c.harlo.exposure === "direct", "pi: harlo's bridge added (" + JSON.stringify(c.harlo) + ")")
            win.check(c.other && c.other.command === "other-server", "pi's other server kept")
            win.clickItem(win.findName("orchBridge-omp", settingsDialog.contentItem), "omp's bridge switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, p = win.findName("orchProblem", d)
            const c = JSON.parse(TestInput.readFile(win.selftestDir + "/home/.omp/agent/mcp.json")).mcpServers
            win.check(c.harlo.command === "someone-elses" && !win.findName("orchBridge-omp", d).checked, "omp's own harlo server left alone")
            win.check(p && p.text.indexOf("already has") >= 0, "and said so (" + (p ? p.text : "") + ")")
            win.clickItem(win.findName("orchBridge-pi", d), "pi's bridge switch, off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const c = JSON.parse(TestInput.readFile(win.selftestDir + "/home/.pi/agent/mcp.json")).mcpServers
            win.check(!c.harlo && c.other, "pi: the bridge removed, the other server kept")
            Workspace.setOrchestrationBridge("pi", true)
            win.clickItem(win.findName("orchEnabled", settingsDialog.contentItem), "Orchestration's switch, off") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const h = win.selftestDir + "/home", d = settingsDialog.contentItem
            const c = JSON.parse(TestInput.readFile(h + "/.pi/agent/mcp.json")).mcpServers
            win.check(TestInput.readFile(h + "/.claude/skills/harlo/SKILL.md") === "" && TestInput.readFile(h + "/.agents/skills/harlo-committee/SKILL.md") === "", "off: harlo's skills removed")
            win.check(!c.harlo && c.other && TestInput.readFile(h + "/.agents/skills/harlo-handoff/SKILL.md") === "mine\n", "and pi's bridge (the user's own things kept)")
            win.check(win.findName("orchSkillsAll", d) === null && win.findName("orchBridge-pi", d) === null, "and the page shows only the switch")
            win.shotWindow("clicks-orchestration-off")
            win.clickItem(win.findName("orchEnabled", d), "Orchestration's switch, on") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const h = win.selftestDir + "/home"
            const c = JSON.parse(TestInput.readFile(h + "/.pi/agent/mcp.json")).mcpServers
            win.check(TestInput.readFile(h + "/.claude/skills/harlo/SKILL.md") !== "" && TestInput.readFile(h + "/.agents/skills/harlo-committee/SKILL.md") !== "" && TestInput.readFile(h + "/.claude/skills/harlo-advisor/SKILL.md") === "", "on again: the same skills back")
            win.check(c.harlo && c.harlo.args.indexOf("--mcp-bridge") >= 0, "and pi's bridge")
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
    // details, and its switch, which decides what New session offers. Only
    // a stand-in claude is installed (--fake-claude=); the others point at
    // nothing, so no real agent starts.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksproviders"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            for (const id of ["opencode", "pi", "omp"]) Workspace.setSetting(id + "_path", win.selftestDir + "/missing-" + id)
            settingsDialog.openPage("providers") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem, st = id => { const l = win.findName("providerStatus-" + id, d); return l ? l.text : "(none)" }
            win.check(st("claude") === "Available · 17 models", "Claude: available, 17 models (" + st("claude") + ")")
            win.check(st("opencode") === "Not installed" && st("pi") === "Not installed", "the others: not installed (" + st("opencode") + ")")
            win.clickItem(win.findName("provider-claude", d), "the Claude row") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const d = settingsDialog.contentItem
            win.check(win.findText("Version 2.1.296", d) !== null && win.findText("Opus 4.8", d) !== null, "its details: version and models")
            win.shotWindow("clicks-providers")
            win.clickItem(win.findName("providerSwitch-pi", d), "Pi's switch") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            const ids = JSON.parse(Workspace.chatProvidersJson()).map(p => p.id)
            const st = win.findName("providerStatus-pi", settingsDialog.contentItem)
            win.check(st && st.text === "Disabled" && ids.indexOf("pi") < 0 && ids.indexOf("omp") >= 0, "turned off: Disabled, and not offered (" + ids + ")")
            win.check(ids.indexOf("claude") >= 0 && ids.indexOf("opencode") >= 0, "Claude and OpenCode are offered (" + ids + ")")
            settingsDialog.close(); win.openDraft(-1) } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: {
            // No default set: it shows (and starts) the first one on and
            // installed, Claude here, as harlo does.
            win.check(chat.draftView.effectiveHarness === "claude" && Workspace.defaultAgent() === "claude", "with no default it shows the agent harlo starts (" + chat.draftView.effectiveHarness + ")")
            win.clickItem(win.findName("draftHarness", chat.draftView), "the provider chip") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: {
            win.check(win.findText("Oh My Pi") !== null && win.findText("Pi") === null, "New session offers what's on")
            TestInput.key(Qt.Key_Escape, 0)
            Workspace.setProviderEnabled("pi", true)
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
            for (const id of ["opencode", "pi", "omp"]) Workspace.setSetting(id + "_path", win.selftestDir + "/missing-" + id)
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
    // the agents (a fake Claude; the others missing: their Install buttons,
    // never pressed), the default agent and its model (then the new-session
    // page starts with it), git (the services' cards, the default, a
    // server; no sign-in), the theme; Finish marks it done; Settings opens
    // it again.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clickssetup"
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            for (const id of ["opencode", "pi", "omp"]) Workspace.setSetting(id + "_path", win.selftestDir + "/missing-" + id)
            Workspace.refreshProviders() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(Workspace.setting("setup_done") === "", "not set up yet")
            win.maybeShowSetup() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            win.check(setupGuide.visible && setupGuide.step === 0, "a first start shows the guide")
            win.check(["claude", "opencode", "pi", "omp"].every(id => win.findName("setupAgent-" + id) !== null), "a card for each agent")
            const install = win.findName("setupInstall-pi")
            win.check(install !== null && install.visible && win.findName("setupInstall-claude") === null || !win.findName("setupInstall-claude").visible, "Install for a missing agent, not for an installed one")
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
            win.check(i >= 0 && chat.transcript.latestUndo === key && latest !== null && latest.visible && latest.text === "↶ Undo these changes (1 file)",
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

    // Accounts (Settings → Providers): signing the agents in from harlo.
    // Claude and omp against stand-ins (scripts/fake-signin.py: their
    // links and prompts, the test's code and key only); OpenCode and pi,
    // when installed, the real ones in the scratch home with a made-up
    // DeepSeek key (stored there, never used), signed out again.
    SequentialAnimation {
        id: accountsTest
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "clicksaccounts"
        property string opencode: win.argValue("--real-opencode=")
        property string pi: win.argValue("--real-pi=")
        PauseAnimation { duration: 900 }
        ScriptAction { script: {
            if (win.argValue("--winwidth=")) win.width = Number(win.argValue("--winwidth="))
            Workspace.setSetting("claude_path", win.argValue("--fake-claude="))
            Workspace.setSetting("omp_path", win.argValue("--fake-omp="))
            Workspace.setSetting("opencode_path", accountsTest.opencode || win.selftestDir + "/missing-opencode")
            Workspace.setSetting("pi_path", accountsTest.pi || win.selftestDir + "/missing-pi")
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
            win.clickItem(win.findName("provider-omp"), "the omp row") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.clickItem(win.findName("signIn-omp"), "omp's Sign in…") }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const p = JSON.parse(Accounts.providers("omp"))
            win.check(p.list.length === 3, "omp's providers, from its list (" + p.list.map(x => x.name) + ")")
            const claudeSub = win.findName("signInMethod-1-login")
            win.check(claudeSub !== null && !claudeSub.enabled && win.findText(p.list[0].note) !== null, "a Claude subscription isn't offered in omp, with why")
            win.clickItem(win.findName("signInMethod-2-login"), "DeepSeek") } }
        PauseAnimation { duration: 2000 }
        ScriptAction { script: {
            const f = signInDialog.flow
            win.check(f && f.steps.some(s => s.kind === "link" && s.url === "https://platform.deepseek.com/api_keys") && !f.steps.some(s => (s.text || "").indexOf("OpenRouter") >= 0),
                      "omp's link to get a key (its list not repeated)")
            win.check(f && f.ask && f.ask.kind === "secret", "and its question for the key, hidden as typed (" + JSON.stringify(f ? f.ask : null) + ")")
            win.clickItem(win.findName("signInAnswer"), "the key box")
            TestInput.typeText("sk-good")
            win.clickItem(win.findName("signInSend"), "Continue") } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: {
            const f = signInDialog.flow
            win.check(f && f.state === "done", "omp signed in (" + (f ? f.state + " " + f.message : "") + ")")
            win.clickItem(win.findName("signInClose"), "Done") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") { win.check(true, "OpenCode (not installed here; skipped)"); return }
            win.clickItem(win.findName("provider-opencode"), "the OpenCode row") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") return
            win.clickItem(win.findName("signIn-opencode"), "OpenCode's Sign in…") } }
        PauseAnimation { duration: 800 }
        // (The real OpenCode and pi start up: their time varies with the machine's load.)
        ScriptAction { script: win.waitUntil(accountsTest, () => accountsTest.opencode === "" || JSON.parse(Accounts.providers("opencode")).list.length > 20, 40000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") return
            const p = JSON.parse(Accounts.providers("opencode"))
            win.check(p.list.length > 20 && p.list.some(x => x.id === "deepseek"), "OpenCode's providers (" + p.list.length + (p.error ? ", " + p.error : "") + ")")
            const search = win.findName("signInSearch")
            win.clickItem(search, "the search")
            TestInput.typeText("deepseek") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: if (accountsTest.opencode !== "") win.clickItem(win.findName("signInMethod-deepseek-key"), "DeepSeek's Add key") }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") return
            win.clickItem(win.findName("signInKey"), "the key field")
            TestInput.typeText("sk-made-up-for-a-test")
            win.clickItem(win.findName("signInSave"), "Save key") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.waitUntil(accountsTest, () => accountsTest.opencode === "" || signInDialog.flow && ["done", "failed"].indexOf(signInDialog.flow.state) >= 0
                                                                   && JSON.parse(Accounts.status("opencode")).state !== "loading", 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") return
            const f = signInDialog.flow, st = JSON.parse(Accounts.status("opencode"))
            const ds = st.accounts.find(a => a.label === "DeepSeek")
            win.check(f && f.state === "done" && ds && ds.detail === "API key", "OpenCode keeps the key: DeepSeek · API key (" + (f ? f.state + " " + f.message : "") + ", " + JSON.stringify(st.accounts) + ")")
            win.clickItem(win.findName("signInClose"), "Done") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: {
            if (accountsTest.opencode === "") return
            const st = JSON.parse(Accounts.status("opencode")), ds = st.accounts.find(a => a.label === "DeepSeek")
            win.clickItem(ds ? win.findName("signOut-opencode-" + ds.id) : null, "DeepSeek's Sign out") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.waitUntil(accountsTest, () => {
            const st = JSON.parse(Accounts.status("opencode"))
            return accountsTest.opencode === "" || st.state !== "loading" && !st.accounts.some(a => a.label === "DeepSeek")
        }, 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.opencode !== "") win.check(!JSON.parse(Accounts.status("opencode")).accounts.some(a => a.label === "DeepSeek"), "signed out of DeepSeek in OpenCode")
            if (accountsTest.pi === "") { win.check(true, "pi (not installed here; skipped)"); return }
            win.clickItem(win.findName("provider-pi"), "the pi row") } }
        PauseAnimation { duration: 800 }
        ScriptAction { script: if (accountsTest.pi !== "") win.clickItem(win.findName("signIn-pi"), "pi's Sign in…") }
        PauseAnimation { duration: 800 }
        ScriptAction { script: win.waitUntil(accountsTest, () => accountsTest.pi === "" || JSON.parse(Accounts.providers("pi")).list.some(x => x.id === "deepseek"), 40000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.pi === "") return
            const p = JSON.parse(Accounts.providers("pi"))
            const anthropic = p.list.find(x => x.id === "anthropic")
            win.check(p.list.some(x => x.id === "deepseek") && anthropic && anthropic.methods.find(m => m.id === "oauth").blocked === true && !anthropic.methods.find(m => m.id === "api_key").blocked,
                      "pi's providers: Anthropic by API key only (" + p.list.length + (p.error ? ", " + p.error : "") + ")")
            win.clickItem(win.findName("signInSearch"), "the search")
            TestInput.typeText("deepseek") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: if (accountsTest.pi !== "") win.clickItem(win.findName("signInMethod-deepseek-api_key"), "DeepSeek (pi)") }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.waitUntil(accountsTest, () => accountsTest.pi === "" || signInDialog.flow && (!!signInDialog.flow.ask || signInDialog.flow.state === "failed"), 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.pi === "") return
            const f = signInDialog.flow
            win.check(f && f.ask && f.ask.kind === "secret", "pi asks for the key (" + JSON.stringify(f ? f.ask : null) + ")")
            win.clickItem(win.findName("signInAnswer"), "the key box")
            TestInput.typeText("sk-made-up-for-a-test")
            win.clickItem(win.findName("signInSend"), "Continue") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.waitUntil(accountsTest, () => accountsTest.pi === "" || signInDialog.flow && ["done", "failed"].indexOf(signInDialog.flow.state) >= 0
                                                                   && JSON.parse(Accounts.status("pi")).state !== "loading", 30000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.pi === "") return
            const f = signInDialog.flow, st = JSON.parse(Accounts.status("pi"))
            win.check(f && f.state === "done" && st.accounts.some(a => a.id === "deepseek" && a.signOut), "pi keeps the key: DeepSeek (" + (f ? f.state + " " + f.message : "") + ", " + JSON.stringify(st.accounts) + ")")
            win.clickItem(win.findName("signInClose"), "Done") } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: if (accountsTest.pi !== "") win.clickItem(win.findName("signOut-pi-deepseek"), "pi's DeepSeek Sign out") }
        PauseAnimation { duration: 1000 }
        // (pi's sign-out starts Node: slow on a busy machine.)
        ScriptAction { script: win.waitUntil(accountsTest, () => {
            const st = JSON.parse(Accounts.status("pi"))
            return accountsTest.pi === "" || st.state !== "loading" && !st.accounts.some(a => a.id === "deepseek")
        }, 40000) }
        PauseAnimation { duration: 100 }
        ScriptAction { script: {
            if (accountsTest.pi !== "") win.check(!JSON.parse(Accounts.status("pi")).accounts.some(a => a.id === "deepseek"), "signed out of DeepSeek in pi")
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
    // Runs of tool calls fold under a summary line (a saved session that
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
            win.check(win.findText("Read 1 file, ran 1 command", t) !== null, "a run's summary line")
            const run = win.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t)
            win.check(run !== null, "the second run's summary")
            win.check(win.findText("npm test -- auth", t) === null, "a finished run starts folded")
            win.shotWindow("clicks-runs-folded")
            win.clickItem(run, "the summary") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            const t = chat.transcript
            win.check(win.findText("npm test -- auth", t) !== null && win.findText("test/return-to.test.ts", t) !== null, "clicking it opens the run")
            win.check(win.findText("src/auth/guard.ts", t) !== null && win.findText("grep -rn returnTo src", t) === null, "only that run")
            win.shotWindow("clicks-runs-open")
            win.clickItem(win.findText("Thought, edited 1 file, wrote 1 file, ran 1 command", t), "the summary again") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: {
            win.check(win.findText("npm test -- auth", chat.transcript) === null, "and again folds it")
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
    // Real omp, no prompts (not in the click suite): approval and plan
    // modes rewrite the session's overlay while it runs.
    property string modesKey: ""
    property bool otherAddress: false
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "ompmodes"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: win.modesKey = Workspace.newSession(-1, "omp") }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: { win.log("start mode:", Workspace.approvalMode(win.modesKey), "status:", chat.session.status); Workspace.setApprovalMode(win.modesKey, "always-ask"); Workspace.setPlanMode(win.modesKey, true); win.log("modes set") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: { win.log("status:", chat.session.status); Workspace.setPlanMode(win.modesKey, false); win.log("plan off") } }
        PauseAnimation { duration: 5000 }
        ScriptAction { script: { win.log("status:", chat.session.status); Qt.quit() } }
    }
    // The real thing, read-only (not in the click suite): omp usage via harlo.
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

    // A goal straight from the new-session page (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "draftgoal"
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.openDraft(-1); chat.draftView.harness = "demo" } }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { const d = chat.draftView; d.text = "Make the parser handle nested lists"; d.asGoal = true; d.goalBudget = "abc"; win.log("bad budget canSend:", d.canSend); d.goalBudget = "100"; win.shot("draft-goal-tiny") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: { const d = chat.draftView; d.goalBudget = "50k"; win.log("goalOn:", d.goalOn, "canSend:", d.canSend) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: win.shot("draft-goal") }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { chat.draftView.parallel = true; win.log("parallel turns goal off:", chat.draftView.asGoal); chat.draftView.parallel = false; chat.draftView.asGoal = true; chat.draftView.send() } }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { const g = chat.session.goalJson !== "" ? JSON.parse(chat.session.goalJson) : null; win.log("session goal:", g ? g.objective + " | " + g.status + " | budget " + g.tokenBudget : "none", "rows:", chat.transcript.count); win.shot("draft-goal-started") } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: Qt.quit() }
    }

    // omp goal mode (demo harness): start from the composer, pause/resume,
    // complete; then a goal that runs into its token budget, and drop.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "goal"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.composerItem.text = "Make the parser handle nested lists, with tests"; chat.composerItem.goalPopup.open() } }
        PauseAnimation { duration: 500 }
        ScriptAction { script: win.shotPopup(chat.composerItem.goalPopup, "goal-popover") }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { chat.composerItem.goalPopup.start(); win.log("composer after start:", JSON.stringify(chat.composerItem.text)) } }
        PauseAnimation { duration: 1300 }
        ScriptAction { script: { const g = JSON.parse(chat.session.goalJson); win.log("goal:", g.status, g.tokensUsed, "dock visible:", chat.goalDock.visible); win.shot("goal-active") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.goalCommand("pause", "", 0) }
        PauseAnimation { duration: 900 }
        ScriptAction { script: { const g = JSON.parse(chat.session.goalJson); win.log("paused:", g.status, g.tokensUsed); win.shot("goal-paused") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.goalCommand("resume", "", 0) }
        PauseAnimation { duration: 4000 }
        ScriptAction { script: { const g = JSON.parse(chat.session.goalJson); win.log("finished:", g.status, g.tokensUsed, g.timeUsedSeconds); win.shot("goal-complete") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: { chat.goalDock.dismissed = JSON.parse(chat.session.goalJson).id; win.log("dismissed, dock visible:", chat.goalDock.visible); chat.session.goalCommand("create", "", 0) } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: { win.log("empty objective error:", chat.session.goalError); chat.session.goalCommand("create", "Refactor the store module", 9000) } }
        PauseAnimation { duration: 1800 }
        ScriptAction { script: { const g = JSON.parse(chat.session.goalJson); win.log("budget goal:", g.status, g.tokensUsed + "/" + g.tokenBudget, "error:", JSON.stringify(chat.session.goalError)); win.shot("goal-budget") } }
        PauseAnimation { duration: 300 }
        ScriptAction { script: chat.session.goalCommand("drop", "", 0) }
        PauseAnimation { duration: 600 }
        ScriptAction { script: { win.log("after drop: goalJson", JSON.stringify(chat.session.goalJson), "dock visible:", chat.goalDock.visible) } }
        ScriptAction { script: Qt.quit() }
    }

    // omp tool approvals on their tool row (demo harness), approve then deny.
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
                const id = Workspace.startRun(JSON.stringify({ projectId: 1, prompt: "Add a cache to the parser", starter: "", attachments: [], askFirst: false,
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
        ScriptAction { script: { win.log("branches:", chat.branchCount, "rows:", chat.transcript.count); win.shot("branches-header"); chat.branchDialog.show(win.argValue("--open="), "omp", "/tmp") } }
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

    // omp todos, queue editing and side questions (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "ompextras"
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
        ScriptAction { script: { win.log("after remove:", chat.session.queued.join(" | ")); chat.composerItem.openBtw(); chat.session.askBtw("why does the cache need a lock?", "") } }
        PauseAnimation { duration: 1200 }
        ScriptAction { script: { win.log("btw:", chat.session.btwJson.length > 0 ? JSON.parse(chat.session.btwJson).map(r => r.status + ": " + r.answer).join(" / ") : "-"); win.shotPopup(chat.composerItem.btwPopup, "btw-popover") } }
        PauseAnimation { duration: 400 }
        ScriptAction { script: Qt.quit() }
    }

    // omp subagents (demo harness): dock, transcript dialog, steer, cancel.
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

    // Commit message written by a (stand-in) harness: --fake-omp=<script>.
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "commitmsg"
        PauseAnimation { duration: 1500 }
        ScriptAction {
            script: {
                Workspace.setSetting("omp_path", win.argValue("--fake-omp="))
                Workspace.addProject(win.argValue("--cwd="))
                Workspace.newSession(1, "demo")
                windowSettings.rightMode = "changes"
            }
        }
        PauseAnimation { duration: 1500 }
        ScriptAction { script: { win.log("changes:", git.unstagedCount, "harness:", changes.harness); git.generateMessage("omp", Workspace.setting("omp_path"), "", false); win.log("generating:", git.generating) } }
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
        ScriptAction { script: { const d = chat.draftView; win.log("project:", d.project ? d.project.name : "-", "branch:", d.currentBranch, "default:", d.onDefaultBranch, "tab:", Workspace.activeKey); win.shot("draft-project") } }
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

    // omp `ask` questions and a `select` with descriptions (demo harness).
    SequentialAnimation {
        running: win.selftestDir !== "" && win.argValue("--scenario=") === "ask"
        PauseAnimation { duration: 1500 }
        ScriptAction { script: Workspace.newSession(-1, "demo") }
        PauseAnimation { duration: 1000 }
        ScriptAction { script: { chat.session.setAskFirstForSession(true); win.log("ask first:", chat.session.askFirst); chat.submit("please ask me something", "") } }
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
        ScriptAction { script: { sidebar.sessionMenu.close(); sidebar.projectMenu.harness = "omp"; sidebar.projectMenu.popup(sidebar, 120, 110) } }
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
        ScriptAction { script: { sidebar.trashDialog.close(); sidebar.openNewWorktree(-1, "houdini-mcp", "omp") } }
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
}
