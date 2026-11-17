import QtQuick
import harlo

// A session as an avatar: its model's maker's logo on a rounded square tinted
// for the maker, with the agent that runs it as a small tag on the lower
// left; the agent's own mark when the model isn't known yet, or when it's
// the agent's own maker's (Claude Code on Claude). A session's status sits
// on the upper right (working, waiting for you, unread).
Item {
    id: avatar
    /// The agent ("harlo", "claude", …).
    property string provider
    /// The model's maker, a logo id (Workspace.modelMaker); "" not known.
    property string maker: ""
    property string badge: ""
    /// What's behind it (the corners' rings cut into it).
    property color ring: Theme.chrome
    readonly property bool modelFirst: maker !== "" && !(provider === "claude" && maker === "anthropic")
    readonly property var tint: modelFirst ? Theme.makerColor(maker) : Theme.agentColor(provider)
    implicitWidth: 26
    implicitHeight: 26
    // A mark's size for a box: about `share` of it, with an even gap on each
    // side so it sits centred on whole pixels (14 in 26, 8 in 12).
    function fit(box, share) { return box - 2 * Math.round(box * (1 - share) / 2) }
    Rectangle {
        anchors.fill: parent
        radius: Math.round(width * 0.27)
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
    // The agent, as a small badge on the corner: solid in its own colour, its
    // mark in near-black (both themes), so it reads against any logo's tile;
    // ringed in what's behind so it cuts into the corner.
    Rectangle {
        objectName: "sessionAgentTag"
        visible: avatar.modelFirst
        width: Math.round(avatar.width * 0.58)
        height: width
        radius: width / 2
        x: -Math.round(avatar.width * 0.115)
        y: avatar.height - height + Math.round(avatar.width * 0.115)
        color: avatar.ring
        Rectangle {
            anchors.fill: parent
            anchors.margins: Math.max(1, Math.round(avatar.width * 0.05))
            radius: width / 2
            color: Theme.agentBase(avatar.provider)
            ProviderMark {
                anchors.centerIn: parent
                implicitWidth: avatar.fit(parent.width, 0.66)
                implicitHeight: implicitWidth
                provider: avatar.provider
                tip: ""
                color: "#16171b"
            }
        }
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
