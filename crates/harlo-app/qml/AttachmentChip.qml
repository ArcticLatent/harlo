import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// An attached file: type icon, name (full path on hover), optional ×.
Rectangle {
    id: chip
    property string path
    property bool removable: false
    signal removeClicked()
    readonly property string name: path.split("/").pop()

    implicitHeight: 26
    implicitWidth: Math.min(260, row.implicitWidth + 16)
    radius: 6
    color: hover.hovered ? Theme.selected : Theme.elevated
    border.color: Theme.border
    HoverHandler { id: hover }
    ToolTip.visible: hover.hovered
    ToolTip.text: chip.path
    ToolTip.delay: 500

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: chip.removable ? 4 : 8
        spacing: 6
        FileIcon { path: chip.name; Layout.preferredWidth: 16; Layout.preferredHeight: 16 }
        Label {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            text: chip.name
            color: Theme.foreground
            font.pixelSize: Theme.px(12)
            elide: Text.ElideMiddle
        }
        Button {
            id: removeButton
            visible: chip.removable
            flat: true
            implicitWidth: 18
            implicitHeight: 18
            padding: 2
            onClicked: chip.removeClicked()
            contentItem: Icon { name: "close"; color: removeButton.hovered ? Theme.foreground : Theme.muted }
            background: Rectangle { radius: 4; color: removeButton.hovered ? Theme.hover : "transparent" }
        }
    }
}
