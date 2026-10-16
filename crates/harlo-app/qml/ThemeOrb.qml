import QtQuick
import harlo

// A theme variant as a small sphere of its colours.
Item {
    id: orb
    property var colors: []
    readonly property color bg: colors[0] || "#202020"
    readonly property color acc: colors[2] || "#5b9bf0"
    readonly property color card: colors[3] || bg
    readonly property bool light: (0.2126 * bg.r + 0.7152 * bg.g + 0.0722 * bg.b) > 0.5
    readonly property color topColor: light ? Qt.tint(card, Qt.rgba(1, 1, 1, 0.6)) : Qt.tint(card, Qt.rgba(1, 1, 1, 0.12))
    readonly property color midColor: Qt.tint(bg, Qt.rgba(acc.r, acc.g, acc.b, light ? 0.45 : 0.5))
    readonly property color lowColor: Qt.tint(bg, Qt.rgba(acc.r, acc.g, acc.b, light ? 0.8 : 0.85))
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        gradient: Gradient {
            GradientStop { position: 0.0; color: orb.topColor }
            GradientStop { position: 0.55; color: orb.midColor }
            GradientStop { position: 1.0; color: orb.lowColor }
        }
        border.color: Qt.rgba(0, 0, 0, orb.light ? 0.08 : 0.25)
    }
    // A soft highlight.
    Rectangle {
        x: orb.width * 0.22
        y: orb.height * 0.14
        width: orb.width * 0.36
        height: orb.height * 0.22
        radius: height / 2
        color: Qt.rgba(1, 1, 1, orb.light ? 0.5 : 0.18)
    }
}
