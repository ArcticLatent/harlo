import QtQuick
import harlo

// A session as an avatar: its model's maker's logo on a rounded square tinted
// for the maker; the agent's own mark while the model isn't known yet. A
// session's status sits on the upper right (working, waiting for you,
// unread).
Item {
    id: avatar
    /// The agent ("pi", "demo").
    property string provider
    /// The model's maker, a logo id (Workspace.modelMaker); "" not known.
    property string maker: ""
    property string badge: ""
    /// What's behind it (the corners' rings cut into it).
    property color ring: Theme.chrome
    readonly property bool modelFirst: maker !== ""
    readonly property var tint: modelFirst ? Theme.makerColor(maker) : Theme.agentColor(provider)
    implicitWidth: 26
    implicitHeight: 26
    // A mark's size for a box: about `share` of it, with an even gap on each
    // side so it sits centred on whole pixels (14 in 26, 8 in 12).
    function fit(box, share) { return box - 2 * Math.round(box * (1 - share) / 2) }
    Rectangle {
        anchors.fill: parent
        radius: Math.round(width * 0.12)
        color: avatar.tint.fill
    }
    ProviderMark {
        objectName: "sessionProvider"
        visible: !avatar.modelFirst
        anchors.centerIn: parent
        implicitWidth: avatar.fit(avatar.width, 0.52)
        implicitHeight: implicitWidth
        provider: avatar.provider
        tip: ""
        color: avatar.tint.ink
    }
    ProviderLogo {
        objectName: "sessionMaker"
        visible: avatar.modelFirst
        anchors.centerIn: parent
        implicitWidth: avatar.fit(avatar.width, 0.54)
        implicitHeight: implicitWidth
        provider: avatar.modelFirst ? avatar.maker : ""
        color: avatar.tint.ink
    }
    Rectangle {
        visible: avatar.badge !== "" && avatar.badge !== "idle" && avatar.badge !== "live"
        width: 11
        height: 11
        radius: 5.5
        x: parent.width - 8
        y: -3
        color: avatar.ring
        StatusDot { anchors.centerIn: parent; status: avatar.badge; implicitWidth: 7; implicitHeight: 7; radius: 3.5 }
    }
}
