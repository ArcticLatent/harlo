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
        DialogButton { objectName: "quitRunsQuit"; text: qsTr("Quit and pause"); onClicked: Qt.quit() }
        DialogButton {
            objectName: "quitRunsKeep"
            text: dialog.count === 1 ? qsTr("Keep it running") : qsTr("Keep them running")
            accent: true
            onClicked: { dialog.close(); dialog.keepRequested() }
        }
    }
    PlainLabel {
        width: parent.width
        wrapMode: Text.Wrap
        color: Theme.muted
        text: dialog.count === 1
            ? qsTr("Harlo can close its window and finish the run, then quit. If you quit now, the run pauses and goes on when you next open harlo.")
            : qsTr("Harlo can close its window and finish the runs, then quit. If you quit now, the runs pause and go on when you next open harlo.")
    }
}
