import QtQuick
import QtQuick.Controls.Basic
import harlo

// A value next to a slider ("100%", "160 ms").
Rectangle {
    property alias text: chipLabel.text
    implicitWidth: Math.max(52, chipLabel.implicitWidth + 16)
    implicitHeight: 26
    radius: 7
    color: Theme.controlFill
    Label { id: chipLabel; anchors.centerIn: parent; color: Theme.text; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
}
