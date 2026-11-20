pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import harlo

// Settings → Agents: the agents, their programs, sign-ins and models, and
// how Harlo behaves.
ColumnLayout {
    id: providersView
    signal signInRequested(string agent, string label)
    spacing: 0
    SectionHeader {
        // (Here: the page's own title says it.)
        title: Workspace.instance === "" ? "" : "Agents on " + Workspace.instance
        subtitle: Workspace.instance === ""
            ? "The coding agents Harlo runs. Each uses its own login and settings; Harlo finds them on PATH and in their usual install folders."
            : "The coding agents on " + Workspace.instance + ", the instance shown: Harlo runs them there, each with its own login and settings on that machine. The switches (which agents Harlo offers) are the same for every instance."
    }
    // A host's agents (the instance shown is a remote one).
    ColumnLayout {
        id: hostAgents
        visible: Workspace.instance !== ""
        Layout.fillWidth: true
        spacing: 0
        readonly property var host: Remotes.revision >= 0 ? (JSON.parse(Remotes.hostsJson()).find(h => h.name === Workspace.instance) || null) : null
        readonly property var updates: Updates.entriesJson !== "" ? JSON.parse(Updates.entriesJson) : []
        readonly property var providers: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []
        // pi's extensions there shown below.
        property bool extensions: false
        Component.onCompleted: if (visible && host && host.state !== "connected" && host.state !== "connecting") Remotes.connectHost(Workspace.instance)
        Label {
            visible: !hostAgents.host || hostAgents.host.state !== "connected"
            Layout.topMargin: 8
            text: !hostAgents.host ? "" : hostAgents.host.state === "connecting" ? "Connecting to " + Workspace.instance + "…" : hostAgents.host.state === "error" ? "Couldn't reach " + Workspace.instance + ": " + hostAgents.host.message : "Not connected."
            color: hostAgents.host && hostAgents.host.state === "error" ? Theme.error : Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Group {
            visible: hostAgents.host !== null && hostAgents.host.state === "connected"
            Repeater {
                model: ["pi"]
                SettingRow {
                    id: hostAgent
                    required property string modelData
                    required property int index
                    readonly property var h: hostAgents.host || ({})
                    readonly property bool found: (h[modelData] || "") !== ""
                    readonly property string version: (h.versions || {})[modelData] || ""
                    readonly property var install: (h.installs || {})[modelData] || null
                    readonly property var update: hostAgents.updates.find(u => u.key === Workspace.instance + "/" + modelData) || null
                    readonly property var local: hostAgents.providers.find(p => p.id === modelData) || ({ enabled: true })
                    objectName: "hostProvider-" + modelData
                    divider: index > 0
                    icon: "harness-" + modelData
                    title: Theme.harnessLabel(modelData)
                    // (With what harlo's sandbox there lacks, when it's on.)
                    readonly property string sandbox: Workspace.setting("harlo_sandbox") === "0" ? ""
                        : h.harloWhy ? " · Sandbox: " + h.harloWhy
                        : h.bwrap === "no" ? " · Sandbox: needs bubblewrap" : h.bwrap === "blocked" ? " · Sandbox: bubblewrap can't run there" : ""
                    detail: install && install.state === "running" ? install.message
                        : !found ? (install && install.state === "failed" ? install.message : "Not installed there")
                        : (version !== "" ? "Version " + version : "Installed") + (h[modelData] ? " · " + h[modelData] : "")
                          + (update && update.state === "available" ? " · " + update.latest + " available" : "") + sandbox
                    detailColor: install && install.state === "failed" && !found ? Theme.error : Theme.muted
                    RowLayout {
                        spacing: 6
                        FlatButton {
                            objectName: "hostProviderInstall-" + hostAgent.modelData
                            visible: !hostAgent.found && !(hostAgent.install && hostAgent.install.state === "running")
                            text: "Install"
                            font.pixelSize: Theme.px(12)
                            onClicked: Remotes.installAgent(Workspace.instance, hostAgent.modelData)
                        }
                        FlatButton {
                            visible: hostAgent.update !== null && hostAgent.update.state === "available"
                            text: "Update"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            onClicked: Updates.update(hostAgent.update.key)
                        }
                        FlatButton {
                            objectName: "hostPiExtensions"
                            visible: hostAgent.found && hostAgent.modelData === "pi"
                            text: hostAgents.extensions ? "Hide extensions" : "Extensions"
                            font.pixelSize: Theme.px(12)
                            onClicked: hostAgents.extensions = !hostAgents.extensions
                        }
                        AppSwitch {
                            checked: hostAgent.local.enabled
                            onToggled: Workspace.setProviderEnabled(hostAgent.modelData, checked)
                        }
                    }
                }
            }
        }
        PiExtensionsSection {
            visible: hostAgents.extensions && hostAgents.host !== null && hostAgents.host.state === "connected" && (hostAgents.host.pi || "") !== ""
            Layout.fillWidth: true
            Layout.topMargin: 10
            host: Workspace.instance
        }
        RowLayout {
            Layout.topMargin: 8
            FlatButton { objectName: "hostProvidersRefresh"; text: "Check again"; font.pixelSize: Theme.px(12); onClicked: Remotes.connectHost(Workspace.instance) }
        }
    }
    ColumnLayout {
        id: providerList
        visible: Workspace.instance === ""
        Layout.fillWidth: true
        spacing: 0
        readonly property var list: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []
        property string open: ""
        Component.onCompleted: Workspace.refreshProviders()
        Group {
            Repeater {
                model: providerList.list
                ColumnLayout {
                    id: prov
                    required property var modelData
                    required property int index
                    readonly property bool expanded: providerList.open === modelData.id
                    // (Its models listed on demand: a long list otherwise.)
                    property bool showModels: false
                    // (Signed out: Sign in on the row itself.)
                    readonly property bool signedOut: modelData.enabled && modelData.state === "error" && String(modelData.error || "").startsWith("Not signed in")
                    readonly property string configured: Workspace.providersRevision >= 0 ? Workspace.setting(modelData.id + "_path") : ""
                    objectName: "provider-" + modelData.id
                    Layout.fillWidth: true
                    spacing: 0
                    Rectangle { visible: prov.index > 0; Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
                    // The row: what it is and how it is, and its switch.
                    ItemDelegate {
                        id: provRow
                        Layout.fillWidth: true
                        implicitHeight: 52
                        hoverEnabled: true
                        leftPadding: 14
                        rightPadding: 14
                        onClicked: providerList.open = prov.expanded ? "" : prov.modelData.id
                        background: Rectangle { color: provRow.hovered ? Theme.hover : "transparent"; radius: 4 }
                        contentItem: RowLayout {
                            spacing: 10
                            Icon { name: prov.expanded ? "arrow-down-s" : "arrow-right-s"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                            Icon { name: "harness-" + prov.modelData.id; color: Theme.foreground; implicitWidth: 18; implicitHeight: 18 }
                            Label { text: prov.modelData.label; color: Theme.foreground; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium }
                            Label { text: "·"; color: Theme.muted }
                            Rectangle {
                                implicitWidth: 7
                                implicitHeight: 7
                                radius: 3.5
                                color: !prov.modelData.enabled ? Theme.muted
                                    : prov.modelData.state === "ready" ? Theme.success
                                    : prov.modelData.state === "checking" ? Theme.info
                                    : Theme.warning
                            }
                            Label {
                                objectName: "providerStatus-" + prov.modelData.id
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                color: prov.modelData.enabled && prov.modelData.state === "error" ? Theme.warning : Theme.muted
                                font.pixelSize: Theme.px(12.5)
                                // (An error says why, in its first line; the rest is under the row.)
                                text: !prov.modelData.enabled ? "Disabled"
                                    : prov.modelData.state === "checking" ? "Checking…"
                                    : prov.modelData.state === "missing" ? "Not installed"
                                    : prov.modelData.state === "error" ? (String(prov.modelData.error || "").trim().split("\n")[0] || "Didn't start")
                                    : "Available · " + prov.modelData.modelCount + (prov.modelData.modelCount === 1 ? " model" : " models")
                            }
                            FlatButton {
                                objectName: "rowSignIn-" + prov.modelData.id
                                visible: prov.signedOut
                                text: "Sign in…"
                                font.pixelSize: Theme.px(12.5)
                                padding: 4
                                horizontalPadding: 10
                                onClicked: { providerList.open = prov.modelData.id; providersView.signInRequested(prov.modelData.id, prov.modelData.label) }
                            }
                            AppSwitch {
                                objectName: "providerSwitch-" + prov.modelData.id
                                checked: prov.modelData.enabled
                                onToggled: Workspace.setProviderEnabled(prov.modelData.id, checked)
                            }
                        }
                    }
                    // Details: the program, its version, what went wrong, its models.
                    ColumnLayout {
                        visible: prov.expanded
                        Layout.fillWidth: true
                        Layout.leftMargin: 52
                        Layout.rightMargin: 16
                        Layout.bottomMargin: 14
                        spacing: 8
                        Label {
                            visible: prov.modelData.error !== ""
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: prov.modelData.error
                            color: Theme.error
                            font.pixelSize: Theme.px(12)
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            AppTextField {
                                objectName: "providerPath-" + prov.modelData.id
                                Layout.fillWidth: true
                                text: prov.configured
                                placeholderText: prov.modelData.path !== "" && prov.configured === "" ? prov.modelData.path + "  (found automatically)" : "Not found; set the full path"
                                font.family: Theme.mono
                                font.pixelSize: Theme.px(12)
                                onEditingFinished: if (text !== prov.configured) { Workspace.setSetting(prov.modelData.id + "_path", text); Workspace.refreshProviders() }
                            }
                            FlatButton { text: "Browse…"; font.pixelSize: Theme.px(12); onClicked: { programDialog.harness = prov.modelData.id; programDialog.open() } }
                            FlatButton { text: "Reset"; visible: prov.configured !== ""; font.pixelSize: Theme.px(12); onClicked: { Workspace.setSetting(prov.modelData.id + "_path", ""); Workspace.refreshProviders() } }
                        }
                        Label {
                            visible: prov.modelData.version !== ""
                            text: "Version " + prov.modelData.version
                            color: Theme.muted
                            font.pixelSize: Theme.px(12)
                        }
                        // Accounts: what it's signed in to, signing in and out.
                        ColumnLayout {
                            id: acct
                            objectName: "accounts-" + prov.modelData.id
                            visible: prov.modelData.state !== "missing"
                            Layout.fillWidth: true
                            Layout.topMargin: 4
                            spacing: 6
                            readonly property var st: Accounts.revision >= 0 ? JSON.parse(Accounts.status(prov.modelData.id)) : ({ accounts: [] })
                            readonly property var rows: st.accounts || []
                            property string signOutError: ""
                            onVisibleChanged: if (visible) Accounts.refresh(prov.modelData.id)
                            Component.onCompleted: if (visible) Accounts.refresh(prov.modelData.id)
                            Label { text: "Accounts"; color: Theme.foreground; font.pixelSize: Theme.px(12.5); font.weight: Font.Medium }
                            Repeater {
                                model: acct.rows
                                RowLayout {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Icon { name: "checkbox-circle"; color: Theme.success; implicitWidth: 13; implicitHeight: 13 }
                                    Label { text: modelData.label; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                                    Label { Layout.fillWidth: true; text: modelData.detail || ""; elide: Text.ElideRight; color: Theme.muted; font.pixelSize: Theme.px(12) }
                                    FlatButton {
                                        visible: modelData.signOut === true
                                        objectName: "signOut-" + prov.modelData.id + "-" + modelData.id
                                        text: "Sign out"
                                        padding: 3
                                        font.pixelSize: Theme.px(11)
                                        onClicked: { acct.signOutError = Accounts.signOut(prov.modelData.id, modelData.id); Workspace.refreshProviders() }
                                    }
                                }
                            }
                            Label {
                                objectName: "accountsNone-" + prov.modelData.id
                                visible: acct.st.state === "ready" && (acct.st.error || "") === "" && acct.rows.length === 0
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                text: "Not signed in to anything yet."
                                color: Theme.warning
                                font.pixelSize: Theme.px(12)
                            }
                            // (Not knowing isn't an error: Sign in… below either way.)
                            Label {
                                visible: (acct.st.error || "") !== "" || acct.signOutError !== ""
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                text: acct.signOutError !== "" ? acct.signOutError : (acct.st.error || "")
                                color: acct.signOutError !== "" ? Theme.error : Theme.muted
                                font.pixelSize: Theme.px(12)
                            }
                            RowLayout {
                                spacing: 8
                                FlatButton {
                                    objectName: "signIn-" + prov.modelData.id
                                    text: acct.rows.length > 0 ? "Sign in to another…" : "Sign in…"
                                    accent: Theme.primary
                                    font.pixelSize: Theme.px(12)
                                    onClicked: providersView.signInRequested(prov.modelData.id, prov.modelData.label)
                                }
                                Label {
                                    visible: acct.st.state === "loading"
                                    text: "Checking…"
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(12)
                                }
                            }
                        }
                        // pi: its extensions.
                        PiExtensionsSection {
                            objectName: "piExtensions"
                            visible: prov.modelData.id === "pi" && prov.modelData.state !== "missing" && prov.expanded
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                            readonly property var claudeAccounts: Accounts.revision >= 0 ? JSON.parse(Accounts.status("claude")) : ({})
                            claudeSignedIn: claudeAccounts.state !== "ready" ? "" : (claudeAccounts.accounts || []).length > 0 ? "yes" : "no"
                            claudeAccount: ((claudeAccounts.accounts || [])[0] || {}).label || ""
                            onVisibleChanged: if (visible) Accounts.refresh("claude")
                            onClaudeSignInRequested: providersView.signInRequested("claude", "Claude")
                            onClaudeSignOutRequested: Accounts.signOut("claude", "claude")
                        }
                        FlatButton {
                            objectName: "providerModels-" + prov.modelData.id
                            visible: prov.modelData.models.length > 0
                            text: prov.showModels ? qsTr("Hide its models") : prov.modelData.models.length === 1 ? qsTr("Show its model") : qsTr("Show its %1 models").arg(prov.modelData.models.length)
                            font.pixelSize: Theme.px(12)
                            onClicked: prov.showModels = !prov.showModels
                        }
                        Flow {
                            visible: prov.showModels
                            Layout.fillWidth: true
                            spacing: 5
                            Repeater {
                                model: prov.modelData.models.slice(0, 60)
                                Rectangle {
                                    required property string modelData
                                    implicitWidth: modelName.implicitWidth + 14
                                    implicitHeight: 22
                                    radius: 2
                                    color: Theme.controlFill
                                    border.color: Theme.controlBorder
                                    Label { id: modelName; anchors.centerIn: parent; text: Theme.modelLabel(parent.modelData); color: Theme.muted; font.pixelSize: Theme.px(11) }
                                }
                            }
                        }
                        Label {
                            visible: prov.showModels && prov.modelData.models.length > 60
                            text: "… and " + (prov.modelData.models.length - 60) + " more"
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.topMargin: 8
            FlatButton { objectName: "providersRefresh"; text: "Check again"; font.pixelSize: Theme.px(12); onClicked: Workspace.refreshProviders() }
        }
    }
    Rule { }
    SectionHeader {
        title: "Sandbox"
    }
    Group {
        SettingRow {
            title: "Sandbox commands"
            detail: "pi's commands can change the project and nothing else, and can't see your keys or sign-ins."
            more: "The system and your home folder are read-only: commands can change the project, /tmp and tool caches, and can't see SSH and GPG keys, credentials, sign-ins or browser profiles. pi's file tools keep to the same. A command it needs outside the sandbox is left to you. Needs bubblewrap (bwrap) on the machine the session runs on; MCP servers and Harlo's tools aren't sandboxed. Applies to sessions started afterwards."
            icon: "shield-check"
            AppSwitch {
                id: harloSandboxSwitch
                objectName: "harloSandbox"
                checked: Workspace.setting("harlo_sandbox") !== "0"
                onToggled: Workspace.setSetting("harlo_sandbox", checked ? "1" : "0")
            }
        }
        SettingRow {
            divider: true
            enabled: harloSandboxSwitch.checked
            opacity: enabled ? 1 : 0.5
            title: "Sandboxed commands can use the network"
            detail: "Off: they can't reach the network, this machine's own servers included, so downloads by cargo, npm or git fetch fail."
            icon: "global"
            AppSwitch {
                objectName: "harloSandboxNet"
                checked: Workspace.setting("harlo_sandbox_net") !== "0"
                onToggled: Workspace.setSetting("harlo_sandbox_net", checked ? "1" : "0")
            }
        }
    }
    // (Browse… for an agent's program.)
    FileDialog {
        id: programDialog
        property string harness: ""
        title: "Choose the " + harness + " binary"
        onAccepted: { Workspace.setSetting(harness + "_path", decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, ""))); Workspace.refreshProviders() }
    }
}
