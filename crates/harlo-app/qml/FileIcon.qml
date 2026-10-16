import QtQuick
import harlo

// The file-type icon for `path` (a file name or path).
Image {
    property string path
    source: path !== "" ? Workspace.fileIcon(path) : ""
    sourceSize: Qt.size(16, 16)
    width: 16
    height: 16
}
