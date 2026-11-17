pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Models: the providers an agent knows (Harlo's, here or on a
// machine; Claude Code's coding plans), each with its API key. An API
// variant sharing another's key goes inside that one's row.
ColumnLayout {
    id: cloud
    /// "harlo", "harlo@<host>" for a machine's agent, or "claude".
    property string agent: "harlo"
    /// The providers with a key first, the rest behind Add a provider (a
    /// long list otherwise).
    property bool folded: false
    /// OpenRouter's sign-in for `agent`.
    signal providerSignInRequested(string agent, string provider, string name)

    readonly property var st: Accounts.revision >= 0 ? JSON.parse(Accounts.cloud(agent)) : ({ list: [], results: {} })
    readonly property var list: st.list || []
    readonly property var mains: list.filter(p => (p.keyOf || p.id) === p.id)
    readonly property var keyed: folded ? mains.filter(p => hasKey(p)) : mains
    readonly property var rest: folded ? mains.filter(p => !hasKey(p)) : []
    /// Add a provider is open (it always is while none has a key).
    property bool adding: false
    readonly property bool addShown: folded && (adding || keyed.length === 0)
    property string search: ""
    /// The ones most people have a key for, offered first (the rest under
    /// More providers, or found by searching).
    readonly property var common: ["anthropic", "openrouter", "gemini", "xai", "deepseek"]
    property bool everyProvider: false
    readonly property bool commonOnly: !everyProvider && search.trim() === "" && rest.filter(p => common.indexOf(p.id) >= 0).length > 0
    readonly property var found: {
        const q = search.trim().toLowerCase()
        const hits = rest.filter(p => q === "" || (p.name + " " + p.id + " " + (p.maker || "")).toLowerCase().includes(q))
        if (!commonOnly) return hits
        return common.map(id => hits.find(p => p.id === id)).filter(p => !!p)
    }
    /// The provider opened ("" none).
    property string open: ""

    function hasKey(p) { return p.replaced === true || (p.key || "none") !== "none" }
    function variantsOf(id) { return list.filter(p => p.id !== id && p.keyOf === id) }

    objectName: "cloudProviders"
    spacing: 0
    Component.onCompleted: Accounts.loadCloud(agent)
    onAgentChanged: { open = ""; adding = false; search = ""; everyProvider = false; Accounts.loadCloud(agent) }

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
    Component {
        id: providerRow
        CloudProviderRow {
            agent: cloud.agent
            expanded: cloud.open === modelData.id
            result: (cloud.st.results || {})[modelData.id] || null
            variants: cloud.variantsOf(modelData.id)
            onToggled: cloud.open = expanded ? "" : modelData.id
            onSignInRequested: cloud.providerSignInRequested(cloud.agent, modelData.id, modelData.name)
        }
    }
    Group {
        visible: cloud.keyed.length > 0
        Repeater { model: cloud.keyed; delegate: providerRow }
    }
    FlatButton {
        objectName: "cloudAdd"
        visible: cloud.folded && !cloud.addShown && cloud.rest.length > 0
        Layout.topMargin: 10
        text: qsTr("Add a provider…")
        font.pixelSize: Theme.px(12)
        onClicked: { cloud.adding = true; searchField.forceActiveFocus() }
    }
    // Add a provider: the rest, searched by name.
    ColumnLayout {
        visible: cloud.addShown && cloud.rest.length > 0
        Layout.fillWidth: true
        Layout.topMargin: cloud.keyed.length > 0 ? 14 : 4
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: cloud.keyed.length > 0 ? qsTr("Add a provider") : qsTr("No provider has a key yet. Pick the one you pay for and paste its key.")
                wrapMode: Text.Wrap
                color: cloud.keyed.length > 0 ? Theme.foreground : Theme.muted
                font.pixelSize: Theme.px(12.5)
                font.weight: cloud.keyed.length > 0 ? Font.Medium : Font.Normal
            }
            FlatButton {
                visible: cloud.keyed.length > 0
                text: qsTr("Done")
                font.pixelSize: Theme.px(12)
                onClicked: { cloud.adding = false; cloud.everyProvider = false; cloud.search = ""; searchField.text = "" }
            }
        }
        AppTextField {
            id: searchField
            objectName: "cloudSearch"
            Layout.fillWidth: true
            leftPadding: 30
            placeholderText: qsTr("Search %1 providers").arg(cloud.rest.length)
            font.pixelSize: Theme.px(12.5)
            onTextChanged: cloud.search = text
            Icon { x: 10; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
        }
        Group {
            visible: cloud.found.length > 0
            Repeater { model: cloud.found; delegate: providerRow }
        }
        FlatButton {
            objectName: "cloudMore"
            visible: cloud.commonOnly && cloud.rest.length > cloud.found.length
            text: qsTr("More providers (%1)").arg(cloud.rest.length - cloud.found.length)
            font.pixelSize: Theme.px(12)
            onClicked: cloud.everyProvider = true
        }
        Label {
            visible: cloud.found.length === 0
            text: qsTr("No provider is called that.")
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
    }
}
