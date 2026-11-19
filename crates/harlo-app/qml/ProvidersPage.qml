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
    /// Straight to signing `agent` in to `provider` (OpenRouter).
    signal providerSignInRequested(string agent, string provider, string name)
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
        // The agent whose sign-ins there are shown below ("" none).
        property string open: ""
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
                model: ["harlo", "claude"]
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
                    // (Harlo: harlo puts its agent there when it's needed.)
                    detail: modelData === "harlo" ? (found ? "Harlo puts its agent there as sessions start"
                            + (Workspace.setting("harlo_sandbox") === "0" ? "" : h.bwrap === "no" ? " · Sandbox: needs bubblewrap" : h.bwrap === "blocked" ? " · Sandbox: bubblewrap can't run there" : "")
                        : (h.harloWhy || "Not checked yet"))
                        : install && install.state === "running" ? install.message
                        : !found ? (install && install.state === "failed" ? install.message : "Not installed there")
                        : (version !== "" ? "Version " + version : "Installed") + (h[modelData] ? " · " + h[modelData] : "")
                          + (modelData === "claude" && h.claudeAuth === "no" ? " · not signed in" : "")
                          + (update && update.state === "available" ? " · " + update.latest + " available" : "")
                    detailColor: install && install.state === "failed" && !found ? Theme.error : Theme.muted
                    RowLayout {
                        spacing: 6
                        FlatButton {
                            objectName: "hostProviderInstall-" + hostAgent.modelData
                            visible: !hostAgent.found && hostAgent.modelData !== "harlo" && !(hostAgent.install && hostAgent.install.state === "running")
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
                            objectName: "hostProviderAccounts-" + hostAgent.modelData
                            visible: hostAgent.found
                            text: hostAgents.open === hostAgent.modelData ? "Hide accounts" : "Accounts"
                            font.pixelSize: Theme.px(12)
                            onClicked: {
                                hostAgents.open = hostAgents.open === hostAgent.modelData ? "" : hostAgent.modelData
                                if (hostAgents.open !== "") Accounts.refresh(hostAgents.open + "@" + Workspace.instance)
                            }
                        }
                        FlatButton {
                            visible: hostAgent.found && hostAgent.modelData === "claude" && hostAgent.h.claudeAuth === "no"
                            text: "Sign in"
                            font.pixelSize: Theme.px(12)
                            ToolTip.visible: hovered
                            ToolTip.text: "Claude isn't signed in on " + Workspace.instance + "."
                            onClicked: providersView.signInRequested("claude@" + Workspace.instance, "Claude on " + Workspace.instance)
                        }
                        AppSwitch {
                            checked: hostAgent.local.enabled
                            onToggled: Workspace.setProviderEnabled(hostAgent.modelData, checked)
                        }
                    }
                }
            }
        }
        // One agent's sign-ins on that machine (its own logins there).
        ColumnLayout {
            id: hostAccounts
            visible: hostAgents.open !== "" && hostAgents.host !== null && hostAgents.host.state === "connected"
            Layout.fillWidth: true
            Layout.topMargin: 10
            spacing: 6
            readonly property string agent: hostAgents.open + "@" + Workspace.instance
            readonly property var st: Accounts.revision >= 0 && hostAgents.open !== "" ? JSON.parse(Accounts.status(agent)) : ({ accounts: [] })
            property string signOutError: ""
            Label {
                text: Theme.harnessLabel(hostAgents.open) + "'s accounts on " + Workspace.instance
                color: Theme.foreground
                font.pixelSize: Theme.px(12.5)
                font.weight: Font.Medium
            }
            Repeater {
                model: hostAccounts.st.accounts || []
                RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 8
                    Icon { name: "checkbox-circle"; color: Theme.success; implicitWidth: 13; implicitHeight: 13 }
                    Label { text: modelData.label; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                    Label { Layout.fillWidth: true; text: modelData.detail || ""; elide: Text.ElideRight; color: Theme.muted; font.pixelSize: Theme.px(12) }
                    FlatButton {
                        visible: modelData.signOut === true
                        text: "Sign out"
                        padding: 3
                        font.pixelSize: Theme.px(11)
                        onClicked: hostAccounts.signOutError = Accounts.signOut(hostAccounts.agent, modelData.id)
                    }
                }
            }
            Label {
                visible: hostAccounts.st.state === "ready" && (hostAccounts.st.error || "") === "" && (hostAccounts.st.accounts || []).length === 0
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Not signed in to anything there yet."
                color: Theme.warning
                font.pixelSize: Theme.px(12)
            }
            Label {
                visible: (hostAccounts.st.error || "") !== "" || hostAccounts.signOutError !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: hostAccounts.signOutError !== "" ? hostAccounts.signOutError : (hostAccounts.st.error || "")
                color: Theme.error
                font.pixelSize: Theme.px(12)
            }
            RowLayout {
                spacing: 8
                FlatButton {
                    text: (hostAccounts.st.accounts || []).length > 0 ? "Sign in to another…" : "Sign in…"
                    accent: Theme.primary
                    font.pixelSize: Theme.px(12)
                    onClicked: providersView.signInRequested(hostAccounts.agent, Theme.harnessLabel(hostAgents.open) + " on " + Workspace.instance)
                }
                Label {
                    visible: hostAccounts.st.state === "loading"
                    text: "Checking…"
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
            }
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
                        background: Rectangle { color: provRow.hovered ? Theme.hover : "transparent"; radius: 10 }
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
                                    radius: 6
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
        title: "Harlo"
        subtitle: harloModels.ids.length === 0 ? "Its model choices fill in once it's signed in (Sign in… above)." : ""
    }
    Group {
        SettingRow {
            title: "Sandbox commands"
            detail: "Harlo's commands can change the project and nothing else, and can't see your keys or sign-ins."
            more: "The system and your home folder are read-only: commands can change the project, /tmp and tool caches, and can't see SSH and GPG keys, credentials, sign-ins or browser profiles. Harlo's file tools keep to the same. A command that needs more asks you before it runs outside the sandbox. Needs bubblewrap (bwrap) on the machine the session runs on; MCP servers and Harlo's tools aren't sandboxed. Applies to Harlo sessions started afterwards."
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
        // Harlo's models (provider/model), for the two pickers below.
        QtObject {
            id: harloModels
            readonly property var prov: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(p => p.id === "harlo") || null : null
            readonly property var ids: prov ? prov.modelIds || [] : []
            readonly property string none: ids.length === 0 ? " Harlo lists its models once it's signed in." : ""
            // The ids to offer: Harlo's, and the saved one if it no longer lists it.
            function choices(saved) { return saved === "" || ids.indexOf(saved) >= 0 ? ids : ids.concat([saved]) }
            // Names as the model picker shows them, with the provider where two share one.
            function labels(list) {
                const names = list.map(id => { const i = ids.indexOf(id); return i >= 0 ? Theme.modelLabel((prov.models || [])[i] || id) : id })
                return names.map((n, i) => names.indexOf(n) !== names.lastIndexOf(n) ? n + " · " + list[i].split("/")[0] : n)
            }
        }
        SettingRow {
            id: taskModelRow
            divider: true
            property string saved: Workspace.setting("harlo_task_model")
            readonly property var choices: harloModels.choices(saved)
            title: "Its own helpers' model"
            detail: "What Harlo's own helpers run on when the task doesn't name a model."
            more: "Its own helpers are its task tool's. A smaller model often does searches and simple edits as well, for less. Applies to Harlo sessions started afterwards."
            icon: "robot-2"
            ComboField {
                objectName: "harloTaskModel"
                // (Nothing to pick before sign-in: said once, at the heading.)
                enabled: taskModelRow.choices.length > 0
                width: 240
                model: ["The session's model"].concat(harloModels.labels(taskModelRow.choices))
                currentIndex: taskModelRow.choices.indexOf(taskModelRow.saved) + 1
                onActivated: i => {
                    taskModelRow.saved = i === 0 ? "" : taskModelRow.choices[i - 1]
                    Workspace.setSetting("harlo_task_model", taskModelRow.saved)
                }
            }
        }
        SettingRow {
            id: fallbackRow
            divider: true
            property string saved: Workspace.setting("harlo_fallback")
            readonly property var choices: harloModels.choices(saved)
            title: "When the plan's limit is reached"
            detail: "Stop, or go on with another model and say so."
            more: "When the model's plan says its usage limit is reached, or the server stays busy after its retries. Harlo tries the plan's model again after the limit resets. Applies to Harlo sessions started afterwards."
            icon: "refresh"
            ComboField {
                objectName: "harloFallback"
                enabled: fallbackRow.choices.length > 0
                width: 240
                model: ["Stop"].concat(harloModels.labels(fallbackRow.choices).map(n => "Go on with " + n))
                currentIndex: fallbackRow.choices.indexOf(fallbackRow.saved) + 1
                onActivated: i => {
                    fallbackRow.saved = i === 0 ? "" : fallbackRow.choices[i - 1]
                    Workspace.setSetting("harlo_fallback", fallbackRow.saved)
                }
            }
        }
        SettingRow {
            id: budgetRow
            divider: true
            property string saved: Workspace.setting("harlo_helper_budget")
            // (A value set some other way is kept as a choice.)
            readonly property var choices: ["", "500k", "1m", "2m", "5m", "10m"].concat(["", "500k", "1m", "2m", "5m", "10m"].indexOf(saved) >= 0 ? [] : [saved])
            title: "Token budget per helper"
            detail: "The most a Harlo helper session may use, in tokens sent and received."
            more: "A helper session is one another session starts; its own helpers count too. A helper's coding task often takes a few hundred thousand. Past it, it makes no more requests and its parent is told."
            icon: "scales-3"
            ComboField {
                objectName: "harloHelperBudget"
                width: 240
                model: budgetRow.choices.map(v => v === "" ? "No limit" : v.replace(/m$/, "M") + " tokens")
                currentIndex: budgetRow.choices.indexOf(budgetRow.saved)
                onActivated: i => {
                    budgetRow.saved = budgetRow.choices[i]
                    Workspace.setSetting("harlo_helper_budget", budgetRow.saved)
                }
            }
        }
        SettingRow {
            id: advisorRow
            divider: true
            property string saved: Workspace.setting("harlo_advisor")
            readonly property var choices: harloModels.choices(saved)
            title: qsTr("Reviewer before it finishes")
            detail: qsTr("A second model reads each turn's work and diff, and sends it back once with a note when something's wrong or missing.")
            more: qsTr("It costs a request every turn, so it's off unless a model is picked. Its notes show in the chat. Applies to Harlo sessions started afterwards.")
            icon: "search-eye"
            ComboField {
                objectName: "harloAdvisor"
                enabled: advisorRow.choices.length > 0
                width: 240
                model: [qsTr("Off")].concat(harloModels.labels(advisorRow.choices))
                currentIndex: advisorRow.choices.indexOf(advisorRow.saved) + 1
                onActivated: i => {
                    advisorRow.saved = i === 0 ? "" : advisorRow.choices[i - 1]
                    Workspace.setSetting("harlo_advisor", advisorRow.saved)
                }
            }
        }
        SettingRow {
            id: steeringRow
            divider: true
            property string saved: Workspace.setting("harlo_steering")
            title: qsTr("Messages sent while it works")
            detail: qsTr("Go in right away, or wait until it finishes what it's doing.")
            more: qsTr("Right away: Harlo reads a message at its next step, stopping a wait or putting a long command in the background so it can. Waiting: the message starts the next turn. Applies to Harlo sessions started afterwards.")
            icon: "send-plane"
            ComboField {
                objectName: "harloSteering"
                width: 240
                model: [qsTr("Right away"), qsTr("When it finishes")]
                currentIndex: steeringRow.saved === "wait" ? 1 : 0
                onActivated: i => {
                    steeringRow.saved = i === 1 ? "wait" : ""
                    Workspace.setSetting("harlo_steering", steeringRow.saved)
                }
            }
        }
        Repeater {
            model: [
                { key: "harlo_memory", icon: "bookmark", title: qsTr("Notes between sessions"),
                  detail: qsTr("Harlo keeps what it learns about a project (a command that works, a pitfall) and the next sessions start with it.") },
                { key: "harlo_debug", icon: "bug", title: qsTr("Debugger"),
                  detail: qsTr("Harlo can run a program under a debugger: breakpoints, stepping, the stack and variables (Python, Go, C, C++, Rust).") },
                { key: "harlo_ast", icon: "node-tree", title: qsTr("Structural search and edit"),
                  detail: qsTr("Harlo can find and rewrite code by its shape, as patterns over the syntax tree.") },
                { key: "harlo_background", icon: "ai-agent", title: qsTr("Helpers in the background"),
                  detail: qsTr("Harlo's own helpers can work while it goes on, and it waits for them when it needs their results.") }
            ]
            delegate: SettingRow {
                required property var modelData
                divider: true
                title: modelData.title
                detail: modelData.detail
                more: qsTr("Off by default: each adds to every request. Applies to Harlo sessions started afterwards.")
                icon: modelData.icon
                AppSwitch {
                    objectName: modelData.key
                    checked: Workspace.setting(modelData.key) === "1"
                    onToggled: Workspace.setSetting(modelData.key, checked ? "1" : "0")
                }
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
