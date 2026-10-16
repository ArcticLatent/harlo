import QtQuick
import QtQuick.Controls
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
    property bool onTop: false
    signal onTopToggled(bool on)
    color: Theme.background
    border.color: Theme.border

    ChatSession { id: session }
    readonly property string key: Workspace.activeKey.startsWith("view:") || Workspace.activeKey.startsWith("file:") ? "" : Workspace.activeKey
    onKeyChanged: session.bind(key)
    Component.onCompleted: session.bind(key)
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
                Label {
                    Layout.fillWidth: true
                    text: root.key !== "" ? (session.title || "New session") : "harlo"
                    color: Theme.foreground
                    font.pixelSize: Theme.px(13)
                    font.bold: true
                    elide: Text.ElideRight
                }
                Chip { visible: session.status !== ""; text: session.status === "offline" ? "saved" : session.status; fg: Theme.statusColor(session.status) }
                FlatButton {
                    visible: root.pinnable
                    objectName: "keepOnTop"
                    text: "📌"
                    font.pixelSize: Theme.px(12)
                    padding: 4
                    outlined: root.onTop
                    accent: root.onTop ? Theme.primary : Theme.muted
                    onClicked: root.onTopToggled(!root.onTop)
                    ToolTip.visible: hovered
                    ToolTip.text: root.onTop ? "Stays on top of other windows (click to stop)" : "Keep on top of other windows"
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
            session: session
        }
        Composer {
            id: composer
            Layout.fillWidth: true
            Layout.margins: 10
            visible: root.key !== ""
            session: session
            onSubmitted: (text, mode) => {
                const busy = session.status === "busy"
                session.send(text, busy ? (mode === "steer" ? "steer" : "followUp") : "normal")
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
                    Workspace.startDraft(JSON.stringify({ projectId: -1, harness: "", worktreeBranch: "", base: "", starter: "", text: text, attachments: [], askFirst: false }))
                    text = ""
                }
            }
            Item { Layout.fillHeight: true }
        }
    }
}
