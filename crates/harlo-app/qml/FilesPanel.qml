import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The active session's folder as a tree; clicking a file opens it in a tab.
Rectangle {
    id: root
    required property string folder
    signal closeRequested()
    /// A file was clicked; the owner shows it (after any unsaved-edits prompt).
    signal openRequested(string path)
    color: Theme.canvas
    clip: true

    property alias browser: browser
    FileBrowser { id: browser }
    onFolderChanged: browser.setRoot(folder)
    Component.onCompleted: browser.setRoot(folder)

    function absolute(rel) { return root.folder + "/" + rel }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Files"
            subtitle: root.folder.split("/").pop()
            onCloseRequested: root.closeRequested()
            PanelButton { iconName: "refresh"; tip: "Refresh"; onClicked: browser.refresh() }
        }

        Label {
            Layout.margins: 18
            visible: root.folder === ""
            text: "No session selected."
            color: Theme.muted
        }

        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 8
            visible: root.folder !== ""
            clip: true
            model: browser
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: ItemDelegate {
                id: node
                required property int index
                required property string rel
                required property string name
                required property int depth
                required property bool isDir
                required property bool expanded
                required property string fileIcon
                readonly property bool open: Workspace.activeKey === "file:" + root.absolute(rel)
                width: ListView.view.width
                height: 32
                topPadding: 0
                bottomPadding: 0
                leftInset: 8
                rightInset: 8
                leftPadding: 18 + depth * 16
                rightPadding: 16
                // A soft pill, as the sidebar's rows.
                background: Rectangle { radius: 8; color: node.open ? Theme.rowSelected : node.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                contentItem: RowLayout {
                    spacing: 9
                    Icon {
                        visible: node.isDir
                        name: node.expanded ? "folder-open" : "folder"
                        color: node.expanded ? Theme.accent : Theme.textMuted
                    }
                    Image {
                        visible: !node.isDir
                        source: node.fileIcon
                        sourceSize: Qt.size(16, 16)
                        Layout.preferredWidth: 16
                        Layout.preferredHeight: 16
                    }
                    Label {
                        Layout.fillWidth: true
                        text: node.name
                        color: node.open || node.isDir ? Theme.text : Theme.textMuted
                        font.pixelSize: Theme.px(13.5)
                        font.weight: node.open ? Font.Medium : Font.Normal
                        elide: Text.ElideRight
                    }
                }
                onClicked: node.isDir ? browser.toggle(index) : root.openRequested(root.absolute(node.rel))
            }
        }
    }
}
