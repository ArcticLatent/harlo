pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Other machines: SSH hosts harlo runs agents on.
ColumnLayout {
    id: remotes
    property bool compact
    signal runInTerminalRequested(string command)
    signal signInRequested(string agent, string label)
    spacing: 0
    readonly property var hosts: Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()) : []
    /// The host being edited ("" new, null none).
    property var editing: null
    property string formError: ""
    function edit(h) { editing = h.name; nameField.text = h.name; sshField.text = h.ssh; formError = ""; nameField.forceActiveFocus() }
    function save() {
        const err = Remotes.saveHost(editing, nameField.text, sshField.text)
        if (err !== "") { formError = err; return }
        const name = nameField.text.trim()
        editing = null
        Remotes.connectHost(name)
    }
    SectionHeader {
        title: "SSH connections"
        subtitle: "Work on other machines over SSH, each with its own projects and sessions."
        more: "Switch to one from the instance menu at the top of the sidebar, then + Project adds its folders; its chats, terminal, git and files work there. Harlo signs in with your SSH keys or agent. Each connected machine shows whether it has Claude Code, installs it for you, and signs Claude in there; Harlo brings Harlo itself."
        AddButton { objectName: "addRemoteHost"; text: "Add SSH connection"; enabled: remotes.editing === null; onClicked: addHostDialog.open() }
    }
    // Add: from ~/.ssh/config, or typed (also the instance menu's).
    AddHostDialog { id: addHostDialog }
    // Edit.
    Group {
        visible: remotes.editing !== null
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 6
            Label { text: "Edit " + remotes.editing; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(13) }
            FormLabel { Layout.topMargin: 8; text: "Name" }
            AppTextField { id: nameField; objectName: "editRemoteName"; Layout.fillWidth: true; placeholderText: "nas" }
            FormLabel { Layout.topMargin: 8; text: "SSH command" }
            AppTextField {
                id: sshField
                objectName: "editRemoteSsh"
                Layout.fillWidth: true
                placeholderText: "user@host, a name from ~/.ssh/config, or ssh -p 2222 me@10.0.0.5"
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                onAccepted: remotes.save()
            }
            FormNote { visible: remotes.formError !== ""; color: Theme.error; text: remotes.formError }
            RowLayout {
                Layout.topMargin: 6
                spacing: 8
                Item { Layout.fillWidth: true }
                FlatButton { text: "Cancel"; font.pixelSize: Theme.px(12); onClicked: remotes.editing = null }
                FlatButton { objectName: "saveEditedHost"; text: "Save and connect"; font.pixelSize: Theme.px(12); accent: Theme.primary; onClicked: remotes.save() }
            }
        }
    }
    Group { visible: remotes.hosts.length === 0 && remotes.editing === null; EmptyNote { text: "No remote machines yet." } }
    Group {
        visible: remotes.hosts.length > 0
        Repeater {
            model: remotes.hosts
            ColumnLayout {
                id: hostBox
                required property var modelData
                required property int index
                Layout.fillWidth: true
                spacing: 0
            SettingRow {
                id: hostRow
                readonly property var modelData: hostBox.modelData
                readonly property int index: hostBox.index
                objectName: "remoteHost-" + modelData.name
                readonly property string st: modelData.state
                divider: index > 0
                title: modelData.name
                detail: (st === "connected" ? "Connected" + (modelData.message !== "" ? ". " + modelData.message : "")
                         : st === "connecting" ? "Connecting…"
                         : st === "error" ? modelData.message
                         : "Not connected") + "\n" + (modelData.ssh.length > 60 ? "ssh … " + modelData.ssh.split(" ").pop() : "ssh " + modelData.ssh)
                icon: "server"
                Row {
                    visible: !remotes.compact
                    // (Hidden, it takes no room from the name.)
                    width: visible ? implicitWidth : 0
                    height: visible ? implicitHeight : 0
                    spacing: 6
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 8; height: 8; radius: 4
                        color: hostRow.st === "connected" ? (hostRow.modelData.message !== "" ? Theme.warning : Theme.success)
                             : hostRow.st === "error" ? Theme.error : hostRow.st === "connecting" ? Theme.warning : Theme.muted
                    }
                    FlatButton {
                        objectName: "remoteConnect-" + hostRow.modelData.name
                        text: hostRow.st === "connected" ? "Disconnect" : "Connect"
                        enabled: hostRow.st !== "connecting"
                        font.pixelSize: Theme.px(12)
                        onClicked: hostRow.st === "connected" ? Remotes.disconnectHost(hostRow.modelData.name) : Remotes.connectHost(hostRow.modelData.name)
                    }
                    FlatButton { text: "Edit"; font.pixelSize: Theme.px(12); onClicked: remotes.edit(hostRow.modelData) }
                    FlatButton {
                        objectName: "remoteDelete-" + hostRow.modelData.name
                        text: "Delete"
                        accent: Theme.error
                        font.pixelSize: Theme.px(12)
                        onClicked: Remotes.deleteHost(hostRow.modelData.name)
                    }
                }
            }
            // (Narrow: the buttons under the name.)
            Row {
                visible: remotes.compact
                Layout.leftMargin: 60
                Layout.bottomMargin: 8
                spacing: 6
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8; height: 8; radius: 4
                    color: hostRow.st === "connected" ? (hostRow.modelData.message !== "" ? Theme.warning : Theme.success)
                         : hostRow.st === "error" ? Theme.error : hostRow.st === "connecting" ? Theme.warning : Theme.muted
                }
                FlatButton {
                    objectName: "remoteConnect-" + hostRow.modelData.name
                    text: hostRow.st === "connected" ? "Disconnect" : "Connect"
                    enabled: hostRow.st !== "connecting"
                    font.pixelSize: Theme.px(12)
                    onClicked: hostRow.st === "connected" ? Remotes.disconnectHost(hostRow.modelData.name) : Remotes.connectHost(hostRow.modelData.name)
                }
                FlatButton { text: "Edit"; font.pixelSize: Theme.px(12); onClicked: remotes.edit(hostRow.modelData) }
                FlatButton {
                    objectName: "remoteDelete-" + hostRow.modelData.name
                    text: "Delete"
                    accent: Theme.error
                    font.pixelSize: Theme.px(12)
                    onClicked: Remotes.deleteHost(hostRow.modelData.name)
                }
            }
            // The agents on it: each with its version, or a way to
            // install it (in the background, its output a click
            // away); Claude's sign-in (in a terminal there).
            Flow {
                id: agentsFlow
                visible: hostRow.st === "connected"
                Layout.fillWidth: true
                Layout.leftMargin: 60
                Layout.rightMargin: 16
                Layout.bottomMargin: 12
                spacing: 6
                readonly property string folder: "ssh://" + hostBox.modelData.name + (hostBox.modelData.home || "")
                Repeater {
                    model: ["claude"]
                    Rectangle {
                        id: agentChip
                        required property string modelData
                        readonly property var host: hostBox.modelData
                        readonly property bool found: (host[modelData] || "") !== ""
                        readonly property bool signIn: found && modelData === "claude" && host.claudeAuth === "no"
                        readonly property bool installing: ((host.installs || {})[modelData] || {}).state === "running"
                        objectName: "hostAgent-" + host.name + "-" + modelData
                        implicitWidth: chipRow.implicitWidth + 14
                        implicitHeight: 30
                        radius: 7
                        color: Theme.elevated
                        border.color: found && !signIn ? Theme.border : Theme.warning
                        Row {
                            id: chipRow
                            anchors.centerIn: parent
                            spacing: 6
                            ProviderMark { anchors.verticalCenter: parent.verticalCenter; provider: agentChip.modelData; tip: ""; implicitWidth: 13; implicitHeight: 13; color: agentChip.found ? Theme.foreground : Theme.muted }
                            Label {
                                anchors.verticalCenter: parent.verticalCenter
                                text: Theme.harnessLabel(agentChip.modelData) + (agentChip.found ? ((agentChip.host.versions || {})[agentChip.modelData] ? " " + agentChip.host.versions[agentChip.modelData] : "") : agentChip.installing ? " · installing…" : " · not installed")
                                color: agentChip.found ? Theme.foreground : Theme.muted
                                font.pixelSize: Theme.px(12)
                            }
                            FlatButton {
                                objectName: "hostInstall-" + agentChip.host.name + "-" + agentChip.modelData
                                visible: !agentChip.found && !agentChip.installing
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Install"
                                padding: 3
                                font.pixelSize: Theme.px(11)
                                readonly property string command: (agentChip.host.install || {})[agentChip.modelData] || ""
                                ToolTip.visible: hovered
                                ToolTip.text: "Runs its installer on " + agentChip.host.name + ":\n" + command
                                onClicked: Remotes.installAgent(agentChip.host.name, agentChip.modelData)
                            }
                            // Installing: a bar that runs until it's done.
                            Rectangle {
                                objectName: "hostInstalling-" + agentChip.host.name + "-" + agentChip.modelData
                                visible: agentChip.installing
                                anchors.verticalCenter: parent.verticalCenter
                                width: 64
                                height: 4
                                radius: 2
                                clip: true
                                color: Theme.border
                                Rectangle {
                                    width: parent.width * 0.4
                                    height: parent.height
                                    radius: 2
                                    color: Theme.primary
                                    NumberAnimation on x {
                                        running: agentChip.installing
                                        loops: Animation.Infinite
                                        from: -64 * 0.4
                                        to: 64
                                        duration: 1100
                                    }
                                }
                            }
                            FlatButton {
                                objectName: "hostSignIn-" + agentChip.host.name + "-" + agentChip.modelData
                                visible: agentChip.signIn
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Sign in"
                                padding: 3
                                font.pixelSize: Theme.px(11)
                                ToolTip.visible: hovered
                                ToolTip.text: "Claude isn't signed in on " + agentChip.host.name + "."
                                onClicked: remotes.signInRequested("claude@" + agentChip.host.name, "Claude on " + agentChip.host.name)
                            }
                        }
                    }
                }
                // Each install's outcome, with its output a click away.
                Repeater {
                    model: Object.keys(hostBox.modelData.installs || {})
                    RowLayout {
                        id: installRow
                        required property string modelData
                        readonly property var info: hostBox.modelData.installs[modelData]
                        width: agentsFlow.width
                        spacing: 8
                        Label {
                            objectName: "hostInstallMessage-" + hostBox.modelData.name + "-" + installRow.modelData
                            Layout.fillWidth: true
                            text: installRow.info.message
                            color: installRow.info.state === "failed" ? Theme.error : installRow.info.state === "done" ? Theme.success : Theme.muted
                            font.pixelSize: Theme.px(12)
                            wrapMode: Text.Wrap
                        }
                        FlatButton {
                            objectName: "hostInstallOutput-" + hostBox.modelData.name + "-" + installRow.modelData
                            text: "View output"
                            font.pixelSize: Theme.px(11)
                            padding: 3
                            onClicked: remotes.runInTerminalRequested(Remotes.installOutputCommand(hostBox.modelData.name, installRow.modelData))
                        }
                    }
                }
                FlatButton {
                    objectName: "hostRecheck-" + hostBox.modelData.name
                    text: "Check again"
                    outlined: false
                    font.pixelSize: Theme.px(11)
                    ToolTip.visible: hovered
                    ToolTip.text: "After installing or signing in"
                    onClicked: Remotes.connectHost(hostBox.modelData.name)
                }
            }
            }
        }
    }
}
