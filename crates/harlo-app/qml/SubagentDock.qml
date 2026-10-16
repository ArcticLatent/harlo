import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// omp subagents of the session (task tool): live status, current tool, and
// View / Steer / Cancel. A summary line until opened (the transcript
// shows each subagent under the tool call that started it).
Rectangle {
    id: root
    property ChatSession session
    signal viewRequested(var agent, bool steer)

    // Its agent's own subagents, and the harlo sessions it started (orchestration).
    readonly property var agents: (session && session.subagentsJson !== "" ? JSON.parse(session.subagentsJson) : [])
        .concat(session && session.key !== "" && Workspace.orchestrationRevision >= 0 ? JSON.parse(Workspace.orchestratedJson(session.key)) : [])
    readonly property int running: agents.filter(a => a.status === "running" || a.status === "pending").length
    // Collapsed to its summary line: the details sit under the task tool
    // row in the transcript; open it to manage them from here.
    property bool expanded: false

    visible: agents.length > 0
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    border.color: running > 0 ? Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.6) : Theme.border

    ColumnLayout {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 6

        // Header
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "node-tree"; color: root.running > 0 ? Theme.primary : Theme.muted; implicitWidth: 15; implicitHeight: 15 }
            Label {
                Layout.fillWidth: true
                text: "Subagents · " + (root.running > 0 ? root.running + " running" : "all done")
                    + (root.agents.length - root.running > 0 && root.running > 0 ? " · " + (root.agents.length - root.running) + " finished" : "")
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
                font.bold: true
            }
            Label {
                visible: root.session && root.session.subagentMessage !== ""
                text: root.session ? root.session.subagentMessage : ""
                color: Theme.muted
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
                Layout.maximumWidth: 260
            }
            Button {
                id: toggle
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                onClicked: root.expanded = !root.expanded
                contentItem: Icon { name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
            }
        }

        Repeater {
            model: root.expanded ? root.agents : []
            SubagentRow {
                required property var modelData
                Layout.fillWidth: true
                a: modelData
                session: root.session
                onViewRequested: (agent, steer) => root.viewRequested(agent, steer)
            }
        }
    }
}
