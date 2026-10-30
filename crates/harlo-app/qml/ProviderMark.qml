import QtQuick
import QtQuick.Controls
import harlo

// A provider's mark (Claude, OpenCode, Pi, Oh My Pi, Demo), so a session or
// project shows what runs it; its name on hover. `badge`: a status dot on
// its corner (a session's: busy, waiting for you, unread).
Item {
    id: mark
    property string provider
    property color color: Theme.muted
    property string badge: ""
    property color ring: Theme.background
    property string tip: provider !== "" ? Theme.harnessLabel(provider) : ""
    readonly property bool known: ["claude", "opencode", "pi", "omp", "hermes", "demo"].indexOf(provider) >= 0
    implicitWidth: 14
    implicitHeight: 14
    Icon {
        anchors.fill: parent
        visible: mark.known
        name: mark.known ? "harness-" + mark.provider : ""
        color: mark.color
    }
    // An unknown provider (or none): the bare dot.
    StatusDot {
        anchors.centerIn: parent
        visible: !mark.known && status !== ""
        status: mark.badge
    }
    Rectangle {
        visible: mark.known && mark.badge !== ""
        width: 8
        height: 8
        radius: 4
        x: parent.width - 6
        y: parent.height - 6
        color: mark.ring
        StatusDot { anchors.centerIn: parent; status: mark.badge; implicitWidth: 6; implicitHeight: 6; radius: 3 }
    }
    HoverHandler { id: hover; enabled: mark.tip !== "" }
    ToolTip.visible: hover.hovered && mark.tip !== ""
    ToolTip.text: mark.tip
    ToolTip.delay: 400
}
