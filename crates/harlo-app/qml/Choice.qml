import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// One of a few choices, as a radio row: its title, a line about it.
Rectangle {
    id: choice
    property string title
    property string detail
    property bool selected: false
    signal picked()
    Layout.fillWidth: true
    implicitHeight: choiceCol.implicitHeight + 22
    color: choiceHover.hovered ? Theme.hover : "transparent"
    HoverHandler { id: choiceHover }
    TapHandler { onTapped: choice.picked() }
    RowLayout {
        anchors.fill: parent
        anchors.margins: 11
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        spacing: 12
        Rectangle {
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: 1
            implicitWidth: 16
            implicitHeight: 16
            radius: 8
            color: "transparent"
            border.color: choice.selected ? Theme.primary : Theme.muted
            border.width: choice.selected ? 5 : 1.5
        }
        ColumnLayout {
            id: choiceCol
            Layout.fillWidth: true
            spacing: 2
            Label { Layout.fillWidth: true; text: choice.title; color: Theme.foreground; font.pixelSize: Theme.px(13) }
            Label { visible: choice.detail !== ""; text: choice.detail; color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
}
