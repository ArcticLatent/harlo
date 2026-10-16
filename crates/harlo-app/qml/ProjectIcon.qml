import QtQuick
import harlo

// A project's icon (Settings → Projects): a preset symbol, or the folder,
// in the project's colour (grey without one).
Item {
    id: pi
    /// A preset's name, or "" (the folder).
    property string icon: ""
    /// A Theme.projectColors key, or "".
    property string color: ""
    property color fallback: Theme.muted
    implicitWidth: 16
    implicitHeight: 16
    Icon {
        anchors.fill: parent
        name: pi.icon !== "" ? pi.icon : "folder-3"
        color: Theme.projectColor(pi.color, pi.fallback)
    }
}
