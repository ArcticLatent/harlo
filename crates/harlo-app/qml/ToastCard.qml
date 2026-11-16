import QtQuick
import QtQuick.Layouts
import harlo

// A ToastStack's card: a raised card with a soft shadow, an icon, and what
// it holds beside the icon (a column). Going, it fades and slides away,
// then leaves the stack.
Rectangle {
    id: toast
    required property string json
    required property string key
    required property bool leaving
    /// The stack it's in.
    required property ToastStack stack
    /// The entry, parsed.
    readonly property var entry: JSON.parse(json)
    /// Its objectName while it shows: this and its key.
    property string namePrefix
    property string iconName
    property color iconColor: Theme.primary
    property alias spacing: column.spacing
    /// The pointer is on it.
    readonly property alias hovered: cardHover.hovered
    default property alias content: column.data

    objectName: leaving ? "" : namePrefix + key
    opacity: leaving ? 0 : 1
    Behavior on opacity {
        enabled: Theme.paneMotion > 0
        NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic; onRunningChanged: if (!running && toast.leaving) toast.stack.remove(toast.key) }
    }
    onLeavingChanged: if (leaving && Theme.paneMotion === 0) Qt.callLater(toast.stack.remove, key)
    transform: Translate { x: toast.leaving ? 32 : 0; Behavior on x { NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic } } }
    width: stack.width
    implicitHeight: body.implicitHeight + 24
    radius: Theme.radiusLg
    color: Theme.elevated
    border.color: Theme.border
    HoverHandler { id: cardHover }
    // A soft shadow under the card.
    Rectangle { z: -1; anchors.fill: parent; anchors.topMargin: 3; anchors.leftMargin: -1; anchors.rightMargin: -1; anchors.bottomMargin: -4; radius: Theme.radiusXl; color: "#000000"; opacity: 0.18 }

    RowLayout {
        id: body
        x: 14
        y: 12
        width: parent.width - 28
        spacing: 12
        Icon { Layout.alignment: Qt.AlignTop; Layout.topMargin: 2; name: toast.iconName; color: toast.iconColor; implicitWidth: 18; implicitHeight: 18 }
        ColumnLayout {
            id: column
            Layout.fillWidth: true
            spacing: 6
        }
    }
}
