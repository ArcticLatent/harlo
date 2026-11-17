pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Providers → Cloud models: the providers Harlo knows, here or
// on a machine, each with its API key and its models' prices.
ColumnLayout {
    id: cloud
    /// "harlo", or "harlo@<host>" for a machine's agent.
    property string agent: "harlo"
    /// OpenRouter's sign-in for `agent`.
    signal providerSignInRequested(string agent, string provider, string name)

    readonly property var st: Accounts.revision >= 0 ? JSON.parse(Accounts.cloud(agent)) : ({ list: [], results: {} })
    readonly property var list: st.list || []
    /// The provider opened ("" none).
    property string open: ""

    function nameOf(id) {
        const p = list.find(p => p.id === id)
        return p ? p.name : id
    }

    objectName: "cloudProviders"
    spacing: 0
    Component.onCompleted: Accounts.loadCloud(agent)
    onAgentChanged: { open = ""; Accounts.loadCloud(agent) }

    Label {
        visible: cloud.list.length === 0 && cloud.st.state === "loading"
        Layout.topMargin: 6
        text: qsTr("Reading Harlo's providers…")
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: (cloud.st.error || "") !== ""
        Layout.fillWidth: true
        Layout.topMargin: 6
        wrapMode: Text.Wrap
        text: cloud.st.error || ""
        color: Theme.error
        font.pixelSize: Theme.px(12)
    }
    Group {
        visible: cloud.list.length > 0
        Repeater {
            model: cloud.list
            CloudProviderRow {
                agent: cloud.agent
                expanded: cloud.open === modelData.id
                result: (cloud.st.results || {})[modelData.id] || null
                ownerName: cloud.nameOf(modelData.keyOf || "")
                onToggled: cloud.open = expanded ? "" : modelData.id
                onSignInRequested: cloud.providerSignInRequested(cloud.agent, modelData.id, modelData.name)
            }
        }
    }
}
