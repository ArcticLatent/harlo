import QtQuick
import QtQuick.Controls
import harlo

// An icon button on a composer's bar (the chat's and New session's): an
// 18 px icon in a 26 px target, `on` for a toggled state.
Button {
    id: ib
    property string iconName
    property color tint: Theme.foreground
    property bool on: false
    property string tip: ""
    flat: true
    padding: 0
    implicitWidth: 26
    implicitHeight: 26
    contentItem: Item {
        Icon { anchors.centerIn: parent; name: ib.iconName; color: ib.tint; implicitWidth: 18; implicitHeight: 18; opacity: ib.enabled ? 1 : 0.35 }
    }
    background: Rectangle { radius: 6; color: ib.on ? Theme.selected : ib.hovered ? Theme.hover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
    ToolTip.visible: hovered && tip !== ""
    ToolTip.delay: 400
    ToolTip.text: tip
}
