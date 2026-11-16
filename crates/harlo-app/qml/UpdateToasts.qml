pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Agent updates: a notice per newer version
// (here or on a host) with Dismiss / Update; while it updates, a bar;
// then what happened, with its output a click away. A finished update
// goes by itself after a while (not while the pointer is on it); a failed
// one, and an offer, stay until dismissed. A notice slides in from the
// side and fades out as it goes (Settings → Appearance → Animations).
ToastStack {
    id: root
    signal viewOutput(string command)
    entries: Updates.entriesJson !== "" ? JSON.parse(Updates.entriesJson) : []
    /// How long a finished update's notice stays (ms).
    property int doneTimeout: 10000

    card: ToastCard {
        id: toast
        stack: root
        namePrefix: "update-"
        spacing: 3
        iconName: toast.entry.state === "failed" ? "alert" : toast.entry.state === "done" ? "checkbox-circle" : "information"
        iconColor: toast.entry.state === "failed" ? Theme.error : toast.entry.state === "done" ? Theme.success : Theme.info
        Timer {
            interval: root.doneTimeout
            // (harlo's own stays: it waits for a restart.)
            running: toast.entry.state === "done" && toast.entry.agent !== "harlo" && !toast.hovered && !toast.leaving
            onTriggered: Updates.dismiss(toast.entry.key)
        }
        Label {
            Layout.fillWidth: true
            text: toast.entry.label + " update" + (toast.entry.host !== "" ? " · " + toast.entry.host : "")
            color: Theme.foreground
            font.pixelSize: Theme.px(13)
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Label {
            objectName: "updateMessage-" + toast.entry.key
            Layout.fillWidth: true
            text: toast.entry.message
            color: toast.entry.state === "failed" ? Theme.error : Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        // harlo's: what's new, in short (Read more: all of it).
        Label {
            objectName: "updateNotes-" + toast.entry.key
            visible: toast.entry.state === "available" && (toast.entry.notes || "") !== ""
            Layout.fillWidth: true
            Layout.topMargin: 4
            text: toast.entry.notes || ""
            color: Theme.foreground
            font.pixelSize: Theme.px(12)
            lineHeight: 1.15
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
        // Updating: a bar that runs until it's done.
        Rectangle {
            visible: toast.entry.state === "running"
            Layout.fillWidth: true
            Layout.topMargin: 4
            implicitHeight: 4
            radius: 2
            clip: true
            color: Theme.border
            Rectangle {
                id: runner
                width: parent.width * 0.35
                height: parent.height
                radius: 2
                color: Theme.primary
                NumberAnimation on x {
                    running: toast.entry.state === "running"
                    loops: Animation.Infinite
                    from: -runner.width
                    to: runner.parent.width
                    duration: 1200
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            spacing: 8
            Button {
                objectName: "updateReadMore-" + toast.entry.key
                visible: (toast.entry.notesUrl || "") !== ""
                flat: true
                padding: 2
                contentItem: Label { text: "Read more"; color: Theme.accent; font.pixelSize: Theme.px(12); font.underline: true }
                background: null
                onClicked: Qt.openUrlExternally(toast.entry.notesUrl)
            }
            Item { Layout.fillWidth: true }
            FlatButton {
                objectName: "updateOutput-" + toast.entry.key
                // (Something ran: an AppImage's update just replaces its file.)
                visible: toast.entry.state !== "available" && toast.entry.output === true
                text: "View output"
                font.pixelSize: Theme.px(12)
                onClicked: root.viewOutput(Updates.outputCommand(toast.entry.key))
            }
            FlatButton {
                objectName: "updateDismiss-" + toast.entry.key
                visible: toast.entry.state !== "running"
                text: "Dismiss"
                font.pixelSize: Theme.px(12)
                onClicked: Updates.dismiss(toast.entry.key)
            }
            DialogButton {
                objectName: "updateRestart-" + toast.entry.key
                visible: toast.entry.agent === "harlo" && toast.entry.state === "done"
                text: "Restart harlo"
                accent: true
                implicitHeight: 28
                font.pixelSize: Theme.px(12)
                onClicked: { Updates.restartHarlo(); Qt.quit() }
            }
            DialogButton {
                objectName: "updateGo-" + toast.entry.key
                visible: toast.entry.state === "available"
                text: "Update"
                accent: true
                implicitHeight: 28
                font.pixelSize: Theme.px(12)
                onClicked: Updates.update(toast.entry.key)
            }
        }
    }
}
