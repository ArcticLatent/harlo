import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Adds an SSH host (Settings → Remote Instances, and the instance menu's
// Add instance): one from ~/.ssh/config, or a connection typed in.
AppDialog {
    id: addHostDialog
    objectName: "addHostDialog"
    /// A host was added (and is connecting).
    signal added(string name)
    readonly property var hosts: Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()) : []
    /// Adds a host from ~/.ssh/config (its name as harlo's, made unique)
    /// and connects.
    function useConfigHost(h) {
        let name = h.name.replace(/[^A-Za-z0-9_.-]/g, "-").substring(0, 40), n = 2
        while (hosts.some(x => x.name === name)) name = h.name.substring(0, 36) + "-" + n++
        const err = Remotes.saveHost("", name, h.name)
        if (err !== "") { error = err; return }
        close()
        Remotes.connectHost(name)
        added(name)
    }
    function addTyped() {
        const err = Remotes.saveHost("", addName.text, addSsh.text)
        if (err !== "") { error = err; return }
        const name = addName.text.trim()
        close()
        Remotes.connectHost(name)
        added(name)
    }
    title: "Add SSH connection"
    standardButtons: Dialog.Cancel
    width: Math.min(520, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
    property string tab: "config"
    property var configHosts: []
    property string error: ""
    readonly property var shown: configHosts.filter(h => (h.name + " " + h.hostname + " " + h.user).toLowerCase().includes(hostSearch.text.trim().toLowerCase()))
    onAboutToShow: {
        configHosts = JSON.parse(Remotes.sshConfigHostsJson())
        tab = configHosts.length > 0 ? "config" : "manual"
        hostSearch.text = ""
        addName.text = ""
        addSsh.text = ""
        error = ""
    }
    ColumnLayout {
        width: parent.width
        spacing: 8
        Note { text: "Pick a host from your SSH config, or type the connection yourself." }
        Row {
            spacing: 6
            Pill { objectName: "remoteTabConfig"; text: "From SSH config"; selected: addHostDialog.tab === "config"; onClicked: addHostDialog.tab = "config" }
            Pill { objectName: "remoteTabManual"; text: "Type it myself"; selected: addHostDialog.tab === "manual"; onClicked: { addHostDialog.tab = "manual"; addName.forceActiveFocus() } }
        }
        // From ~/.ssh/config
        AppTextField {
            id: hostSearch
            visible: addHostDialog.tab === "config" && addHostDialog.configHosts.length > 0
            Layout.fillWidth: true
            placeholderText: "Search hosts"
        }
        Note {
            visible: addHostDialog.tab === "config" && addHostDialog.shown.length === 0
            text: addHostDialog.configHosts.length === 0 ? "No hosts found in ~/.ssh/config. Type the connection yourself instead." : "No host matches this search."
        }
        ListView {
            objectName: "sshConfigHosts"
            visible: addHostDialog.tab === "config" && addHostDialog.shown.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 320)
            clip: true
            model: addHostDialog.shown
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Item {
                id: cfgRow
                required property var modelData
                required property int index
                width: ListView.view.width
                implicitHeight: 52
                Rectangle { visible: cfgRow.index > 0; width: parent.width; height: 1; color: Theme.border }
                RowLayout {
                    anchors.fill: parent
                    anchors.rightMargin: 8
                    spacing: 10
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label { Layout.fillWidth: true; text: cfgRow.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(13); font.weight: Font.Medium; elide: Text.ElideRight }
                        Label {
                            Layout.fillWidth: true
                            // What it connects to.
                            text: (cfgRow.modelData.user ? cfgRow.modelData.user + "@" : "") + (cfgRow.modelData.hostname || cfgRow.modelData.name) + (cfgRow.modelData.port ? ":" + cfgRow.modelData.port : "")
                            color: Theme.muted
                            font.family: Theme.mono
                            font.pixelSize: Theme.px(11.5)
                            elide: Text.ElideRight
                        }
                    }
                    Label { visible: cfgRow.modelData.added; text: "Added"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                    FlatButton {
                        objectName: "useHost-" + cfgRow.modelData.name
                        visible: !cfgRow.modelData.added
                        text: "Use"
                        font.pixelSize: Theme.px(12)
                        accent: Theme.primary
                        onClicked: addHostDialog.useConfigHost(cfgRow.modelData)
                    }
                }
            }
        }
        // Typed
        ColumnLayout {
            visible: addHostDialog.tab === "manual"
            Layout.fillWidth: true
            spacing: 6
            FieldName { text: "Name" }
            AppTextField { id: addName; objectName: "remoteName"; Layout.fillWidth: true; placeholderText: "nas" }
            FieldName { text: "SSH command" }
            AppTextField {
                id: addSsh
                objectName: "remoteSsh"
                Layout.fillWidth: true
                placeholderText: "user@host, or ssh -p 2222 me@10.0.0.5"
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                onAccepted: addHostDialog.addTyped()
            }
            RowLayout {
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                FlatButton { objectName: "saveRemoteHost"; text: "Save and connect"; font.pixelSize: Theme.px(12); accent: Theme.primary; onClicked: addHostDialog.addTyped() }
            }
        }
        Note { visible: addHostDialog.error !== ""; color: Theme.error; text: addHostDialog.error }
    }

    component Note: Label { color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
    component FieldName: Label { color: Theme.foreground; font.pixelSize: Theme.px(13); Layout.topMargin: 8 }
    component Pill: Button {
        id: pill
        property bool selected: false
        flat: true
        padding: 5
        leftPadding: 11
        rightPadding: 11
        font.pixelSize: Theme.px(12)
        contentItem: Label { text: pill.text; color: pill.selected ? Theme.foreground : Theme.muted; font: pill.font }
        background: ControlSurface { control: pill; variant: pill.selected ? "selected" : "outline"; radius: 7 }
    }
}
