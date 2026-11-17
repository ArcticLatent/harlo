import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Quit while workflow runs are under way: keep harlo going without its
// window (it quits once they're over), or quit now (they pick up where they
// were when harlo opens again).
AppDialog {
    id: dialog
    objectName: "quitRunsDialog"
    property int count: 0
    signal keepRequested()
    title: count === 1 ? qsTr("A workflow run is under way") : qsTr("%1 workflow runs are under way").arg(count)
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 460
    footer: AppButtonBox {
        DialogButton { objectName: "quitRunsQuit"; text: qsTr("Quit"); onClicked: Qt.quit() }
        DialogButton {
            objectName: "quitRunsKeep"
            text: qsTr("Keep running without the window")
            accent: true
            onClicked: { dialog.close(); dialog.keepRequested() }
        }
    }
    PlainLabel {
        width: parent.width
        wrapMode: Text.Wrap
        color: Theme.muted
        text: dialog.count === 1
            ? qsTr("Kept running, harlo goes on with it without its window and quits when it's over; open harlo again to see it. Quit now, and it picks up where it was when harlo next opens.")
            : qsTr("Kept running, harlo goes on with them without its window and quits when they're over; open harlo again to see them. Quit now, and they pick up where they were when harlo next opens.")
    }
}
