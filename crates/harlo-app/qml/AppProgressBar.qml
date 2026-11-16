import QtQuick
import QtQuick.Controls.Basic
import harlo

// A progress bar that reads at a glance: a primary-coloured fill on a
// visible track; indeterminate, a segment sweeping across.
ProgressBar {
    id: bar
    implicitHeight: 8
    background: Rectangle {
        implicitWidth: 200
        implicitHeight: bar.implicitHeight
        radius: height / 2
        color: Theme.blend(Theme.background, Theme.foreground, Theme.isLight ? 0.12 : 0.16)
    }
    contentItem: Item {
        implicitHeight: bar.implicitHeight
        clip: true
        Rectangle {
            // (The fraction moves, not the width: a resize applies at once.)
            property real shown: bar.visualPosition
            Behavior on shown { NumberAnimation { duration: Theme.motion } }
            visible: !bar.indeterminate
            width: Math.max(height, shown * parent.width)
            height: parent.height
            radius: height / 2
            color: Theme.primary
        }
        Rectangle {
            id: sweep
            visible: bar.indeterminate
            width: parent.width * 0.3
            // (Still, in the middle, with animations off.)
            x: (bar.availableWidth - width) / 2
            height: parent.height
            radius: height / 2
            color: Theme.primary
            XAnimator on x {
                running: bar.indeterminate && bar.visible && Theme.motion > 0
                from: -sweep.width
                to: bar.availableWidth
                duration: 1300
                loops: Animation.Infinite
            }
        }
    }
}
