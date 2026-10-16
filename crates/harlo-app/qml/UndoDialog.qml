import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Undoing a turn: what goes back as it was before your message (the files
// the agent changed or deleted), what goes (the files it made), and what
// stays (commits, anything outside the folder); then how it went.
AppDialog {
    id: dlg
    objectName: "undoDialog"
    required property ChatSession session
    signal undone()
    readonly property var plan: session.undoPlanJson !== "" ? JSON.parse(session.undoPlanJson) : ({ state: "" })
    readonly property int changes: (plan.restore || []).length + (plan.remove || []).length
    readonly property bool busy: session.status === "busy"
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
    title: (plan.later || 0) > 0 ? "Undo from this message" : "Undo this turn"

    function show(rowKey) {
        session.previewUndo(rowKey)
        open()
    }
    onPlanChanged: if (plan.state === "done") undone()

    component PathList: ColumnLayout {
        property string heading
        property var paths: []
        visible: paths.length > 0
        Layout.fillWidth: true
        spacing: 2
        Label { text: parent.heading + " (" + parent.paths.length + ")"; color: Theme.text; font.pixelSize: Theme.px(13); font.weight: Font.Medium; Layout.topMargin: 6 }
        Repeater {
            model: parent.paths.slice(0, 8)
            Label {
                required property string modelData
                Layout.fillWidth: true
                text: modelData
                elide: Text.ElideMiddle
                color: Theme.textMuted
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
            }
        }
        Label {
            visible: parent.paths.length > 8
            text: "and " + (parent.paths.length - 8) + " more"
            color: Theme.textFaint
            font.pixelSize: Theme.px(12)
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: 6
        RowLayout {
            visible: dlg.plan.state === "loading" || dlg.plan.state === "undoing"
            spacing: 8
            BusyIndicator { running: parent.visible; implicitWidth: 18; implicitHeight: 18 }
            Label { text: dlg.plan.state === "undoing" ? "Undoing…" : "Checking what changed…"; color: Theme.textMuted; font.pixelSize: Theme.px(13) }
        }
        Label {
            visible: dlg.plan.state === "ready"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.text
            font.pixelSize: Theme.px(13.5)
            text: dlg.changes === 0 ? "Nothing to undo: the files are as they were before this message."
                : "The folder's files go back to how they were before this message"
                  + ((dlg.plan.later || 0) > 0 ? ": this turn and the " + (dlg.plan.later === 1 ? "one" : dlg.plan.later) + " after it." : ".")
        }
        PathList { visible: dlg.plan.state === "ready" && paths.length > 0; heading: "Brought back"; paths: dlg.plan.restore || [] }
        PathList { visible: dlg.plan.state === "ready" && paths.length > 0; heading: "Removed (new since)"; paths: dlg.plan.remove || [] }
        Label {
            visible: dlg.plan.state === "ready" && dlg.changes > 0
            Layout.fillWidth: true
            Layout.topMargin: 6
            wrapMode: Text.Wrap
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
            text: "Edits you made in these files since are undone too. What's there now is kept in git, so this can be reversed."
                  + (dlg.plan.headMoved ? " Commits made since stay; only the files change." : "")
                  + " Things outside the folder (installed packages, a database, a push) stay as they are."
        }
        Label {
            visible: dlg.plan.state === "ready" && dlg.busy
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: "The agent is still working: stop it first."
            color: Theme.warning
            font.pixelSize: Theme.px(12.5)
        }
        Label {
            objectName: "undoResult"
            visible: dlg.plan.state === "done" || dlg.plan.state === "failed"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlg.plan.message || ""
            color: dlg.plan.state === "failed" ? Theme.error : Theme.success
            font.pixelSize: Theme.px(13.5)
        }
    }

    footer: Item {
        implicitHeight: buttons.implicitHeight + 26
        RowLayout {
            id: buttons
            x: 20
            y: 8
            width: parent.width - 40
            spacing: 8
            Item { Layout.fillWidth: true }
            DialogButton {
                text: dlg.plan.state === "done" || dlg.plan.state === "failed" || dlg.changes === 0 && dlg.plan.state === "ready" ? "Close" : "Cancel"
                onClicked: dlg.close()
            }
            DialogButton {
                objectName: "undoConfirm"
                visible: dlg.plan.state === "ready" && dlg.changes > 0
                text: "Undo"
                accent: true
                enabled: !dlg.busy
                onClicked: dlg.session.undoTurn(dlg.plan.row)
            }
        }
    }
}
