import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Picks a folder on a remote host (Settings → Remote Instances) for a new
// project: starts in the host's home; click a folder to open it.
AppDialog {
    id: dlg
    /// The host being browsed.
    property string host: ""
    signal picked(string path)
    title: "Add a project on " + host
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
    standardButtons: Dialog.Cancel

    readonly property var listing: { try { return JSON.parse(Remotes.foldersJson) } catch (e) { return { path: "", dirs: [], error: "", loading: true } } }
    readonly property bool ready: !listing.loading && listing.error === "" && listing.path.startsWith("ssh://")

    function openFor(name) { host = name; open(); Remotes.requestFolders(name) }
    function openFolder(dir) { Remotes.requestFolders(listing.path.replace(/\/$/, "") + "/" + dir) }
    function up() {
        const p = listing.path, i = p.lastIndexOf("/")
        // ssh://host/a → ssh://host/
        if (i > ("ssh://" + host).length) Remotes.requestFolders(p.substring(0, i))
        else if (p !== "ssh://" + host + "/") Remotes.requestFolders("ssh://" + host + "/")
    }

    ColumnLayout {
        width: parent.width
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            FlatButton {
                objectName: "remoteFolderUp"
                text: "↑"
                font.pixelSize: Theme.px(13)
                enabled: dlg.ready && dlg.listing.path !== "ssh://" + dlg.host + "/"
                onClicked: dlg.up()
                ToolTip.visible: hovered
                ToolTip.text: "Parent folder"
            }
            Label {
                Layout.fillWidth: true
                text: dlg.listing.display || dlg.host
                color: Theme.foreground
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                elide: Text.ElideMiddle
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 300
            radius: 8
            color: Theme.background
            border.color: Theme.border
            Label {
                anchors.centerIn: parent
                width: parent.width - 32
                visible: dlg.listing.loading || dlg.listing.error !== "" || (dlg.ready && dlg.listing.dirs.length === 0)
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: dlg.listing.loading ? "Connecting to " + dlg.host + "…" : dlg.listing.error !== "" ? dlg.listing.error : "No folders here."
                color: dlg.listing.error !== "" ? Theme.error : Theme.muted
                font.pixelSize: Theme.px(12)
            }
            ListView {
                id: folderList
                anchors.fill: parent
                anchors.margins: 4
                clip: true
                visible: dlg.ready
                model: dlg.ready ? dlg.listing.dirs : []
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                delegate: ItemDelegate {
                    id: folderRow
                    required property string modelData
                    width: ListView.view.width
                    implicitHeight: 30
                    leftPadding: 10
                    contentItem: RowLayout {
                        spacing: 8
                        Icon { name: "folder"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                        Label { Layout.fillWidth: true; text: folderRow.modelData; color: Theme.foreground; font.pixelSize: Theme.px(13); elide: Text.ElideRight }
                    }
                    background: Rectangle { radius: 6; color: folderRow.hovered ? Theme.hover : "transparent" }
                    onClicked: dlg.openFolder(modelData)
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            DialogButton {
                objectName: "remoteFolderAdd"
                accent: true
                enabled: dlg.ready
                text: "Add " + (dlg.ready ? (dlg.listing.path.split("/").pop() || "/") : "folder")
                onClicked: { dlg.picked(dlg.listing.path); dlg.close() }
            }
        }
    }
}
