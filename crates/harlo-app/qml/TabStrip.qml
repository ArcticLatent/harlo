import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Open sessions as tabs. Click activates, middle-click or × closes,
// drag reorders.
Rectangle {
    id: root
    /// { path: text } of files with unsaved edits (dot instead of ×).
    property var dirtyPaths: ({})
    signal closeRequested(string key)
    implicitHeight: 44
    color: Theme.chrome
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline }

    ListView {
        id: tabs
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        orientation: ListView.Horizontal
        spacing: 4
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: Workspace.tabs
        currentIndex: Workspace.tabs.findIndex(t => t.split("\t")[0] === Workspace.activeKey)
        onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
        // Vertical wheel scrolls the strip sideways.
        WheelHandler {
            orientation: Qt.Vertical
            onWheel: event => { tabs.contentX = Math.max(0, Math.min(tabs.contentWidth - tabs.width, tabs.contentX - event.angleDelta.y)) }
        }

        delegate: Item {
            id: tab
            required property string modelData
            required property int index
            readonly property var parts: modelData.split("\t")
            readonly property string key: parts[0]
            readonly property string title: parts[1]
            readonly property string status: parts[2]
            readonly property string branch: parts[4] || ""
            readonly property bool isDiff: key.startsWith("diff:")
            readonly property string harness: key.startsWith("view:") || key.startsWith("file:") || isDiff ? "" : parts[3] || ""
            readonly property bool isFile: key.startsWith("file:")
            // diff:<0|1>:<path>
            readonly property string diffPath: isDiff ? key.substring(7) : ""
            readonly property bool dirty: isFile && root.dirtyPaths[key.substring(5)] !== undefined
            readonly property bool active: key === Workspace.activeKey
            width: Math.min(280, Math.max(120, label.implicitWidth + (isFile || isDiff ? 80 : 64)))
            height: tabs.height

            // A soft pill, as the sidebar's rows.
            Rectangle {
                anchors.fill: parent
                anchors.topMargin: 7
                anchors.bottomMargin: 7
                radius: 9
                color: tab.active ? Theme.rowSelected : hover.hovered ? Theme.rowHover : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 4
                    spacing: 6
                    // A session: what runs it, its status on the corner.
                    ProviderMark {
                        objectName: "tabProvider"
                        visible: !tab.key.startsWith("view:") && !tab.isFile && !tab.isDiff
                        provider: tab.harness
                        badge: tab.status === "live" ? "" : tab.status
                        implicitWidth: 13
                        implicitHeight: 13
                        color: tab.active ? Theme.agentColor(tab.harness).ink : Theme.textMuted
                        ring: tab.active ? Theme.rowSelected : hover.hovered ? Theme.rowHover : Theme.chrome
                    }
                    FileIcon { visible: tab.isFile || tab.isDiff; path: tab.isFile ? tab.key.substring(5) : tab.diffPath; Layout.preferredWidth: 14; Layout.preferredHeight: 14; sourceSize: Qt.size(14, 14) }
                    Icon { visible: tab.key.startsWith("view:"); name: tab.key === "view:new" ? "chat-new" : tab.key.startsWith("view:run:") ? "node-tree" : tab.key.substring(5); color: Theme.primary; implicitWidth: 12; implicitHeight: 12 }
                    Label {
                        id: label
                        Layout.fillWidth: true
                        text: (tab.branch !== "" ? "⎇ " : "") + tab.title
                        color: tab.active ? Theme.text : Theme.textMuted
                        elide: Text.ElideRight
                        font.pixelSize: Theme.px(13.5)
                        font.weight: tab.active ? Font.Medium : Font.Normal
                    }
                    FlatButton {
                        // Unsaved files show a dot until hovered.
                        text: tab.dirty && !hover.hovered ? "●" : "×"
                        outlined: false
                        accent: tab.dirty && !hover.hovered ? Theme.primary : Theme.muted
                        padding: 1
                        implicitWidth: 20
                        implicitHeight: 20
                        opacity: tab.active || hover.hovered || tab.dirty ? 1 : 0
                        onClicked: root.closeRequested(tab.key)
                    }
                }
            }
            HoverHandler { id: hover }
            ToolTip.visible: hover.hovered && label.truncated
            ToolTip.text: tab.title
            ToolTip.delay: 600
            TapHandler { onTapped: Workspace.open(tab.key) }
            TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: root.closeRequested(tab.key) }
            // Drag to reorder: swap with the neighbour once past its middle.
            DragHandler {
                id: drag
                target: null
                yAxis.enabled: false
                property real startX: 0
                onActiveChanged: startX = tab.x
                onTranslationChanged: {
                    const center = tab.x + tab.width / 2 + translation.x
                    const over = tabs.indexAt(center + tabs.contentX - tabs.originX, tabs.height / 2)
                    if (over >= 0 && over !== tab.index)
                        Workspace.moveTab(tab.index, over)
                }
            }
        }
    }
}
