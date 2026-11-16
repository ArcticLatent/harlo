import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A section that waits for phone access says so once, under its heading.
RowLayout {
    objectName: "needsPhone"
    visible: !Phone.enabled
    Layout.topMargin: 8
    spacing: 6
    Icon { name: "information"; color: Theme.warning; implicitWidth: 14; implicitHeight: 14 }
    Label { Layout.fillWidth: true; text: "Turn on phone access above first."; color: Theme.warning; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap }
}
