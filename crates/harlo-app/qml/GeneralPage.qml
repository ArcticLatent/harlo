pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → General: setup, sessions, startup, the tray and drop-down, graphics.
ColumnLayout {
    id: general
    signal setupRequested()
    signal trayToggled(bool on)
    signal pageRequested(string id)
    spacing: 0
    property string harness: Workspace.setting("default_harness")
    property bool restoreTabs: Workspace.setting("restore_tabs") !== "0"
    SectionHeader { title: "New sessions"; subtitle: "For sessions outside a project and for newly added projects. Each project keeps its own default (right-click it in the sidebar)." }
    Group {
        SettingRow {
            title: "Provider"
            detail: "Automatic picks the first one on in Settings → Agents that's installed."
            icon: "robot-2"
            Flow {
                spacing: 6
                Repeater {
                    model: [{ id: "", label: "Automatic" }].concat(Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : [])
                    ChoicePill {
                        required property var modelData
                        text: modelData.label
                        provider: modelData.id
                        selected: general.harness === modelData.id
                        onClicked: { Workspace.setSetting("default_harness", modelData.id); general.harness = modelData.id }
                    }
                }
            }
        }
        // The model the provider's new sessions start with.
        SettingRow {
            id: modelRow
            divider: true
            readonly property string agent: general.harness !== "" ? general.harness : (Workspace.providersRevision >= 0 ? Workspace.defaultAgent() : "")
            readonly property var prov: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(p => p.id === agent) || null : null
            readonly property var ids: prov ? prov.modelIds || [] : []
            property string current: Workspace.setting("default_model_" + agent)
            onAgentChanged: current = Workspace.setting("default_model_" + agent)
            title: "Model"
            detail: !prov ? "Pick a provider first."
                  : ids.length === 0 ? prov.label + " lists its models once it's signed in."
                  : "New " + prov.label + " sessions start with this one; the new-session page can still pick another."
            icon: "sparkling"
            // (No models before sign-in: where to sign in.)
            FlatButton {
                objectName: "defaultModelSignIn"
                visible: modelRow.prov !== null && modelRow.ids.length === 0
                text: "Sign in on Providers"
                font.pixelSize: Theme.px(12.5)
                onClicked: general.pageRequested("providers")
            }
            ComboField {
                objectName: "defaultModel"
                visible: modelRow.ids.length > 0
                width: 260
                model: [modelRow.prov ? modelRow.prov.label + "'s own default" : ""].concat(modelRow.ids.map((id, i) => Theme.modelLabel((modelRow.prov.models || [])[i] || id)))
                currentIndex: Math.max(0, modelRow.ids.indexOf(modelRow.current) + 1)
                onActivated: i => {
                    const id = i === 0 ? "" : modelRow.ids[i - 1]
                    Workspace.setSetting("default_model_" + modelRow.agent, id)
                    modelRow.current = id
                }
            }
        }
    }
    Rule { }
    SectionHeader { title: "System tray" }
    Group {
        SettingRow {
            title: "Show in the system tray"
            detail: "The icon shows when sessions are working (blue) or need you (amber), and lists them."
            icon: "notification-3"
            AppSwitch {
                checked: Workspace.setting("tray") !== "0"
                onToggled: general.trayToggled(checked)
            }
        }
        SettingRow {
            divider: true
            title: "Keep running when the window is closed"
            icon: "window"
            detail: "Closing the window hides it to the tray, and your sessions keep running; quit from the tray menu" + (Keymap.seq("quit") !== "" ? " or with " + Keymap.seq("quit") : "") + ". Off, or without the tray icon, closing the window quits harlo."
            AppSwitch {
                checked: Workspace.setting("close_to_tray") !== "0"
                onToggled: Workspace.setSetting("close_to_tray", checked ? "1" : "0")
            }
        }
    }
    Rule { }
    SectionHeader { title: "Drop-down window"; subtitle: "A chat with the active session that slides over everything from the top of the screen. Bind this command to a key in your desktop's shortcut settings (KDE: System Settings → Keyboard → Shortcuts → Add New → Command or Script)." }
    Group {
        SettingRow {
            title: "Command"
            detail: "Shows or hides the drop-down window of the running Harlo (it starts Harlo if needed)."
            icon: "terminal-box"
            RowLayout {
                spacing: 6
                Rectangle {
                    implicitWidth: cmdLabel.implicitWidth + 16
                    implicitHeight: 28
                    radius: 6
                    color: Theme.background
                    border.color: Theme.border
                    Label { id: cmdLabel; anchors.centerIn: parent; text: "harlo --toggle-dropdown"; color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
                }
                CopyButton { copyText: "harlo --toggle-dropdown" }
            }
        }
    }
    Rule { }
    SectionHeader { title: "Setup guide"; subtitle: "The guide Harlo shows on its first start: the agents, git and the theme. From a terminal: harlo --setup" }
    Group {
        SettingRow {
            title: "Open the setup guide"
            detail: "Install and sign in agents, pick the default agent and model, sign git in, choose a theme."
            icon: "compass-3"
            FlatButton { objectName: "openSetupGuide"; text: "Open"; font.pixelSize: Theme.px(12); onClicked: general.setupRequested() }
        }
    }
    Rule { }
    SectionHeader { title: "Startup" }
    Group {
        SettingRow {
            id: autostartRow
            property string error: ""
            title: "Start Harlo when you log in"
            detail: autostartRow.error !== "" ? autostartRow.error
                  : "Starts in the background, in the system tray."
            more: autostartRow.error !== "" ? "" : "The window opens if there is no tray. Adds Harlo to ~/.config/autostart."
            icon: "home"
            AppSwitch {
                objectName: "autostartSwitch"
                checked: Workspace.autostartEnabled()
                onToggled: {
                    autostartRow.error = Workspace.setAutostart(checked)
                    if (autostartRow.error !== "") checked = !checked
                }
            }
        }
        SettingRow {
            divider: true
            title: "Reopen tabs"
            detail: "Open the sessions, files and views that were open when Harlo last closed."
            icon: "time"
            AppSwitch {
                checked: general.restoreTabs
                onToggled: { Workspace.setSetting("restore_tabs", checked ? "1" : "0"); general.restoreTabs = checked }
            }
        }
        SettingRow {
            divider: true
            title: "Check for updates"
            detail: "Looks for new versions of Harlo and your agents every 6 hours, and offers to update them."
                    + (Updates.checkStatus !== "" ? "\n" + Updates.checkStatus : "")
            more: "Also at startup and when a remote host connects. Harlo updates itself when it was installed from a package or an AppImage."
            icon: "refresh"
            RowLayout {
                spacing: 8
                FlatButton {
                    objectName: "checkUpdatesNow"
                    text: Updates.checking ? "Checking…" : "Check now"
                    enabled: !Updates.checking
                    font.pixelSize: Theme.px(12)
                    onClicked: Updates.checkNow()
                }
                AppSwitch {
                    objectName: "checkUpdatesSwitch"
                    checked: Workspace.setting("check_updates") !== "0"
                    onToggled: Workspace.setSetting("check_updates", checked ? "1" : "0")
                }
            }
        }
    }
    Rule { }
    SectionHeader { title: "Graphics" }
    Group {
        SettingRow {
            id: graphicsRow
            objectName: "graphicsRow"
            // What draws the window now, and what's chosen for the next start.
            readonly property string running: {
                switch (GraphicsInfo.api) {
                case GraphicsInfo.OpenGL: return "OpenGL"
                case GraphicsInfo.Vulkan: return "Vulkan"
                case GraphicsInfo.Software: return "Software"
                default: return "another renderer"
                }
            }
            property string chosen: Workspace.setting("graphics") || "auto"
            readonly property string startedWith: Workspace.graphicsStartedWith()
            readonly property var choices: [["auto", "Automatic"], ["opengl", "OpenGL"], ["vulkan", "Vulkan"], ["software", "Software"]]
            title: "Renderer"
            detail: "What draws Harlo's window. Automatic is OpenGL; Software uses no graphics card (slow, for broken drivers). Now: " + running
                    + (startedWith === "" ? " (chosen by QSG_RHI_BACKEND or QT_QUICK_BACKEND in Harlo's environment)." : ".")
            icon: "window"
            Flow {
                spacing: 6
                Repeater {
                    model: graphicsRow.choices
                    ChoicePill {
                        required property var modelData
                        objectName: "graphics-" + modelData[0]
                        text: modelData[1]
                        selected: graphicsRow.chosen === modelData[0]
                        onClicked: { Workspace.setSetting("graphics", modelData[0]); graphicsRow.chosen = modelData[0] }
                    }
                }
            }
        }
        SettingRow {
            objectName: "graphicsRestartRow"
            // ("" : the environment chose, QSG_RHI_BACKEND or QT_QUICK_BACKEND.)
            visible: graphicsRow.startedWith !== "" && graphicsRow.chosen !== graphicsRow.startedWith
            divider: true
            title: "Restart to switch"
            detail: "The renderer is chosen when Harlo starts. Restarting stops agents that are running. If Harlo can't start with it, it goes back to Automatic by itself."
            detailColor: Theme.warning
            icon: "refresh"
            DialogButton { text: "Restart Harlo"; accent: true; onClicked: { Workspace.restartApp(); Qt.quit() } }
        }
    }
}
