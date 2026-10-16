import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Picks your avatar: the six penguins in a grid; the chosen one ringed.
// Saved with the look (Workspace.chooseLook("avatar", …)).
Popup {
    id: picker
    padding: 14
    margins: 8
    background: GlassSurface { radius: 12 }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Math.round(Theme.motion * 0.56) }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Math.round(Theme.motion * 0.56); easing.type: Easing.OutCubic }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.44) }
    }

    contentItem: ColumnLayout {
        spacing: 10
        Label {
            text: "Your avatar"
            color: Theme.text
            font.pixelSize: Theme.px(13.5)
            font.weight: Font.DemiBold
        }
        GridLayout {
            columns: 3
            rowSpacing: 8
            columnSpacing: 8
            Repeater {
                model: Theme.avatars
                delegate: Item {
                    id: tile
                    required property string modelData
                    readonly property bool chosen: Theme.avatar === modelData
                    objectName: "avatar-" + modelData
                    implicitWidth: 64
                    implicitHeight: 64
                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        color: "transparent"
                        border.width: 2
                        border.color: tile.chosen ? Theme.accent : tileHover.hovered ? Theme.hairline : "transparent"
                    }
                    UserAvatar {
                        anchors.centerIn: parent
                        name: tile.modelData
                        width: 54
                        height: 54
                        scale: tileHover.hovered && !tile.chosen ? 1.04 : 1
                        Behavior on scale { NumberAnimation { duration: Theme.motion } }
                    }
                    HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: {
                            Workspace.chooseLook("avatar", JSON.stringify(tile.modelData))
                            picker.close()
                        }
                    }
                }
            }
        }
    }
}
