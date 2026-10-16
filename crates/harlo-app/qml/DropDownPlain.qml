import QtQuick
import QtQuick.Window
import harlo

// The drop-down window without layer-shell: a frameless, always-on-top
// window across the top of the screen (X11 places it; Wayland compositors
// may not).
Window {
    id: win
    signal openInWindowRequested()
    property real heightFraction: 0.5
    x: Screen.virtualX
    y: Screen.virtualY
    width: Screen.width
    height: Math.round(Screen.height * heightFraction)
    color: Theme.background
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool

    function focusInput() { content.focusInput() }

    DropDownContent {
        id: content
        anchors.fill: parent
        onHideRequested: win.visible = false
        onOpenInWindowRequested: win.openInWindowRequested()
    }
}
