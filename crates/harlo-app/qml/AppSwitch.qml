import QtQuick
import QtQuick.Controls
import harlo

// On/off switch in the app's colours.
Switch {
    id: sw
    padding: 0
    implicitWidth: 34
    implicitHeight: 20
    contentItem: Item { }
    indicator: Rectangle {
        implicitWidth: 34
        implicitHeight: 20
        y: (sw.height - height) / 2
        radius: height / 2
        color: sw.checked ? Theme.primary : Theme.border
        opacity: sw.enabled ? 1 : 0.5
        Rectangle {
            width: 14
            height: 14
            radius: 7
            y: 3
            x: sw.checked ? parent.width - width - 3 : 3
            color: sw.checked ? Theme.onPrimary : Theme.muted
            Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
        }
    }
}
