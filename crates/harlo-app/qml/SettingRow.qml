import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Title and description on the left, a control on the right.
Item {
    id: settingRow
    readonly property bool seekable: true
    property string title
    property string detail
    property string more: ""
    property color detailColor: Theme.muted
    property string icon: ""
    property bool divider: false
    default property alias control: controlSlot.data
    // Too narrow for the control beside the text: it goes under it.
    readonly property real textX: 16 + (icon !== "" ? 44 : 0)
    readonly property bool stacked: controlSlot.implicitWidth > 0 && width - textX - 16 - controlSlot.implicitWidth - 14 < Math.min(220, width * 0.4)
    readonly property real textWidth: stacked ? width - textX - 16 : width - textX - 16 - controlSlot.implicitWidth - 14
    Layout.fillWidth: true
    implicitHeight: stacked ? rowText.implicitHeight + controlSlot.implicitHeight + 34
                            : Math.max(rowText.implicitHeight, controlSlot.implicitHeight) + 26
    Rectangle { visible: settingRow.divider; width: parent.width; height: 1; color: Theme.hairline }
    Rectangle {
        visible: settingRow.icon !== ""
        x: 16
        y: settingRow.stacked ? 13 : (settingRow.height - height) / 2
        width: 32
        height: 32
        radius: 3
        color: Theme.controlFill
        Icon { anchors.centerIn: parent; name: settingRow.icon; color: Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
    }
    ColumnLayout {
        id: rowText
        x: settingRow.textX
        y: settingRow.stacked ? 13 : (settingRow.height - implicitHeight) / 2
        width: settingRow.textWidth
        spacing: 2
        Label { text: settingRow.title; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium; Layout.fillWidth: true; wrapMode: Text.Wrap }
        MoreText { visible: settingRow.detail !== ""; full: settingRow.detail; more: settingRow.more; color: settingRow.detailColor === Theme.muted ? Theme.textMuted : settingRow.detailColor; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true }
    }
    Item {
        id: controlSlot
        x: settingRow.stacked ? settingRow.textX : settingRow.width - 16 - implicitWidth
        y: settingRow.stacked ? rowText.y + rowText.implicitHeight + 8 : (settingRow.height - implicitHeight) / 2
        implicitWidth: childrenRect.width
        implicitHeight: childrenRect.height
        width: implicitWidth
        height: implicitHeight
    }
}
