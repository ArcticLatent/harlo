import QtQuick
import Qt.labs.platform as Platform
import harlo

// The system tray icon: shows whether sessions are working or waiting for
// you, and lists them. Loaded through a Loader (an Item root), so desktops
// without a tray simply go without.
Item {
    id: root
    property var window: null
    signal newSessionRequested()
    signal dropDownRequested()
    signal miniChatRequested()
    readonly property bool available: tray.available

    function showWindow() {
        window.show()
        window.raise()
        window.requestActivate()
    }
    function toggleWindow() {
        if (window.visible && window.active) window.hide()
        else showWindow()
    }

    Platform.SystemTrayIcon {
        id: tray
        visible: true
        icon.source: Workspace.inputCount > 0 ? "qrc:/qt/qml/harlo/icons/app/tray-input.svg"
                   : Workspace.busyCount > 0 ? "qrc:/qt/qml/harlo/icons/app/tray-busy.svg"
                   : "qrc:/qt/qml/harlo/icons/app/tray.svg"
        tooltip: Workspace.inputCount > 0 ? Workspace.inputCount + (Workspace.inputCount === 1 ? " session needs" : " sessions need") + " your input"
               : Workspace.busyCount > 0 ? Workspace.busyCount + (Workspace.busyCount === 1 ? " session" : " sessions") + " working"
               : "harlo"
        onActivated: reason => { if (reason === Platform.SystemTrayIcon.Trigger) root.toggleWindow() }

        property var sessions: []
        menu: Platform.Menu {
            id: menu
            // Not open to begin with: KDE's menus take "visible" (true by
            // default) as "show it now", which on Wayland pops an unanchored
            // menu at startup ("Failed to create grabbing popup"). The tray
            // still opens it on a click.
            visible: false
            onAboutToShow: tray.sessions = JSON.parse(Workspace.runningSessions())
            Platform.MenuItem {
                text: root.window && root.window.visible ? "Hide harlo" : "Show harlo"
                onTriggered: root.toggleWindow()
            }
            Platform.MenuItem {
                text: "New session"
                onTriggered: { root.showWindow(); root.newSessionRequested() }
            }
            Platform.MenuItem {
                text: "Drop-down window"
                onTriggered: root.dropDownRequested()
            }
            Platform.MenuItem {
                text: "Mini chat"
                onTriggered: root.miniChatRequested()
            }
            Platform.MenuSeparator { }
            Platform.MenuItem {
                visible: tray.sessions.length === 0
                enabled: false
                text: "No running sessions"
            }
            // Running sessions are inserted here.
            Platform.MenuSeparator { }
            Platform.MenuItem {
                text: "Quit harlo"
                onTriggered: Qt.quit()
            }
        }
    }
    Instantiator {
        model: tray.sessions
        delegate: Platform.MenuItem {
            required property var modelData
            text: (modelData.status === "input" ? "● Needs input: " : modelData.status === "busy" ? "◐ " : "") + modelData.title
            onTriggered: { root.showWindow(); Workspace.open(modelData.key) }
        }
        onObjectAdded: (index, object) => menu.insertItem(6 + index, object)
        onObjectRemoved: (index, object) => menu.removeItem(object)
    }
}
