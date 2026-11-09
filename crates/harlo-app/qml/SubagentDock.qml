import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The subagents of the session (task tool): live status, current tool, and
// View / Steer / Cancel. A summary line until opened (the transcript
// shows each subagent under the tool call that started it).
Rectangle {
    id: root
    objectName: "subagentDock"
    property ChatSession session
    signal viewRequested(var agent, bool steer)

    // Its agent's own subagents, and the harlo sessions it started (orchestration).
    readonly property var agents: (session && session.subagentsJson !== "" ? JSON.parse(session.subagentsJson) : [])
        .concat(session && session.key !== "" && Workspace.orchestrationRevision >= 0 ? JSON.parse(Workspace.orchestratedJson(session.key)) : [])
    // What the harlo sessions it started used, together.
    readonly property string used: Theme.usedLabel(Theme.usedTotal(agents.map(a => a.used || null)))
    readonly property int running: agents.filter(a => a.status === "running" || a.status === "pending").length
    // harlo sessions waiting: on their parent's answer, or on you.
    readonly property int stalled: agents.filter(a => a.status === "asking" || a.status === "waiting").length
    // Collapsed to its summary line: the details sit under the task tool
    // row in the transcript; open it to manage them from here.
    property bool expanded: false
    // The open list's order: waiting on you, then running, then the rest.
    function rank(status) { return status === "asking" || status === "waiting" ? 0 : status === "running" || status === "pending" ? 1 : 2 }

    visible: agents.length > 0
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    // (Accent borders are for a panel that needs you; its icon says it runs.)
    border.color: Theme.border

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
                // (Narrow: it shortens, the chevron stays clear.)
                Layout.minimumWidth: 0
                Layout.preferredWidth: 0
                elide: Text.ElideRight
                readonly property int finished: root.agents.length - root.running - root.stalled
                text: "Helpers · " + (root.running > 0 ? root.running + " running" : root.stalled > 0 ? "" : root.agents.every(a => a.status === "stopped") ? "stopped" : "all done")
                    + (root.stalled > 0 ? (root.running > 0 ? " · " : "") + root.stalled + " waiting" : "")
                    + (finished > 0 && root.running + root.stalled > 0 ? " · " + finished + " finished" : "")
                    + (root.used !== "" ? " · " + root.used : "")
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
                Layout.maximumWidth: Math.min(260, root.width * 0.35)
            }
            Button {
                id: toggle
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                padding: 0
                onClicked: root.expanded = !root.expanded
                contentItem: Item { Icon { anchors.centerIn: parent; name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted; implicitWidth: 18; implicitHeight: 18 } }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
            }
        }

        // Open: the helpers that need you or still run first, then the
        // rest; about six rows tall at most, the others a scroll away, so
        // many helpers never bury the chat.
        Flickable {
            id: list
            objectName: "subagentList"
            visible: root.expanded
            Layout.fillWidth: true
            // (Whole rows: the last one that fits under the cap ends it.)
            Layout.preferredHeight: {
                const cap = Theme.px(216)
                if (rows.implicitHeight <= cap) return rows.implicitHeight
                let h = 0
                for (let i = 0; i < rows.children.length; i++) {
                    const r = rows.children[i]
                    if (!r.visible || r.height <= 0 || r.y === undefined) continue
                    if (r.y + r.height > cap) break
                    h = r.y + r.height
                }
                return h > 0 ? h : cap
            }
            contentWidth: width
            contentHeight: rows.implicitHeight
            clip: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: list.contentHeight > list.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            // More below: the bottom row fades into the card.
            Rectangle {
                parent: list
                z: 2
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: Theme.px(22)
                visible: list.contentY + list.height < list.contentHeight - 1
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(Theme.elevated.r, Theme.elevated.g, Theme.elevated.b, 0) }
                    GradientStop { position: 1; color: Theme.elevated }
                }
            }
            Column {
                id: rows
                width: list.width
                spacing: 6
                Repeater {
                    model: root.expanded ? root.agents.map((a, i) => ({ a: a, i: i }))
                        .sort((x, y) => root.rank(x.a.status) - root.rank(y.a.status) || x.i - y.i).map(x => x.a) : []
                    SubagentRow {
                        required property var modelData
                        width: rows.width
                        a: modelData
                        session: root.session
                        onViewRequested: (agent, steer) => root.viewRequested(agent, steer)
                    }
                }
            }
        }
    }
}
