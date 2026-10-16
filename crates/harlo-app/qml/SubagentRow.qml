import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// One subagent (omp's, Claude's, OpenCode's): status, name, task, the tool
// it is running, and View / Steer / Cancel (where its provider can).
// Used by the subagent dock and under task tool rows.
Rectangle {
    id: agentRow
    property ChatSession session
    property var a: ({})
    signal viewRequested(var agent, bool steer)
    function statusColor(s) {
        return s === "running" || s === "pending" ? Theme.primary : s === "completed" ? Theme.success
             : s === "failed" ? Theme.error : Theme.muted
    }
    function toolLine(a) {
        const p = a.progress || {}
        if (a.status !== "running" || !p.currentTool) return ""
        const args = p.currentToolIntent || (p.currentToolArgs ? (p.currentToolArgs.command || p.currentToolArgs.path || p.currentToolArgs.pattern || p.currentToolArgs.query || "") : "")
        return p.currentTool + (args ? ": " + String(args).replace(/\s+/g, " ").slice(0, 80) : "")
    }
    readonly property bool live: a.status === "running" || a.status === "pending"
    implicitHeight: rowCol.implicitHeight + 12
    radius: 7
    color: rowHover.hovered ? Theme.hover : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
    HoverHandler { id: rowHover }
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 4
        spacing: 10
        // Status: pulsing while running.
        Rectangle {
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: 6
            implicitWidth: 8
            implicitHeight: 8
            radius: 4
            color: agentRow.statusColor(agentRow.a.status)
            SequentialAnimation on opacity {
                running: agentRow.live
                loops: Animation.Infinite
                NumberAnimation { from: 1; to: 0.3; duration: 700 }
                NumberAnimation { from: 0.3; to: 1; duration: 700 }
                onRunningChanged: if (!running) parent.opacity = 1
            }
        }
        ColumnLayout {
            id: rowCol
            Layout.fillWidth: true
            spacing: 2
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Label {
                    Layout.maximumWidth: agentRow.width * 0.45
                    text: agentRow.a.name || agentRow.a.id
                    color: Theme.foreground
                    font.pixelSize: Theme.px(13)
                    font.bold: true
                    elide: Text.ElideRight
                }
                Rectangle {
                    implicitWidth: agentLabel.implicitWidth + 10
                    implicitHeight: 16
                    radius: 4
                    color: Theme.subtle
                    border.color: Theme.border
                    Label { id: agentLabel; anchors.centerIn: parent; text: agentRow.a.agent || "task"; color: Theme.muted; font.pixelSize: Theme.px(10) }
                }
                Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    // (Named after its description: the task instead.)
                    text: (agentRow.a.name && agentRow.a.name === agentRow.a.description ? "" : agentRow.a.description) || agentRow.a.task || ""
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                }
                Label {
                    visible: !agentRow.live
                    text: agentRow.a.status
                    color: agentRow.statusColor(agentRow.a.status)
                    font.pixelSize: Theme.px(11)
                }
            }
            Label {
                Layout.fillWidth: true
                visible: text !== ""
                text: { const t = agentRow.toolLine(agentRow.a); return t !== "" ? "↳ " + t : "" }
                color: Theme.muted
                font.family: Theme.mono
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
            }
        }
        FlatButton {
            // Narrow and running: Steer opens the same view, and Cancel
            // must stay in reach.
            // (A harlo session an agent started: opens as its tab.)
            objectName: "subagentView"
            visible: (!!agentRow.a.sessionFile || !!agentRow.a.openKey) && !(agentRow.live && agentRow.width < 380)
            text: "View"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.a.openKey ? Workspace.open(agentRow.a.openKey) : agentRow.viewRequested(agentRow.a, false)
        }
        FlatButton {
            visible: agentRow.live && agentRow.a.canSteer !== false
            text: "Steer"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.viewRequested(agentRow.a, true)
        }
        FlatButton {
            objectName: "subagentCancel"
            visible: agentRow.live && agentRow.a.canCancel !== false
            text: "Cancel"
            accent: Theme.error
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.a.openKey ? Workspace.stopSession(agentRow.a.openKey) : agentRow.session.cancelSubagent(agentRow.a.id)
        }
    }
}
