import QtQuick
import harlo

// A step on a run's line: a dot whose colour is how it went (a
// thought's is hollow). The ring in the canvas colour cuts the line.
Item {
    id: node
    property string step: "done"   // done, error, running, asking, thought
    // (For tests: the old marks' names.)
    readonly property string name: step === "error" ? "close" : step === "asking" ? "question" : step === "running" ? "loader-4" : "check"
    implicitWidth: 14
    implicitHeight: 14
    Rectangle {
        anchors.centerIn: parent
        width: 11
        height: 11
        radius: 5.5
        color: Theme.canvas
        Rectangle {
            id: dot
            anchors.centerIn: parent
            width: 7
            height: width
            radius: width / 2
            color: node.step === "error" ? Theme.error : node.step === "asking" ? Theme.warning : node.step === "running" ? Theme.primary
                 : node.step === "thought" ? Theme.canvas : Theme.textFaint
            border.width: node.step === "thought" ? 1.25 : 0
            border.color: Theme.textFaint
            SequentialAnimation on opacity {
                running: node.step === "running" && Theme.motion > 0
                loops: Animation.Infinite
                onRunningChanged: if (!running) dot.opacity = 1
                NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
            }
        }
    }
}
