import QtQuick
import QtQuick.Controls
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
        onAccepted: Workspace.addProject(selectedFolder.toString())
    }
    // A folder of the instance shown: this computer's, or the host's.
    RemoteFolderDialog { id: remoteFolderDialog; onPicked: path => Workspace.addProject(path) }
    signal manageInstancesRequested()

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
        title: "Move session to trash?"
        modal: true
        anchors.centerIn: Overlay.overlay
        footer: AppButtonBox {
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            DialogButton { text: "Move to trash"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole; accent: false; destructive: true }
        }
        Label { text: trashDialog.label; width: 360; wrapMode: Text.Wrap; color: Theme.muted }
        onAccepted: Workspace.trashSession(key)
    }

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
    function openAddProject() {
        if (Workspace.instance === "") folderDialog.open()
        else remoteFolderDialog.openFor(Workspace.instance)
    }
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
        AppMenuItem { text: "Project settings…"; onTriggered: root.projectSettingsRequested(projectMenu.projectId) }
        AppMenuItem { text: "Open folder"; visible: !projectMenu.path.startsWith("ssh://"); onTriggered: Qt.openUrlExternally("file://" + projectMenu.path) }
        AppMenuItem { text: "Remove project"; danger: true; onTriggered: Workspace.removeProject(projectMenu.projectId) }
    }

    AppMenu {
        id: sessionMenu
        property string key: ""
        property string sessionTitle: ""
        property bool pinned: false
        property bool live: false
        property string branch: ""
        // Repository of the session's folder ("" outside one).
        property string repo: ""
        property bool repoIgnored: false
        AppMenuItem { text: sessionMenu.pinned ? "Unpin" : "Pin"; onTriggered: Workspace.setPinned(sessionMenu.key, !sessionMenu.pinned) }
        AppMenuItem { text: "Rename…"; onTriggered: renameDialog.ask(sessionMenu.key, sessionMenu.sessionTitle) }
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
            text: "Move to trash…"
            danger: true
            onTriggered: { trashDialog.key = sessionMenu.key; trashDialog.label = sessionMenu.sessionTitle; trashDialog.open() }
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
            placeholderText: "Search"
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
                        contentItem: Label { text: moreRow.title; color: Theme.muted; font.pixelSize: Theme.px(12); verticalAlignment: Text.AlignVCenter }
                        background: Rectangle { radius: 8; color: moreRow.hovered ? Theme.hover : "transparent" }
                        onClicked: Workspace.showMore(projectId)
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
                        readonly property bool active: rowKey === Workspace.activeKey
                        // On hover: the cwd (other sessions) and/or the worktree branch.
                        readonly property string detail: (branch !== "" ? "⎇ " + branch : "") + (branch !== "" && subtitle !== "" ? "  ·  " : "") + subtitle
                        width: ListView.view.width
                        height: 42
                        hoverEnabled: true
                        ToolTip.visible: hovered && detail !== ""
                        ToolTip.text: detail
                        ToolTip.delay: 700
                        // Rounded, inset from the sidebar's edges.
                        leftInset: 10
                        rightInset: 10
                        topInset: 1
                        bottomInset: 1
                        leftPadding: 18
                        rightPadding: 18
                        topPadding: 2
                        bottomPadding: 2
                        readonly property color rowColor: active ? Theme.rowSelected : hovered ? Theme.rowHover : Theme.chrome
                        background: Rectangle {
                            radius: 9
                            color: sessionRow.active || sessionRow.hovered ? sessionRow.rowColor : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        }
                        contentItem: RowLayout {
                            spacing: 11
                            // What runs it, with its status on the corner.
                            AgentAvatar {
                                provider: sessionRow.harness
                                badge: sessionRow.status
                                ring: sessionRow.rowColor
                            }
                            Label {
                                Layout.fillWidth: true
                                text: (sessionRow.pinned ? "★ " : "") + sessionRow.title
                                color: sessionRow.active || sessionRow.status === "unread" ? Theme.text : Theme.textMuted
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
                            // Uncommitted changes in the session's repository.
                            Rectangle {
                                id: changesPill
                                visible: sessionRow.changes > 0
                                implicitWidth: pillRow.implicitWidth + 10
                                implicitHeight: 16
                                radius: 8
                                color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.16)
                                Row {
                                    id: pillRow
                                    anchors.centerIn: parent
                                    spacing: 3
                                    Rectangle { width: 5; height: 5; radius: 2.5; color: Theme.warning; anchors.verticalCenter: parent.verticalCenter }
                                    Label { text: sessionRow.changes > 99 ? "99+" : sessionRow.changes; color: Theme.warning; font.pixelSize: Theme.px(10); font.bold: true }
                                }
                                HoverHandler { id: pillHover }
                                ToolTip.visible: pillHover.hovered
                                ToolTip.text: sessionRow.changes + (sessionRow.changes === 1 ? " uncommitted change" : " uncommitted changes") + " in this session's repository"
                                ToolTip.delay: 400
                            }
                            Label { text: Theme.ago(sessionRow.modified, root.now); color: Theme.textFaint; font.pixelSize: Theme.px(12) }
                        }
                        onClicked: Workspace.open(rowKey)
                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            onTapped: {
                                sessionMenu.key = sessionRow.rowKey
                                sessionMenu.sessionTitle = sessionRow.title
                                sessionMenu.pinned = sessionRow.pinned
                                sessionMenu.live = sessionRow.rowKey.startsWith("live:")
                                sessionMenu.branch = sessionRow.branch
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
        required property string projectIcon
        required property string projectColor
        required property int index
        // Room above each group but the first.
        readonly property real gap: index > 0 ? 16 : 2
        width: ListView.view.width
        height: 30 + gap
        // The arrow shows on hover, whatever the platform's hover setting.
        hoverEnabled: true
        leftInset: 10
        rightInset: 10
        topInset: gap
        topPadding: gap
        leftPadding: 18
        rightPadding: 14
        background: Rectangle { radius: 8; color: groupRow.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
        contentItem: RowLayout {
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
            // A section label: small, uppercase, spaced.
            Label {
                Layout.fillWidth: true
                Layout.leftMargin: 2
                text: groupRow.title
                color: groupRow.hovered ? Theme.textMuted : Theme.label
                font.pixelSize: Theme.px(12)
                font.weight: Font.Medium
                font.capitalization: Font.AllUppercase
                font.letterSpacing: Theme.px(12) * 0.07
                elide: Text.ElideRight
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
                HoverHandler { id: badgeHover }
                ToolTip.visible: badgeHover.hovered
                ToolTip.text: "On " + host + ": " + Remotes.displayPath(groupRow.path)
            }
            StatusDot { status: groupRow.status === "unread" || groupRow.status === "input" || groupRow.status === "busy" ? groupRow.status : "" }
            // What the project's new sessions run.
            ProviderMark {
                objectName: "projectProvider-" + groupRow.title
                visible: groupRow.kind === "project" && known
                provider: groupRow.harness
                implicitWidth: 12
                implicitHeight: 12
                opacity: 0.8
                tip: "New sessions here use " + Theme.harnessLabel(groupRow.harness)
            }
            Label { text: groupRow.count; color: Theme.textFaint; font.pixelSize: Theme.px(12) }
            FlatButton {
                text: "+"
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
        TapHandler {
            acceptedButtons: Qt.RightButton
            enabled: groupRow.kind === "project"
            onTapped: {
                projectMenu.projectId = groupRow.projectId
                projectMenu.path = groupRow.path
                projectMenu.harness = groupRow.harness
                projectMenu.name = groupRow.title
                projectMenu.popup()
            }
        }
    }
}
