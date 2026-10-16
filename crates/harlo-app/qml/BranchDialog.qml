import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The branches of a session (each edit of an earlier message starts one):
// pick one to preview it read-only, and continue it in a new session.
Popup {
    id: root
    property string file: ""          // the session file
    property string harness: ""
    property string cwd: ""
    property var branches: []
    property int selected: 0
    readonly property var current: branches[selected] || null

    function show(path, harnessName, folder) {
        file = path
        harness = harnessName
        cwd = folder
        // (Read off the UI thread: a remote session's over SSH.)
        branches = []
        open()
        Workspace.requestSessionBranches(path)
    }
    Connections {
        target: Workspace
        function onSessionBranchesReady(path, json) {
            if (path !== root.file || !root.visible) return
            root.branches = JSON.parse(json)
            root.pick(root.branches.length > 1 ? 1 : 0)
        }
    }
    function pick(i) {
        selected = i
        const b = branches[i]
        if (!b) return
        const preview = Workspace.branchPreviewFile(file, b.leaf)
        if (preview !== "") previewSession.bindFile(preview, harness === "pi" ? "pi" : "omp", cwd)
    }
    signal continued(string key)
    property alias previewSession: previewSession

    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(1000, parent.width - 48)
    height: Math.min(720, parent.height - 48)
    padding: 0
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }
    onClosed: previewSession.bind("")

    ChatSession { id: previewSession }

    function ago(ts) { const t = Date.parse(ts); return isNaN(t) ? "" : Theme.ago(t / 1000, Date.now()) }

    contentItem: RowLayout {
        spacing: 0
        // Branch list
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 300
            color: Theme.sidebar
            radius: 11
            Rectangle { anchors.right: parent.right; width: 12; height: parent.height; color: parent.color }
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.border }
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                anchors.rightMargin: 13
                spacing: 4
                RowLayout {
                    spacing: 8
                    Icon { name: "git-branch"; color: Theme.primary; implicitWidth: 16; implicitHeight: 16 }
                    Label { text: "Branches"; color: Theme.foreground; font.pixelSize: Theme.px(15); font.bold: true }
                    Label { text: root.branches.length; color: Theme.muted; font.pixelSize: Theme.px(12) }
                }
                Label {
                    Layout.fillWidth: true
                    Layout.bottomMargin: 6
                    text: "Each edit of an earlier message starts a branch. The highlighted one is where the session is now."
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                    wrapMode: Text.Wrap
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 4
                    model: root.branches
                    delegate: Rectangle {
                        id: branchRow
                        required property var modelData
                        required property int index
                        readonly property bool chosen: root.selected === index
                        width: ListView.view.width
                        implicitHeight: branchCol.implicitHeight + 14
                        radius: 7
                        color: chosen ? Theme.selected : branchHover.hovered ? Theme.hover : "transparent"
                        border.color: chosen ? Theme.primary : "transparent"
                        HoverHandler { id: branchHover }
                        TapHandler { onTapped: root.pick(branchRow.index) }
                        ColumnLayout {
                            id: branchCol
                            x: 10
                            y: 7
                            width: parent.width - 20
                            spacing: 2
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Rectangle {
                                    visible: branchRow.modelData.current
                                    implicitWidth: nowLabel.implicitWidth + 10
                                    implicitHeight: 16
                                    radius: 8
                                    color: Qt.rgba(Theme.success.r, Theme.success.g, Theme.success.b, 0.16)
                                    Label { id: nowLabel; anchors.centerIn: parent; text: "current"; color: Theme.success; font.pixelSize: Theme.px(10); font.bold: true }
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: branchRow.modelData.messages + " messages · " + root.ago(branchRow.modelData.timestamp)
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(11)
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                visible: branchRow.modelData.divergesAt !== ""
                                text: "Branches at: " + branchRow.modelData.divergesAt
                                color: Theme.foreground
                                font.pixelSize: Theme.px(12)
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                text: "Last: " + branchRow.modelData.lastUser
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }
        // Preview
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 12
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    text: root.current ? (root.current.current ? "The current branch" : "Preview (read-only)") : ""
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
                DialogButton {
                    visible: root.current !== null && !root.current.current
                    text: "Continue in a new session"
                    accent: true
                    onClicked: {
                        const key = Workspace.continueBranch(root.file, root.current.leaf)
                        root.close()
                        if (key !== "") root.continued(key)
                    }
                    ToolTip.visible: hovered
                    ToolTip.text: "Copies this branch into a new session; the original stays as it is"
                }
                Button {
                    id: closeButton
                    flat: true
                    implicitWidth: 30
                    implicitHeight: 30
                    onClicked: root.close()
                    contentItem: Icon { name: "close"; color: closeButton.hovered ? Theme.foreground : Theme.muted }
                    background: Rectangle { radius: 6; color: closeButton.hovered ? Theme.hover : "transparent" }
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
            TranscriptView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                session: previewSession
            }
        }
    }
}
