import QtQuick
import harlo

// Your avatar (Theme.avatar, or `name`): a round penguin picture.
Item {
    id: avatar
    property string name: Theme.avatar
    readonly property alias source: picture.source
    implicitWidth: 32
    implicitHeight: 32
    Image {
        id: picture
        anchors.fill: parent
        source: Theme.avatarUrl(avatar.name)
        sourceSize: Qt.size(128, 128)
        fillMode: Image.PreserveAspectFit
        smooth: true
        mipmap: true
    }
}
