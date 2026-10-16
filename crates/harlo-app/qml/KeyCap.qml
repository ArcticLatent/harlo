import QtQuick
import QtQuick.Controls
import harlo

// One key of a shortcut, drawn like a keyboard key.
Rectangle {
    property string text
    implicitWidth: Math.max(22, capLabel.implicitWidth + 12)
    implicitHeight: 22
    radius: 5
    color: Theme.background
    border.color: Theme.border
    // A darker bottom edge, like a physical key.
    Rectangle { anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - 4; height: 1; color: "#40000000" }
    Label { id: capLabel; anchors.centerIn: parent; text: parent.text; color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(11) }
}
