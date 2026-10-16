import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// One omp subagent: its transcript (read from its session file, refreshed
// while it runs) and a field to steer it.
Popup {
    id: root
    property alias transcriptSession: transcriptSession
    property ChatSession parentSession
    property var agent: ({})
    readonly property var live: {
        // The latest snapshot of this agent from the parent session.
        const list = parentSession && parentSession.subagentsJson !== "" ? JSON.parse(parentSession.subagentsJson) : []
        return list.find(a => a.id === agent.id) || agent
    }
    readonly property bool running: live.status === "running" || live.status === "pending"

    function show(a, steer) {
        agent = a
        open()
        transcriptSession.bindFile(a.sessionFile || "", parentSession ? parentSession.harness : "omp", parentSession ? parentSession.cwd : "")
        if (steer) steerField.forceActiveFocus()
    }

    // Centred on the window, not the chat pane.
    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(900, parent.width - 48)
    height: Math.min(760, parent.height - 48)
    padding: 0
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }
    onClosed: transcriptSession.bind("")

    ChatSession { id: transcriptSession }
    // Re-read the transcript while the subagent works.
    Timer {
        interval: 2000
        repeat: true
        running: root.opened && root.running && !!root.agent.sessionFile
        property string stamp: ""
        onTriggered: {
            const now = Workspace.fileStamp(root.agent.sessionFile)
            if (now !== stamp) {
                stamp = now
                transcriptSession.bindFile(root.agent.sessionFile, root.parentSession ? root.parentSession.harness : "omp", root.parentSession ? root.parentSession.cwd : "")
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 0
        // Header
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 16
            Layout.bottomMargin: 10
            spacing: 10
            Rectangle {
                implicitWidth: 32
                implicitHeight: 32
                radius: 8
                color: Theme.selected
                Icon { anchors.centerIn: parent; name: "robot-2"; color: Theme.primary; implicitWidth: 18; implicitHeight: 18 }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label { text: (root.live.name || root.live.id || "") + "  ·  " + (root.live.agent || "task"); color: Theme.foreground; font.pixelSize: Theme.px(15); font.bold: true }
                Label {
                    Layout.fillWidth: true
                    text: [root.live.status, root.live.progress && root.live.progress.resolvedModel ? root.live.progress.resolvedModel : "", root.live.description || ""].filter(x => x).join("  ·  ")
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                }
            }
            FlatButton {
                visible: root.running && root.live.canCancel !== false
                text: "Cancel subagent"
                accent: Theme.error
                font.pixelSize: Theme.px(12)
                onClicked: root.parentSession.cancelSubagent(root.live.id)
            }
            Button {
                id: closeButton
                flat: true
                implicitWidth: 30
                implicitHeight: 30
                onClicked: root.close()
                contentItem: Icon { name: "close"; color: closeButton.hovered ? Theme.foreground : Theme.muted }
                background: Rectangle { radius: 6; color: closeButton.hovered ? Theme.hover : "transparent" }
            }
        }
        // Task
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.bottomMargin: 8
            // The transcript starts with the task; without one, show it here.
            visible: !!root.live.task && !root.agent.sessionFile
            text: root.live.task || ""
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            TranscriptView {
                anchors.fill: parent
                session: transcriptSession
                visible: !!root.agent.sessionFile
            }
            Label {
                anchors.centerIn: parent
                visible: !root.agent.sessionFile
                text: "This subagent has no saved transcript."
                color: Theme.muted
            }
        }

        // Steer (where the provider can).
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border; visible: root.live.canSteer !== false }
        RowLayout {
            visible: root.live.canSteer !== false
            Layout.fillWidth: true
            Layout.margins: 12
            spacing: 8
            AppTextField {
                id: steerField
                objectName: "steerField"
                Layout.fillWidth: true
                // The placeholder may be wider than the dialog.
                Layout.preferredWidth: 0
                enabled: root.running
                placeholderText: root.running ? "Message " + (root.live.id || "the subagent") + " (it reads it at its next step)…"
                    : root.live.status === "failed" ? "The subagent failed."
                    : root.live.status === "aborted" ? "The subagent was cancelled."
                    : "The subagent has finished."
                onAccepted: send()
                function send() {
                    if (text.trim() === "") return
                    root.parentSession.steerSubagent(root.live.id, text.trim())
                    text = ""
                }
            }
            DialogButton { text: "Steer"; accent: true; enabled: root.running && steerField.text.trim() !== ""; onClicked: steerField.send() }
        }
    }
}
