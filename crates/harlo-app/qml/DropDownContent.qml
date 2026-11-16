import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// What the drop-down window shows: the active session (its own view of it)
// with a composer, or a quick way to start one.
Rectangle {
    id: root
    signal hideRequested()
    signal openInWindowRequested()
    /// The mini-chat window: a keep-on-top toggle in the header.
    property bool pinnable: false
    property bool stayOnTop: false
    signal stayOnTopToggled(bool on)
    color: Theme.background
    border.color: Theme.border

    ChatSession { id: activeSession }
    // (Bound only while its window shows: hidden, it lets the session go
    // instead of keeping a second copy of every session switched to.)
    readonly property bool shown: Window.window !== null && Window.window.visible
    readonly property string key: !shown || Workspace.activeKey.startsWith("view:") || Workspace.activeKey.startsWith("file:") ? "" : Workspace.activeKey
    onKeyChanged: activeSession.bind(key)
    Component.onCompleted: activeSession.bind(key)
    function focusInput() { if (root.key !== "") composer.focusInput(); else quickField.forceActiveFocus() }

    Keys.onEscapePressed: root.hideRequested()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 1
        spacing: 0

        // Header
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 36
            color: Theme.subtle
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 6
                spacing: 8
                Image { source: "qrc:/qt/qml/harlo/icons/app/harlo.svg"; sourceSize: Qt.size(18, 18) }
                PlainLabel {
                    Layout.fillWidth: true
                    text: root.key !== "" ? (activeSession.title || "New session") : "harlo"
                    color: Theme.foreground
                    font.pixelSize: Theme.px(13)
                    font.bold: true
                    elide: Text.ElideRight
                }
                Chip { visible: activeSession.status !== ""; text: activeSession.status === "offline" ? "saved" : activeSession.status; fg: Theme.statusColor(activeSession.status) }
                FlatButton {
                    visible: root.pinnable
                    objectName: "keepOnTop"
                    text: "📌"
                    font.pixelSize: Theme.px(12)
                    padding: 4
                    outlined: root.stayOnTop
                    accent: root.stayOnTop ? Theme.primary : Theme.muted
                    onClicked: root.stayOnTopToggled(!root.stayOnTop)
                    ToolTip.visible: hovered
                    ToolTip.text: root.stayOnTop ? "Stays on top of other windows (click to stop)" : "Keep on top of other windows"
                }
                FlatButton { text: "Open in window"; font.pixelSize: Theme.px(11); padding: 4; onClicked: root.openInWindowRequested() }
                Button {
                    id: hideButton
                    flat: true
                    implicitWidth: 26
                    implicitHeight: 26
                    onClicked: root.hideRequested()
                    contentItem: Icon { name: "close"; color: hideButton.hovered ? Theme.foreground : Theme.muted }
                    background: Rectangle { radius: 5; color: hideButton.hovered ? Theme.hover : "transparent" }
                    ToolTip.visible: hovered
                    ToolTip.text: "Hide (Esc)"
                }
            }
        }

        // The active session
        TranscriptView {
            id: transcript
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.key !== ""
            session: activeSession
        }
        Composer {
            id: composer
            Layout.fillWidth: true
            Layout.margins: 10
            visible: root.key !== ""
            session: activeSession
            onSubmitted: (text, mode) => {
                const busy = activeSession.status === "busy"
                activeSession.send(text, busy ? (mode === "steer" ? "steer" : "followUp") : "normal")
                composer.text = ""
                transcript.scrollToEnd()
            }
        }

        // No session open: start one in the last project.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 24
            visible: root.key === ""
            spacing: 10
            Item { Layout.fillHeight: true }
            Label { text: "What are we working on?"; color: Theme.foreground; font.pixelSize: Theme.px(20); Layout.alignment: Qt.AlignHCenter }
            AppTextField {
                id: quickField
                Layout.fillWidth: true
                Layout.maximumWidth: 700
                Layout.alignment: Qt.AlignHCenter
                placeholderText: "Start a session in the home folder… (Enter)"
                onAccepted: {
                    if (text.trim() === "") return
                    Workspace.startDraft(JSON.stringify({ projectId: -1, harness: "", worktreeBranch: "", base: "", starter: "", text: text, attachments: [] }))
                    text = ""
                }
            }
            Item { Layout.fillHeight: true }
        }
    }
}
