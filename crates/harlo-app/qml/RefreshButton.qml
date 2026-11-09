import QtQuick
import QtQuick.Controls
import harlo

// Refresh, as an icon: the same at every section that reads something again.
// `tip` says what it refreshes or when it last did.
Button {
    id: rb
    property string tip: "Refresh"
    flat: true
    padding: 0
    implicitWidth: 28
    implicitHeight: 28
    Accessible.name: "Refresh"
    contentItem: Item {
        Icon { anchors.centerIn: parent; name: "refresh"; color: rb.enabled ? Theme.textMuted : Theme.textFaint; implicitWidth: 15; implicitHeight: 15 }
    }
    background: Rectangle {
        radius: 6
        color: rb.down ? Theme.pressed : rb.hovered ? Theme.hover : "transparent"
        border.color: rb.visualFocus ? Theme.accent : "transparent"
    }
    ToolTip.visible: hovered || visualFocus
    ToolTip.delay: 400
    ToolTip.text: tip
}
