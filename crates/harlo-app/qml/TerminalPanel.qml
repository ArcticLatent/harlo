import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Terminal panel under the chat. Each folder keeps its own shells (tabs),
// alive while harlo runs: switching sessions switches to that session's
// terminals. + (Ctrl+Shift+T) opens another shell; `exit` in one that
// isn't the folder's last closes its tab.
Rectangle {
    id: root
    /// Folder of the active session ("" = home).
    property string cwd: ""
    signal closeRequested()
    color: Theme.codeBg

    readonly property string home: Workspace.homeDir()
    // A folder asked for (a remote host's home, to install an agent there),
    // until the session changes.
    property string pinnedFolder: ""
    onCwdChanged: pinnedFolder = ""
    readonly property string folder: pinnedFolder !== "" ? pinnedFolder : cwd !== "" ? cwd : home
    /// folder → { tabs: [TerminalView], active: index, next: number for the next tab's name }
    property var folders: ({})
    readonly property var entry: folders[folder] || null
    readonly property var tabs: entry ? entry.tabs : []
    readonly property var current: entry ? entry.tabs[entry.active] || null : null

    Component { id: viewComponent; TerminalView { anchors.fill: parent } }

    function update(f, change) {
        const next = Object.assign({}, folders)
        next[f] = Object.assign({}, next[f], change)
        folders = next
        syncVisible()
    }
    function ensure() {
        if (visible && !folders[folder])
            newTab()
    }
    /// Another shell in the current folder, made current.
    /// A new tab that runs `command` once its shell is up.
    function runInNewTab(command) { newTab(command) }
    function runIn(folder, command) { pinnedFolder = folder; newTab(command) }
    function newTab(command) {
        const f = folder
        const e = folders[f] || { tabs: [], active: 0, next: 1 }
        const v = viewComponent.createObject(stack, { cwd: f, initialCommand: command || "" })
        v.objectName = "Shell " + e.next
        v.exited.connect(() => root.shellExited(f, v))
        update(f, { tabs: e.tabs.concat([v]), active: e.tabs.length, next: e.next + 1 })
        Qt.callLater(() => { if (root.current === v) v.forceActiveFocus() })
    }
    function closeTab(index) {
        const f = folder
        const e = folders[f]
        if (!e || !e.tabs[index]) return
        const v = e.tabs[index]
        const tabs = e.tabs.filter((_, i) => i !== index)
        if (tabs.length === 0) {
            const next = Object.assign({}, folders)
            delete next[f]
            folders = next
            v.destroy()
            root.closeRequested()
            return
        }
        update(f, { tabs: tabs, active: Math.min(e.active > index ? e.active - 1 : e.active, tabs.length - 1) })
        v.destroy()
        focusTerminal()
    }
    function activate(index) {
        if (!entry || !entry.tabs[index]) return
        update(folder, { active: index })
        focusTerminal()
    }
    // `exit` closes the tab, unless it is the folder's only shell (that one
    // shows Restart instead).
    function shellExited(f, v) {
        const e = folders[f]
        if (!e || e.tabs.length < 2) return
        const i = e.tabs.indexOf(v)
        if (i < 0) return
        const tabs = e.tabs.filter(t => t !== v)
        update(f, { tabs: tabs, active: Math.min(e.active > i ? e.active - 1 : e.active, tabs.length - 1) })
        v.destroy()
    }
    // "2 · repo": the tab's number and the shell's folder, from the usual
    // `user@host:path` window title; a running program's title instead
    // when the shell sets one ("2 · vim notes.md").
    function tabTitle(v) {
        if (!v) return ""
        const n = v.objectName.replace("Shell ", "")
        const t = v.session.title
        if (t === "") return v.objectName
        const m = t.match(/^[^@\s]+@[^:\s]+:\s*(.*)$/)
        if (!m) return n + " · " + t
        const path = m[1].replace(/\/+$/, "")
        return n + " · " + (path === "~" || path === "" ? "~" : path.substring(path.lastIndexOf("/") + 1))
    }

    onFolderChanged: { ensure(); syncVisible() }
    onVisibleChanged: { ensure(); if (visible && current) current.forceActiveFocus() }
    Component.onCompleted: ensure()
    function focusTerminal() { Qt.callLater(() => { if (root.current) root.current.forceActiveFocus() }) }
    readonly property bool terminalFocused: current !== null && current.activeFocus

    Rectangle { width: parent.width; height: 1; color: Theme.hairline }

    ColumnLayout {
        anchors.fill: parent
        anchors.topMargin: 1
        spacing: 0

        // Header: tabs (soft pills, as the window's), the folder, actions.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 40
            color: Theme.chrome
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 4
                Icon { name: "terminal-box"; color: Theme.textMuted; implicitWidth: 15; implicitHeight: 15 }
                Item { implicitWidth: 4 }
                // Tabs
                ListView {
                    id: tabList
                    Layout.fillHeight: true
                    Layout.preferredWidth: Math.min(contentWidth, parent.width * 0.6)
                    Layout.maximumWidth: contentWidth
                    orientation: ListView.Horizontal
                    spacing: 2
                    clip: true
                    interactive: contentWidth > width
                    model: root.tabs
                    currentIndex: root.entry ? root.entry.active : -1
                    onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                    delegate: Item {
                        id: tab
                        required property var modelData
                        required property int index
                        readonly property bool active: root.entry && root.entry.active === index
                        width: Math.min(220, tabLabel.implicitWidth + (closeTab.visible ? 40 : 24))
                        height: tabList.height
                        Rectangle {
                            anchors.fill: parent
                            anchors.topMargin: 6
                            anchors.bottomMargin: 6
                            radius: 8
                            color: tab.active ? Theme.rowSelected : tabHover.hovered ? Theme.rowHover : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        }
                        Label {
                            id: tabLabel
                            anchors.verticalCenter: parent.verticalCenter
                            x: 12
                            width: parent.width - (closeTab.visible ? 36 : 24)
                            text: root.tabTitle(tab.modelData)
                            color: tab.active ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(13)
                            font.weight: tab.active ? Font.Medium : Font.Normal
                            elide: Text.ElideMiddle
                        }
                        HoverHandler { id: tabHover }
                        TapHandler { onTapped: root.activate(tab.index) }
                        TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: root.closeTab(tab.index) }
                        PanelButton {
                            id: closeTab
                            objectName: "terminalTabClose"
                            visible: root.tabs.length > 1 && (tab.active || tabHover.hovered)
                            anchors.right: parent.right
                            anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            implicitWidth: 20
                            implicitHeight: 20
                            iconName: "close"
                            onClicked: root.closeTab(tab.index)
                        }
                        ToolTip.visible: tabHover.hovered && tabLabel.truncated
                        ToolTip.text: tabLabel.text
                    }
                }
                PanelButton {
                    id: addButton
                    objectName: "terminalAdd"
                    implicitWidth: 28
                    implicitHeight: 28
                    iconName: "add"
                    tip: "New terminal in this folder" + Keymap.hint("newTerminalTab")
                    onClicked: root.newTab()
                }
                Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    horizontalAlignment: Text.AlignRight
                    text: root.folder.replace(root.home, "~")
                    color: Theme.textFaint
                    font.pixelSize: Theme.px(12.5)
                    elide: Text.ElideMiddle
                }
                PanelButton {
                    objectName: "terminalRestart"
                    implicitWidth: 28
                    implicitHeight: 28
                    iconName: "refresh"
                    tip: "Restart: a fresh shell in this tab"
                    onClicked: if (root.current) root.current.restart()
                }
                PanelButton {
                    id: closeButton
                    implicitWidth: 26
                    implicitHeight: 26
                    round: true
                    iconName: "close"
                    tip: "Hide" + Keymap.hint("terminal")
                    onClicked: root.closeRequested()
                }
            }
        }

        // Terminals; only the current folder's current tab is shown.
        Item {
            id: stack
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }
    function syncVisible() {
        for (const f in folders) {
            const e = folders[f]
            for (let i = 0; i < e.tabs.length; i++)
                e.tabs[i].visible = (f === folder && i === e.active)
        }
    }
}
