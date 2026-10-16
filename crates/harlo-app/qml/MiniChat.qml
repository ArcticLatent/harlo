import QtQuick
import QtQuick.Window
import QtCore
import harlo

// A small floating chat window (Ctrl+Shift+M, the tray, `harlo --mini-chat`):
// the active session with a composer, like the drop-down, in a window of
// its own that remembers its place and can stay on top.
Window {
    id: mini
    signal openInWindowRequested()
    title: "harlo — mini chat"
    width: 440
    height: 600
    minimumWidth: 320
    minimumHeight: 320
    color: Theme.background
    property bool onTop: Workspace.setting("mini_chat_on_top") !== "0"
    flags: Qt.Window | (onTop ? Qt.WindowStaysOnTopHint : 0)

    Settings {
        category: "miniChat"
        property alias x: mini.x
        property alias y: mini.y
        property alias width: mini.width
        property alias height: mini.height
    }

    function focusInput() { content.focusInput() }
    readonly property alias content: content

    DropDownContent {
        id: content
        anchors.fill: parent
        pinnable: true
        onTop: mini.onTop
        onOnTopToggled: on => { mini.onTop = on; Workspace.setSetting("mini_chat_on_top", on ? "1" : "0") }
        onHideRequested: mini.visible = false
        onOpenInWindowRequested: mini.openInWindowRequested()
    }
}
