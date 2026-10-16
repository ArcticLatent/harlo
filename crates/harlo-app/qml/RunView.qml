import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A multirun's overview: one card per lane (model, status, latest answer,
// changed files) with Open and Keep this.
Rectangle {
    id: root
    property string runId: ""
    color: Theme.background

    property var run: null
    function refresh() {
        const json = runId !== "" ? Workspace.runJson(runId) : ""
        run = json !== "" ? JSON.parse(json) : null
    }
    onRunIdChanged: refresh()
    onVisibleChanged: if (visible) refresh()
    Timer { interval: 2000; repeat: true; running: root.visible && root.runId !== ""; onTriggered: root.refresh() }

    readonly property var lanes: run ? run.lanes : []
    readonly property bool kept: run !== null && run.kept !== null && run.kept !== undefined
    function count(pred) { return lanes.filter(pred).length }
    function statusColor(s) { return Theme.statusColor(s === "saved" ? "" : s) }
    function statusText(s) {
        return s === "busy" ? "working" : s === "input" ? "needs you" : s === "starting" ? "starting" : s === "saved" || s === "live" || s === "idle" ? "turn ended" : s
    }

    AppDialog {
        id: confirmKeep
        property int idx: -1
        title: "Keep lane " + (root.lanes[idx] ? root.lanes[idx].letter : "") + "?"
        footer: AppButtonBox {
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            DialogButton { text: "Keep this lane"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        }
        Label {
            width: 420
            wrapMode: Text.Wrap
            color: Theme.muted
            text: "The other lanes are stopped and archived. Their worktrees and branches are removed after their work is saved under refs/harlo/runs/" + root.runId + "/ in the project's repository."
        }
        onAccepted: { Workspace.keepRunLane(root.runId, idx); root.refresh() }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: body.implicitHeight + 48
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        ColumnLayout {
            id: body
            x: Math.max(20, (flick.width - width) / 2)
            y: 24
            width: Math.min(flick.width - 40, 1100)
            spacing: 14

            Label { visible: root.run === null; text: "This run is gone."; color: Theme.muted }

            // Header
            RowLayout {
                visible: root.run !== null
                Layout.fillWidth: true
                spacing: 10
                Rectangle {
                    implicitWidth: 34
                    implicitHeight: 34
                    radius: 8
                    color: Theme.selected
                    Icon { anchors.centerIn: parent; name: "node-tree"; color: Theme.primary; implicitWidth: 18; implicitHeight: 18 }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Label { text: root.run ? root.run.title : ""; color: Theme.foreground; font.pixelSize: Theme.px(18); font.bold: true; elide: Text.ElideRight; Layout.fillWidth: true }
                    Label {
                        text: root.kept ? "Lane " + root.lanes.find(l => l.idx === root.run.kept).letter + " kept"
                            : [root.count(l => l.status === "busy" || l.status === "starting") + " working",
                               root.count(l => l.status === "input") + " need you",
                               root.count(l => l.status === "saved" || l.status === "idle" || l.status === "live") + " ended"].join(" · ")
                        color: Theme.muted
                        font.pixelSize: Theme.px(12)
                    }
                }
                FlatButton {
                    visible: root.kept
                    text: "Dismiss run"
                    onClicked: Workspace.dismissRun(root.runId)
                    ToolTip.visible: hovered
                    ToolTip.text: "Forget this run; its sessions stay"
                }
            }

            // Prompt
            Rectangle {
                visible: root.run !== null && root.run.prompt !== ""
                Layout.fillWidth: true
                implicitHeight: promptLabel.implicitHeight + 20
                radius: 8
                color: Theme.userBubble
                Label {
                    id: promptLabel
                    x: 12
                    y: 10
                    width: parent.width - 24
                    text: root.run ? root.run.prompt : ""
                    color: Theme.foreground
                    font.pixelSize: Theme.px(13)
                    wrapMode: Text.Wrap
                    maximumLineCount: 4
                    elide: Text.ElideRight
                }
            }

            // Lanes
            GridLayout {
                Layout.fillWidth: true
                columns: body.width > 760 ? 2 : 1
                columnSpacing: 12
                rowSpacing: 12
                Repeater {
                    model: root.lanes
                    Rectangle {
                        id: card
                        required property var modelData
                        readonly property var l: modelData
                        readonly property bool isKept: root.kept && root.run.kept === l.idx
                        readonly property bool dropped: root.kept && !isKept
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        Layout.alignment: Qt.AlignTop
                        implicitHeight: cardCol.implicitHeight + 24
                        radius: 10
                        color: Theme.elevated
                        border.color: isKept ? Theme.success : l.status === "input" ? Theme.warning : Theme.border
                        border.width: isKept ? 2 : 1
                        opacity: dropped ? 0.55 : 1
                        ColumnLayout {
                            id: cardCol
                            x: 12
                            y: 12
                            width: parent.width - 24
                            spacing: 8
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Rectangle {
                                    implicitWidth: 24
                                    implicitHeight: 24
                                    radius: 6
                                    color: Theme.selected
                                    Label { anchors.centerIn: parent; text: card.l.letter; color: Theme.primary; font.pixelSize: Theme.px(13); font.bold: true }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Label { Layout.fillWidth: true; text: card.l.model || "default model"; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true; elide: Text.ElideRight }
                                    Label {
                                        Layout.fillWidth: true
                                        text: [Theme.harnessLabel(card.l.harness), card.l.branch].filter(x => x).join(" · ")
                                        color: Theme.muted
                                        font.pixelSize: Theme.px(11)
                                        font.family: Theme.mono
                                        elide: Text.ElideRight
                                    }
                                }
                                Rectangle {
                                    implicitWidth: statusLabel.implicitWidth + 14
                                    implicitHeight: 20
                                    radius: 10
                                    color: Qt.rgba(root.statusColor(card.l.status).r, root.statusColor(card.l.status).g, root.statusColor(card.l.status).b, 0.15)
                                    Label { id: statusLabel; anchors.centerIn: parent; text: card.isKept ? "kept" : card.dropped ? "archived" : root.statusText(card.l.status); color: card.isKept ? Theme.success : root.statusColor(card.l.status); font.pixelSize: Theme.px(11); font.bold: true }
                                }
                            }
                            // Latest answer
                            Rectangle {
                                Layout.fillWidth: true
                                implicitHeight: Math.min(answer.implicitHeight + 16, 220)
                                radius: 6
                                color: Theme.background
                                clip: true
                                Label {
                                    id: answer
                                    x: 10
                                    y: 8
                                    width: parent.width - 20
                                    text: card.l.answer !== "" ? card.l.answer : (card.l.status === "starting" || card.l.status === "busy" ? "Working…" : "No answer yet.")
                                    color: card.l.answer !== "" ? Theme.foreground : Theme.muted
                                    font.pixelSize: Theme.px(12)
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Label {
                                    Layout.fillWidth: true
                                    text: card.l.worktreeGone ? "worktree removed" : card.l.changes >= 0 ? card.l.changes + (card.l.changes === 1 ? " changed file" : " changed files") : ""
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(11)
                                }
                                FlatButton {
                                    visible: card.l.key !== "" && !card.dropped
                                    text: "Open"
                                    font.pixelSize: Theme.px(12)
                                    onClicked: Workspace.open(card.l.key)
                                }
                                DialogButton {
                                    visible: !root.kept && card.l.status !== "starting"
                                    text: "Keep this"
                                    accent: true
                                    onClicked: { confirmKeep.idx = card.l.idx; confirmKeep.open() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
