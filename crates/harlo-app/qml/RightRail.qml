import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Vertical icon bar at the window's right edge; picks the side panel.
// Clicking the active icon closes the panel.
Rectangle {
    id: root
    property string mode: ""
    /// Count badges per mode, e.g. { changes: 3 }.
    property var counts: ({})
    signal picked(string mode)
    /// The terminal panel under the chat (its button sits at the bottom).
    property bool terminalOpen: false
    signal terminalToggled()
    implicitWidth: 52
    color: Theme.chrome
    Rectangle { anchors.left: parent.left; width: 1; height: parent.height; color: Theme.hairline }

    readonly property var items: [
        { mode: "context", icon: "pie", label: "Context", keys: Keymap.seq("contextPanel") },
        { mode: "activity", icon: "list-check-2", label: "Activity: what the agent ran and touched", keys: Keymap.seq("activityPanel") },
        { mode: "changes", icon: "git-branch", label: "Changes", keys: Keymap.seq("changesPanel") },
        { mode: "files", icon: "folders", label: "Files", keys: Keymap.seq("filesPanel") },
        { mode: "notes", icon: "note", label: "Notes", keys: Keymap.seq("notesPanel") },
    ]

    Column {
        anchors.top: parent.top
        anchors.topMargin: 10
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6
        Repeater {
            model: root.items
            delegate: Item {
                id: btn
                required property var modelData
                objectName: "rail-" + modelData.mode
                readonly property bool active: root.mode === modelData.mode
                width: 38
                height: 38
                Rectangle {
                    anchors.fill: parent
                    radius: 10
                    color: btn.active ? Theme.rowSelected : hover.hovered ? Theme.rowHover : "transparent"
                    Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                }
                Icon {
                    anchors.centerIn: parent
                    name: btn.modelData.icon
                    implicitWidth: 19
                    implicitHeight: 19
                    color: btn.active ? Theme.accent : hover.hovered ? Theme.text : Theme.textMuted
                }
                CountBadge {
                    count: root.counts[btn.modelData.mode] || 0
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.topMargin: -3
                    anchors.rightMargin: -5
                }
                HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.picked(btn.active ? "" : btn.modelData.mode) }
                ToolTip.visible: hover.hovered
                ToolTip.text: btn.modelData.label + (btn.modelData.keys !== "" ? "  (" + btn.modelData.keys + ")" : "")
                ToolTip.delay: 400
            }
        }
    }

    Item {
        id: termButton
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 10
        anchors.horizontalCenter: parent.horizontalCenter
        width: 38
        height: 38
        Rectangle {
            anchors.fill: parent
            radius: 10
            color: root.terminalOpen ? Theme.rowSelected : termHover.hovered ? Theme.rowHover : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
        }
        Icon {
            anchors.centerIn: parent
            name: "terminal-box"
            implicitWidth: 19
            implicitHeight: 19
            color: root.terminalOpen ? Theme.accent : termHover.hovered ? Theme.text : Theme.textMuted
        }
        HoverHandler { id: termHover }
        TapHandler { onTapped: root.terminalToggled() }
        ToolTip.visible: termHover.hovered
        ToolTip.text: "Terminal" + Keymap.hint("terminal")
        ToolTip.delay: 400
    }
}
