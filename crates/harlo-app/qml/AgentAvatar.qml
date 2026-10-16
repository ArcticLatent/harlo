import QtQuick
import harlo

// An agent as an avatar: its logo in a round, softly tinted badge of its
// colour, with a session's status on the corner (working, waiting for you,
// unread).
Item {
    id: avatar
    property string provider
    property string badge: ""
    /// What's behind it (the badge's ring cuts into it).
    property color ring: Theme.chrome
    readonly property var tint: Theme.agentColor(provider)
    implicitWidth: 26
    implicitHeight: 26
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: avatar.tint.fill
    }
    ProviderMark {
        objectName: "sessionProvider"
        anchors.centerIn: parent
        implicitWidth: Math.round(avatar.width * 0.52)
        implicitHeight: implicitWidth
        provider: avatar.provider
        color: avatar.tint.ink
    }
    Rectangle {
        visible: avatar.badge !== "" && avatar.badge !== "idle" && avatar.badge !== "live"
        width: 11
        height: 11
        radius: 5.5
        x: parent.width - 9
        y: parent.height - 9
        color: avatar.ring
        StatusDot { anchors.centerIn: parent; status: avatar.badge; implicitWidth: 7; implicitHeight: 7; radius: 3.5 }
    }
}
