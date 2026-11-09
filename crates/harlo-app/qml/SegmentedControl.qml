import QtQuick
import QtQuick.Controls
import harlo

// A choice of two or three views side by side (Before | After): the chosen
// one tinted, the others plain, all in one shallow tray. Each segment is a
// checkable button, so a screen reader says which one is on.
Rectangle {
    id: seg
    property var labels: []
    property int current: 0
    /// objectName prefix for the segments ("<prefix><label>").
    property string namePrefix: ""
    property real fontSize: Theme.px(11.5)
    signal picked(int index)

    implicitWidth: row.implicitWidth + 4
    implicitHeight: row.implicitHeight + 4
    radius: 8
    color: Theme.controlFill
    border.color: Theme.controlBorder

    Row {
        id: row
        x: 2
        y: 2
        spacing: 2
        Repeater {
            model: seg.labels
            FlatButton {
                required property string modelData
                required property int index
                objectName: seg.namePrefix !== "" ? seg.namePrefix + modelData : ""
                text: modelData
                checkable: true
                checked: index === seg.current
                outlined: checked
                variant: "selected"
                radius: 6
                padding: 3
                horizontalPadding: 9
                font.pixelSize: seg.fontSize
                accent: checked ? Theme.foreground : Theme.textMuted
                Accessible.role: Accessible.RadioButton
                onClicked: { checked = Qt.binding(() => index === seg.current); seg.current = index; seg.picked(index) }
            }
        }
    }
}
