import QtQuick
import harlo

// harlo's window, small, in a theme variant's colours (`colors`:
// background, foreground, primary, elevated, muted text, sidebar).
Rectangle {
    property var colors: []
    readonly property color bg: colors[0] || "#202020"
    readonly property color fg: colors[1] || "#e0e0e0"
    readonly property color acc: colors[2] || "#5b9bf0"
    readonly property color card: colors[3] || bg
    readonly property color mut: colors[4] || fg
    readonly property color side: colors[5] || card
    radius: 7
    color: bg
    border.color: Qt.rgba(fg.r, fg.g, fg.b, 0.12)
    clip: true
    Rectangle {
        width: parent.width * 0.3
        height: parent.height
        color: parent.side
        radius: 7
        Rectangle { x: parent.width - 7; width: 7; height: parent.height; color: parent.color }
        Column {
            x: 8
            y: 9
            spacing: 6
            Rectangle { width: parent.parent.width - 16; height: 8; radius: 4; color: Qt.rgba(parent.parent.parent.fg.r, parent.parent.parent.fg.g, parent.parent.parent.fg.b, 0.1) }
            Repeater {
                model: 3
                Rectangle {
                    required property int index
                    width: (parent.parent.width - 16) * (index === 0 ? 1 : 0.75)
                    height: 6
                    radius: 3
                    color: index === 0 ? Qt.rgba(parent.parent.parent.acc.r, parent.parent.parent.acc.g, parent.parent.parent.acc.b, 0.45) : Qt.rgba(parent.parent.parent.mut.r, parent.parent.parent.mut.g, parent.parent.parent.mut.b, 0.45)
                }
            }
        }
    }
    Column {
        x: parent.width * 0.36
        y: parent.height * 0.18
        spacing: 6
        Rectangle { width: 52; height: 6; radius: 3; color: Qt.rgba(parent.parent.fg.r, parent.parent.fg.g, parent.parent.fg.b, 0.6) }
        Rectangle { width: 38; height: 6; radius: 3; color: Qt.rgba(parent.parent.mut.r, parent.parent.mut.g, parent.parent.mut.b, 0.5) }
    }
    Rectangle {
        x: parent.width - width - 10
        y: parent.height * 0.16
        width: parent.width * 0.22
        height: 10
        radius: 5
        color: Qt.rgba(acc.r, acc.g, acc.b, 0.55)
    }
    Rectangle {
        x: parent.width * 0.34
        y: parent.height - height - 9
        width: parent.width * 0.6
        height: 16
        radius: 6
        color: parent.card
        border.color: Qt.rgba(parent.fg.r, parent.fg.g, parent.fg.b, 0.15)
        Rectangle { x: parent.width - 13; anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: parent.parent.acc }
    }
}
