import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Agent updates: a notice per newer version
// (here or on a host) with Dismiss / Update; while it updates, a bar;
// then what happened, with its output a click away. A finished update
// goes by itself after a while (not while the pointer is on it); a failed
// one, and an offer, stay until dismissed. A notice slides in from the
// side and fades out as it goes (Settings → Appearance → Animations).
Column {
    id: root
    signal viewOutput(string command)
    readonly property var entries: Updates.entriesJson !== "" ? JSON.parse(Updates.entriesJson) : []
    spacing: 8
    width: 380
    /// How long a finished update's notice stays (ms).
    property int doneTimeout: 10000

    // The notices shown, by key: kept (not rebuilt) as they change, so each
    // comes in once and one that's gone fades out first.
    ListModel { id: shown }
    function sync() {
        const keys = root.entries.map(e => e.key)
        for (let i = 0; i < shown.count; i++)
            if (keys.indexOf(shown.get(i).key) < 0) shown.setProperty(i, "leaving", true)
        for (const e of root.entries) {
            let i = 0
            while (i < shown.count && shown.get(i).key !== e.key) i++
            if (i < shown.count) shown.set(i, { json: JSON.stringify(e), leaving: false })
            else shown.append({ key: e.key, json: JSON.stringify(e), leaving: false })
        }
    }
    function remove(key) {
        for (let i = 0; i < shown.count; i++)
            if (shown.get(i).key === key && shown.get(i).leaving) { shown.remove(i); return }
    }
    onEntriesChanged: sync()
    Component.onCompleted: sync()

    add: Transition {
        NumberAnimation { property: "x"; from: 48; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion }
    }
    move: Transition {
        NumberAnimation { property: "y"; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    }

    Repeater {
        model: shown
        Rectangle {
            id: toast
            required property string json
            required property string key
            required property bool leaving
            readonly property var e: JSON.parse(json)
            objectName: leaving ? "" : "update-" + e.key
            // Going: fade and slide away, then out of the list.
            opacity: leaving ? 0 : 1
            Behavior on opacity {
                enabled: Theme.paneMotion > 0
                NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic; onRunningChanged: if (!running && toast.leaving) root.remove(toast.key) }
            }
            onLeavingChanged: if (leaving && Theme.paneMotion === 0) Qt.callLater(root.remove, key)
            transform: Translate { x: toast.leaving ? 32 : 0; Behavior on x { NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic } } }
            width: root.width
            implicitHeight: body.implicitHeight + 24
            radius: 10
            color: Theme.elevated
            border.color: Theme.border
            HoverHandler { id: toastHover }
            Timer {
                interval: root.doneTimeout
                // (harlo's own stays: it waits for a restart.)
                running: toast.e.state === "done" && toast.e.agent !== "harlo" && !toastHover.hovered && !toast.leaving
                onTriggered: Updates.dismiss(toast.e.key)
            }
            // A soft shadow under the card.
            Rectangle { z: -1; anchors.fill: parent; anchors.topMargin: 3; anchors.leftMargin: -1; anchors.rightMargin: -1; anchors.bottomMargin: -4; radius: 12; color: "#000000"; opacity: 0.18 }

            RowLayout {
                id: body
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 12
                Icon {
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 2
                    name: toast.e.state === "failed" ? "alert" : toast.e.state === "done" ? "checkbox-circle" : "information"
                    color: toast.e.state === "failed" ? Theme.error : toast.e.state === "done" ? Theme.success : Theme.info
                    implicitWidth: 18
                    implicitHeight: 18
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 3
                    Label {
                        Layout.fillWidth: true
                        text: toast.e.label + " update" + (toast.e.host !== "" ? " · " + toast.e.host : "")
                        color: Theme.foreground
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    Label {
                        objectName: "updateMessage-" + toast.e.key
                        Layout.fillWidth: true
                        text: toast.e.message
                        color: toast.e.state === "failed" ? Theme.error : Theme.muted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                    // harlo's: what's new, in short (Read more: all of it).
                    Label {
                        objectName: "updateNotes-" + toast.e.key
                        visible: toast.e.state === "available" && (toast.e.notes || "") !== ""
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        text: toast.e.notes || ""
                        color: Theme.foreground
                        font.pixelSize: Theme.px(12)
                        lineHeight: 1.15
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                    }
                    // Updating: a bar that runs until it's done.
                    Rectangle {
                        visible: toast.e.state === "running"
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
                                running: toast.e.state === "running"
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
                            objectName: "updateReadMore-" + toast.e.key
                            visible: (toast.e.notesUrl || "") !== ""
                            flat: true
                            padding: 2
                            contentItem: Label { text: "Read more"; color: Theme.accent; font.pixelSize: Theme.px(12); font.underline: true }
                            background: null
                            onClicked: Qt.openUrlExternally(toast.e.notesUrl)
                        }
                        Item { Layout.fillWidth: true }
                        FlatButton {
                            objectName: "updateOutput-" + toast.e.key
                            // (Something ran: an AppImage's update just replaces its file.)
                            visible: toast.e.state !== "available" && toast.e.output === true
                            text: "View output"
                            font.pixelSize: Theme.px(12)
                            onClicked: root.viewOutput(Updates.outputCommand(toast.e.key))
                        }
                        FlatButton {
                            objectName: "updateDismiss-" + toast.e.key
                            visible: toast.e.state !== "running"
                            text: "Dismiss"
                            font.pixelSize: Theme.px(12)
                            onClicked: Updates.dismiss(toast.e.key)
                        }
                        DialogButton {
                            objectName: "updateRestart-" + toast.e.key
                            visible: toast.e.agent === "harlo" && toast.e.state === "done"
                            text: "Restart harlo"
                            accent: true
                            implicitHeight: 28
                            onClicked: { Updates.restartHarlo(); Qt.quit() }
                        }
                        DialogButton {
                            objectName: "updateGo-" + toast.e.key
                            visible: toast.e.state === "available"
                            text: "Update"
                            accent: true
                            implicitHeight: 28
                            onClicked: Updates.update(toast.e.key)
                        }
                    }
                }
            }
        }
    }
}
