pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Working-tree changes of the active session's folder: files (clicking one
// shows its changes side by side in the diff tab) and a commit box.
Rectangle {
    id: root
    // (Its window shown: not minimized, not in the tray. Polling waits
    // otherwise; Item.visible doesn't change when the window hides.)
    readonly property bool windowShown: Window.visibility !== Window.Minimized && Window.visibility !== Window.Hidden
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
    // Commit and push: the push follows a commit that succeeded.
    property bool pushAfterCommit: false
    // Test hook.
    property alias commitText: commitMessage.text
    Connections {
        target: root.git
        function onBusyChanged() {
            if (!root.git.busy && root.committing) {
                root.committing = false
                const push = root.pushAfterCommit
                root.pushAfterCommit = false
                if (!root.git.error) {
                    commitMessage.text = ""
                    if (push)
                        root.git.push()
                }
            }
        }
    }

    readonly property var stagedEntries: entries(true)
    readonly property var unstagedEntries: entries(false)
    // The list's rows, kept by key and changed in place as git's status
    // changes: a new array would make every row again and scroll to the top.
    ListModel { id: fileRows; objectName: "changesRows" }
    function syncRows() {
        const row = (key, header, count, f) => ({ key: key, header: header, count: count, path: f ? f.path : "", name: f ? f.name : "",
                                                  dir: f ? f.dir : "", code: f ? f.code : "", staged: f ? f.staged : false })
        const want = (stagedEntries.length ? [row("h:Staged", "Staged", stagedEntries.length, null)] : [])
            .concat(stagedEntries.map(f => row("s:" + f.path, "", 0, f)))
            .concat(unstagedEntries.length ? [row("h:Changes", "Changes", unstagedEntries.length, null)] : [])
            .concat(unstagedEntries.map(f => row("u:" + f.path, "", 0, f)))
        for (let i = 0; i < want.length; i++) {
            const w = want[i]
            let j = i
            while (j < fileRows.count && fileRows.get(j).key !== w.key) j++
            if (j === fileRows.count) { fileRows.insert(i, w); continue }
            if (j !== i) fileRows.move(j, i, 1)
            const m = fileRows.get(i)
            for (const k in w) if (m[k] !== w[k]) fileRows.setProperty(i, k, w[k])
        }
        if (fileRows.count > want.length) fileRows.remove(want.length, fileRows.count - want.length)
    }
    onStagedEntriesChanged: Qt.callLater(syncRows)
    onUnstagedEntriesChanged: Qt.callLater(syncRows)
    Component.onCompleted: syncRows()

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
        onDiscarded: { root.git.revertFile(path); close() }
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
                DialogButton { visible: confirmSwitch.dirty; text: "Bring changes along"; onClicked: { root.git.switchBranch(confirmSwitch.name, false, confirmSwitch.remote, false); confirmSwitch.close() } }
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
        onAccepted: root.git.switchBranch(name, false, remote, dirty)
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
            subtitle: root.git.directory.split("/").pop() + (repoList.repos.length > 0 && root.git.root !== "" ? " / " + root.git.root.split("/").pop() : "")
            onCloseRequested: root.closeRequested()
            PanelButton { iconName: "refresh"; tip: "Refresh"; onClicked: root.git.refresh(true) }
        }

        // A folder of repositories (not one itself): each, with its branch
        // and how many files it changed; the rest of the panel shows the one
        // picked here. Those without changes fold away.
        ColumnLayout {
            id: repoList
            objectName: "repoList"
            readonly property var repos: root.git.reposJson !== "" ? JSON.parse(root.git.reposJson) : []
            /// Each one's open pull request (and its checks), by root.
            readonly property var requests: root.git.requestsJson !== "" ? JSON.parse(root.git.requestsJson) : ({})
            // Changes, or an open pull request: shown; the rest fold away.
            readonly property var changed: repos.filter(r => r.changes > 0 || !!requests[r.root])
            readonly property var clean: repos.filter(r => r.changes === 0 && !requests[r.root])
            onReposChanged: if (repos.length > 0 && root.visible) root.git.loadAllRequests()
            // Every repository's pull request: read again now and then; those
            // whose checks run are watched (harlo says when they finish).
            Timer {
                interval: 60000
                repeat: true
                running: root.visible && root.windowShown && repoList.repos.length > 0
                onTriggered: root.git.loadAllRequests()
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
                    readonly property bool active: modelData.root === root.git.root
                    objectName: "repo-" + modelData.name
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    implicitHeight: 30
                    verticalPadding: 0  // (the style's 12 px would push the content low)
                    leftPadding: 8
                    rightPadding: 8
                    onClicked: root.git.selectRepo(modelData.root)
                    background: Rectangle {
                        radius: 8
                        color: repoRow.active ? Theme.rowSelected : repoRow.hovered ? Theme.rowHover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                    }
                    contentItem: RowLayout {
                        spacing: 7
                        Icon { name: "git-repository"; color: repoRow.active ? Theme.accent : Theme.textMuted; implicitWidth: 14; implicitHeight: 14 }
                        PlainLabel {
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
                verticalPadding: 0  // (the style's 12 px would push the content low)
                leftPadding: 8
                onClicked: repoList.showClean = !repoList.showClean
                background: Rectangle { radius: 8; color: parent.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                contentItem: Label {
                    text: repoList.showClean ? "Hide those without changes" : repoList.clean.length + (repoList.clean.length === 1 ? " without changes" : " without changes") + "…"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                    verticalAlignment: Text.AlignVCenter
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
            visible: root.git.isRepo
            spacing: 6
            // The branch: icon, name (elides first), ahead/behind, a chevron.
            Rectangle {
                id: branchChip
                objectName: "branchChip"
                readonly property string text: (root.git.branch !== "" ? root.git.branch : "detached HEAD") + (root.git.ahead > 0 ? "  ↑" + root.git.ahead : "") + (root.git.behind > 0 ? "  ↓" + root.git.behind : "")
                Layout.fillWidth: true
                Layout.minimumWidth: 52
                Layout.preferredWidth: implicitWidth
                Layout.maximumWidth: implicitWidth
                visible: root.git.isRepo
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
                    PlainLabel {
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
                TapHandler { enabled: !root.git.busy; onTapped: picker.opened ? picker.close() : picker.open() }
            }
            // Stashes, with how many there are.
            Rectangle {
                objectName: "stashButton"
                visible: root.git.isRepo
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
                    Label { anchors.verticalCenter: parent.verticalCenter; visible: root.git.stashCount > 0; text: root.git.stashCount; color: Theme.text; font.pixelSize: Theme.px(11.5) }
                }
                HoverHandler { id: stashHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { id: stashTap; onTapped: stashes.open() }
                ToolTip.visible: stashHover.hovered
                ToolTip.text: root.git.stashCount > 0 ? "Stashes (" + root.git.stashCount + ")" : "Stash changes"
            }
            Item { Layout.fillWidth: true }
            BusyIndicator { running: root.git.busy; visible: root.git.busy; implicitWidth: 18; implicitHeight: 18 }
            FlatButton { text: "Pull"; font.pixelSize: Theme.px(11); visible: root.git.isRepo && root.git.upstream !== ""; enabled: !root.git.busy; onClicked: root.git.pull() }
            FlatButton {
                text: root.git.upstream === "" ? "Publish" : "Push"
                font.pixelSize: Theme.px(11)
                visible: root.git.isRepo
                enabled: !root.git.busy
                onClicked: root.git.push()
            }
        }

        // The branch's pull request (merge request on GitLab): open it, or
        // open a new one. (Pushed branches other than the main one.)
        RowLayout {
            id: requestRow
            objectName: "requestRow"
            readonly property var req: root.git.requestJson !== "" ? JSON.parse(root.git.requestJson) : ({ state: "none" })
            readonly property bool has: (req.url || "") !== ""
            // (While it's looked up again, as the panel opens: what it
            // knew stays, rather than the row going for a moment.)
            readonly property bool known: req.state === "ready" || req.state === "creating" || req.state === "loading" && (req.label || "") !== ""
            visible: root.git.isRepo && root.git.upstream !== "" && known && req.head !== req.base
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
                // (Creating one waits for the look-up.)
                enabled: requestRow.has || requestRow.req.state === "ready"
                onClicked: requestRow.has ? Qt.openUrlExternally(requestRow.req.url) : requestDialog.show()
            }
        }
        // Its CI checks: running, passed, failed (send what failed to the agent).
        RowLayout {
            id: checksRow
            objectName: "checksRow"
            readonly property var checks: requestRow.req.checks || null
            readonly property string ciState: checks ? checks.state : "none"
            property bool sending: false
            visible: requestRow.visible && requestRow.has && ciState !== "none"
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 2
            spacing: 8
            Icon {
                name: checksRow.ciState === "passing" ? "checkbox-circle" : checksRow.ciState === "failing" ? "close-circle" : "loader-4"
                color: checksRow.ciState === "passing" ? Theme.success : checksRow.ciState === "failing" ? Theme.error : Theme.warning
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
                    : checksRow.ciState === "failing" ? "Failed: " + checksRow.checks.failing.map(f => f.name).join(", ")
                    : checksRow.ciState === "pending" ? "Checks running · " + checksRow.checks.passed + " of " + (checksRow.checks.passed + checksRow.checks.pending) + " done"
                    : "Checks passed"
                ToolTip.delay: 400
                color: checksRow.ciState === "failing" ? Theme.error : Theme.textMuted
                font.pixelSize: Theme.px(12)
                ToolTip.visible: summaryHover.hovered
                ToolTip.text: checksRow.checks ? checksRow.checks.summary : ""
                HoverHandler { id: summaryHover }
            }
            FlatButton {
                objectName: "ciSend"
                visible: checksRow.ciState === "failing"
                enabled: root.canSend && !checksRow.sending
                text: checksRow.sending ? "Reading the logs…" : "Send to the agent"
                font.pixelSize: Theme.px(11.5)
                accent: Theme.primary
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Send the failed checks, with the end of their logs, to this session's agent to fix"
                onClicked: { checksRow.sending = true; root.git.reportFailures() }
            }
        }
        Connections {
            target: root.git
            function onCiReportChanged() {
                if (root.git.ciReport !== "" && checksRow.sending) {
                    checksRow.sending = false
                    root.sendToAgent(root.git.ciReport)
                }
            }
        }
        // While checks run: read them again now and then, and have harlo
        // tell you when they finish (even with this panel closed).
        Timer {
            interval: 20000
            repeat: true
            running: root.visible && root.windowShown && checksRow.ciState === "pending"
            onTriggered: root.git.loadRequest()
        }
        Connections {
            target: root.git
            function onRequestJsonChanged() {
                const r = requestRow.req
                if (r.state === "ready" && r.url && r.checks && r.checks.state === "pending")
                    Workspace.watchRequest(JSON.stringify({ service: r.service, host: r.host, repo: r.repo, url: r.url, number: r.number, word: r.word, title: r.title }))
            }
        }
        // (Read when the branch or its upstream changes, a moment later.)
        Timer { id: requestTimer; interval: 500; onTriggered: root.git.loadRequest() }
        Connections {
            target: root.git
            function onBranchChanged() { requestTimer.restart() }
            function onUpstreamChanged() { requestTimer.restart() }
            function onRootChanged() { requestTimer.restart() }
        }

        Label {
            Layout.fillWidth: true
            Layout.margins: 18
            visible: !root.git.isRepo && repoList.repos.length === 0
            wrapMode: Text.Wrap
            color: Theme.muted
            text: root.git.directory === "" ? "No session selected." : "Not a git repository, and none inside it:\n" + root.git.directory
        }

        // File lists
        ListView {
                id: fileList
                objectName: "changesFiles"
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: root.git.isRepo
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                model: fileRows
                footer: Label {
                    visible: root.git.files.length === 0
                    width: fileList.width
                    leftPadding: 18
                    topPadding: 12
                    text: "No changes."
                    color: Theme.textMuted
                }
                delegate: Loader {
                    id: entry
                    // (The row's roles; a header's path is empty. Kept as
                    // their own properties, they hold their values while a
                    // removed row goes.)
                    required property string header
                    required property int count
                    required property string path
                    required property string name
                    required property string dir
                    required property string code
                    required property bool staged
                    width: ListView.view.width
                    // Fixed height: the Loader sizes its item to this.
                    height: header !== "" ? 34 : 30
                    sourceComponent: header !== "" ? sectionHeader : fileRow
                    Component {
                        id: sectionHeader
                        RowLayout {
                            height: 34
                            // A section label, in sentence case (as Context's).
                            Label {
                                Layout.leftMargin: 18
                                Layout.alignment: Qt.AlignBottom
                                bottomPadding: 6
                                text: entry.header + " · " + entry.count
                                color: Theme.label
                                font.pixelSize: Theme.px(12)
                                font.weight: Font.Medium
                            }
                            Item { Layout.fillWidth: true }
                            FlatButton {
                                visible: entry.header === "Changes"
                                Layout.rightMargin: 10
                                Layout.alignment: Qt.AlignBottom
                                text: "Stage all"
                                font.pixelSize: Theme.px(11.5)
                                outlined: false
                                onClicked: root.git.stageAll()
                            }
                        }
                    }
                    Component {
                        id: fileRow
                        ItemDelegate {
                            id: row
                            readonly property string diffKey: "diff:" + (entry.staged ? "1" : "0") + ":" + root.git.root + "/" + entry.path
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
                                Label { text: entry.code; color: root.codeColor(entry.code); font.family: Theme.mono; font.pixelSize: Theme.mpx(12); Layout.preferredWidth: 12 }
                                FileIcon { path: entry.name }
                                Label { text: entry.name; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: row.selected ? Font.Medium : Font.Normal; elide: Text.ElideRight; Layout.maximumWidth: row.width * 0.55 }
                                Label { text: entry.dir; color: Theme.textFaint; font.pixelSize: Theme.px(12); elide: Text.ElideLeft; Layout.fillWidth: true }
                                FlatButton {
                                    visible: row.hovered && !entry.staged
                                    text: "↺"; font.pixelSize: Theme.px(12); outlined: false; padding: 2; accent: Theme.error
                                    ToolTip.visible: hovered; ToolTip.text: "Discard changes"
                                    onClicked: { confirmRevert.path = entry.path; confirmRevert.open() }
                                }
                                FlatButton {
                                    visible: row.hovered
                                    text: entry.staged ? "−" : "+"; font.pixelSize: Theme.px(13); outlined: false; padding: 2
                                    ToolTip.visible: hovered; ToolTip.text: entry.staged ? "Unstage" : "Stage"
                                    onClicked: entry.staged ? root.git.unstageFile(entry.path) : root.git.stageFile(entry.path)
                                }
                            }
                            onClicked: Workspace.openDiffTab(root.git.root + "/" + entry.path, entry.staged)
                        }
                    }
                }
            }

        // Result of the last action
        PlainLabel {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 18
            visible: root.git.message !== ""
            text: root.git.message
            color: root.git.error ? Theme.error : Theme.textMuted
            wrapMode: Text.Wrap
            maximumLineCount: 4
            elide: Text.ElideRight
            font.pixelSize: Theme.px(12)
        }

        // Commit
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            visible: root.git.isRepo
            spacing: 8
            TextArea {
                id: commitMessage
                objectName: "commitMessage"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(Math.max(implicitHeight, 56), 140)
                padding: 12
                rightPadding: 40
                placeholderText: root.git.stagedCount + root.git.unstagedCount === 0 ? "No changes to commit"
                    : repoList.repos.length > 0 ? "Commit message for " + root.git.root.split("/").pop() + " (Ctrl+Enter)" : "Commit message (Ctrl+Enter to commit)"
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
                    objectName: "commitWrite"
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.margins: 7
                    implicitWidth: 28
                    implicitHeight: 28
                    flat: true
                    enabled: !root.git.generating && root.git.stagedCount + root.git.unstagedCount > 0
                    onClicked: root.git.generateMessage(Workspace.setting("harlo_path"), Workspace.setting("commit_model"), root.git.stagedCount > 0)
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
                        : "Write a commit message for the " + (root.git.stagedCount > 0 ? "staged changes" : "changes") + " with Harlo"
                }
                Connections {
                    target: root.git
                    function onGeneratedMessageChanged() { if (root.git.generatedMessage !== "") commitMessage.text = root.git.generatedMessage }
                }
            }
            RowLayout {
                id: commitRow
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Item {
                    Layout.fillWidth: true
                    visible: !countsLabel.visible
                }
                Label {
                    id: countsLabel
                    Layout.fillWidth: true
                    // (Narrow: the buttons keep the room; the list above
                    // already counts the changes.)
                    visible: commitRow.width - commitButton.implicitWidth - commitRow.spacing - (commitPushButton.visible ? commitPushButton.implicitWidth + commitRow.spacing : 0) >= implicitWidth
                    elide: Text.ElideRight
                    text: root.git.stagedCount + " staged · " + root.git.unstagedCount + " unstaged"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                FlatButton {
                    id: commitPushButton
                    objectName: "commitPush"
                    visible: !commitButton.nothing
                    text: root.git.upstream === "" ? "Commit & publish" : "Commit & push"
                    enabled: commitButton.enabled
                    opacity: enabled ? 1 : 0.45
                    ToolTip.visible: hovered && enabled
                    ToolTip.text: root.git.upstream === "" ? "Commit, then publish this branch to its remote" : "Commit, then push to " + root.git.upstream
                    onClicked: {
                        root.pushAfterCommit = true
                        commitButton.commit()
                    }
                }
                FlatButton {
                    id: commitButton
                    // Nothing staged: commit everything, as most git UIs do.
                    readonly property bool all: root.git.stagedCount === 0 && root.git.unstagedCount > 0
                    readonly property bool nothing: root.git.stagedCount + root.git.unstagedCount === 0
                    function commit() {
                        root.committing = true
                        all ? root.git.commitAll(commitMessage.text.trim()) : root.git.commit(commitMessage.text.trim())
                    }
                    text: nothing ? "Nothing to commit" : all ? "Commit all" : "Commit"
                    accent: Theme.primary
                    enabled: !nothing && commitMessage.text.trim() !== "" && !root.git.busy
                    opacity: enabled ? 1 : 0.45
                    ToolTip.visible: hovered && !enabled && !nothing
                    ToolTip.text: "Write a commit message first"
                    onClicked: commit()
                }
            }
            // Committed but not pushed: said in words, with Push beside it.
            RowLayout {
                objectName: "unpushed"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                visible: root.git.upstream !== "" && root.git.ahead > 0
                spacing: 8
                Icon {
                    name: "git-commit"
                    color: Theme.warning
                    implicitWidth: 15
                    implicitHeight: 15
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: (root.git.ahead === 1 ? "1 commit not pushed" : root.git.ahead + " commits not pushed")
                        + (root.git.behind > 0 ? ", " + root.git.behind + " to pull" : "")
                    color: Theme.text
                    font.pixelSize: Theme.px(12)
                }
                FlatButton {
                    objectName: "unpushedPush"
                    text: "Push"
                    accent: Theme.primary
                    enabled: !root.git.busy
                    onClicked: root.git.push()
                }
            }
        }
    }
}
