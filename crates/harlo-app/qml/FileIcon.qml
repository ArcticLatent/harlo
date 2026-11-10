import QtQuick
import harlo

// The file-type icon for `path` (a file name or path): the type's glyph in
// the bundled "Harlo File Icons" font, in its colour, filling a 16-pixel box
// (or the box it's given).
Text {
    objectName: "fileIcon"
    property string path
    /// "#rrggbb" then the glyph (Workspace.fileIcon); a list that has it
    /// already sets this instead of `path`.
    property string spec: path !== "" ? Workspace.fileIcon(path) : ""
    width: 16
    height: 16
    text: spec.substring(7)
    color: spec !== "" ? Theme.fileTint(spec.substring(0, 7)) : "transparent"
    font.family: "Harlo File Icons"
    font.pixelSize: Math.min(width, height)
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    textFormat: Text.PlainText
    Accessible.ignored: true
}
