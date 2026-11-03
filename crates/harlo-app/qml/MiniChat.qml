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
    property bool stayOnTop: Workspace.setting("mini_chat_on_top") !== "0"
    flags: Qt.Window | (stayOnTop ? Qt.WindowStaysOnTopHint : 0)
    // (On KDE's Wayland the hint alone does nothing: asked of the desktop,
    // once the window is up.)
    onStayOnTopChanged: if (visible) Workspace.keepWindowAbove(title, stayOnTop)
    onVisibleChanged: if (visible && stayOnTop) keepAbove.restart()
    Timer { id: keepAbove; interval: 300; onTriggered: Workspace.keepWindowAbove(mini.title, mini.stayOnTop) }

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
        stayOnTop: mini.stayOnTop
        onStayOnTopToggled: on => { mini.stayOnTop = on; Workspace.setSetting("mini_chat_on_top", on ? "1" : "0") }
        onHideRequested: mini.visible = false
        onOpenInWindowRequested: mini.openInWindowRequested()
    }
}
