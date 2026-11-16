import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A section's heading, its one-line lede (More for the rest), and its
// actions at the right of the heading ("Add …": every page adds the
// same way).
ColumnLayout {
    id: sectionHead
    readonly property bool seekable: true
    property string title
    property string subtitle
    property string more: ""
    default property alias actions: actionRow.data
    Layout.fillWidth: true
    spacing: 3
    // (None: a page's first section, under the page's own title.)
    GridLayout {
        visible: sectionHead.title !== "" || actionRow.children.length > 0
        Layout.fillWidth: true
        // (Narrow: the actions go under the heading rather than squeeze it.)
        columns: actionRow.implicitWidth > sectionHead.width * 0.55 ? 1 : 2
        columnSpacing: 8
        rowSpacing: 8
        Label { Layout.fillWidth: true; visible: sectionHead.title !== ""; text: sectionHead.title; color: Theme.text; font.pixelSize: Theme.px(16); font.weight: Font.DemiBold; font.letterSpacing: -0.1; wrapMode: Text.Wrap }
        Item { visible: sectionHead.title === ""; Layout.fillWidth: true }
        Row { id: actionRow; spacing: 6; Layout.alignment: Qt.AlignVCenter }
    }
    MoreText { visible: sectionHead.subtitle !== ""; full: sectionHead.subtitle; more: sectionHead.more; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true }
}
