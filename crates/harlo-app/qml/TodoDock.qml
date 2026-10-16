import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// omp's todo list: progress and the task in hand on one line; the phases
// and their tasks when expanded.
Rectangle {
    id: root
    property ChatSession session
    readonly property var phases: session && session.todosJson !== "" ? JSON.parse(session.todosJson) : []
    readonly property var tasks: phases.reduce((all, p) => all.concat(p.tasks || []), [])
    readonly property int done: tasks.filter(t => t.status === "completed").length
    readonly property var current: tasks.find(t => t.status === "in_progress") || null
    property bool expanded: false

    visible: tasks.length > 0
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    border.color: Theme.border

    function mark(status) {
        switch (status) {
        case "completed": return { icon: "checkbox-circle", color: Theme.success }
        case "in_progress": return { icon: "arrow-right-s", color: Theme.primary }
        case "blocked": return { icon: "close-circle", color: Theme.warning }
        case "abandoned": return { icon: "close", color: Theme.muted }
        default: return { icon: "", color: Theme.muted }
        }
    }

    ColumnLayout {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 4

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "list-check-2"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label {
                text: "Plan · " + root.done + "/" + root.tasks.length
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
                font.bold: true
            }
            // Progress bar
            Rectangle {
                implicitWidth: 60
                implicitHeight: 4
                radius: 2
                color: Theme.subtle
                Rectangle { width: parent.width * (root.tasks.length ? root.done / root.tasks.length : 0); height: parent.height; radius: 2; color: Theme.success }
            }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.current ? root.current.content : (root.done === root.tasks.length ? "All done" : "")
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                elide: Text.ElideRight
            }
            Button {
                id: toggle
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                onClicked: root.expanded = !root.expanded
                contentItem: Icon { name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
                ToolTip.visible: hovered
                ToolTip.text: root.expanded ? "Hide the plan" : "Show the whole plan"
            }
        }

        Repeater {
            model: root.expanded ? root.phases : []
            ColumnLayout {
                id: phaseBlock
                required property var modelData
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 2
                Label {
                    text: phaseBlock.modelData.name.toUpperCase()
                    color: Theme.muted
                    font.pixelSize: Theme.px(10)
                    font.bold: true
                    font.letterSpacing: 0.6
                }
                Repeater {
                    model: phaseBlock.modelData.tasks || []
                    RowLayout {
                        id: taskRow
                        required property var modelData
                        readonly property var m: root.mark(modelData.status)
                        Layout.fillWidth: true
                        spacing: 8
                        Item {
                            implicitWidth: 14
                            implicitHeight: 14
                            Icon { anchors.fill: parent; visible: taskRow.m.icon !== ""; name: taskRow.m.icon; color: taskRow.m.color }
                            Rectangle { visible: taskRow.m.icon === ""; anchors.centerIn: parent; width: 10; height: 10; radius: 5; color: "transparent"; border.color: Theme.muted }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: taskRow.modelData.content + (taskRow.modelData.blocker ? "  — " + taskRow.modelData.blocker : "")
                            color: taskRow.modelData.status === "in_progress" ? Theme.foreground : Theme.muted
                            font.pixelSize: Theme.px(12)
                            font.strikeout: taskRow.modelData.status === "abandoned"
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }
    }
}
