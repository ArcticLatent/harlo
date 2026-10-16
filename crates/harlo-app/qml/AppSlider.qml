import QtQuick
import QtQuick.Controls
import harlo

// Slider in the app's colours: a thin track, filled in the accent colour up
// to a round handle.
Slider {
    id: slider
    implicitWidth: 160
    implicitHeight: 22
    padding: 0
    background: Item {
        x: slider.leftPadding
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth
        height: 4
        Rectangle { anchors.fill: parent; radius: 2; color: Theme.border }
        Rectangle { width: slider.visualPosition * parent.width; height: parent.height; radius: 2; color: slider.enabled ? Theme.primary : Theme.muted }
    }
    handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: 16
        height: 16
        radius: 8
        color: Theme.background
        border.width: 2
        border.color: slider.enabled ? Theme.primary : Theme.muted
        // A soft ring while it's held or focused.
        Rectangle {
            anchors.centerIn: parent
            width: 26
            height: 26
            radius: 13
            z: -1
            color: "transparent"
            border.width: 3
            border.color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.3)
            visible: slider.pressed || slider.visualFocus
        }
    }
}
