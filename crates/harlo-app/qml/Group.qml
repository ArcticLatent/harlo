import QtQuick
import QtQuick.Layouts
import harlo

// A bordered group of setting rows.
Rectangle {
    default property alias rows: groupCol.data
    Layout.fillWidth: true
    Layout.topMargin: 12
    implicitHeight: groupCol.implicitHeight
    radius: 4
    color: Theme.raised
    border.color: Theme.hairline
    ColumnLayout { id: groupCol; width: parent.width; spacing: 0 }
}
