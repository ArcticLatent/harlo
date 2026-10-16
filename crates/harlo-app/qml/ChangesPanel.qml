import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Working-tree changes of the active session's folder: files (clicking one
// shows its changes side by side in the diff tab) and a commit box.
Rectangle {
    id: root
    required property GitPanel git
    /// Harness of the active session; commit messages are written with it.
    property string harness: ""
    /// The active session's agent is working (in this folder).
    property bool agentBusy: false
    // Test hooks.
    readonly property alias branchPicker: picker
    readonly property alias stashesDialog: stashes
    readonly property alias switchDialog: confirmSwitch
    signal closeRequested()
    /// A message for the session's agent (what failed in CI).
    signal sendToAgent(string message)
    /// A session to send it to.
    property bool canSend: false
    readonly property alias requestDialog: requestDialog
    PullRequestDialog { id: requestDialog; git: root.git }
    onVisibleChanged: if (visible) { requestTimer.restart(); if (git.reposJson !== "") git.loadAllRequests() }
    color: Theme.canvas
    clip: true

    // Files split into the two lists; each entry: {path, dir, name, code, untracked, conflicted, staged}
    function entries(staged) {
        const out = []
        for (const line of git.files) {
            const p = line.split("\t")
            const f = { path: p[0], index: p[1], worktree: p[2], untracked: p[3] === "1", conflicted: p[4] === "1" }
            const isStaged = !f.untracked && f.index !== "."
            const isUnstaged = f.untracked || f.worktree !== "."
            if (staged ? !isStaged : !isUnstaged)
                continue
            const slash = f.path.lastIndexOf("/")
            out.push({
                path: f.path,
                name: f.path.substring(slash + 1),
                dir: slash >= 0 ? f.path.substring(0, slash) : "",
                code: f.conflicted ? "!" : f.untracked ? "U" : (staged ? f.index : f.worktree),
                staged: staged,
            })
        }
        return out
    }
    // Clear the message only once the commit succeeded.
    property bool committing: false
    // Test hook.
    property alias commitText: commitMessage.text
    Connections {
        target: root.git
        function onBusyChanged() {
            if (!root.git.busy && root.committing) {
                root.committing = false
                if (!root.git.error)
                    commitMessage.text = ""
            }
        }
    }

    readonly property var stagedEntries: entries(true)
    readonly property var unstagedEntries: entries(false)

    function codeColor(code) {
        switch (code) {
        case "A": case "U": return Theme.success
        case "D": return Theme.error
        case "R": case "C": return Theme.info
        case "!": return Theme.error
        default: return Theme.warning
        }
    }

    AppDialog {
        id: confirmRevert
        property string path: ""
        title: "Discard changes to " + path + "?"
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label {
            width: 340
            wrapMode: Text.Wrap
            text: "This cannot be undone from harlo. Untracked files are moved to the trash."
            color: Theme.muted
        }
        onDiscarded: { git.revertFile(path); close() }
    }

    // Switching branches: with uncommitted changes, stash them or bring
    // them along (git refuses if they clash with the other branch).
    readonly property int changedCount: git.stagedCount + git.unstagedCount
    function requestSwitch(name, remote, create) {
        if (create) { git.switchBranch(name, true, false, false); return }
        if (changedCount === 0 && !agentBusy) { git.switchBranch(name, false, remote, false); return }
        confirmSwitch.name = name
        confirmSwitch.remote = remote
        confirmSwitch.dirty = changedCount > 0
        confirmSwitch.open()
    }
    AppDialog {
        id: confirmSwitch
        property string name: ""
        property bool remote: false
        property bool dirty: false
        readonly property string target: remote ? name.substring(name.indexOf("/") + 1) : name
        title: "Switch to " + target + "?"
        // A plain row: the button box's platform order put Cancel in the middle.
        footer: Item {
            implicitHeight: switchButtons.implicitHeight + 26
            implicitWidth: switchButtons.implicitWidth + 40
            Row {
                id: switchButtons
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.top: parent.top
                anchors.topMargin: 8
                spacing: 8
                DialogButton { text: "Cancel"; onClicked: confirmSwitch.reject() }
                DialogButton { visible: confirmSwitch.dirty; text: "Bring changes along"; onClicked: { git.switchBranch(confirmSwitch.name, false, confirmSwitch.remote, false); confirmSwitch.close() } }
                DialogButton { text: confirmSwitch.dirty ? "Stash and switch" : "Switch"; accent: true; onClicked: confirmSwitch.accept() }
            }
        }
        Column {
            width: 400
            spacing: 10
            Label {
                width: parent.width
                visible: confirmSwitch.dirty
                wrapMode: Text.Wrap
                color: Theme.muted
                text: (root.changedCount === 1 ? "1 file has" : root.changedCount + " files have") + " uncommitted changes. Stash them (bring them back later from Stashes), or take them to " + confirmSwitch.target + "; git refuses that if they clash with it."
            }
            Label {
                width: parent.width
                visible: root.agentBusy
                wrapMode: Text.Wrap
                color: Theme.warning
                text: "The agent is working in this folder. Switching changes the files under it."
            }
        }
        onAccepted: git.switchBranch(name, false, remote, dirty)
    }
    BranchPicker {
        id: picker
        git: root.git
        parent: branchChip
        y: branchChip.height + 4
        onChosen: (name, remote, create) => root.requestSwitch(name, remote, create)
    }
    StashesDialog { id: stashes; git: root.git }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Changes"
            subtitle: git.directory.split("/").pop() + (repoList.repos.length > 0 && git.root !== "" ? " / " + git.root.split("/").pop() : "")
            onCloseRequested: root.closeRequested()
            PanelButton { iconName: "refresh"; tip: "Refresh"; onClicked: git.refresh(true) }
        }

        // A folder of repositories (not one itself): each, with its branch
        // and how many files it changed; the rest of the panel shows the one
        // picked here. Those without changes fold away.
        ColumnLayout {
            id: repoList
            objectName: "repoList"
            readonly property var repos: git.reposJson !== "" ? JSON.parse(git.reposJson) : []
            /// Each one's open pull request (and its checks), by root.
            readonly property var requests: git.requestsJson !== "" ? JSON.parse(git.requestsJson) : ({})
            // Changes, or an open pull request: shown; the rest fold away.
            readonly property var changed: repos.filter(r => r.changes > 0 || !!requests[r.root])
            readonly property var clean: repos.filter(r => r.changes === 0 && !requests[r.root])
            onReposChanged: if (repos.length > 0 && root.visible) git.loadAllRequests()
            // Every repository's pull request: read again now and then; those
            // whose checks run are watched (harlo says when they finish).
            Timer {
                interval: 60000
                repeat: true
                running: root.visible && repoList.repos.length > 0
                onTriggered: git.loadAllRequests()
            }
            onRequestsChanged: {
                for (const r in requests) {
                    const q = requests[r]
                    if (q.checks && q.checks.state === "pending")
                        Workspace.watchRequest(JSON.stringify({ service: q.service, host: q.host, repo: q.repo, url: q.url, number: q.number, word: q.word, title: q.title }))
                }
            }
            property bool showClean: false
            visible: repos.length > 0
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.topMargin: 10
            spacing: 1
            Label {
                Layout.leftMargin: 8
                Layout.bottomMargin: 4
                text: "REPOSITORIES · " + repoList.repos.filter(r => r.changes > 0).length + " OF " + repoList.repos.length + " CHANGED"
                color: Theme.label
                font.pixelSize: Theme.px(11)
                font.weight: Font.Medium
                font.letterSpacing: Theme.px(11) * 0.07
            }
            Repeater {
                model: repoList.changed.concat(repoList.showClean ? repoList.clean : [])
                ItemDelegate {
                    id: repoRow
                    required property var modelData
                    readonly property bool active: modelData.root === git.root
                    objectName: "repo-" + modelData.name
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    implicitHeight: 30
                    leftPadding: 8
                    rightPadding: 8
                    onClicked: git.selectRepo(modelData.root)
                    background: Rectangle {
                        radius: 8
                        color: repoRow.active ? Theme.rowSelected : repoRow.hovered ? Theme.rowHover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                    }
                    contentItem: RowLayout {
                        spacing: 7
                        Icon { name: "git-repository"; color: repoRow.active ? Theme.accent : Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                        Label {
                            // (Its name keeps its width; the branch gives way first.)
                            Layout.preferredWidth: implicitWidth
                            Layout.minimumWidth: Math.min(implicitWidth, 90)
                            text: repoRow.modelData.name
                            color: Theme.text
                            font.pixelSize: Theme.px(12.5)
                            font.weight: repoRow.active ? Font.DemiBold : Font.Normal
                            elide: Text.ElideMiddle
                        }
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: repoRow.modelData.branch + (repoRow.modelData.ahead > 0 ? " ↑" + repoRow.modelData.ahead : "") + (repoRow.modelData.behind > 0 ? " ↓" + repoRow.modelData.behind : "")
                            color: Theme.textFaint
                            font.pixelSize: Theme.px(11.5)
                            elide: Text.ElideRight
                        }
                        // Its open pull request: the number, and a dot for its checks.
                        Row {
                            readonly property var req: repoList.requests[repoRow.modelData.root] || null
                            readonly property string ci: req && req.checks ? req.checks.state : "none"
                            objectName: "repoRequest-" + repoRow.modelData.name
                            visible: req !== null
                            spacing: 4
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: parent.ci !== "none"
                                width: 7; height: 7; radius: 3.5
                                color: parent.ci === "passing" ? Theme.success : parent.ci === "failing" ? Theme.error : Theme.warning
                            }
                            Label {
                                text: parent.req ? (parent.req.word === "merge request" ? "!" : "#") + parent.req.number : ""
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(11.5)
                            }
                        }
                        Label {
                            visible: repoRow.modelData.changes > 0
                            text: repoRow.modelData.changes
                            color: repoRow.modelData.staged > 0 ? Theme.success : Theme.warning
                            font.pixelSize: Theme.px(11.5)
                            font.weight: Font.DemiBold
                        }
                    }
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: modelData.root + "\n" + (modelData.changes === 0 ? "No changes" : modelData.changes + (modelData.changes === 1 ? " changed file" : " changed files") + (modelData.staged > 0 ? ", " + modelData.staged + " staged" : ""))
                        + (repoList.requests[modelData.root] ? "\n" + (repoList.requests[modelData.root].word === "merge request" ? "Merge request !" : "Pull request #") + repoList.requests[modelData.root].number
                           + (repoList.requests[modelData.root].checks ? ": " + repoList.requests[modelData.root].checks.summary : "") : "")
                }
            }
            ItemDelegate {
                objectName: "repoShowClean"
                visible: repoList.clean.length > 0
                Layout.fillWidth: true
                implicitHeight: 26
                leftPadding: 8
                onClicked: repoList.showClean = !repoList.showClean
                background: Rectangle { radius: 8; color: parent.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                contentItem: Label {
                    text: repoList.showClean ? "Hide those without changes" : repoList.clean.length + (repoList.clean.length === 1 ? " without changes" : " without changes") + "…"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
            }
            Rectangle { Layout.fillWidth: true; Layout.topMargin: 6; implicitHeight: 1; color: Theme.hairline }
        }

        // Branch and remote actions. Rows here set minimumWidth 0: a row
        // whose fixed items don't fit would otherwise force the whole column
        // (every row of the panel) wider than the panel.
        RowLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 10
            Layout.bottomMargin: 4
            visible: git.isRepo
            spacing: 6
            // The branch: icon, name (elides first), ahead/behind, a chevron.
            Rectangle {
                id: branchChip
                objectName: "branchChip"
                readonly property string text: (git.branch !== "" ? git.branch : "detached HEAD") + (git.ahead > 0 ? "  ↑" + git.ahead : "") + (git.behind > 0 ? "  ↓" + git.behind : "")
                Layout.fillWidth: true
                Layout.minimumWidth: 52
                Layout.preferredWidth: implicitWidth
                Layout.maximumWidth: implicitWidth
                visible: git.isRepo
                implicitWidth: branchRow.implicitWidth + 20
                implicitHeight: 26
                radius: 8
                color: chipHover.hovered || picker.opened ? Theme.controlFillHover : Theme.controlFill
                Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                border.color: Theme.hairline
                RowLayout {
                    id: branchRow
                    x: 10
                    width: parent.width - 20
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 5
                    Icon { name: "git-branch"; color: Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        text: branchChip.text
                        color: Theme.text
                        font.pixelSize: Theme.px(12.5)
                        elide: Text.ElideRight
                    }
                    Icon { name: "arrow-down-s"; color: Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                }
                ToolTip.visible: chipHover.hovered
                ToolTip.text: width < implicitWidth ? text + "\nSwitch branch" : "Switch branch"
                HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { enabled: !git.busy; onTapped: picker.opened ? picker.close() : picker.open() }
            }
            // Stashes, with how many there are.
            Rectangle {
                objectName: "stashButton"
                visible: git.isRepo
                implicitWidth: stashRow.implicitWidth + 12
                implicitHeight: 26
                radius: 8
                color: stashTap.pressed ? Theme.controlFillDown : stashHover.hovered ? Theme.controlFillHover : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                border.color: Theme.hairline
                Row {
                    id: stashRow
                    anchors.centerIn: parent
                    spacing: 4
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "archive-stack"; color: Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                    Label { anchors.verticalCenter: parent.verticalCenter; visible: git.stashCount > 0; text: git.stashCount; color: Theme.text; font.pixelSize: Theme.px(11.5) }
                }
                HoverHandler { id: stashHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { id: stashTap; onTapped: stashes.open() }
                ToolTip.visible: stashHover.hovered
                ToolTip.text: git.stashCount > 0 ? "Stashes (" + git.stashCount + ")" : "Stash changes"
            }
            Item { Layout.fillWidth: true }
            BusyIndicator { running: git.busy; visible: git.busy; implicitWidth: 18; implicitHeight: 18 }
            FlatButton { text: "Pull"; font.pixelSize: Theme.px(11); visible: git.isRepo && git.upstream !== ""; enabled: !git.busy; onClicked: git.pull() }
            FlatButton {
                text: git.upstream === "" ? "Publish" : "Push"
                font.pixelSize: Theme.px(11)
                visible: git.isRepo
                enabled: !git.busy
                onClicked: git.push()
            }
        }

        // The branch's pull request (merge request on GitLab): open it, or
        // open a new one. (Pushed branches other than the main one.)
        RowLayout {
            id: requestRow
            objectName: "requestRow"
            readonly property var req: git.requestJson !== "" ? JSON.parse(git.requestJson) : ({ state: "none" })
            readonly property bool has: (req.url || "") !== ""
            visible: git.isRepo && git.upstream !== "" && (req.state === "ready" || req.state === "creating") && req.head !== req.base
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 4
            spacing: 8
            Icon { name: "git-branch"; color: requestRow.has ? Theme.success : Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                elide: Text.ElideRight
                text: requestRow.has ? (requestRow.req.word === "merge request" ? "Merge request !" : "Pull request #") + requestRow.req.number + " is open"
                    : "No " + (requestRow.req.word || "pull request") + " yet · " + (requestRow.req.label || "")
                color: requestRow.has ? Theme.text : Theme.textMuted
                font.pixelSize: Theme.px(12.5)
            }
            FlatButton {
                objectName: "requestButton"
                text: requestRow.has ? "Open" : "Create " + (requestRow.req.word || "pull request")
                font.pixelSize: Theme.px(11.5)
                accent: requestRow.has ? Theme.foreground : Theme.primary
                onClicked: requestRow.has ? Qt.openUrlExternally(requestRow.req.url) : requestDialog.show()
            }
        }
        // Its CI checks: running, passed, failed (send what failed to the agent).
        RowLayout {
            id: checksRow
            objectName: "checksRow"
            readonly property var checks: requestRow.req.checks || null
            readonly property string state: checks ? checks.state : "none"
            property bool sending: false
            visible: requestRow.visible && requestRow.has && state !== "none"
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 2
            spacing: 8
            Icon {
                name: checksRow.state === "passing" ? "checkbox-circle" : checksRow.state === "failing" ? "close-circle" : "loader-4"
                color: checksRow.state === "passing" ? Theme.success : checksRow.state === "failing" ? Theme.error : Theme.warning
                implicitWidth: 14
                implicitHeight: 14
            }
            Label {
                objectName: "checksSummary"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                elide: Text.ElideRight
                // Short: the panel is narrow (the whole summary on hover).
                text: !checksRow.checks ? ""
                    : checksRow.state === "failing" ? "Failed: " + checksRow.checks.failing.map(f => f.name).join(", ")
                    : checksRow.state === "pending" ? "Checks running · " + checksRow.checks.passed + " of " + (checksRow.checks.passed + checksRow.checks.pending) + " done"
                    : "Checks passed"
                ToolTip.delay: 400
                color: checksRow.state === "failing" ? Theme.error : Theme.textMuted
                font.pixelSize: Theme.px(12)
                ToolTip.visible: summaryHover.hovered
                ToolTip.text: checksRow.checks ? checksRow.checks.summary : ""
                HoverHandler { id: summaryHover }
            }
            FlatButton {
                objectName: "ciSend"
                visible: checksRow.state === "failing"
                enabled: root.canSend && !checksRow.sending
                text: checksRow.sending ? "Reading the logs…" : "Send to the agent"
                font.pixelSize: Theme.px(11.5)
                accent: Theme.primary
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Send the failed checks, with the end of their logs, to this session's agent to fix"
                onClicked: { checksRow.sending = true; git.reportFailures() }
            }
        }
        Connections {
            target: git
            function onCiReportChanged() {
                if (git.ciReport !== "" && checksRow.sending) {
                    checksRow.sending = false
                    root.sendToAgent(git.ciReport)
                }
            }
        }
        // While checks run: read them again now and then, and have harlo
        // tell you when they finish (even with this panel closed).
        Timer {
            interval: 20000
            repeat: true
            running: root.visible && checksRow.state === "pending"
            onTriggered: git.loadRequest()
        }
        Connections {
            target: git
            function onRequestJsonChanged() {
                const r = requestRow.req
                if (r.state === "ready" && r.url && r.checks && r.checks.state === "pending")
                    Workspace.watchRequest(JSON.stringify({ service: r.service, host: r.host, repo: r.repo, url: r.url, number: r.number, word: r.word, title: r.title }))
            }
        }
        // (Read when the branch or its upstream changes, a moment later.)
        Timer { id: requestTimer; interval: 500; onTriggered: git.loadRequest() }
        Connections {
            target: git
            function onBranchChanged() { requestTimer.restart() }
            function onUpstreamChanged() { requestTimer.restart() }
            function onRootChanged() { requestTimer.restart() }
        }

        Label {
            Layout.fillWidth: true
            Layout.margins: 18
            visible: !git.isRepo && repoList.repos.length === 0
            wrapMode: Text.Wrap
            color: Theme.muted
            text: git.directory === "" ? "No session selected." : "Not a git repository, and none inside it:\n" + git.directory
        }

        // File lists
        ListView {
                id: fileList
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: git.isRepo
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                model: (root.stagedEntries.length ? [{ header: "Staged", count: root.stagedEntries.length }] : [])
                    .concat(root.stagedEntries)
                    .concat(root.unstagedEntries.length ? [{ header: "Changes", count: root.unstagedEntries.length }] : [])
                    .concat(root.unstagedEntries)
                footer: Label {
                    visible: git.files.length === 0
                    width: fileList.width
                    leftPadding: 18
                    topPadding: 12
                    text: "No changes."
                    color: Theme.textMuted
                }
                delegate: Loader {
                    id: entry
                    required property var modelData
                    width: ListView.view.width
                    // Fixed height: the Loader sizes its item to this.
                    height: modelData.header !== undefined ? 34 : 30
                    sourceComponent: modelData.header !== undefined ? sectionHeader : fileRow
                    Component {
                        id: sectionHeader
                        RowLayout {
                            height: 34
                            // A section label, as the sidebar's.
                            Label {
                                Layout.leftMargin: 18
                                Layout.alignment: Qt.AlignBottom
                                bottomPadding: 6
                                text: entry.modelData.header + " · " + entry.modelData.count
                                font.capitalization: Font.AllUppercase
                                color: Theme.label
                                font.pixelSize: Theme.px(11)
                                font.weight: Font.Medium
                                font.letterSpacing: Theme.px(11) * 0.07
                            }
                            Item { Layout.fillWidth: true }
                            FlatButton {
                                visible: entry.modelData.header === "Changes"
                                Layout.rightMargin: 10
                                Layout.alignment: Qt.AlignBottom
                                text: "Stage all"
                                font.pixelSize: Theme.px(11.5)
                                outlined: false
                                onClicked: git.stageAll()
                            }
                        }
                    }
                    Component {
                        id: fileRow
                        ItemDelegate {
                            id: row
                            readonly property var f: entry.modelData
                            readonly property string diffKey: "diff:" + (f.staged ? "1" : "0") + ":" + git.root + "/" + f.path
                            readonly property bool selected: Workspace.activeKey === diffKey
                            height: 30
                            leftInset: 8
                            rightInset: 8
                            leftPadding: 18
                            rightPadding: 12
                            topPadding: 0
                            bottomPadding: 0
                            // A soft pill, as the sidebar's rows.
                            background: Rectangle { radius: 8; color: row.selected ? Theme.rowSelected : row.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                            contentItem: RowLayout {
                                spacing: 8
                                Label { text: row.f.code; color: root.codeColor(row.f.code); font.family: Theme.mono; font.pixelSize: Theme.mpx(12); Layout.preferredWidth: 12 }
                                FileIcon { path: row.f.name; Layout.preferredWidth: 16; Layout.preferredHeight: 16 }
                                Label { text: row.f.name; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: row.selected ? Font.Medium : Font.Normal; elide: Text.ElideRight; Layout.maximumWidth: row.width * 0.55 }
                                Label { text: row.f.dir; color: Theme.textFaint; font.pixelSize: Theme.px(12); elide: Text.ElideLeft; Layout.fillWidth: true }
                                FlatButton {
                                    visible: row.hovered && !row.f.staged
                                    text: "↺"; font.pixelSize: Theme.px(12); outlined: false; padding: 2; accent: Theme.error
                                    ToolTip.visible: hovered; ToolTip.text: "Discard changes"
                                    onClicked: { confirmRevert.path = row.f.path; confirmRevert.open() }
                                }
                                FlatButton {
                                    visible: row.hovered
                                    text: row.f.staged ? "−" : "+"; font.pixelSize: Theme.px(13); outlined: false; padding: 2
                                    ToolTip.visible: hovered; ToolTip.text: row.f.staged ? "Unstage" : "Stage"
                                    onClicked: row.f.staged ? git.unstageFile(row.f.path) : git.stageFile(row.f.path)
                                }
                            }
                            onClicked: Workspace.openDiffTab(git.root + "/" + f.path, f.staged)
                        }
                    }
                }
            }

        // Result of the last action
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 18
            visible: git.message !== ""
            text: git.message
            color: git.error ? Theme.error : Theme.textMuted
            wrapMode: Text.Wrap
            maximumLineCount: 4
            elide: Text.ElideRight
            font.pixelSize: Theme.px(12)
        }

        // Commit
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            visible: git.isRepo
            spacing: 8
            TextArea {
                id: commitMessage
                objectName: "commitMessage"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(Math.max(implicitHeight, 56), 140)
                padding: 12
                rightPadding: 40
                placeholderText: git.stagedCount + git.unstagedCount === 0 ? "No changes to commit"
                    : repoList.repos.length > 0 ? "Commit message for " + git.root.split("/").pop() + " (Ctrl+Enter)" : "Commit message (Ctrl+Enter to commit)"
                placeholderTextColor: Theme.textFaint
                color: Theme.text
                wrapMode: TextEdit.Wrap
                font.pixelSize: Theme.px(13)
                background: Rectangle { radius: 12; color: Theme.raised; border.color: commitMessage.activeFocus ? Theme.accent : Theme.hairline }
                Keys.onPressed: event => {
                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
                        commitButton.clicked()
                        event.accepted = true
                    }
                }
                // Write the message with the model.
                Button {
                    id: writeButton
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.margins: 7
                    implicitWidth: 28
                    implicitHeight: 28
                    flat: true
                    enabled: !root.git.generating && root.git.stagedCount + root.git.unstagedCount > 0
                    readonly property string harnessToUse: root.harness === "omp" || root.harness === "pi" ? root.harness
                        : (Workspace.setting("default_harness") || "omp")
                    onClicked: root.git.generateMessage(harnessToUse, Workspace.setting(harnessToUse + "_path"), Workspace.setting("commit_model"), root.git.stagedCount > 0)
                    contentItem: Icon {
                        name: root.git.generating ? "loader-4" : "sparkling"
                        color: root.git.generating ? Theme.primary : writeButton.hovered ? Theme.primary : Theme.muted
                        opacity: writeButton.enabled || root.git.generating ? 1 : 0.4
                        RotationAnimation on rotation { running: root.git.generating; from: 0; to: 360; duration: 900; loops: Animation.Infinite }
                        onVisibleChanged: rotation = 0
                    }
                    background: Rectangle { radius: 6; color: writeButton.hovered ? Theme.hover : "transparent" }
                    ToolTip.visible: hovered
                    ToolTip.delay: 300
                    ToolTip.text: root.git.generating ? "Writing a commit message…"
                        : "Write a commit message for the " + (root.git.stagedCount > 0 ? "staged changes" : "changes") + " with " + harnessToUse
                }
                Connections {
                    target: root.git
                    function onGeneratedMessageChanged() { if (root.git.generatedMessage !== "") commitMessage.text = root.git.generatedMessage }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: git.stagedCount + " staged · " + git.unstagedCount + " unstaged"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                FlatButton {
                    id: commitButton
                    // Nothing staged: commit everything, as most git UIs do.
                    readonly property bool all: git.stagedCount === 0 && git.unstagedCount > 0
                    readonly property bool nothing: git.stagedCount + git.unstagedCount === 0
                    text: nothing ? "Nothing to commit" : all ? "Commit all" : "Commit"
                    accent: Theme.primary
                    enabled: !nothing && commitMessage.text.trim() !== "" && !git.busy
                    opacity: enabled ? 1 : 0.45
                    ToolTip.visible: hovered && !enabled && !nothing
                    ToolTip.text: "Write a commit message first"
                    onClicked: {
                        root.committing = true
                        all ? git.commitAll(commitMessage.text.trim()) : git.commit(commitMessage.text.trim())
                    }
                }
            }
        }
    }
}
