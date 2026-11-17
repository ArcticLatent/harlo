import QtQuick
import harlo

// A session as an avatar: its model's maker's logo in a round badge tinted
// for the maker, with the agent that runs it as a small tag on the lower
// left; the agent's own mark when the model isn't known yet, or when it's
// the agent's own maker's (Claude Code on Claude). A session's status sits
// on the lower right (working, waiting for you, unread).
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
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: avatar.tint.fill
    }
    ProviderMark {
        objectName: "sessionProvider"
        visible: !avatar.modelFirst
        anchors.centerIn: parent
        implicitWidth: Math.round(avatar.width * 0.52)
        implicitHeight: implicitWidth
        provider: avatar.provider
        // (Its row says the model and agent on hover.)
        tip: ""
        color: avatar.tint.ink
    }
    ProviderLogo {
        objectName: "sessionMaker"
        visible: avatar.modelFirst
        anchors.centerIn: parent
        implicitWidth: Math.round(avatar.width * 0.54)
        implicitHeight: implicitWidth
        provider: avatar.modelFirst ? avatar.maker : ""
        color: avatar.tint.ink
    }
    // The agent, as a tag on the corner.
    Rectangle {
        objectName: "sessionAgentTag"
        visible: avatar.modelFirst
        width: Math.round(avatar.width * 0.5)
        height: width
        radius: width / 2
        x: -Math.round(avatar.width * 0.115)
        y: avatar.height - height + Math.round(avatar.width * 0.115)
        color: avatar.ring
        ProviderMark {
            anchors.centerIn: parent
            implicitWidth: Math.round(parent.width * 0.7)
            implicitHeight: implicitWidth
            provider: avatar.provider
            tip: ""
            color: Theme.textMuted
        }
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
