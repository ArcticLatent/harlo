pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Models: the cloud providers Harlo uses with your API keys
// (here, or on the machine shown), and coding plans for Claude Code.
ColumnLayout {
    id: modelsView
    /// Straight to signing `agent` in to `provider` (OpenRouter).
    signal providerSignInRequested(string agent, string provider, string name)

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
            ? qsTr("Harlo can use these providers' models with an API key of yours; each charges for what Harlo sends and gets. Keys stay in a file only you can read (harlo-agent's keys.json).")
            : qsTr("Keys for Harlo on %1, kept there in a file only you can read.").arg(Workspace.instance)
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
        agent: modelsView.here ? "harlo" : modelsView.hostReady ? "harlo@" + Workspace.instance : "harlo"
        folded: true
        onProviderSignInRequested: (agent, provider, name) => modelsView.providerSignInRequested(agent, provider, name)
    }
    // (Only once there's a key to give.)
    Group {
        visible: modelsView.here && apiProviders.keyed.length > 0
        Layout.topMargin: 14
        SettingRow {
            title: qsTr("Give SSH machines these keys")
            detail: qsTr("A Harlo session on another machine gets the keys saved here that the machine has none of, kept there in its own private file.")
            icon: "server"
            AppSwitch {
                objectName: "harloShareKeys"
                checked: Workspace.setting("harlo_share_keys") !== "0"
                onToggled: Workspace.setSetting("harlo_share_keys", checked ? "1" : "0")
            }
        }
    }
    Rule { visible: modelsView.here }
    // Subscriptions Claude Code can use instead of Anthropic's models.
    SectionHeader {
        objectName: "codingPlansHeader"
        visible: modelsView.here
        title: qsTr("Coding plans for Claude Code")
        subtitle: qsTr("Subscriptions whose makers support Claude Code. With a key saved, their models are in a Claude Code session's model picker. Keys stay in a file only you can read (Harlo's coding-plans.json); a session on another machine gets its plan's key for as long as it runs.")
    }
    CloudProviders {
        objectName: "codingPlans"
        visible: modelsView.here
        Layout.fillWidth: true
        Layout.topMargin: 8
        agent: "claude"
    }
}
