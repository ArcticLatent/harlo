import QtQuick
import QtQuick.Controls
import harlo

// Pictures in a message, a tool call or a reply, as thumbnails: a click (or
// Enter) opens one in the viewer over the chat. One that's gone from disk
// says so.
Flow {
    id: strip
    objectName: "pictureStrip"
    /// The pictures' files.
    property var paths: []
    /// A thumbnail's side (logical pixels).
    property real side: 112
    signal openRequested(string path)
    spacing: 6
    visible: paths.length > 0

    Repeater {
        model: strip.paths
        Rectangle {
            id: tile
            objectName: "pictureTile"
            required property string modelData
            required property int index
            readonly property alias thumb: thumb
            width: strip.side
            height: strip.side
            radius: 8
            color: Theme.codeBg
            border.width: activeFocus ? 2 : 1
            border.color: activeFocus ? Theme.accent : tileHover.hovered ? Theme.controlBorderHover : Theme.border
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Picture " + modelData.substring(modelData.lastIndexOf("/") + 1)
            ImageThumb {
                id: thumb
                anchors.fill: parent
                anchors.margins: 4
                path: tile.modelData
            }
            Label {
                anchors.centerIn: parent
                width: parent.width - 12
                visible: thumb.status === "error"
                text: "No longer on disk"
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.textMuted
                font.pixelSize: Theme.px(11)
            }
            HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: strip.openRequested(tile.modelData) }
            Keys.onReturnPressed: strip.openRequested(tile.modelData)
            Keys.onEnterPressed: strip.openRequested(tile.modelData)
            Keys.onSpacePressed: strip.openRequested(tile.modelData)
            ToolTip.visible: tileHover.hovered
            ToolTip.delay: 600
            ToolTip.text: tile.modelData.substring(tile.modelData.lastIndexOf("/") + 1)
        }
    }
}
