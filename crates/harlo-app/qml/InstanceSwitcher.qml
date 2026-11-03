import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Which machine the window shows (its instance): Local, or
// one of the SSH hosts, each with its own projects, sessions and tabs. The
// button says which, and how it's connected; its menu switches, marks the
// one harlo starts on, and adds hosts.
Item {
    id: root
    signal manageRequested()
    readonly property var hosts: Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()) : []
    readonly property string current: Workspace.instance
    function host(name) { return hosts.find(h => h.name === name) || null }
    function label(name) { return name === "" ? "Local" : name }
    function stateOf(name) {
        if (name === "") return "connected"
        const h = host(name)
        return h ? (h.state || "") : "error"
    }
    function dotColor(name) {
        const s = stateOf(name)
        return s === "connected" ? Theme.success : s === "connecting" ? Theme.warning : s === "error" ? Theme.error : Theme.muted
    }
    function statusText(name) {
        if (name === "") return "This computer"
        const h = host(name)
        if (!h) return "Not set up"
        switch (h.state) {
        case "connected": return "Connected" + (h.ping !== null && h.ping !== undefined ? " · " + h.ping + "ms ping" : "")
        case "connecting": return "Connecting…"
        case "error": return "Unreachable" + (h.message ? ": " + h.message : "")
        default: return "Not connected"
        }
    }
    function choose(name) {
        menu.close()
        Workspace.switchInstance(name)
        if (name !== "" && stateOf(name) !== "connected" && stateOf(name) !== "connecting") Remotes.connectHost(name)
    }
    // Asked first if sessions are working there: closing the connection
    // stops them. The window doesn't stay on a machine it isn't connected to.
    property string stopAsked: ""
    property int stopCount: 0
    function disconnect(name) {
        const working = Workspace.workingOnHost(name)
        if (working > 0 && stopAsked !== name) {
            stopAsked = name
            stopCount = working
            return
        }
        stopAsked = ""
        if (name === current) Workspace.switchInstance("")
        Remotes.disconnectHost(name)
    }
    function refresh() {
        for (const h of hosts) {
            if (h.state === "connected") Remotes.pingHost(h.name)
            else if (h.state !== "connecting" && (h.name === current || h.state === "error")) Remotes.connectHost(h.name)
        }
    }
    implicitWidth: 160
    implicitHeight: button.implicitHeight

    // On a host harlo started on (or switched to): connect it. If the
    // default host can't be reached at startup, Local instead.
    property bool startupCheck: Workspace.instance !== ""
    property string connectedHosts: ""
    Component.onCompleted: if (current !== "" && stateOf(current) === "") Remotes.connectHost(current)
    onCurrentChanged: if (current !== "" && stateOf(current) === "") Remotes.connectHost(current)
    Connections {
        target: Remotes
        function onRevisionChanged() {
            // Just connected: its sessions now, not at the next look.
            const now = root.hosts.filter(h => h.state === "connected").map(h => h.name).join(",")
            if (now !== root.connectedHosts) {
                if (now.split(",").some(n => n !== "" && root.connectedHosts.split(",").indexOf(n) < 0)) Workspace.checkRemoteSessions()
                root.connectedHosts = now
            }
            if (!root.startupCheck) return
            const s = root.stateOf(root.current)
            if (s === "connected") root.startupCheck = false
            else if (s === "error") {
                root.startupCheck = false
                const name = root.current
                Workspace.switchInstance("")
                Workspace.showNotice("Couldn't reach " + name + " (the instance harlo starts on), so this is Local. Switch back from the instance menu once it's reachable.", true)
            }
        }
    }

    Button {
        id: button
        objectName: "instanceButton"
        width: root.width
        flat: true
        padding: 4
        leftPadding: 10
        rightPadding: 8
        implicitHeight: 32
        onClicked: menu.opened ? menu.close() : menu.open()
        contentItem: RowLayout {
            spacing: 6
            Rectangle { implicitWidth: 7; implicitHeight: 7; radius: 3.5; color: root.dotColor(root.current) }
            Icon { name: root.current === "" ? "window" : "server"; color: Theme.foreground; implicitWidth: 13; implicitHeight: 13 }
            Label {
                objectName: "instanceLabel"
                Layout.fillWidth: true
                text: root.label(root.current)
                color: Theme.foreground
                font.pixelSize: Theme.px(13)
                elide: Text.ElideRight
            }
            Icon { name: "arrow-down-s"; color: Theme.muted; implicitWidth: 12; implicitHeight: 12 }
        }
        background: ControlSurface { control: button; variant: menu.opened ? "selected" : "outline"; radius: 7 }
        ToolTip.visible: hovered && !menu.opened
        ToolTip.text: "Instance: " + root.label(root.current) + " — " + root.statusText(root.current)
        ToolTip.delay: 500
    }

    Popup {
        id: menu
        objectName: "instanceMenu"
        y: button.height + 6
        width: Math.max(root.width, 340)
        padding: 0
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        background: GlassSurface { radius: 12 }
        onOpened: root.refresh()
        onClosed: root.stopAsked = ""

        contentItem: ColumnLayout {
            spacing: 0
            // Current • Default, and a refresh.
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 12
                Layout.bottomMargin: 8
                spacing: 6
                Label { text: "Current"; color: Theme.foreground; font.pixelSize: Theme.px(12); font.weight: Font.DemiBold }
                Label { text: root.label(root.current); color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight; Layout.maximumWidth: 90 }
                Label { text: "•"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                Label { text: "Default"; color: Theme.foreground; font.pixelSize: Theme.px(12); font.weight: Font.DemiBold }
                Label { text: root.label(Workspace.defaultInstance); color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight; Layout.maximumWidth: 90 }
                Item { Layout.fillWidth: true }
                FlatButton {
                    objectName: "instanceRefresh"
                    outlined: false
                    padding: 3
                    implicitWidth: 24
                    implicitHeight: 24
                    text: "↻"
                    font.pixelSize: Theme.px(13)
                    onClicked: root.refresh()
                    ToolTip.visible: hovered
                    ToolTip.text: "Check the connections again"
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }

            Repeater {
                model: [""].concat(root.hosts.map(h => h.name))
                Rectangle {
                    id: row
                    required property string modelData
                    required property int index
                    readonly property bool isCurrent: modelData === root.current
                    objectName: "instance-" + (modelData === "" ? "local" : modelData)
                    Layout.fillWidth: true
                    Layout.leftMargin: 8
                    Layout.rightMargin: 8
                    Layout.topMargin: index === 0 ? 8 : 4
                    implicitHeight: rowCol.implicitHeight + 18
                    radius: 9
                    color: isCurrent ? Theme.selected : rowHover.hovered ? Theme.hover : "transparent"
                    HoverHandler { id: rowHover }
                    TapHandler { onTapped: root.choose(row.modelData) }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 8
                        spacing: 8
                        ColumnLayout {
                            id: rowCol
                            Layout.fillWidth: true
                            spacing: 3
                            RowLayout {
                                spacing: 6
                                Label { text: root.label(row.modelData); color: Theme.foreground; font.pixelSize: Theme.px(13); font.weight: Font.Medium; elide: Text.ElideRight; Layout.maximumWidth: 150 }
                                Rectangle {
                                    visible: row.isCurrent
                                    implicitWidth: curLabel.implicitWidth + 10
                                    implicitHeight: 16
                                    radius: 4
                                    color: Theme.subtle
                                    border.color: Theme.border
                                    Label { id: curLabel; anchors.centerIn: parent; text: "Current"; color: Theme.foreground; font.pixelSize: Theme.px(10) }
                                }
                                Rectangle {
                                    visible: row.modelData !== ""
                                    implicitWidth: sshLabel.implicitWidth + 10
                                    implicitHeight: 16
                                    radius: 4
                                    color: Qt.rgba(Theme.info.r, Theme.info.g, Theme.info.b, 0.15)
                                    Label { id: sshLabel; anchors.centerIn: parent; text: "SSH"; color: Theme.info; font.pixelSize: Theme.px(10) }
                                }
                            }
                            RowLayout {
                                spacing: 6
                                Rectangle { implicitWidth: 7; implicitHeight: 7; radius: 3.5; color: root.dotColor(row.modelData) }
                                Label {
                                    objectName: "instanceStatus-" + (row.modelData === "" ? "local" : row.modelData)
                                    Layout.fillWidth: true
                                    text: root.statusText(row.modelData)
                                    color: root.stateOf(row.modelData) === "connected" ? Theme.success : root.stateOf(row.modelData) === "error" ? Theme.error : Theme.muted
                                    font.pixelSize: Theme.px(12)
                                    elide: Text.ElideRight
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: row.modelData === "" ? Workspace.homeDir().replace(/^\/home\/[^/]+/, "~") : (root.host(row.modelData) || {}).ssh || ""
                                color: Theme.muted
                                font.family: Theme.mono
                                font.pixelSize: Theme.px(11)
                                elide: Text.ElideRight
                            }
                        }
                        FlatButton {
                            objectName: "instanceDisconnect-" + row.modelData
                            readonly property bool asking: root.stopAsked === row.modelData
                            visible: row.modelData !== "" && (root.stateOf(row.modelData) === "connected" || root.stateOf(row.modelData) === "connecting")
                            text: asking ? "Stop " + root.stopCount + (root.stopCount === 1 ? " session" : " sessions") + " and disconnect" : "Disconnect"
                            accent: asking ? Theme.error : Theme.muted
                            font.pixelSize: Theme.px(11.5)
                            padding: 4
                            onClicked: root.disconnect(row.modelData)
                            ToolTip.visible: hovered
                            ToolTip.text: "Closes the connection to " + row.modelData + ". Sessions running there stop; what they saved stays there. Click it here to connect again."
                            ToolTip.delay: 500
                        }
                        // The one harlo starts on.
                        FlatButton {
                            objectName: "instanceDefault-" + (row.modelData === "" ? "local" : row.modelData)
                            outlined: false
                            padding: 3
                            implicitWidth: 26
                            implicitHeight: 26
                            text: Workspace.defaultInstance === row.modelData ? "★" : "☆"
                            accent: Workspace.defaultInstance === row.modelData ? Theme.primary : Theme.muted
                            font.pixelSize: Theme.px(15)
                            onClicked: Workspace.setDefaultInstanceTo(row.modelData)
                            ToolTip.visible: hovered
                            ToolTip.text: Workspace.defaultInstance === row.modelData ? "harlo starts on this one" : "Start harlo on this one"
                        }
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; Layout.topMargin: 8; implicitHeight: 1; color: Theme.border }
            RowLayout {
                Layout.fillWidth: true
                spacing: 0
                ItemDelegate {
                    objectName: "instanceAdd"
                    Layout.fillWidth: true
                    implicitHeight: 38
                    leftPadding: 14
                    background: Rectangle { color: parent.hovered ? Theme.hover : "transparent"; radius: 10 }
                    contentItem: Label { text: "+  Add instance"; color: Theme.foreground; font.pixelSize: Theme.px(12.5); verticalAlignment: Text.AlignVCenter }
                    onClicked: { menu.close(); addHost.open() }
                }
                FlatButton {
                    Layout.rightMargin: 8
                    text: "Manage"
                    outlined: false
                    font.pixelSize: Theme.px(12)
                    onClicked: { menu.close(); root.manageRequested() }
                    ToolTip.visible: hovered
                    ToolTip.text: "Settings → Remote Instances: edit hosts, install agents"
                }
            }
        }
    }

    // A new host becomes the instance shown.
    AddHostDialog {
        id: addHost
        onAdded: name => Workspace.switchInstance(name)
    }
}
