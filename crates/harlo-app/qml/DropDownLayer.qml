import QtQuick
import QtQuick.Window
import org.kde.layershell as LayerShell
import harlo

// The drop-down window on Wayland desktops with layer-shell (KDE): pinned
// to the top edge across the screen. (DropDownPlain.qml is
// the fallback.)
Window {
    id: win
    signal openInWindowRequested()
    property real heightFraction: 0.5
    height: Math.round(Screen.height * heightFraction)
    width: Screen.width
    color: "transparent"
    flags: Qt.FramelessWindowHint

    LayerShell.Window.scope: "harlo-dropdown"
    LayerShell.Window.layer: LayerShell.Window.LayerTop
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft | LayerShell.Window.AnchorRight
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityOnDemand
    LayerShell.Window.exclusionZone: -1

    function focusInput() { content.focusInput() }

    DropDownContent {
        id: content
        anchors.fill: parent
        onHideRequested: win.visible = false
        onOpenInWindowRequested: win.openInWindowRequested()
    }
}
