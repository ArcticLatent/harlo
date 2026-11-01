import QtQuick
import QtQuick.Controls
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
            visible: !bar.indeterminate
            width: Math.max(height, bar.visualPosition * parent.width)
            height: parent.height
            radius: height / 2
            color: Theme.primary
            Behavior on width { NumberAnimation { duration: 200 } }
        }
        Rectangle {
            id: sweep
            visible: bar.indeterminate
            width: parent.width * 0.3
            height: parent.height
            radius: height / 2
            color: Theme.primary
            XAnimator on x {
                running: bar.indeterminate && bar.visible
                from: -sweep.width
                to: bar.availableWidth
                duration: 1300
                loops: Animation.Infinite
            }
        }
    }
}
