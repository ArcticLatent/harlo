pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Models: the cloud providers pi uses with your API keys (here,
// or on the machine shown).
ColumnLayout {
    id: modelsView
    /// A pi key: `command` in harlo's terminal.
    signal runInTerminalRequested(string command)

    readonly property bool here: Workspace.instance === ""
    readonly property var host: Remotes.revision >= 0 && !here ? (JSON.parse(Remotes.hostsJson()).find(h => h.name === Workspace.instance) || null) : null
    readonly property bool hostReady: host !== null && host.state === "connected"

    spacing: 0
    Component.onCompleted: if (host && host.state !== "connected" && host.state !== "connecting") Remotes.connectHost(Workspace.instance)

    SectionHeader {
        objectName: "cloudModelsHeader"
        // (Here: the page's own title says it.)
        title: modelsView.here ? "" : qsTr("Cloud models on %1").arg(Workspace.instance)
        subtitle: modelsView.here
            ? qsTr("pi can use these providers' models with an API key of yours; each charges for what pi sends and gets. You type the key into pi, which keeps it in a file only you can read (its auth.json).")
            : qsTr("Keys for pi on %1, kept there in a file only you can read.").arg(Workspace.instance)
    }
    Label {
        visible: !modelsView.here && !modelsView.hostReady
        Layout.topMargin: 8
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: !modelsView.host ? "" : modelsView.host.state === "connecting" ? qsTr("Connecting to %1…").arg(Workspace.instance)
            : modelsView.host.state === "error" ? qsTr("Couldn't reach %1: %2").arg(Workspace.instance).arg(modelsView.host.message) : qsTr("Not connected.")
        color: modelsView.host && modelsView.host.state === "error" ? Theme.error : Theme.muted
        font.pixelSize: Theme.px(12)
    }
    CloudProviders {
        id: apiProviders
        visible: modelsView.here || modelsView.hostReady
        Layout.fillWidth: true
        Layout.topMargin: 8
        // (Its agent there answers once the machine is connected.)
        agent: modelsView.here ? "pi" : modelsView.hostReady ? "pi@" + Workspace.instance : "pi"
        folded: true
        onRunInTerminalRequested: command => modelsView.runInTerminalRequested(command)
    }
}
