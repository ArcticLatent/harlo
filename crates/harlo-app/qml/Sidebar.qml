import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Shapes
import QtQuick.Layouts
import QtQuick.Dialogs
import Qt.labs.qmlmodels
import harlo

// Projects and their sessions, plus sessions outside every project.
Rectangle {
    id: root
    color: Theme.chrome
    // (Nothing spills under the chat when it's narrow.)
    clip: true

    // Ticks so relative times ("5m") stay current.
    property double now: Date.now()
    Timer { interval: 30000; running: true; repeat: true; onTriggered: root.now = Date.now() }

    FolderDialog {
        id: folderDialog
        title: "Add project folder"
        onAccepted: addProjectDialog.setFolder(selectedFolder.toString())
    }
    // A folder of the instance shown: this computer's, or the host's.
    RemoteFolderDialog { id: remoteFolderDialog; onPicked: path => addProjectDialog.setFolder(path) }
    // Its folder, agent and model; then the new-session page for it.
    AddProjectDialog {
        id: addProjectDialog
        onChooseFolder: {
            if (Workspace.instance === "") folderDialog.open()
            else remoteFolderDialog.openFor(Workspace.instance)
        }
        onAdded: id => root.newSessionRequested(id)
    }
    signal manageInstancesRequested()
    /// The filter's words, to search the messages for (Ctrl+Shift+F).
    signal searchMessagesRequested(string text)

    AppDialog {
        id: renameDialog
        property string key: ""
        title: "Rename session"
        modal: true
        anchors.centerIn: Overlay.overlay
        footer: AppButtonBox {
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            DialogButton { text: "Rename"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        }
        AppTextField { id: renameField; width: 340; onAccepted: renameDialog.accept() }
        onAccepted: if (renameField.text.trim() !== "") Workspace.renameSession(key, renameField.text.trim())
        function ask(key, current) { renameDialog.key = key; renameField.text = current; open(); renameField.selectAll(); renameField.forceActiveFocus() }
    }

    AppDialog {
        id: trashDialog
        property string key: ""
        property string label: ""
        // What happens to it: its file goes to the trash.
        property string harness: ""
        // (One name for it everywhere; what it does here, said below.)
        title: "Delete session?"
        modal: true
        anchors.centerIn: Overlay.overlay
        footer: AppButtonBox {
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            DialogButton { text: "Delete"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole; accent: false; destructive: true }
        }
        Label {
            text: trashDialog.label + "\n\nIts file goes to the trash, so you can still restore it from there."
            width: 360; wrapMode: Text.Wrap; color: Theme.muted
        }
        onAccepted: Workspace.trashSession(key)
    }

    RemoveProjectDialog { id: removeProjectDialog }
    WorktreeDialogs { id: worktreeDialogs }
    // For the command palette.
    /// Settings → Projects, on this project.
    signal projectSettingsRequested(int projectId)
    /// Open the new-session page for a project (-1: home folder).
    signal newSessionRequested(int projectId)
    /// Export a session as Markdown (the window asks where).
    signal exportRequested(string key, string title)
    // Test hooks.
    property alias sessionMenu: sessionMenu
    property alias projectMenu: projectMenu
    property alias renameDialog: renameDialog
    property alias trashDialog: trashDialog
    property alias worktreeDialogs: worktreeDialogs
    function openAddProject() { addProjectDialog.open() }
    // Test hook.
    property alias remoteFolderDialog: remoteFolderDialog
    property alias addProjectButtonItem: addProjectButton
    function openNewWorktree(projectId, name, harness) { worktreeDialogs.openNew(projectId, name, harness) }
    function openWorktree(key) { worktreeDialogs.openFinish(key) }

    AppMenu {
        id: projectMenu
        property int projectId: -1
        property string path: ""
        property string harness: ""
        property string name: ""
        // "first", "last", "only" or "": which moves it has.
        property string place: ""
        // The providers on (Settings → Providers).
        Repeater {
            // (A remote project: the agents its host has.)
            model: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).filter(p => !projectMenu.path.startsWith("ssh://") || Theme.offeredOnHost(projectMenu.path.substring(6).split("/")[0], p.id))
            AppMenuItem {
                required property var modelData
                text: "New " + modelData.label + " session"
                provider: modelData.id
                onTriggered: Workspace.newSession(projectMenu.projectId, modelData.id)
            }
        }
        AppMenuItem { text: "New worktree session…"; onTriggered: worktreeDialogs.openNew(projectMenu.projectId, projectMenu.name, projectMenu.harness) }
        AppMenuItem { text: "Worktree setup commands…"; onTriggered: worktreeDialogs.openSetup(projectMenu.projectId, projectMenu.name) }
        AppMenuSeparator { }
        Repeater {
            // (A remote project: the agents its host has.)
            model: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).filter(p => !projectMenu.path.startsWith("ssh://") || Theme.offeredOnHost(projectMenu.path.substring(6).split("/")[0], p.id))
            AppMenuItem {
                required property var modelData
                text: "Default: " + modelData.label
                provider: modelData.id
                marked: projectMenu.harness === modelData.id
                onTriggered: Workspace.setProjectHarness(projectMenu.projectId, modelData.id)
            }
        }
        AppMenuSeparator { }
        // (Or drag the project's header.)
        AppMenuItem { objectName: "projectMoveUp"; text: "Move up"; enabled: projectMenu.place !== "first" && projectMenu.place !== "only"; onTriggered: Workspace.moveProjectStep(projectMenu.projectId, -1) }
        AppMenuItem { objectName: "projectMoveDown"; text: "Move down"; enabled: projectMenu.place !== "last" && projectMenu.place !== "only"; onTriggered: Workspace.moveProjectStep(projectMenu.projectId, 1) }
        AppMenuSeparator { }
        AppMenuItem { text: "Project settings…"; onTriggered: root.projectSettingsRequested(projectMenu.projectId) }
        AppMenuItem { text: "Open folder"; visible: !projectMenu.path.startsWith("ssh://"); onTriggered: Qt.openUrlExternally("file://" + projectMenu.path) }
        AppMenuItem { text: "Remove project…"; danger: true; onTriggered: removeProjectDialog.ask(projectMenu.projectId, projectMenu.name) }
    }

    // A Settled shelf's.
    AppMenu {
        id: shelfMenu
        property int projectId: -1
        property int sessions: 0
        AppMenuItem { objectName: "shelfArchiveAll"; text: "Archive all " + shelfMenu.sessions; onTriggered: Workspace.archiveShelf(shelfMenu.projectId) }
    }

    AppMenu {
        id: sessionMenu
        property string key: ""
        property string sessionTitle: ""
        property string harness: ""
        property bool pinned: false
        property bool live: false
        property string branch: ""
        // On its project's Settled shelf.
        property bool settled: false
        // Repository of the session's folder ("" outside one).
        property string repo: ""
        property bool repoIgnored: false
        AppMenuItem { text: sessionMenu.pinned ? "Unpin" : "Pin"; onTriggered: Workspace.setPinned(sessionMenu.key, !sessionMenu.pinned) }
        AppMenuItem { text: "Rename…"; onTriggered: renameDialog.ask(sessionMenu.key, sessionMenu.sessionTitle) }
        AppMenuItem { objectName: "sessionPutBack"; text: "Put back"; visible: sessionMenu.settled; onTriggered: Workspace.putBack(sessionMenu.key) }
        AppMenuItem { text: "Archive"; onTriggered: Workspace.setArchived(sessionMenu.key, true) }
        AppMenuItem { objectName: "sessionExport"; text: "Export as Markdown…"; onTriggered: root.exportRequested(sessionMenu.key, sessionMenu.sessionTitle) }
        AppMenuItem { objectName: "sessionCopyMarkdown"; text: "Copy as Markdown"; onTriggered: Workspace.copyMarkdown(sessionMenu.key) }
        AppMenuItem { text: "Stop process"; enabled: sessionMenu.live; onTriggered: Workspace.closeSession(sessionMenu.key) }
        AppMenuItem {
            text: "Worktree " + sessionMenu.branch + "…"
            visible: sessionMenu.branch !== ""
            onTriggered: worktreeDialogs.openFinish(sessionMenu.key)
        }
        AppMenuItem {
            visible: sessionMenu.repo !== ""
            text: (sessionMenu.repoIgnored ? "Show change count for " : "Hide change count for ") + sessionMenu.repo.split("/").pop()
            onTriggered: Workspace.setRepoIgnored(sessionMenu.repo, !sessionMenu.repoIgnored)
        }
        AppMenuSeparator { }
        AppMenuItem {
            text: "Delete session…"
            danger: true
            onTriggered: { trashDialog.key = sessionMenu.key; trashDialog.label = sessionMenu.sessionTitle; trashDialog.harness = sessionMenu.harness; trashDialog.open() }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            Layout.bottomMargin: 16
            // The logo (its own colours; the wordmark in the theme's text).
            LogoMark { objectName: "sidebarLogo"; Layout.leftMargin: 2; implicitHeight: Math.round(Theme.px(22)) }
            Item { Layout.fillWidth: true }
            FlatButton {
                id: addProjectButton
                objectName: "addProjectButton"
                // (A narrow sidebar: the sign alone.)
                text: root.width < 240 ? "+" : "+ Project"
                font.pixelSize: Theme.px(12)
                onClicked: root.openAddProject()
                ToolTip.visible: hovered
                ToolTip.text: Workspace.instance === "" ? "Add a folder on this computer" : "Add a folder on " + Workspace.instance
            }
        }

        // Local, or a host: the machine this window shows.
        InstanceSwitcher {
            id: instanceSwitcher
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.bottomMargin: 10
            onManageRequested: root.manageInstancesRequested()
        }

        // New session: opens the new-session page (Ctrl+N).
        Button {
            id: newSessionButton
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.bottomMargin: 10
            implicitHeight: 38
            flat: true
            hoverEnabled: true
            readonly property bool current: Workspace.activeKey === "view:new"
            // -2: the project of the session last worked in.
            onClicked: root.newSessionRequested(-2)
            leftPadding: 12
            rightPadding: 12
            contentItem: RowLayout {
                spacing: 10
                Icon { name: "chat-new"; color: Theme.accent; implicitWidth: 17; implicitHeight: 17 }
                Label { Layout.fillWidth: true; text: "New session"; color: Theme.text; font.pixelSize: Theme.px(14); font.weight: Font.Medium }
                Label { visible: root.width >= 220; text: Keymap.seq("newSession"); color: Theme.textFaint; font.pixelSize: Theme.px(12) }
            }
            background: Rectangle {
                radius: 10
                color: newSessionButton.current ? Theme.rowSelected : newSessionButton.down ? Theme.rowSelected : newSessionButton.hovered ? Theme.rowHover : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
            }
        }

        TextField {
            id: search
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.bottomMargin: 10
            implicitHeight: 38
            leftPadding: 36
            objectName: "sessionFilter"
            // (Titles only; the messages are searched with Ctrl+Shift+F.)
            placeholderText: "Filter sessions"
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.px(14)
            background: Rectangle {
                radius: 10
                color: Theme.well
                border.color: search.activeFocus ? Theme.accent : "transparent"
                Icon { name: "search"; color: Theme.textFaint; implicitWidth: 16; implicitHeight: 16; x: 12; anchors.verticalCenter: parent.verticalCenter }
            }
            onTextChanged: Workspace.setFilter(text)
            Keys.onEscapePressed: text = ""
        }
        // Filtering: the same words in the messages, a click away.
        ItemDelegate {
            id: searchMessages
            objectName: "filterSearchMessages"
            visible: search.text.trim() !== ""
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.bottomMargin: 6
            implicitHeight: 30
            leftPadding: 12
            rightPadding: 10
            contentItem: RowLayout {
                spacing: 8
                Icon { name: "search-eye"; color: Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                Label {
                    Layout.fillWidth: true
                    text: "Find “" + search.text.trim() + "” in messages"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12.5)
                    elide: Text.ElideRight
                }
                Label { visible: root.width >= 240; text: Keymap.seq("searchAll"); color: Theme.textFaint; font.pixelSize: Theme.px(11.5) }
            }
            background: Rectangle { radius: 8; color: searchMessages.hovered ? Theme.rowHover : "transparent" }
            onClicked: root.searchMessagesRequested(search.text.trim())
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: Workspace
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }

            // Rows inserted above the visible ones (a new project, a new
            // session) would otherwise land out of view: stay at the top
            // unless the user has scrolled down.
            property bool pinnedTop: true
            onMovementEnded: pinnedTop = atYBeginning
            onCountChanged: if (pinnedTop) Qt.callLater(positionViewAtBeginning)

            // A project dragged by its header to another place: its id, the
            // project it goes before (-1: after the last), the line where
            // it lands, and the pointer (in the window). While dragging the
            // rows stay made, so the dragged one isn't dropped scrolling.
            property int dragId: -1
            property int dropBefore: -1
            property real dropY: -1
            property real dragSceneY: 0
            // What the lifted copy shows, and where on the header it was
            // taken (so it doesn't jump under the pointer).
            property string dragTitle: ""
            property string dragIcon: ""
            property string dragColor: ""
            property int dragCount: 0
            property real dragGrab: 0
            // The lifted copy's top, from the pointer: where it was taken
            // at first, then sliding to just below the pointer (so the line
            // and where it lands stay in sight).
            property real ghostShift: 0
            // ...and never over the name of the header it lands above: no
            // higher than just under it (sliding there as the line moves on).
            property real dropGap: 0
            readonly property real landFloor: dropY >= 0 ? dropY - contentY + dropGap / 2 + 26 : -1e6
            property real ghostFloor: landFloor
            Behavior on ghostFloor { enabled: list.dragId >= 0; NumberAnimation { duration: Math.round(Theme.motion * 0.4); easing.type: Easing.OutCubic } }
            NumberAnimation { id: ghostSlide; target: list; property: "ghostShift"; to: 10; duration: Math.round(Theme.motion * 0.6); easing.type: Easing.OutCubic }
            // Esc calls a drag off.
            Shortcut {
                sequence: "Esc"
                enabled: list.dragId >= 0
                onActivated: list.endDrag(false)
            }
            readonly property real dragY: dragId >= 0 ? mapFromItem(null, 0, dragSceneY).y : 0
            cacheBuffer: dragId >= 0 ? 100000 : 320
            // (A drag moves the project, not the list; the rows hold still
            // meanwhile, see hold_rows.)
            interactive: dragId < 0
            onDragIdChanged: Workspace.holdRows(dragId >= 0)
            // The pointer, watched by the list too: the drag follows it and
            // ends when it's let go, even if the header it started on is
            // gone.
            PointHandler {
                acceptedButtons: Qt.LeftButton
                onTranslationChanged: if (list.dragId >= 0) { list.dragSceneY = point.scenePosition.y; list.updateDrop() }
                onActiveChanged: if (!active && list.dragId >= 0) list.endDrag(true)
            }
            // Where it goes: past every project whose header the pointer
            // has crossed (a header counts once its middle is above the
            // pointer), so it lands before the first one still below it,
            // or after the last. Its line: above that header (in the gap's
            // middle), or at the end of the projects. Nothing shows where
            // it wouldn't move.
            function updateDrop() {
                const y = contentY + dragY
                const heads = []
                for (const it of contentItem.children)
                    if (it.visible && (it.kind === "project" || it.kind === "group"))
                        heads.push(it)
                heads.sort((a, b) => a.y - b.y)
                let before = -1, at = originY + contentHeight - 4, next = -1, seen = false
                dropGap = 0
                for (const it of heads) {
                    if (seen && next === -1)
                        next = it.kind === "project" ? it.projectId : -2
                    if (it.kind === "project" && it.projectId === dragId) {
                        seen = true
                        continue
                    }
                    if (it.y + it.gap + 15 > y) {
                        before = it.kind === "project" ? it.projectId : -1
                        at = it.y + it.gap / 2
                        dropGap = it.gap
                        break
                    }
                }
                // (Where it is now: before the next project, or last.)
                const stays = before === next || (before === -1 && (next === -1 || next === -2))
                dropBefore = before
                dropY = stays ? -1 : at
            }
            function startDrag() {
                ghostShift = -dragGrab
                ghostSlide.restart()
            }
            function endDrag(drop) {
                ghostSlide.stop()
                if (drop && dragId >= 0 && dropY >= 0)
                    Workspace.moveProject(dragId, dropBefore)
                dragId = -1
                dropY = -1
            }
            // Where it lands: a line with a ring at its start.
            Item {
                objectName: "projectDropLine"
                parent: list
                visible: list.dragId >= 0 && list.dropY >= 0
                // (Over the lifted copy.)
                z: 30
                x: 14
                // (Kept inside the list: the first project's place is at
                // its very top.)
                y: Math.max(0, Math.min(list.height - height, list.dropY - list.contentY - 4))
                width: list.width - 28
                height: 8
                Rectangle { x: 6; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 6; height: 2; radius: 1; color: Theme.accent }
                Rectangle { width: 8; height: 8; radius: 4; color: Theme.sidebar; border.width: 2; border.color: Theme.accent }
            }
            // The project lifted: a copy of its header following the pointer.
            Rectangle {
                objectName: "projectDragGhost"
                parent: list
                visible: list.dragId >= 0
                z: 20
                x: 18
                y: Math.max(0, Math.min(list.height - height, Math.max(list.dragY + list.ghostShift, list.ghostFloor)))
                width: list.width - 36
                height: 30
                radius: 8
                rotation: -1
                antialiasing: true
                color: Theme.raised
                border.color: Theme.controlBorder
                RectangularShadow {
                    z: -1
                    anchors.fill: parent
                    radius: parent.radius
                    offset.y: 6
                    blur: 18
                    spread: -6
                    color: Qt.rgba(0, 0, 0, Theme.isLight ? 0.16 : 0.45)
                }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 12
                    spacing: 8
                    ProjectIcon { Layout.preferredWidth: 13; Layout.preferredHeight: 13; icon: list.dragIcon; color: list.dragColor }
                    Label { Layout.fillWidth: true; text: list.dragTitle; color: Theme.text; font.pixelSize: Theme.px(13); font.weight: Font.DemiBold; elide: Text.ElideRight }
                    Label { text: list.dragCount; color: Theme.textFaint; font.pixelSize: Theme.px(12) }
                }
            }
            // Near the top or bottom edge, the list scrolls on.
            Timer {
                interval: 16
                repeat: true
                running: list.dragId >= 0 && (list.dragY < 28 || list.dragY > list.height - 28)
                onTriggered: {
                    const step = list.dragY < 28 ? -8 : 8
                    list.contentY = Math.max(list.originY, Math.min(list.contentY + step, list.originY + list.contentHeight - list.height))
                    list.updateDrop()
                }
            }

            delegate: DelegateChooser {
                role: "kind"

                DelegateChoice {
                    roleValue: "project"
                    GroupRow { }
                }
                DelegateChoice {
                    roleValue: "group"
                    GroupRow { }
                }
                DelegateChoice {
                    roleValue: "more"
                    ItemDelegate {
                        id: moreRow
                        required property string title
                        required property int projectId
                        width: ListView.view.width
                        height: 30
                        leftPadding: 38
                        leftInset: 6
                        rightInset: 6
                        opacity: list.dragId >= 0 && list.dragId === projectId ? 0.4 : 1
                        contentItem: Label { text: moreRow.title; color: Theme.muted; font.pixelSize: Theme.px(12); verticalAlignment: Text.AlignVCenter }
                        hoverEnabled: list.dragId < 0
                        background: Rectangle { radius: 8; color: moreRow.hovered ? Theme.hover : "transparent" }
                        onClicked: Workspace.showMore(projectId)
                    }
                }
                // A project's Settled shelf: its finished worktree sessions.
                DelegateChoice {
                    roleValue: "shelf"
                    ItemDelegate {
                        id: shelfRow
                        required property string rowKey
                        required property bool collapsed
                        required property int count
                        required property int projectId
                        objectName: "shelfRow-" + rowKey
                        opacity: list.dragId >= 0 && list.dragId === projectId ? 0.4 : 1
                        width: ListView.view.width
                        height: 30
                        leftPadding: 30
                        leftInset: 6
                        rightInset: 6
                        hoverEnabled: list.dragId < 0
                        ToolTip.visible: hovered
                        ToolTip.text: "Finished helpers, and worktree sessions that are merged,\nclosed or quiet for a week. Working in one brings it back.\nArchived after 30 days here; right-click to archive all now."
                        ToolTip.delay: 700
                        contentItem: RowLayout {
                            spacing: 6
                            Icon { name: shelfRow.collapsed ? "arrow-right-s" : "arrow-down-s"; color: Theme.textFaint; implicitWidth: 13; implicitHeight: 13 }
                            Icon { name: "archive-stack"; color: Theme.textFaint; implicitWidth: 13; implicitHeight: 13 }
                            Label { Layout.fillWidth: true; text: "Settled · " + shelfRow.count; color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight }
                        }
                        background: Rectangle { radius: 8; color: shelfRow.hovered ? Theme.hover : "transparent" }
                        onClicked: Workspace.toggleCollapsed(rowKey)
                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            onTapped: {
                                shelfMenu.projectId = shelfRow.projectId
                                shelfMenu.sessions = shelfRow.count
                                shelfMenu.popup()
                            }
                        }
                    }
                }
                DelegateChoice {
                    roleValue: "session"
                    ItemDelegate {
                        id: sessionRow
                        required property string rowKey
                        required property string title
                        required property string subtitle
                        required property string harness
                        required property string status
                        required property double modified
                        required property bool pinned
                        required property string branch
                        required property int changes
                        required property string settled
                        // Started by another session's agent: shown under it.
                        required property string helperOf
                        required property int nest
                        required property int projectId
                        readonly property bool active: rowKey === Workspace.activeKey
                        // Its project lifted to move: faded with it.
                        opacity: list.dragId >= 0 && list.dragId === projectId ? 0.4 : 1
                        // On hover: whose helper it is, the cwd (other sessions)
                        // and/or the worktree branch, and why it's settled.
                        // On the Settled shelf: faded, until you point at it.
                        readonly property bool dim: settled !== "" && !active && !hovered
                        // A narrow sidebar: the title keeps its room (no time,
                        // and a helper shows by its indent alone).
                        readonly property bool tight: width < 240
                        readonly property string detail: [helperOf !== "" ? "Helper of “" + helperOf + "”" : "", branch !== "" ? "⎇ " + branch : "", subtitle, settled].filter(t => t !== "").join("  ·  ")
                        width: ListView.view.width
                        height: 42
                        hoverEnabled: list.dragId < 0
                        ToolTip.visible: hovered && detail !== ""
                        ToolTip.text: detail
                        ToolTip.delay: 700
                        // Rounded, inset from the sidebar's edges.
                        leftInset: 10
                        rightInset: 10
                        topInset: 1
                        bottomInset: 1
                        leftPadding: (tight ? 14 : 18) + Math.min(nest, 3) * (tight ? 10 : 14)
                        rightPadding: tight ? 14 : 18
                        topPadding: 2
                        bottomPadding: 2
                        readonly property color rowColor: active ? Theme.rowSelected : hovered ? Theme.rowHover : Theme.chrome
                        background: Rectangle {
                            radius: 9
                            color: sessionRow.active || sessionRow.hovered ? sessionRow.rowColor : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        }
                        contentItem: RowLayout {
                            spacing: sessionRow.tight ? 7 : 11
                            // A helper: hooked under its parent.
                            Label {
                                objectName: "sessionHelperMark"
                                visible: sessionRow.nest > 0 && !sessionRow.tight
                                text: "↳"
                                color: Theme.textFaint
                                font.pixelSize: Theme.px(13)
                                Layout.rightMargin: -5
                            }
                            // What runs it, with its status on the corner.
                            AgentAvatar {
                                provider: sessionRow.harness
                                badge: sessionRow.status
                                ring: sessionRow.rowColor
                                opacity: sessionRow.dim ? 0.4 : 1
                                Behavior on opacity { NumberAnimation { duration: Theme.hoverMotion } }
                            }
                            Label {
                                objectName: "sessionTitle"
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                Layout.preferredWidth: 0
                                text: (sessionRow.pinned ? "★ " : "") + sessionRow.title
                                color: sessionRow.active || sessionRow.status === "unread" ? Theme.text : sessionRow.dim ? Theme.textFaint : Theme.textMuted
                                elide: Text.ElideRight
                                font.pixelSize: Theme.px(14)
                                font.weight: sessionRow.status === "unread" ? Font.DemiBold : sessionRow.active ? Font.Medium : Font.Normal
                            }
                            // In a worktree (its branch is in the hover tip).
                            Icon {
                                objectName: "sessionBranchMark"
                                visible: sessionRow.branch !== ""
                                name: "git-branch"
                                color: Theme.info
                                implicitWidth: 12
                                implicitHeight: 12
                            }
                            // Uncommitted changes where the session works, when that
                            // isn't its project's folder (a worktree): the
                            // project's own show on its header.
                            ChangesPill {
                                id: changesPill
                                changes: sessionRow.changes
                                tip: " in this session's repository"
                            }
                            Label { visible: !sessionRow.tight; text: Theme.ago(sessionRow.modified, root.now); color: Theme.textFaint; font.pixelSize: Theme.px(12) }
                        }
                        onClicked: Workspace.open(rowKey)
                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            onTapped: {
                                sessionMenu.key = sessionRow.rowKey
                                sessionMenu.sessionTitle = sessionRow.title
                                sessionMenu.harness = sessionRow.harness
                                sessionMenu.pinned = sessionRow.pinned
                                sessionMenu.live = sessionRow.rowKey.startsWith("live:")
                                sessionMenu.branch = sessionRow.branch
                                sessionMenu.settled = sessionRow.settled !== ""
                                sessionMenu.repo = Workspace.sessionRepo(sessionRow.rowKey)
                                sessionMenu.repoIgnored = sessionMenu.repo !== "" && Workspace.isRepoIgnored(sessionMenu.repo)
                                sessionMenu.popup()
                            }
                        }
                    }
                }
            }
        }

        // Result of the last background job (worktrees, setup, merges): a
        // small card, red-edged for a failure.
        Rectangle {
            id: noticeCard
            objectName: "noticeCard"
            readonly property bool failed: Workspace.noticeError
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 10
            visible: Workspace.notice !== ""
            implicitHeight: noticeRow.implicitHeight + 20
            radius: 12
            color: failed ? Qt.rgba(Theme.error.r, Theme.error.g, Theme.error.b, 0.08) : Theme.raised
            border.color: failed ? Qt.rgba(Theme.error.r, Theme.error.g, Theme.error.b, 0.35) : Theme.hairline
            RowLayout {
                id: noticeRow
                x: 12
                y: 10
                width: parent.width - 18
                spacing: 9
                Item {
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 1
                    implicitWidth: 16
                    implicitHeight: 16
                    BusyIndicator { anchors.fill: parent; running: Workspace.busy; visible: Workspace.busy }
                    Icon {
                        anchors.fill: parent
                        visible: !Workspace.busy
                        name: noticeCard.failed ? "alert" : "information"
                        color: noticeCard.failed ? Theme.error : Theme.textMuted
                    }
                }
                Label {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    text: Workspace.notice
                    color: Theme.text
                    wrapMode: Text.Wrap
                    maximumLineCount: 5
                    elide: Text.ElideRight
                    lineHeight: 1.15
                    font.pixelSize: Theme.px(12.5)
                }
                PanelButton {
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: -5
                    implicitWidth: 24
                    implicitHeight: 24
                    round: true
                    iconName: "close"
                    tip: "Dismiss"
                    onClicked: Workspace.clearNotice()
                }
            }
        }
    }

    // Uncommitted changes: a small warning-tinted count.
    component ChangesPill: Rectangle {
        id: pill
        property int changes: 0
        property string tip: ""
        visible: changes > 0
        implicitWidth: pillRow.implicitWidth + 10
        implicitHeight: 16
        radius: 8
        color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.16)
        Row {
            id: pillRow
            anchors.centerIn: parent
            spacing: 3
            Rectangle { width: 5; height: 5; radius: 2.5; color: Theme.warning; anchors.verticalCenter: parent.verticalCenter }
            Label { text: pill.changes > 99 ? "99+" : pill.changes; color: Theme.warning; font.pixelSize: Theme.px(10); font.bold: true }
        }
        HoverHandler { id: pillHover; enabled: list.dragId < 0 }
        ToolTip.visible: pillHover.hovered
        ToolTip.text: pill.changes + (pill.changes === 1 ? " uncommitted change" : " uncommitted changes") + pill.tip
        ToolTip.delay: 400
    }

    // Project header or the "Other sessions" group.
    component GroupRow: ItemDelegate {
        id: groupRow
        required property string kind
        required property string rowKey
        required property int projectId
        required property string title
        required property string path
        required property string harness
        required property string status
        required property bool collapsed
        required property int count
        // Uncommitted changes in the project's folder (its sessions there
        // don't repeat it).
        required property int changes
        required property string projectIcon
        required property string projectColor
        required property int index
        // Room above each group but the first.
        readonly property real gap: index > 0 ? 16 : 2
        // A narrow sidebar: the name keeps its room.
        readonly property bool narrow: width < Theme.px(220)
        width: ListView.view.width
        height: 30 + gap
        // The arrow shows on hover, whatever the platform's hover setting.
        hoverEnabled: list.dragId < 0
        leftInset: 10
        rightInset: 10
        topInset: gap
        topPadding: gap
        leftPadding: 18
        rightPadding: 14
        background: Rectangle {
            radius: 8
            color: groupRow.hovered && !groupRow.lifted ? Theme.rowHover : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
            Shape {
                id: hole
                anchors.fill: parent
                visible: groupRow.lifted
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeColor: Theme.controlBorder
                    strokeWidth: 1
                    strokeStyle: ShapePath.DashLine
                    dashPattern: [4, 3]
                    fillColor: "transparent"
                    PathRectangle { x: 0.5; y: 0.5; width: hole.width - 1; height: hole.height - 1; radius: 8 }
                }
            }
        }
        contentItem: RowLayout {
            opacity: groupRow.lifted ? 0 : 1
            spacing: 6
            // The project's icon in its colour (Settings → Projects; a grey
            // folder by default, and for Other sessions); hovered, the
            // expand arrow in its place.
            Item {
                implicitWidth: 13
                implicitHeight: 13
                ProjectIcon {
                    objectName: "projectIcon-" + groupRow.title
                    anchors.fill: parent
                    visible: !groupRow.hovered
                    icon: groupRow.kind === "project" ? groupRow.projectIcon : ""
                    color: groupRow.kind === "project" ? groupRow.projectColor : ""
                }
                Label {
                    anchors.centerIn: parent
                    visible: groupRow.hovered
                    text: groupRow.collapsed ? "▸" : "▾"
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
            }
            // The project's name as it is written, a step quieter than its
            // sessions' titles but heavier: its icon, weight and the room
            // above set it apart.
            Label {
                objectName: "projectTitle-" + groupRow.title
                Layout.fillWidth: true
                // (Narrow: a few letters at least; the count and the
                // provider's mark give way first.)
                Layout.minimumWidth: Math.min(implicitWidth, Theme.px(48))
                Layout.leftMargin: 2
                text: groupRow.title
                color: groupRow.hovered ? Theme.text : Theme.textMuted
                font.pixelSize: Theme.px(13)
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            ChangesPill {
                objectName: "projectChanges-" + groupRow.title
                changes: groupRow.changes
                tip: " in this project's folder"
            }
            // A project on a remote host: the host's name.
            Rectangle {
                objectName: "remoteBadge"
                readonly property string host: groupRow.path.startsWith("ssh://") ? groupRow.path.substring(6).split("/")[0] : ""
                visible: host !== ""
                implicitWidth: hostRow.implicitWidth + 10
                implicitHeight: 18
                radius: 5
                color: Theme.subtle
                border.color: Theme.border
                Row {
                    id: hostRow
                    anchors.centerIn: parent
                    spacing: 3
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "server"; color: Theme.muted; implicitWidth: 11; implicitHeight: 11 }
                    Label { objectName: "remoteBadgeText"; text: parent.parent.host; color: Theme.muted; font.pixelSize: Theme.px(10.5) }
                }
                HoverHandler { id: badgeHover; enabled: list.dragId < 0 }
                ToolTip.visible: badgeHover.hovered
                ToolTip.text: "On " + host + ": " + Remotes.displayPath(groupRow.path)
            }
            StatusDot { status: groupRow.status === "unread" || groupRow.status === "input" || groupRow.status === "busy" ? groupRow.status : "" }
            // What the project's new sessions run.
            ProviderMark {
                objectName: "projectProvider-" + groupRow.title
                visible: groupRow.kind === "project" && known && !groupRow.narrow
                provider: groupRow.harness
                implicitWidth: 12
                implicitHeight: 12
                opacity: 0.8
                tip: list.dragId >= 0 ? "" : "New sessions here use " + Theme.harnessLabel(groupRow.harness)
            }
            Label { visible: !groupRow.narrow; text: groupRow.count; color: Theme.textFaint; font.pixelSize: Theme.px(12) }
            FlatButton {
                text: "+"
                hoverEnabled: list.dragId < 0
                outlined: false
                accent: Theme.textMuted
                implicitWidth: 24
                implicitHeight: 24
                padding: 0
                font.pixelSize: Theme.px(18)
                ToolTip.visible: hovered
                ToolTip.text: groupRow.kind === "group" ? "New session in home folder" : "New " + Theme.harnessLabel(groupRow.harness) + " session"
                onClicked: root.newSessionRequested(groupRow.projectId)
            }
        }
        onClicked: Workspace.toggleCollapsed(rowKey)
        // A project's header can be picked up.
        HoverHandler {
            enabled: groupRow.kind === "project" && list.dragId < 0
            cursorShape: Qt.OpenHandCursor
        }
        // Lifted: its place shows as a dashed outline.
        readonly property bool lifted: list.dragId >= 0 && list.dragId === projectId && kind === "project"
        // Dragged by its header, a project goes where it's dropped.
        DragHandler {
            objectName: "projectDrag"
            enabled: groupRow.kind === "project"
            target: null
            xAxis.enabled: false
            cursorShape: Qt.ClosedHandCursor
            onActiveChanged: {
                if (active) {
                    list.dragTitle = groupRow.title
                    list.dragIcon = groupRow.projectIcon
                    list.dragColor = groupRow.projectColor
                    list.dragCount = groupRow.count
                    // (From the header's visible part, below its gap.)
                    list.dragGrab = centroid.pressPosition.y - groupRow.gap
                    list.dragId = groupRow.projectId
                    list.dragSceneY = centroid.scenePosition.y
                    list.startDrag()
                    list.updateDrop()
                } else {
                    list.endDrag(true)
                }
            }
            onCanceled: list.endDrag(false)
            onCentroidChanged: if (active && list.dragId >= 0) { list.dragSceneY = centroid.scenePosition.y; list.updateDrop() }
        }
        TapHandler {
            acceptedButtons: Qt.RightButton
            enabled: groupRow.kind === "project"
            onTapped: {
                projectMenu.projectId = groupRow.projectId
                projectMenu.path = groupRow.path
                projectMenu.harness = groupRow.harness
                projectMenu.name = groupRow.title
                projectMenu.place = Workspace.projectPlace(groupRow.projectId)
                projectMenu.popup()
            }
        }
    }
}
