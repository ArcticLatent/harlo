import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The setup guide: on harlo's first start (and from `harlo --setup`, the
// command palette or Settings → General), in Settings' window style, three
// pages: the agents (install them, sign them in, the one and the model new
// sessions start with), git (the hosting service and signing in to it), and
// the theme. Finishing or skipping it marks harlo set up (`setup_done`).
Popup {
    id: root
    objectName: "setupGuide"
    /// Signing an agent in to a model provider (Main opens the dialog).
    signal signInRequested(string agent, string label)
    property int step: 0
    readonly property var steps: [
        { title: "Agents", subtitle: "Install the coding agents you want, sign them in, and pick the one new sessions start with." },
        { title: "Git", subtitle: "Pick where your repositories live and sign git in, so pushing and pulling just work." },
        { title: "Theme", subtitle: "How harlo looks. Everything here is in Settings → Appearance too." },
    ]

    function show() { step = 0; open() }
    function finish() { Workspace.setSetting("setup_done", "1"); close() }

    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(820, parent.width - 48)
    height: Math.min(760, parent.height - 48)
    padding: 0
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }
    onOpened: { Workspace.refreshProviders(); Setup.refreshGit(); contentItem.forceActiveFocus() }
    // Escape skips it, as the button does.
    onClosed: Workspace.setSetting("setup_done", "1")
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Math.round(Theme.motion * 0.7) }
        NumberAnimation { property: "scale"; from: 0.98; to: 1; duration: Math.round(Theme.motion * 0.7); easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.5) } }

    component Heading: ColumnLayout {
        property string title
        property string subtitle
        Layout.fillWidth: true
        spacing: 3
        Label { text: parent.title; color: Theme.text; font.pixelSize: Theme.px(15); font.weight: Font.DemiBold }
        Label { visible: parent.subtitle !== ""; text: parent.subtitle; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
    }

    contentItem: ColumnLayout {
        spacing: 0

        // Head: the logo, the step's title, and where you are.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 24
            Layout.bottomMargin: 16
            spacing: 14
            RowLayout {
                Layout.fillWidth: true
                LogoMark { implicitHeight: Math.round(Theme.px(22)) }
                Item { Layout.fillWidth: true }
                // The steps: done, this one, still to come.
                Row {
                    spacing: 6
                    Repeater {
                        model: root.steps
                        Rectangle {
                            required property var modelData
                            required property int index
                            objectName: "setupStep-" + index
                            implicitWidth: stepLabel.implicitWidth + 22
                            implicitHeight: 28
                            radius: 14
                            color: index === root.step ? Theme.accentSoft : "transparent"
                            border.color: index === root.step ? Theme.accent : Theme.hairline
                            Label {
                                id: stepLabel
                                anchors.centerIn: parent
                                text: (index < root.step ? "✓ " : (index + 1) + "  ") + modelData.title
                                color: index === root.step ? Theme.text : Theme.textMuted
                                font.pixelSize: Theme.px(12)
                                font.weight: index === root.step ? Font.Medium : Font.Normal
                            }
                            TapHandler { onTapped: root.step = index }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                    }
                }
            }
            ColumnLayout {
                spacing: 4
                Label {
                    text: root.step === 0 ? "Welcome to harlo" : root.steps[root.step].title
                    color: Theme.text
                    font.pixelSize: Theme.px(22)
                    font.weight: Font.DemiBold
                    font.letterSpacing: -0.3
                }
                Label {
                    Layout.fillWidth: true
                    text: root.steps[root.step].subtitle
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(13.5)
                    wrapMode: Text.Wrap
                }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline }

        // The page.
        ScrollView {
            id: scroller
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            // (Only while it shows: the pages ask the agents and git tools.)
            Loader {
                width: scroller.availableWidth
                active: root.visible
                sourceComponent: [agentsPage, gitPage, themePage][root.step]
            }
        }

        // Foot: skip, back, next / finish.
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline }
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 16
            Layout.leftMargin: 24
            Layout.rightMargin: 24
            spacing: 8
            FlatButton {
                objectName: "setupSkip"
                text: "Skip setup"
                outlined: false
                accent: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                onClicked: root.finish()
            }
            Label {
                Layout.fillWidth: true
                text: "Open this again with harlo --setup"
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
                elide: Text.ElideRight
            }
            FlatButton {
                objectName: "setupBack"
                visible: root.step > 0
                text: "Back"
                font.pixelSize: Theme.px(13)
                onClicked: root.step--
            }
            FlatButton {
                objectName: "setupNext"
                text: root.step < root.steps.length - 1 ? "Next" : "Finish"
                variant: "primary"
                font.pixelSize: Theme.px(13)
                onClicked: root.step < root.steps.length - 1 ? root.step++ : root.finish()
            }
        }
    }

    // ---- 1: agents ---------------------------------------------------------
    Component {
        id: agentsPage
        ColumnLayout {
            id: agents
            spacing: 12
            readonly property var list: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []
            property string chosen: Workspace.setting("default_harness")
            readonly property var defaultAgent: list.find(p => p.id === chosen) || null
            Item { implicitHeight: 6 }
            Repeater {
                model: agents.list
                Rectangle {
                    id: agentCard
                    required property var modelData
                    readonly property string id: modelData.id
                    readonly property var inst: Setup.revision >= 0 && Setup.installState(id) !== "" ? JSON.parse(Setup.installState(id)) : null
                    readonly property bool installing: inst !== null && inst.state === "running"
                    readonly property bool installed: modelData.state !== "missing"
                    readonly property bool usable: installed && modelData.canChat
                    readonly property var acct: Accounts.revision >= 0 ? JSON.parse(Accounts.status(id)) : ({ accounts: [] })
                    readonly property string signedIn: id === "omp"
                        ? (modelData.modelCount > 0 ? "models from " + [...new Set((modelData.modelIds || []).map(m => m.split("/")[0]))].length + " providers" : "")
                        : (acct.accounts || []).map(a => a.label).join(", ")
                    readonly property bool isDefault: agents.chosen === id
                    objectName: "setupAgent-" + id
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    implicitHeight: agentRow.implicitHeight + 26
                    radius: 12
                    color: Theme.raised
                    border.color: isDefault ? Theme.accent : Theme.hairline
                    border.width: isDefault ? 2 : 1
                    onInstChanged: if (inst && inst.state === "done") Workspace.refreshProviders()
                    Component.onCompleted: if (installed && id !== "omp") Accounts.refresh(id)
                    RowLayout {
                        id: agentRow
                        x: 14
                        y: 13
                        width: parent.width - 28
                        spacing: 12
                        AgentAvatar { provider: agentCard.id; implicitWidth: 34; implicitHeight: 34; ring: Theme.raised }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Label { text: agentCard.modelData.label; color: Theme.text; font.pixelSize: Theme.px(14); font.weight: Font.DemiBold }
                            Label {
                                objectName: "setupAgentStatus-" + agentCard.id
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                font.pixelSize: Theme.px(12.5)
                                color: agentCard.inst && agentCard.inst.state === "failed" ? Theme.error
                                    : agentCard.installed && agentCard.signedIn !== "" ? Theme.success : Theme.textMuted
                                text: agentCard.installing || (agentCard.inst && agentCard.inst.state === "failed" && !agentCard.installed) ? agentCard.inst.message
                                    : agentCard.modelData.state === "checking" ? "Checking…"
                                    : !agentCard.installed ? "Not installed"
                                    : "Installed" + (agentCard.modelData.version ? " · " + agentCard.modelData.version : "")
                                      + (agentCard.signedIn !== "" ? " · signed in: " + agentCard.signedIn
                                         : agentCard.acct.state === "loading" && agentCard.id !== "omp" ? "" : " · not signed in yet")
                            }
                        }
                        BusyIndicator { visible: agentCard.installing; running: visible; implicitWidth: 18; implicitHeight: 18 }
                        FlatButton {
                            objectName: "setupInstall-" + agentCard.id
                            visible: !agentCard.installed && !agentCard.installing && Setup.installCommand(agentCard.id) !== ""
                            text: "Install"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            ToolTip.visible: hovered
                            ToolTip.text: "Runs its official installer:\n" + Setup.installCommand(agentCard.id)
                            onClicked: Setup.installAgent(agentCard.id)
                        }
                        FlatButton {
                            objectName: "setupSignIn-" + agentCard.id
                            visible: agentCard.installed
                            text: agentCard.signedIn !== "" ? "Sign in to another…" : "Sign in…"
                            font.pixelSize: Theme.px(12)
                            onClicked: root.signInRequested(agentCard.id, agentCard.modelData.label)
                        }
                        FlatButton {
                            objectName: "setupDefault-" + agentCard.id
                            visible: agentCard.usable
                            text: agentCard.isDefault ? "Default" : "Use by default"
                            variant: agentCard.isDefault ? "selected" : "outline"
                            font.pixelSize: Theme.px(12)
                            onClicked: {
                                if (!agentCard.modelData.enabled) Workspace.setProviderEnabled(agentCard.id, true)
                                Workspace.setSetting("default_harness", agentCard.id)
                                agents.chosen = agentCard.id
                            }
                        }
                    }
                }
            }
            // The model new sessions with the default agent start with.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.topMargin: 8
                Layout.bottomMargin: 20
                spacing: 8
                visible: agents.defaultAgent !== null
                readonly property var ids: agents.defaultAgent ? agents.defaultAgent.modelIds || [] : []
                readonly property var names: agents.defaultAgent ? agents.defaultAgent.models || [] : []
                property string current: agents.defaultAgent ? Workspace.setting("default_model_" + agents.defaultAgent.id) : ""
                id: modelPick
                Heading {
                    title: "Model"
                    subtitle: agents.defaultAgent ? "New " + agents.defaultAgent.label + " sessions start with this one; you can still pick another for each session." : ""
                }
                ComboField {
                    objectName: "setupModel"
                    visible: modelPick.ids.length > 0
                    Layout.fillWidth: true
                    Layout.maximumWidth: 420
                    model: [agents.defaultAgent ? agents.defaultAgent.label + "'s own default" : ""].concat(modelPick.ids.map((id, i) => modelPick.names[i] || id))
                    currentIndex: Math.max(0, modelPick.ids.indexOf(modelPick.current) + 1)
                    onActivated: i => {
                        const id = i === 0 ? "" : modelPick.ids[i - 1]
                        Workspace.setSetting("default_model_" + agents.defaultAgent.id, id)
                        modelPick.current = id
                    }
                }
                Label {
                    visible: modelPick.ids.length === 0
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: agents.defaultAgent ? agents.defaultAgent.label + " lists its models once it's signed in to a provider." : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12.5)
                }
            }
            Label {
                visible: agents.defaultAgent === null
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.bottomMargin: 20
                wrapMode: Text.Wrap
                text: "No default picked: harlo starts the first installed agent. Pick one above to choose its model too."
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
            }
        }
    }

    // ---- 2: git ------------------------------------------------------------
    Component {
        id: gitPage
        ColumnLayout {
            id: gitHosts
            spacing: 12
            property string chosen: Workspace.setting("git_host") || "github"
            Item { implicitHeight: 6 }
            Repeater {
                model: ["github", "gitlab", "forgejo"]
                GitHostCard {
                    required property string modelData
                    service: modelData
                    chosen: gitHosts.chosen === modelData
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    onPick: { Workspace.setSetting("git_host", modelData); gitHosts.chosen = modelData }
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.bottomMargin: 20
                wrapMode: Text.Wrap
                text: "Signing in sets up git for that site only (HTTPS pushes and pulls); SSH remotes keep using your SSH keys. The sign-in stays with the GitHub CLI, glab, or git's credential store; harlo keeps no token."
                color: Theme.textFaint
                font.pixelSize: Theme.px(12)
            }
        }
    }

    // ---- 3: theme ----------------------------------------------------------
    Component {
        id: themePage
        ColumnLayout {
            id: themes
            spacing: 12
            readonly property var families: Workspace.themeFamilies().map(l => {
                const p = l.split("\t")
                return { name: p[0], dark: p[1].split(","), light: p[2].split(",") }
            })
            readonly property var current: families.find(f => f.name === Workspace.themeFamily) || families[0]
            Item { implicitHeight: 6 }
            Heading { Layout.leftMargin: 24; Layout.rightMargin: 24; title: "Color scheme"; subtitle: "System follows your desktop's light or dark setting." }
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                columns: 3
                columnSpacing: 12
                rowSpacing: 12
                Repeater {
                    model: [["system", "System"], ["light", "Light"], ["dark", "Dark"]]
                    Rectangle {
                        id: schemeCard
                        required property var modelData
                        objectName: "setupScheme-" + modelData[0]
                        readonly property bool chosen: Workspace.themeMode === modelData[0]
                        Layout.fillWidth: true
                        implicitHeight: 118
                        radius: 12
                        color: schemeHover.hovered ? Theme.rowHover : Theme.raised
                        border.color: chosen ? Theme.accent : Theme.hairline
                        border.width: chosen ? 2 : 1
                        HoverHandler { id: schemeHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Workspace.chooseThemeMode(schemeCard.modelData[0]) }
                        Item {
                            x: 8
                            y: 8
                            width: parent.width - 16
                            height: 78
                            MiniWindow { anchors.fill: parent; colors: schemeCard.modelData[0] === "dark" ? themes.current.dark : themes.current.light }
                            Item {
                                visible: schemeCard.modelData[0] === "system"
                                x: parent.width / 2
                                width: parent.width / 2
                                height: parent.height
                                clip: true
                                MiniWindow { x: -parent.x; width: parent.parent.width; height: parent.height; colors: themes.current.dark }
                            }
                        }
                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 93
                            text: schemeCard.modelData[1]
                            color: schemeCard.chosen ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(13)
                            font.weight: schemeCard.chosen ? Font.DemiBold : Font.Normal
                        }
                    }
                }
            }
            Heading { Layout.leftMargin: 24; Layout.rightMargin: 24; Layout.topMargin: 10; title: "Theme"; subtitle: themes.families.length + " themes, each in light and dark." }
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.bottomMargin: 20
                columns: Math.max(2, Math.floor((width + 10) / 170))
                columnSpacing: 10
                rowSpacing: 10
                Repeater {
                    model: themes.families
                    Rectangle {
                        id: themeCard
                        required property var modelData
                        objectName: "setupTheme-" + modelData.name
                        readonly property bool current: Workspace.themeFamily === modelData.name
                        Layout.fillWidth: true
                        implicitHeight: 100
                        radius: 12
                        color: themeHover.hovered ? Theme.rowHover : Theme.raised
                        border.color: current ? Theme.accent : Theme.hairline
                        border.width: current ? 2 : 1
                        HoverHandler { id: themeHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Workspace.chooseTheme(themeCard.modelData.name) }
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 12
                            spacing: 10
                            Repeater {
                                model: ["light", "dark"]
                                Item {
                                    required property string modelData
                                    readonly property var cols: themeCard.modelData[modelData]
                                    width: 46
                                    height: 46
                                    visible: cols[0] !== ""
                                    Rectangle {
                                        visible: themeCard.current && Theme.variant === parent.modelData
                                        anchors.fill: parent
                                        radius: width / 2
                                        color: "transparent"
                                        border.color: Theme.accent
                                        border.width: 2
                                    }
                                    ThemeOrb { anchors.centerIn: parent; width: 38; height: 38; colors: parent.cols }
                                }
                            }
                        }
                        Label {
                            x: 12
                            y: parent.height - height - 10
                            width: parent.width - 24
                            text: themeCard.modelData.name
                            color: themeCard.current ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(12.5)
                            font.weight: themeCard.current ? Font.DemiBold : Font.Normal
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }
}
