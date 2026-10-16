import QtQuick
import QtQuick.Controls
import harlo

// A labelled control on a composer's bar: a 16 px icon (or a custom lead,
// such as a provider logo) and 12.5 px medium text, on the app's pill
// surface (ControlSurface).
Button {
    id: lb
    property string iconName: ""
    property color tint: Theme.foreground
    property string tip: ""
    property real maxLabel: 220
    /// A custom leading item (e.g. a provider logo) instead of `icon`.
    default property alias lead: leadHolder.data
    flat: true
    padding: 0
    leftPadding: 10
    rightPadding: 11
    implicitHeight: 30
    // Its natural width; in a layout that gives it less (Layout.minimumWidth
    // below this), the label elides to fit.
    implicitWidth: leftPadding + rightPadding + leadHolder.implicitWidth + 6 + Math.min(label.implicitWidth, maxLabel)
    contentItem: Row {
        spacing: 6
        opacity: lb.enabled ? 1 : 0.6
        Item {
            id: leadHolder
            anchors.verticalCenter: parent.verticalCenter
            // An icon, or the custom item (a 17 px provider logo) if shown.
            implicitWidth: lb.iconName !== "" ? 16 : Array.from(children).some(c => c.visible && c.objectName === "providerLogo") ? 17 : 0
            implicitHeight: 17
            width: implicitWidth
            height: implicitHeight
            Icon { visible: lb.iconName !== ""; anchors.centerIn: parent; name: lb.iconName; color: lb.tint; implicitWidth: 16; implicitHeight: 16 }
        }
        Label {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            text: lb.text
            color: lb.tint
            font.pixelSize: Theme.px(12.5)
            font.weight: Font.Medium
            elide: Text.ElideRight
            width: Math.max(0, Math.min(implicitWidth, lb.maxLabel, lb.availableWidth - leadHolder.width - 6))
        }
    }
    background: ControlSurface { control: lb; radius: 9 }
    ToolTip.visible: hovered && tip !== ""
    ToolTip.delay: 400
    ToolTip.text: tip
}
