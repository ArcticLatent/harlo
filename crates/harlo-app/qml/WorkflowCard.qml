pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A workflow run the session is part of (its implementation, or one of its
// reviewers): its stages, attempt and what it waits for on one line, why
// it paused under it, and when opened the last check's output and the run's
// log. Pause, Resume and Stop act on the whole run.
Rectangle {
    id: root
    property ChatSession session
    readonly property var run: session && session.key !== "" && Workspace.workflowsRevision >= 0 && Workspace.orchestrationRevision >= 0
        ? (s => s === "" ? null : JSON.parse(s))(Workspace.workflowFor(session.key)) : null
    readonly property bool paused: !!run && run.state === "paused"
    readonly property bool over: !!run && (run.state === "done" || run.state === "stopped")
    property bool expanded: false
    /// Its branch is being pushed for a pull request: the Changes panel
    /// opens once it is.
    property bool publishing: false
    signal changesRequested()
    onRunChanged: if (publishing && run && run.published) { publishing = false; changesRequested() }

    visible: !!run
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    border.color: paused ? Theme.warning : Theme.border

    function stageColor(status) {
        switch (status) {
        case "done": return Theme.success
        case "active": return Theme.primary
        case "paused": return Theme.warning
        default: return Theme.muted
        }
    }
    function faded(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
    function toneColor(tone) {
        switch (tone) {
        case "good": return Theme.success
        case "bad": return Theme.error
        case "warn": return Theme.warning
        default: return Theme.muted
        }
    }
    function stateLine() {
        if (!run) return ""
        switch (run.state) {
        case "done": return qsTr("Done: %1").arg(run.doneText)
        case "stopped": return qsTr("Stopped")
        case "paused": return qsTr("Paused")
        default: return run.waiting
        }
    }

    ColumnLayout {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "node-tree"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.run ? qsTr("%1 · attempt %2").arg(root.run.workflow).arg(root.run.attempt) : ""
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
                font.bold: true
                elide: Text.ElideRight
            }
            FlatButton {
                objectName: "workflowPause"
                visible: !!root.run && !root.over
                text: root.paused ? qsTr("Resume") : qsTr("Pause")
                font.pixelSize: Theme.px(12)
                padding: 3
                onClicked: Workspace.workflowAction(root.run.id, root.paused ? "resume" : "pause")
            }
            FlatButton {
                objectName: "workflowStop"
                visible: !!root.run && !root.over
                text: qsTr("Stop")
                variant: "danger"
                font.pixelSize: Theme.px(12)
                padding: 3
                onClicked: Workspace.workflowAction(root.run.id, "stop")
            }
            Button {
                id: toggle
                objectName: "workflowToggle"
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                padding: 0
                onClicked: root.expanded = !root.expanded
                contentItem: Item { Icon { anchors.centerIn: parent; name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted; implicitWidth: 18; implicitHeight: 18 } }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
                Accessible.name: root.expanded ? qsTr("Hide the run's log") : qsTr("Show the run's log")
                ToolTip.visible: hovered
                ToolTip.text: Accessible.name
            }
        }

        // The stages, and what the run waits for (below them when narrow).
        Flow {
            id: stagesFlow
            Layout.fillWidth: true
            spacing: 8
            Row {
                objectName: "workflowStages"
                spacing: 4
                Repeater {
                    model: root.run ? root.run.stages : []
                    Row {
                        id: stage
                        required property var modelData
                        required property int index
                        spacing: 4
                        Icon {
                            visible: stage.index > 0
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-right-s"
                            color: Theme.muted
                            implicitWidth: 12
                            implicitHeight: 12
                        }
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            implicitWidth: stageLabel.implicitWidth + 12
                            implicitHeight: stageLabel.implicitHeight + 4
                            radius: height / 2
                            color: stage.modelData.status === "todo" ? "transparent" : root.faded(root.stageColor(stage.modelData.status), 0.16)
                            border.color: root.faded(root.stageColor(stage.modelData.status), 0.5)
                            Label {
                                id: stageLabel
                                anchors.centerIn: parent
                                text: stage.modelData.label
                                color: stage.modelData.status === "todo" ? Theme.muted : root.stageColor(stage.modelData.status)
                                font.pixelSize: Theme.px(11)
                            }
                        }
                    }
                }
            }
            Label {
                objectName: "workflowState"
                width: Math.min(implicitWidth, stagesFlow.width)
                text: root.stateLine()
                color: root.paused ? Theme.warning : root.run && root.run.state === "done" ? Theme.success : Theme.muted
                font.pixelSize: Theme.px(12)
                elide: Text.ElideRight
            }
        }

        // Why it paused, and what Resume does.
        PlainLabel {
            objectName: "workflowWhy"
            Layout.fillWidth: true
            visible: root.paused
            text: !root.run ? "" : /[.!?]$/.test(root.run.why) ? root.run.why : root.run.why + "."
            color: Theme.foreground
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }

        // A check running now: its output as it comes.
        Rectangle {
            id: liveBox
            objectName: "workflowLive"
            readonly property var live: root.run ? root.run.liveCheck : null
            Layout.fillWidth: true
            visible: !!live
            implicitHeight: liveCol.implicitHeight + 12
            radius: 6
            color: Theme.background
            border.color: Theme.border
            ColumnLayout {
                id: liveCol
                x: 8
                y: 6
                width: parent.width - 16
                spacing: 4
                PlainLabel {
                    text: liveBox.live ? qsTr("%1 running · %2 s").arg(liveBox.live.name).arg(liveBox.live.seconds) : ""
                    color: Theme.primary
                    font.pixelSize: Theme.px(11)
                    font.bold: true
                }
                PlainLabel {
                    objectName: "workflowLiveOutput"
                    Layout.fillWidth: true
                    visible: text !== ""
                    text: !liveBox.live ? "" : root.expanded ? liveBox.live.output : liveBox.live.output.split("\n").slice(-4).join("\n")
                    color: Theme.muted
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(11)
                    wrapMode: Text.WrapAnywhere
                }
            }
        }

        // The last log line, folded: what just happened.
        PlainLabel {
            objectName: "workflowLast"
            Layout.fillWidth: true
            visible: !root.expanded && !!root.run && root.run.log.length > 0 && !root.paused && root.run.state !== "done" && !root.run.liveCheck
            readonly property var entry: root.run && root.run.log.length > 0 ? root.run.log[root.run.log.length - 1] : null
            text: entry ? entry.text : ""
            color: entry ? root.toneColor(entry.tone) : Theme.muted
            font.pixelSize: Theme.px(12)
            elide: Text.ElideRight
            maximumLineCount: 1
        }

        // What to do with its work: once done, merge it or open a pull
        // request; its worktree can go once it's over (or paused).
        Flow {
            objectName: "workflowFinish"
            Layout.fillWidth: true
            visible: !!root.run && (root.over || root.paused)
            spacing: 6
            FlatButton {
                objectName: "workflowMerge"
                visible: !!root.run && root.run.state === "done" && !root.run.merged && !root.run.removed
                text: qsTr("Merge into %1").arg(root.run && root.run.base !== "" ? root.run.base : qsTr("its branch"))
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                padding: 4
                onClicked: Workspace.workflowAction(root.run.id, "merge")
            }
            FlatButton {
                objectName: "workflowPublish"
                visible: !!root.run && root.run.state === "done" && !root.run.merged && !root.run.removed
                text: root.run && root.run.published ? qsTr("Pull request…") : qsTr("Push for a pull request…")
                font.pixelSize: Theme.px(12)
                padding: 4
                onClicked: {
                    if (root.run.published) { root.changesRequested(); return }
                    root.publishing = true
                    Workspace.workflowAction(root.run.id, "publish")
                }
            }
            FlatButton {
                objectName: "workflowRemove"
                visible: !!root.run && !root.run.removed
                text: qsTr("Remove worktree")
                font.pixelSize: Theme.px(12)
                padding: 4
                onClicked: removeConfirm.open()
            }
            PlainLabel {
                visible: !!root.run && (root.run.merged || root.run.removed)
                text: !root.run ? "" : root.run.merged && root.run.removed ? qsTr("Merged, worktree removed") : root.run.merged ? qsTr("Merged into %1").arg(root.run.base) : qsTr("Worktree removed")
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                height: 26
                verticalAlignment: Text.AlignVCenter
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: root.expanded
            spacing: 6

            PlainLabel {
                Layout.fillWidth: true
                text: root.run ? [qsTr("On %1").arg(root.run.branch || qsTr("its worktree"))].concat(root.run.stages.map(s => s.label + ": " + s.what)).join("  ·  ") : ""
                color: Theme.muted
                font.pixelSize: Theme.px(11)
                wrapMode: Text.Wrap
            }

            // The last check's output.
            Rectangle {
                objectName: "workflowTest"
                Layout.fillWidth: true
                visible: !!root.run && !!root.run.lastCheck
                implicitHeight: testCol.implicitHeight + 12
                radius: 6
                color: Theme.background
                border.color: Theme.border
                ColumnLayout {
                    id: testCol
                    x: 8
                    y: 6
                    width: parent.width - 16
                    spacing: 4
                    PlainLabel {
                        readonly property var t: root.run && root.run.lastCheck ? root.run.lastCheck : null
                        text: !t ? "" : t.passed ? qsTr("%1 passed in %2 s").arg(t.name).arg(t.seconds) : t.exit === null ? qsTr("%1 couldn't finish").arg(t.name) : qsTr("%1 failed (exit %2) in %3 s").arg(t.name).arg(t.exit).arg(t.seconds)
                        color: t && t.passed ? Theme.success : Theme.error
                        font.pixelSize: Theme.px(11)
                        font.bold: true
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: root.run && root.run.lastCheck ? root.run.lastCheck.tail : ""
                        color: Theme.muted
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(11)
                        wrapMode: Text.WrapAnywhere
                    }
                }
            }

            // The run's log, newest last.
            Repeater {
                model: root.run ? root.run.log.slice(-8) : []
                RowLayout {
                    id: entry
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 8
                    Rectangle { Layout.alignment: Qt.AlignTop; Layout.topMargin: 5; implicitWidth: 6; implicitHeight: 6; radius: 3; color: root.toneColor(entry.modelData.tone) }
                    PlainLabel {
                        Layout.preferredWidth: 22
                        Layout.alignment: Qt.AlignTop
                        text: "#" + entry.modelData.attempt
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        text: entry.modelData.text
                        color: entry.modelData.tone === "info" ? Theme.muted : Theme.foreground
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

    AppDialog {
        id: removeConfirm
        objectName: "workflowRemoveConfirm"
        title: qsTr("Remove the run's worktree?")
        standardButtons: Dialog.Discard | Dialog.Cancel
        PlainLabel {
            width: 380
            wrapMode: Text.Wrap
            color: Theme.muted
            text: !root.run ? "" : (root.over ? "" : qsTr("The run stops. ")) + qsTr("Its folder goes, with any changes not committed, and its sessions close. Branch %1 stays%2.").arg(root.run.branch).arg(root.run.merged ? qsTr(" unless it's merged") : "")
        }
        onDiscarded: { Workspace.workflowAction(root.run.id, "remove"); close() }
    }
}
