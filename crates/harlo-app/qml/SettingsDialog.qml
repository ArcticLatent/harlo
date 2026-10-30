import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import harlo

// Settings in a modal window: a searchable list of pages on the left (with
// icons, grouped), the chosen page on the right. Everything applies at once
// and is saved in harlo's database.
Popup {
    id: root
    property var shortcuts: []
    signal trayToggled(bool on)
    signal promptRailToggled(bool on)
    signal pairDeviceRequested()
    /// The setup guide (Settings → General).
    signal setupRequested()
    /// Signing an agent in to a model provider.
    signal signInRequested(string agent, string label)
    /// MCP sign-in: run this in a terminal.
    signal runInTerminalRequested(string command)
    /// A command in a terminal in `folder` (`ssh://host/…`: on that host).
    signal runOnHostRequested(string folder, string command)
    /// Where a page's floating action bar goes.
    readonly property Item pageFooter: footerHost
    property string page: "general"

    readonly property var pages: [
        { id: "general", title: "General", icon: "settings-3", group: "harlo", keywords: "setup guide welcome first model default harness new session startup reopen restore tabs tray close background drop-down quake yakuake hotkey shortcut graphics renderer gpu vulkan opengl software acceleration" },
        { id: "appearance", title: "Appearance", icon: "palette", group: "harlo", keywords: "glass blur opacity transparency shadow theme colour color dark light mode font monospace code text size prompt navigator rail jump" },
        { id: "notifications", title: "Notifications", icon: "notification-3", group: "harlo", keywords: "notify desktop alert finished input" },
        { id: "devices", title: "Devices", icon: "smartphone", group: "harlo", keywords: "phone pair device mobile qr tailscale remote access" },
        { id: "voice", title: "Voice", icon: "mic", group: "harlo", keywords: "voice speech dictation microphone mic whisper transcribe speak" },
        { id: "shortcuts", title: "Shortcuts", icon: "command", group: "harlo", keywords: "keyboard keys hotkeys" },
        { id: "usage", title: "Usage", icon: "bar-chart-2", group: "harlo", keywords: "stats statistics tokens cost models tools" },
        { id: "projects", title: "Projects", icon: "folder-3", group: "workspace", keywords: "project icon colour color name harness setup remove" },
        { id: "worktrees", title: "Worktrees", icon: "git-branch", group: "workspace", keywords: "worktree folder location branch" },
        { id: "git", title: "Git", icon: "git-repository", group: "workspace", keywords: "undo snapshot turn revert rollback github gitlab forgejo codeberg gitea hosting sign in login token credentials changes count repository hidden sidebar uncommitted commit message model ai" },
        { id: "schedules", title: "Schedules", icon: "history", group: "workspace", keywords: "schedule scheduled prompt cron timer daily weekday every hour morning automatic run" },
        { id: "mcp", title: "MCP", icon: "node-tree", group: "agents", keywords: "mcp tool servers model context protocol tools stdio http oauth" },
        { id: "orchestration", title: "Orchestration", icon: "ai-agent", group: "agents", keywords: "orchestration skills handoff advisor committee second opinion delegate subagent agents start sessions mcp bridge tools" },
        { id: "remote", title: "Remote Instances", icon: "server", group: "agents", keywords: "remote ssh server host machine nas instance connect" },
        { id: "about", title: "About Harlo", icon: "information", group: "about", keywords: "about version release licence license mit website source code github changelog what's new report problem issue bug logs" },
        { id: "providers", title: "Providers", icon: "robot-2", group: "agents", keywords: "providers harnesses claude opencode pi omp oh my pi binary path program agent models version enable disable debug log logs protocol crash diagnostics goal subagent model role sign in login log in account accounts api key oauth credentials" },
    ]
    // (About goes last, under no heading.)
    readonly property var groups: [["harlo", "harlo"], ["workspace", "Workspace"], ["agents", "Agents"], ["about", ""]]
    property string query: ""
    readonly property var shownPages: {
        const q = query.trim().toLowerCase()
        return pages.filter(p => q === "" || (p.title + " " + p.keywords).toLowerCase().includes(q))
    }
    onShownPagesChanged: if (shownPages.length > 0 && !shownPages.some(p => p.id === page)) page = shownPages[0].id
    readonly property var current: pages.find(p => p.id === page) || pages[0]

    function openPage(id) { page = id; open() }
    /// Settings → Projects on one project.
    property int projectId: -1
    readonly property var projectIconPresets: ["code-box", "terminal-box", "rocket", "flask", "gamepad", "briefcase", "home", "global", "leaf", "shield", "palette",
        "server", "smartphone", "database-2", "lightbulb", "music", "camera", "book-open", "heart"]
    function openProject(id) { projectId = id; openPage("projects") }
    // Test hook.
    function grab(path) { card.grabToImage(r => r.saveToFile(path)) }

    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(1120, parent.width - 48)
    readonly property bool compact: width < 760
    height: Math.min(860, parent.height - 48)
    padding: 0
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }
    onOpened: searchField.forceActiveFocus()
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Math.round(Theme.motion * 0.7) }
        NumberAnimation { property: "scale"; from: 0.98; to: 1; duration: Math.round(Theme.motion * 0.7); easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.5) } }

    // ---- building blocks --------------------------------------------------
    component SectionHeader: ColumnLayout {
        property string title
        property string subtitle
        Layout.fillWidth: true
        spacing: 3
        Label { text: parent.title; color: Theme.text; font.pixelSize: Theme.px(16); font.weight: Font.DemiBold; font.letterSpacing: -0.1 }
        Label { visible: parent.subtitle !== ""; text: parent.subtitle; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
    }
    component Rule: Rectangle { Layout.fillWidth: true; Layout.topMargin: 22; Layout.bottomMargin: 18; implicitHeight: 1; color: Theme.hairline }
    component Hint: Label { color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
    component FieldLabel: Label { color: Theme.foreground; font.pixelSize: Theme.px(13); Layout.topMargin: 8 }
    // A bordered group of setting rows.
    component Group: Rectangle {
        default property alias rows: groupCol.data
        Layout.fillWidth: true
        Layout.topMargin: 12
        implicitHeight: groupCol.implicitHeight
        radius: 12
        color: Theme.raised
        border.color: Theme.hairline
        ColumnLayout { id: groupCol; width: parent.width; spacing: 0 }
    }
    // Title and description on the left, a control on the right.
    component SettingRow: Item {
        id: settingRow
        property string title
        property string detail
        property color detailColor: Theme.muted
        property string icon: ""
        property bool divider: false
        default property alias control: controlSlot.data
        // Too narrow for the control beside the text: it goes under it.
        readonly property real textX: 16 + (icon !== "" ? 44 : 0)
        readonly property bool stacked: controlSlot.implicitWidth > 0 && width - textX - 16 - controlSlot.implicitWidth - 14 < Math.min(220, width * 0.4)
        readonly property real textWidth: stacked ? width - textX - 16 : width - textX - 16 - controlSlot.implicitWidth - 14
        Layout.fillWidth: true
        implicitHeight: stacked ? rowText.implicitHeight + controlSlot.implicitHeight + 34
                                : Math.max(rowText.implicitHeight, controlSlot.implicitHeight) + 26
        Rectangle { visible: settingRow.divider; width: parent.width; height: 1; color: Theme.hairline }
        Rectangle {
            visible: settingRow.icon !== ""
            x: 16
            y: settingRow.stacked ? 13 : (settingRow.height - height) / 2
            width: 32
            height: 32
            radius: 9
            color: Theme.controlFill
            Icon { anchors.centerIn: parent; name: settingRow.icon; color: Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
        }
        ColumnLayout {
            id: rowText
            x: settingRow.textX
            y: settingRow.stacked ? 13 : (settingRow.height - implicitHeight) / 2
            width: settingRow.textWidth
            spacing: 2
            Label { text: settingRow.title; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium; Layout.fillWidth: true; wrapMode: Text.Wrap }
            Label { visible: settingRow.detail !== ""; text: settingRow.detail; color: settingRow.detailColor === Theme.muted ? Theme.textMuted : settingRow.detailColor; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true; wrapMode: Text.Wrap }
        }
        Item {
            id: controlSlot
            x: settingRow.stacked ? settingRow.textX : settingRow.width - 16 - implicitWidth
            y: settingRow.stacked ? rowText.y + rowText.implicitHeight + 8 : (settingRow.height - implicitHeight) / 2
            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
            width: implicitWidth
            height: implicitHeight
        }
    }
    component Pill: Button {
        id: pill
        property bool selected: false
        // A provider's pill: its mark before the name.
        property string provider: ""
        flat: true
        padding: 5
        leftPadding: 11
        rightPadding: 11
        font.pixelSize: Theme.px(12)
        contentItem: Row {
            spacing: 6
            ProviderMark { anchors.verticalCenter: parent.verticalCenter; visible: pill.provider !== ""; provider: pill.provider; tip: ""; implicitWidth: 12; implicitHeight: 12; color: pill.selected ? Theme.foreground : Theme.muted }
            Label { anchors.verticalCenter: parent.verticalCenter; text: pill.text; color: pill.selected ? Theme.foreground : Theme.muted; font: pill.font }
        }
        background: ControlSurface { control: pill; variant: pill.selected ? "selected" : "outline"; radius: 7 }
    }
    component Choice: Rectangle {
        id: choice
        property string title
        property string detail
        property bool selected: false
        signal picked()
        Layout.fillWidth: true
        implicitHeight: choiceCol.implicitHeight + 22
        color: choiceHover.hovered ? Theme.hover : "transparent"
        HoverHandler { id: choiceHover }
        TapHandler { onTapped: choice.picked() }
        RowLayout {
            anchors.fill: parent
            anchors.margins: 11
            anchors.leftMargin: 16
            anchors.rightMargin: 16
            spacing: 12
            Rectangle {
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: 1
                implicitWidth: 16
                implicitHeight: 16
                radius: 8
                color: "transparent"
                border.color: choice.selected ? Theme.primary : Theme.muted
                border.width: choice.selected ? 5 : 1.5
            }
            ColumnLayout {
                id: choiceCol
                Layout.fillWidth: true
                spacing: 2
                Label { Layout.fillWidth: true; text: choice.title; color: Theme.foreground; font.pixelSize: Theme.px(13) }
                Label { visible: choice.detail !== ""; text: choice.detail; color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
        }
    }
    // A value next to a slider ("100%", "160 ms").
    component ValueChip: Rectangle {
        property alias text: chipLabel.text
        implicitWidth: Math.max(52, chipLabel.implicitWidth + 16)
        implicitHeight: 26
        radius: 7
        color: Theme.controlFill
        Label { id: chipLabel; anchors.centerIn: parent; color: Theme.text; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
    }
    FolderDialog {
        id: worktreeDialog
        title: "Folder for new worktrees"
        onAccepted: Workspace.setSetting("worktree_dir", decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, "")))
    }
    FileDialog {
        id: programDialog
        property string harness: ""
        title: "Choose the " + harness + " binary"
        onAccepted: { Workspace.setSetting(harness + "_path", decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, ""))); Workspace.refreshProviders() }
    }

    // ---- window -----------------------------------------------------------
    contentItem: Item {
        // The glass behind it (the background) shows through.
        Rectangle {
            id: card
            anchors.fill: parent
            radius: 14
            color: "transparent"
            clip: true

            RowLayout {
                anchors.fill: parent
                anchors.margins: 1
                spacing: 0

                // Page list (icons only in a narrow window)
                Rectangle {
                    Layout.fillHeight: true
                    Layout.preferredWidth: root.compact ? 58 : 236
                    // A shade over the glass.
                    color: Theme.glass ? Qt.rgba(Theme.chrome.r, Theme.chrome.g, Theme.chrome.b, 0.6) : Theme.chrome
                    radius: 13
                    topRightRadius: 0
                    bottomRightRadius: 0
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hairline }
                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: root.compact ? 8 : 14
                        anchors.rightMargin: root.compact ? 9 : 15
                        spacing: 2
                        AppTextField {
                            id: searchField
                            visible: !root.compact
                            Layout.fillWidth: true
                            Layout.bottomMargin: 10
                            leftPadding: 32
                            placeholderText: "Search settings"
                            onTextChanged: root.query = text
                            Keys.onEscapePressed: event => { if (text !== "") { text = ""; event.accepted = true } else event.accepted = false }
                            Icon { x: 10; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                        }
                        Repeater {
                            model: root.groups
                            ColumnLayout {
                                id: groupBlock
                                required property var modelData
                                readonly property var items: root.shownPages.filter(p => p.group === modelData[0])
                                visible: items.length > 0
                                Layout.fillWidth: true
                                spacing: 2
                                Label {
                                    visible: !root.compact && groupBlock.modelData[1] !== ""
                                    text: groupBlock.modelData[1].toUpperCase()
                                    color: Theme.label
                                    font.pixelSize: Theme.px(11.5)
                                    font.weight: Font.Medium
                                    font.letterSpacing: Theme.px(11.5) * 0.07
                                    Layout.topMargin: 12
                                    Layout.bottomMargin: 4
                                    Layout.leftMargin: 10
                                }
                                Repeater {
                                    model: groupBlock.items
                                    ItemDelegate {
                                        id: navItem
                                        required property var modelData
                                        required property int index
                                        objectName: "settingsNav-" + modelData.id
                                        readonly property bool current: root.page === modelData.id
                                        Layout.fillWidth: true
                                        implicitHeight: 36
                                        leftPadding: root.compact ? 12 : 10
                                        rightPadding: root.compact ? 0 : 8
                                        Layout.topMargin: root.compact && index === 0 ? 8 : 0
                                        ToolTip.visible: root.compact && hovered
                                        ToolTip.text: modelData.title
                                        contentItem: RowLayout {
                                            spacing: 10
                                            Icon { name: navItem.modelData.icon; color: navItem.current ? Theme.accent : Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
                                            Label {
                                                visible: !root.compact
                                                Layout.fillWidth: true
                                                text: navItem.modelData.title
                                                color: navItem.current ? Theme.text : Theme.textMuted
                                                font.pixelSize: Theme.px(14)
                                                font.weight: navItem.current ? Font.Medium : Font.Normal
                                                elide: Text.ElideRight
                                            }
                                        }
                                        background: Rectangle { radius: 9; color: navItem.current ? Theme.rowSelected : navItem.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                                        onClicked: root.page = modelData.id
                                    }
                                }
                            }
                        }
                        Label {
                            visible: root.shownPages.length === 0
                            text: "No settings match “" + root.query + "”."
                            color: Theme.muted
                            font.pixelSize: Theme.px(12)
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                            Layout.leftMargin: 8
                            Layout.topMargin: 8
                        }
                        Item { Layout.fillHeight: true }
                    }
                }

                // Page
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    Loader {
                        id: pageLoader
                        anchors.fill: parent
                        sourceComponent: root.page === "usage" ? usagePage : scrolledPage
                    }
                    // A page's floating action bar (an MCP server's Save,
                    // Cancel, Delete), over the scrolled content.
                    Item {
                        id: footerHost
                        objectName: "settingsFooter"
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.rightMargin: 12
                        height: childrenRect.height
                        z: 2
                    }

                    Button {
                        id: closeButton
                        anchors.top: parent.top
                        anchors.right: parent.right
                        anchors.margins: 12
                        flat: true
                        implicitWidth: 30
                        implicitHeight: 30
                        onClicked: root.close()
                        contentItem: Icon { name: "close"; color: closeButton.hovered ? Theme.foreground : Theme.muted }
                        background: Rectangle { radius: 6; color: closeButton.hovered ? Theme.hover : "transparent" }
                        ToolTip.visible: hovered
                        ToolTip.text: "Close (Esc)"
                    }
                }
            }
        }
    }

    // ---- pages ------------------------------------------------------------
    Component {
        id: usagePage
        ColumnLayout {
            spacing: 0
            SectionHeader {
                Layout.leftMargin: 16
                Layout.topMargin: 24
                Layout.bottomMargin: 4
                title: "Usage"
                subtitle: "Your plans' limits, and totals across all pi and omp sessions."
            }
            PlanLimits {
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16
                Layout.topMargin: 8
                Layout.bottomMargin: 8
            }
            // Spending limits: per session and per day, in dollars.
            ColumnLayout {
                id: spendLimits
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16
                Layout.topMargin: 10
                Layout.bottomMargin: 8
                spacing: 0
                readonly property var spend: Workspace.spendJson !== "" ? JSON.parse(Workspace.spendJson) : ({ limits: { session: 0, day: 0 }, today: 0 })
                function amount(t) { const v = parseFloat(String(t).replace(/[$,\s]/g, "")); return isFinite(v) && v > 0 ? v : 0 }
                function shown(v) { return v > 0 ? (v % 1 === 0 ? v.toFixed(0) : v.toFixed(2)) : "" }
                function save() { Workspace.setSpendLimits(amount(sessionLimit.text), amount(dayLimit.text)) }
                SectionHeader {
                    title: "Spending limits"
                    subtitle: "What the agents report their sessions cost (checked at each tool call and at the end of each turn; Claude Code at the end of each turn). Past a limit, harlo stops the run and holds the session's messages until you allow more. Empty: no limit. Today so far: $" + (spendLimits.spend.today || 0).toFixed(2) + " in harlo's sessions."
                }
                Group {
                    SettingRow {
                        title: "Per session"
                        detail: "One session's cost so far."
                        icon: "scales-3"
                        AppTextField {
                            id: sessionLimit
                            objectName: "spendSession"
                            implicitWidth: 110
                            placeholderText: "no limit"
                            text: spendLimits.shown(spendLimits.spend.limits.session)
                            leftPadding: 22
                            Label { x: 10; anchors.verticalCenter: parent.verticalCenter; text: "$"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            onEditingFinished: spendLimits.save()
                        }
                    }
                    SettingRow {
                        divider: true
                        title: "Per day"
                        detail: "All of harlo's sessions together, since midnight."
                        icon: "time"
                        AppTextField {
                            id: dayLimit
                            objectName: "spendDay"
                            implicitWidth: 110
                            placeholderText: "no limit"
                            text: spendLimits.shown(spendLimits.spend.limits.day)
                            leftPadding: 22
                            Label { x: 10; anchors.verticalCenter: parent.verticalCenter; text: "$"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            onEditingFinished: spendLimits.save()
                        }
                    }
                }
            }
            StatsView { Layout.fillWidth: true; Layout.fillHeight: true; color: "transparent" }
        }
    }

    Component {
        id: scrolledPage
        Flickable {
            id: pageFlick
            contentHeight: pageBody.implicitHeight + 64
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ColumnLayout {
                id: pageBody
                x: Math.max(28, (pageFlick.width - width) / 2)
                y: 26
                // Room for the close button on the right; shortcuts use two columns.
                width: Math.min(pageFlick.width - 56 - 30, root.page === "shortcuts" ? 940 : 720)
                spacing: 0
                Label {
                    text: root.current.title
                    color: Theme.foreground
                    font.pixelSize: Theme.px(20)
                    font.bold: true
                    Layout.bottomMargin: 18
                }
                Loader {
                    Layout.fillWidth: true
                    sourceComponent: ({
                        general: generalPage, appearance: appearancePage, notifications: notificationsPage, voice: voicePage, devices: devicesPage,
                        shortcuts: shortcutsPage, projects: projectsPage, worktrees: worktreesPage, git: gitPage, schedules: schedulesPage, providers: providersPage, remote: remotePage, mcp: mcpPage, orchestration: orchestrationPage,
                        about: aboutPage
                    })[root.page] || null
                }
            }
        }
    }

    Component {
        id: generalPage
        ColumnLayout {
            id: general
            spacing: 0
            property string harness: Workspace.setting("default_harness")
            property bool restoreTabs: Workspace.setting("restore_tabs") !== "0"
            SectionHeader { title: "New sessions"; subtitle: "For sessions outside a project and for newly added projects. Each project keeps its own default (right-click it in the sidebar)." }
            Group {
                SettingRow {
                    title: "Provider"
                    detail: "Automatic picks the first one on in Settings → Providers that's installed."
                    icon: "robot-2"
                    Flow {
                        spacing: 6
                        Repeater {
                            model: [{ id: "", label: "Automatic" }].concat(Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : [])
                            Pill {
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
                    detail: prov ? "New " + prov.label + " sessions start with this one; the new-session page can still pick another."
                                 + (ids.length === 0 ? " " + prov.label + " lists its models once it's signed in to a provider." : "")
                                 : "Pick a provider first."
                    icon: "sparkling"
                    ComboField {
                        objectName: "defaultModel"
                        visible: modelRow.ids.length > 0
                        width: 260
                        model: [modelRow.prov ? modelRow.prov.label + "'s own default" : ""].concat(modelRow.ids.map((id, i) => (modelRow.prov.models || [])[i] || id))
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
                        onToggled: root.trayToggled(checked)
                    }
                }
                SettingRow {
                    divider: true
                    title: "Keep running when the window is closed"
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
                    detail: "Shows or hides the drop-down window of the running harlo (it starts harlo if needed)."
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
                        FlatButton {
                            text: copied ? "Copied" : "Copy"
                            property bool copied: false
                            font.pixelSize: Theme.px(12)
                            onClicked: { copyHelper.text = "harlo --toggle-dropdown"; copyHelper.selectAll(); copyHelper.copy(); copied = true }
                            TextEdit { id: copyHelper; visible: false }
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Setup guide"; subtitle: "The guide harlo shows on its first start: the agents, git and the theme. From a terminal: harlo --setup" }
            Group {
                SettingRow {
                    title: "Open the setup guide"
                    detail: "Install and sign in agents, pick the default agent and model, sign git in, choose a theme."
                    icon: "compass-3"
                    FlatButton { objectName: "openSetupGuide"; text: "Open"; font.pixelSize: Theme.px(12); onClicked: root.setupRequested() }
                }
            }
            Rule { }
            SectionHeader { title: "Startup" }
            Group {
                SettingRow {
                    id: autostartRow
                    property string error: ""
                    title: "Start harlo when you log in"
                    detail: autostartRow.error !== "" ? autostartRow.error
                          : "Starts in the background, in the system tray (the window opens if there is no tray). Adds harlo to ~/.config/autostart."
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
                    detail: "Open the sessions, files and views that were open when harlo last closed."
                    icon: "time"
                    AppSwitch {
                        checked: general.restoreTabs
                        onToggled: { Workspace.setSetting("restore_tabs", checked ? "1" : "0"); general.restoreTabs = checked }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Check for updates"
                    detail: "At startup and every 6 hours (and when a remote host connects), harlo looks for newer versions of itself (installed from a package or an AppImage) and of the agents you use, and offers to update them."
                            + (Updates.checkStatus !== "" ? "\n" + Updates.checkStatus : "")
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
                    detail: "What draws harlo's window. Automatic is OpenGL; Software uses no graphics card (slow, for broken drivers). Now: " + running
                            + (startedWith === "" ? " (chosen by QSG_RHI_BACKEND or QT_QUICK_BACKEND in harlo's environment)." : ".")
                    icon: "window"
                    Flow {
                        spacing: 6
                        Repeater {
                            model: graphicsRow.choices
                            Pill {
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
                    detail: "The renderer is chosen when harlo starts. Restarting stops agents that are running. If harlo can't start with it, it goes back to Automatic by itself."
                    detailColor: Theme.warning
                    icon: "refresh"
                    DialogButton { text: "Restart harlo"; accent: true; onClicked: { Workspace.restartApp(); Qt.quit() } }
                }
            }
        }
    }

    Component {
        id: appearancePage
        ColumnLayout {
            id: appearance
            spacing: 0
            readonly property var families: Workspace.themeFamilies().map(l => {
                const p = l.split("\t")
                return { name: p[0], dark: p[1].split(","), light: p[2].split(",") }
            })
            // The current family's two variants (the scheme cards draw them).
            readonly property var current: families.find(f => f.name === Workspace.themeFamily) || families[0]
            SectionHeader { title: "Color scheme"; subtitle: "System follows your desktop's light or dark setting." }
            GridLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                columns: 3
                columnSpacing: 12
                rowSpacing: 12
                Repeater {
                    model: [["system", "System"], ["light", "Light"], ["dark", "Dark"]]
                    Rectangle {
                        id: schemeCard
                        required property var modelData
                        objectName: "scheme-" + modelData[0]
                        readonly property bool chosen: Workspace.themeMode === modelData[0]
                        Layout.fillWidth: true
                        implicitHeight: 136
                        radius: 12
                        color: schemeHover.hovered ? Theme.rowHover : Theme.raised
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        border.color: chosen ? Theme.accent : Theme.hairline
                        border.width: chosen ? 2 : 1
                        HoverHandler { id: schemeHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Workspace.chooseThemeMode(schemeCard.modelData[0]) }
                        Item {
                            x: 8
                            y: 8
                            width: parent.width - 16
                            height: 96
                            MiniWindow {
                                anchors.fill: parent
                                colors: schemeCard.modelData[0] === "dark" ? appearance.current.dark : appearance.current.light
                            }
                            // System: light on the left, dark on the right.
                            Item {
                                visible: schemeCard.modelData[0] === "system"
                                x: parent.width / 2
                                width: parent.width / 2
                                height: parent.height
                                clip: true
                                MiniWindow { x: -parent.x; width: parent.parent.width; height: parent.height; colors: appearance.current.dark }
                            }
                        }
                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 110
                            text: schemeCard.modelData[1]
                            color: schemeCard.chosen ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(13)
                            font.weight: schemeCard.chosen ? Font.DemiBold : Font.Normal
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Themes"; subtitle: appearance.families.length + " themes, each in light and dark." }
            GridLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                columns: Math.max(2, Math.floor((width + 12) / 200))
                columnSpacing: 12
                rowSpacing: 12
                Repeater {
                    model: appearance.families
                    Rectangle {
                        id: themeCard
                        required property var modelData
                        objectName: "theme-" + modelData.name
                        readonly property bool current: Workspace.themeFamily === modelData.name
                        Layout.fillWidth: true
                        implicitHeight: 118
                        radius: 12
                        color: themeHover.hovered ? Theme.rowHover : Theme.raised
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        border.color: current ? Theme.accent : Theme.hairline
                        border.width: current ? 2 : 1
                        HoverHandler { id: themeHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Workspace.chooseTheme(themeCard.modelData.name) }
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 14
                            spacing: 14
                            Repeater {
                                model: ["light", "dark"]
                                Item {
                                    required property string modelData
                                    readonly property var cols: themeCard.modelData[modelData]
                                    // The variant showing now: ringed.
                                    readonly property bool showing: themeCard.current && Theme.variant === modelData
                                    width: 56
                                    height: 56
                                    visible: cols[0] !== ""
                                    Rectangle {
                                        visible: parent.showing
                                        anchors.fill: parent
                                        radius: width / 2
                                        color: "transparent"
                                        border.color: Theme.accent
                                        border.width: 2
                                    }
                                    ThemeOrb { anchors.centerIn: parent; width: 46; height: 46; colors: parent.cols }
                                }
                            }
                        }
                        Label {
                            x: 14
                            y: parent.height - height - 12
                            width: parent.width - 28
                            text: themeCard.modelData.name
                            color: themeCard.current ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(13)
                            font.weight: themeCard.current ? Font.DemiBold : Font.Normal
                            elide: Text.ElideRight
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Interface" }
            Group {
                SettingRow {
                    title: "Contrast"
                    detail: "How far apart the colours of surfaces, borders and text are, across the interface."
                    icon: "contrast"
                    RowLayout {
                        spacing: 10
                        FlatButton {
                            visible: (Theme.look.contrast || 100) !== 100
                            text: "Reset"
                            outlined: false
                            font.pixelSize: Theme.px(12)
                            onClicked: Workspace.chooseLook("contrast", "null")
                        }
                        ValueChip { text: Math.round(Theme.contrast * 100) + "%" }
                        AppSlider {
                            objectName: "contrastSlider"
                            implicitWidth: root.compact ? 110 : 160
                            from: 50
                            to: 200
                            stepSize: 5
                            value: Theme.contrast * 100
                            onMoved: Workspace.chooseLook("contrast", String(value))
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Glass"
                    detail: GraphicsInfo.api === GraphicsInfo.Software
                        ? "Needs graphics acceleration, which harlo is running without, so menus and dialogs stay solid."
                        : "Menus, dialogs and the composer show a soft blur of what's behind them."
                    icon: "window"
                    AppSwitch { objectName: "glassSwitch"; checked: Workspace.glass; onToggled: Workspace.chooseGlass(checked, Workspace.glassOpacity) }
                }
                SettingRow {
                    divider: true
                    title: "Glass opacity"
                    detail: "Higher values make menus and dialogs more solid."
                    icon: "eye"
                    RowLayout {
                        spacing: 10
                        FlatButton {
                            visible: Math.abs(Workspace.glassOpacity - 0.8) > 0.001
                            text: "Reset"
                            outlined: false
                            font.pixelSize: Theme.px(12)
                            onClicked: Workspace.chooseGlass(Workspace.glass, 0.8)
                        }
                        ValueChip { text: Math.round(Workspace.glassOpacity * 100) + "%" }
                        AppSlider {
                            objectName: "glassOpacity"
                            implicitWidth: root.compact ? 110 : 160
                            enabled: Workspace.glass
                            from: 0.4
                            to: 1.0
                            stepSize: 0.05
                            value: Workspace.glassOpacity
                            onMoved: Workspace.chooseGlass(true, value)
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Diff colours"
                    detail: "For lines added and removed, and change counts."
                    icon: "git-commit"
                    RowLayout {
                        spacing: 6
                        Repeater {
                            model: [["red-green", "Red & green", "#3dd68c", "#f26d6d"], ["blue-orange", "Blue & orange", "#5b9bf0", "#f2a65a"]]
                            Pill {
                                required property var modelData
                                objectName: "diff-" + modelData[0]
                                text: modelData[1]
                                selected: (Theme.look.diff || "red-green") === modelData[0]
                                onClicked: Workspace.chooseLook("diff", modelData[0] === "red-green" ? "null" : JSON.stringify(modelData[0]))
                            }
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Chat width"
                    detail: "How wide messages and the composer grow in a large window."
                    icon: "layout-column"
                    RowLayout {
                        spacing: 6
                        Repeater {
                            model: [["comfortable", "Comfortable"], ["wide", "Wide"], ["full", "Full width"]]
                            Pill {
                                required property var modelData
                                objectName: "chatWidth-" + modelData[0]
                                text: modelData[1]
                                selected: (Theme.look.chatWidth || "comfortable") === modelData[0]
                                onClicked: Workspace.chooseLook("chatWidth", modelData[0] === "comfortable" ? "null" : JSON.stringify(modelData[0]))
                            }
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Motion" }
            Group {
                SettingRow {
                    title: "Animations"
                    detail: "How fast dialogs and menus open and close (0: at once)."
                    icon: "speed"
                    RowLayout {
                        spacing: 10
                        ValueChip { text: Theme.motion + " ms" }
                        AppSlider {
                            objectName: "motionSlider"
                            implicitWidth: root.compact ? 110 : 160
                            from: 0
                            to: 400
                            stepSize: 20
                            value: Theme.motion
                            onMoved: Workspace.chooseLook("motion", value === 160 ? "null" : String(value))
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Typography" }
            Group {
                Repeater {
                    model: [
                        { kind: "ui", title: "Interface font", detail: "Everything outside code and the terminal.", fixed: false },
                        { kind: "mono", title: "Code font", detail: "Code blocks, diffs, file previews, the editor and the terminal.", fixed: true },
                    ]
                    ColumnLayout {
                        id: fontPick
                        required property var modelData
                        required property int index
                        readonly property bool ui: modelData.kind === "ui"
                        // The defaults: harlo's own fonts (Geist, JetBrains Mono).
                        readonly property string system: ui ? Theme.defaultUiFont : Theme.defaultMonoFont
                        readonly property string chosen: ui ? Workspace.uiFont : Workspace.monoFont
                        readonly property var installed: Workspace.fontFamilies(modelData.fixed).filter(f => f !== system)
                        // A chosen font is always listed, even if it isn't detected as monospace.
                        readonly property var families: ["Default (" + system + ")"].concat(installed,
                            chosen !== "" && installed.indexOf(chosen) < 0 ? [chosen] : [])
                        // Sizes in px: the interface's base size (text scale 1 = 14 px),
                        // and the code's (13 px at scale 1).
                        readonly property var sizes: ui ? [12, 13, 14, 15, 16, 17, 18, 20] : [10, 11, 12, 13, 14, 15, 16, 18]
                        readonly property int size: ui ? Math.round(14 * Workspace.textScale) : Math.round(13 * Workspace.textScale * Theme.monoScale)
                        Layout.fillWidth: true
                        spacing: 10
                        Rectangle { visible: fontPick.index > 0; Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
                        // The title, then the font and its size (under it when narrow).
                        GridLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 16
                            Layout.rightMargin: 16
                            Layout.topMargin: 10
                            columns: root.compact ? 2 : 3
                            columnSpacing: 10
                            rowSpacing: 8
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.columnSpan: root.compact ? 2 : 1
                                spacing: 2
                                Label { text: fontPick.modelData.title; color: Theme.text; font.pixelSize: Theme.px(13); font.weight: Font.Medium }
                                Label { Layout.fillWidth: true; text: fontPick.modelData.detail; color: Theme.textMuted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap }
                            }
                            ComboField {
                                objectName: "fontPick-" + fontPick.modelData.title
                                Layout.fillWidth: root.compact
                                Layout.preferredWidth: root.compact ? -1 : 210
                                fontPreview: true
                                defaultFont: fontPick.system
                                model: fontPick.families
                                currentIndex: fontPick.chosen === "" ? 0 : Math.max(0, fontPick.families.indexOf(fontPick.chosen))
                                onActivated: i => Workspace.chooseFont(fontPick.modelData.kind, i === 0 ? "" : fontPick.families[i])
                            }
                            ComboField {
                                objectName: "fontSize-" + fontPick.modelData.kind
                                Layout.preferredWidth: 90
                                model: fontPick.sizes.map(v => v + " px")
                                currentIndex: Math.max(0, fontPick.sizes.indexOf(fontPick.sizes.reduce((b, v) => Math.abs(v - fontPick.size) < Math.abs(b - fontPick.size) ? v : b)))
                                onActivated: i => {
                                    const v = fontPick.sizes[i]
                                    // Each size on its own: the code's stays when the interface's changes.
                                    const code = fontPick.ui ? 13 * Workspace.textScale * Theme.monoScale : v
                                    if (fontPick.ui) Workspace.chooseTextScale(v / 14)
                                    const m = code / (13 * Workspace.textScale)
                                    Workspace.chooseLook("monoScale", Math.abs(m - 1) < 0.01 ? "null" : String(m))
                                }
                            }
                        }
                        // A preview in the font, at its size.
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.leftMargin: 16
                            Layout.rightMargin: 16
                            Layout.bottomMargin: 14
                            implicitHeight: fontPick.ui ? uiSample.implicitHeight + 24 : codeSample.implicitHeight + 20
                            radius: 10
                            color: fontPick.ui ? Theme.canvas : Theme.codeBg
                            border.color: Theme.hairline
                            Label {
                                id: uiSample
                                visible: fontPick.ui
                                x: 14
                                y: 12
                                width: parent.width - 28
                                wrapMode: Text.Wrap
                                textFormat: Text.StyledText
                                color: Theme.text
                                font.family: Theme.uiFont
                                font.pixelSize: Theme.px(15)
                                text: "Use <b>Plan</b> to fix the flaky test in <font color=\"" + Theme.rgb(Theme.accent) + "\">surface.test.ts</font> and line the header up with the sidebar before shipping."
                            }
                            Column {
                                id: codeSample
                                visible: !fontPick.ui
                                x: 0
                                y: 10
                                width: parent.width
                                spacing: 0
                                Repeater {
                                    model: [
                                        { n: "1", k: " ", t: "export function formatUser(user: User) {" },
                                        { n: "2", k: "-", t: "  return user.name.toUpperCase();" },
                                        { n: "2", k: "+", t: "  return `${user.name} <${user.email}>`; // 0O 1lI" },
                                        { n: "3", k: " ", t: "}" },
                                        { n: "", k: " ", t: "" },
                                        { n: "$", k: "t", t: "✓ 85 passed   ⚠ 2 warnings   ✗ 0 failed" },
                                    ]
                                    Rectangle {
                                        required property var modelData
                                        width: codeSample.width
                                        height: codeLine.implicitHeight + 3
                                        color: modelData.k === "+" ? Qt.rgba(Theme.diffAdd.r, Theme.diffAdd.g, Theme.diffAdd.b, 0.16)
                                             : modelData.k === "-" ? Qt.rgba(Theme.diffRemove.r, Theme.diffRemove.g, Theme.diffRemove.b, 0.16) : "transparent"
                                        Rectangle { visible: modelData.k === "+" || modelData.k === "-"; width: 3; height: parent.height; color: modelData.k === "+" ? Theme.diffAdd : Theme.diffRemove }
                                        Label {
                                            x: 14
                                            width: 22
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: modelData.n
                                            color: Theme.textFaint
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.mpx(13)
                                        }
                                        Label {
                                            id: codeLine
                                            x: 44
                                            width: parent.width - 54
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: modelData.t
                                            elide: Text.ElideRight
                                            color: modelData.k === "t" ? Theme.success : Theme.text
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.mpx(13)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Word wrap"
                    detail: "Long lines wrap in code blocks."
                    icon: "text-wrap"
                    AppSwitch { objectName: "wrapSwitch"; checked: Theme.wrapCode; onToggled: Workspace.chooseLook("wrap", checked ? "true" : "null") }
                }
            }
            Rule { }
            SectionHeader { title: "Chat" }
            Group {
                SettingRow {
                    title: "Prompt navigator"
                    detail: "Ticks at the chat's right edge, one per message you sent. Hover to see them, click to jump. From the keyboard: Jump to a prompt, Previous / Next prompt (Settings → Shortcuts)."
                    icon: "chat-thread"
                    AppSwitch {
                        checked: Workspace.setting("prompt_rail") !== "0"
                        onToggled: { Workspace.setSetting("prompt_rail", checked ? "1" : "0"); root.promptRailToggled(checked) }
                    }
                }
            }
        }
    }

    Component {
        id: notificationsPage
        ColumnLayout {
            id: notify
            spacing: 0
            property var cfg: JSON.parse(Workspace.notifySettingsJson())
            readonly property var info: JSON.parse(Workspace.notifyDefaultsJson())
            function save() { Workspace.setNotifySettings(JSON.stringify(cfg)) }
            function setTop(key, value) { const c = Object.assign({}, cfg); c[key] = value; cfg = c; save() }
            function setEvent(name, key, value) {
                const c = Object.assign({}, cfg)
                c.events = Object.assign({}, c.events)
                c.events[name] = Object.assign({}, c.events[name])
                c.events[name][key] = value
                cfg = c
                save()
            }
            readonly property var eventRows: [
                { name: "completion", title: "Agent completion", detail: "A run finishes (and an omp goal ends)." },
                { name: "subagent", title: "Subagent completion", detail: "An omp subagent finishes." },
                { name: "error", title: "Agent errors", detail: "A reply fails, the harness stops unexpectedly, or a subagent fails." },
                { name: "question", title: "Agent questions", detail: "The agent asks you something or wants a tool approved." },
            ]
            property string testNote: ""

            SectionHeader { title: "Notification delivery"; subtitle: "Choose when and how you get notified. Clicking a notification opens its session." }
            Group {
                SettingRow {
                    title: "Enable notifications"
                    icon: "notification-3"
                    AppSwitch { objectName: "notifyEnabled"; checked: notify.cfg.enabled; onToggled: notify.setTop("enabled", checked) }
                }
                SettingRow {
                    visible: notify.cfg.enabled
                    divider: true
                    title: "Notify while harlo is focused"
                    detail: "Off: only while harlo is in the background. Never for the session you're looking at."
                    icon: "window"
                    AppSwitch { objectName: "notifyWhileFocused"; checked: notify.cfg.whileFocused; onToggled: notify.setTop("whileFocused", checked) }
                }
                SettingRow {
                    visible: notify.cfg.enabled
                    divider: true
                    title: "Play a sound"
                    detail: "If your desktop's notifications play sounds."
                    icon: "volume-up"
                    AppSwitch { objectName: "notifySound"; checked: notify.cfg.sound; onToggled: notify.setTop("sound", checked) }
                }
            }
            RowLayout {
                visible: notify.cfg.enabled
                Layout.topMargin: 10
                spacing: 10
                FlatButton {
                    objectName: "notifyTest"
                    text: "Send test notification"
                    font.pixelSize: Theme.px(12)
                    onClicked: { Workspace.sendTestNotification(); notify.testNote = "Sent. If nothing appeared, check that notify-send (libnotify) is installed." }
                }
                Hint { text: notify.testNote; Layout.fillWidth: true }
            }

            Rule {}
            SectionHeader { title: "Quiet hours"; subtitle: "At night, or whenever you want peace: no notifications here, and your iPhone gets its notifications silently (no sound, the screen stays dark; they wait in Notification Center)." }
            Group {
                id: quietGroup
                readonly property var q: notify.cfg.quiet || ({ on: false, from: 1320, to: 480, asks: false })
                // Every half hour, 00:00 to 23:30.
                readonly property var times: Array.from({ length: 48 }, (_, i) => String(Math.floor(i / 2)).padStart(2, "0") + (i % 2 ? ":30" : ":00"))
                function setQuiet(key, value) { const q = Object.assign({}, quietGroup.q); q[key] = value; notify.setTop("quiet", q) }
                SettingRow {
                    title: "Quiet hours"
                    detail: quietGroup.q.on ? "From " + quietGroup.times[Math.floor(quietGroup.q.from / 30)] + " to " + quietGroup.times[Math.floor(quietGroup.q.to / 30)] + ", every day." : "Off: notifications come at any hour."
                    icon: "moon"
                    AppSwitch { objectName: "quietOn"; checked: quietGroup.q.on; onToggled: quietGroup.setQuiet("on", checked) }
                }
                SettingRow {
                    visible: quietGroup.q.on
                    divider: true
                    title: "From"
                    icon: "time"
                    ComboField {
                        objectName: "quietFrom"
                        implicitWidth: 110
                        model: quietGroup.times
                        currentIndex: Math.floor(quietGroup.q.from / 30)
                        onActivated: i => quietGroup.setQuiet("from", i * 30)
                    }
                }
                SettingRow {
                    visible: quietGroup.q.on
                    divider: true
                    title: "To"
                    icon: "time"
                    ComboField {
                        objectName: "quietTo"
                        implicitWidth: 110
                        model: quietGroup.times
                        currentIndex: Math.floor(quietGroup.q.to / 30)
                        onActivated: i => quietGroup.setQuiet("to", i * 30)
                    }
                }
                SettingRow {
                    visible: quietGroup.q.on
                    divider: true
                    title: "Let questions through"
                    detail: "An agent that asks you something or wants a tool approved still notifies, with sound, so it isn't stuck all night."
                    icon: "question"
                    AppSwitch { objectName: "quietAsks"; checked: quietGroup.q.asks; onToggled: quietGroup.setQuiet("asks", checked) }
                }
            }

            Rule { visible: notify.cfg.enabled }
            SectionHeader { visible: notify.cfg.enabled; title: "Notification events" }
            Group {
                visible: notify.cfg.enabled
                Repeater {
                    model: notify.eventRows
                    SettingRow {
                        id: evRow
                        required property var modelData
                        required property int index
                        divider: index > 0
                        title: modelData.title
                        detail: modelData.detail
                        AppSwitch {
                            objectName: "notifyEvent-" + evRow.modelData.name
                            checked: notify.cfg.events[evRow.modelData.name].on
                            onToggled: notify.setEvent(evRow.modelData.name, "on", checked)
                        }
                    }
                }
            }

            Rule { visible: notify.cfg.enabled }
            SectionHeader {
                visible: notify.cfg.enabled
                title: "Notification templates"
                subtitle: "Leave a field empty for the default (shown faded). Variables:"
            }
            Flow {
                visible: notify.cfg.enabled
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 6
                Repeater {
                    model: notify.info.variables
                    Label {
                        required property string modelData
                        text: "{" + modelData + "}"
                        color: Theme.info
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(11.5)
                    }
                }
            }
            GridLayout {
                visible: notify.cfg.enabled
                Layout.fillWidth: true
                Layout.topMargin: 10
                columns: root.compact ? 1 : 2
                columnSpacing: 14
                rowSpacing: 14
                Repeater {
                    model: notify.eventRows
                    Rectangle {
                        id: card
                        required property var modelData
                        readonly property var ev: notify.cfg.events[modelData.name]
                        readonly property var def: notify.info.defaults[modelData.name]
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        implicitHeight: cardCol.implicitHeight + 24
                        radius: 10
                        color: Theme.elevated
                        border.color: Theme.border
                        ColumnLayout {
                            id: cardCol
                            x: 12
                            y: 12
                            width: parent.width - 24
                            spacing: 4
                            Label { text: card.modelData.title.replace("Agent ", "").replace(/^./, c => c.toUpperCase()); color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(13) }
                            FieldLabel { text: "Title"; Layout.topMargin: 4 }
                            AppTextField {
                                objectName: "notifyTitle-" + card.modelData.name
                                Layout.fillWidth: true
                                text: card.ev.title
                                placeholderText: card.def.title
                                font.pixelSize: Theme.px(12)
                                onEditingFinished: if (text !== card.ev.title) notify.setEvent(card.modelData.name, "title", text)
                            }
                            FieldLabel { text: "Message" }
                            AppTextField {
                                objectName: "notifyMessage-" + card.modelData.name
                                Layout.fillWidth: true
                                text: card.ev.message
                                placeholderText: card.def.message
                                font.pixelSize: Theme.px(12)
                                onEditingFinished: if (text !== card.ev.message) notify.setEvent(card.modelData.name, "message", text)
                            }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: aboutPage
        ColumnLayout {
            id: about
            spacing: 0
            // `{ version, installed }`
            readonly property var info: JSON.parse(Updates.about())
            readonly property bool fromSource: info.installed === "Built from source"
            readonly property string repo: "https://github.com/ArcticLatent/harlo"
            RowLayout {
                spacing: 16
                Image {
                    source: "qrc:/qt/qml/harlo/icons/app/harlo.svg"
                    sourceSize: Qt.size(128, 128)
                    Layout.preferredWidth: 64
                    Layout.preferredHeight: 64
                }
                ColumnLayout {
                    spacing: 3
                    Label { text: "Harlo"; color: Theme.text; font.pixelSize: Theme.px(22); font.weight: Font.DemiBold }
                    Label { objectName: "aboutVersion"; text: "Version " + about.info.version + " · " + about.info.installed; color: Theme.textMuted; font.pixelSize: Theme.px(13) }
                    Label { text: "Your coding agents, in a truly native app."; color: Theme.textMuted; font.pixelSize: Theme.px(13) }
                }
            }
            Group {
                Layout.topMargin: 22
                SettingRow {
                    title: "Updates"
                    detail: (about.fromSource ? "Built from source, so it doesn't update itself: pull and build again for a newer one."
                                              : "harlo looks for a newer version at startup and every 6 hours, and offers it at the bottom right of the window.")
                        + (Updates.checkStatus !== "" ? "\n" + Updates.checkStatus : "")
                    icon: "refresh"
                    FlatButton {
                        text: Updates.checking ? "Checking…" : "Check now"
                        enabled: !Updates.checking
                        font.pixelSize: Theme.px(12)
                        onClicked: Updates.checkNow()
                    }
                }
                SettingRow {
                    divider: true
                    title: "What's new"
                    detail: "What changed in each release."
                    icon: "book-open"
                    FlatButton { text: "Changelog"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally(about.repo + "/blob/" + (about.fromSource ? "main" : "v" + about.info.version) + "/CHANGELOG.md") }
                }
                SettingRow {
                    divider: true
                    title: "Website"
                    detail: "harlo.run: downloads, the iPhone app and help."
                    icon: "global"
                    FlatButton { text: "Open"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally("https://harlo.run") }
                }
                SettingRow {
                    divider: true
                    title: "Source code"
                    detail: "github.com/ArcticLatent/harlo"
                    icon: "git-repository"
                    FlatButton { text: "Open"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally(about.repo) }
                }
                SettingRow {
                    divider: true
                    title: "Report a problem"
                    detail: "Open an issue on GitHub. Logs and crash reports help: they're in " + Workspace.logsDir() + "."
                    icon: "bug"
                    RowLayout {
                        spacing: 8
                        FlatButton { text: "Logs"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally("file://" + Workspace.logsDir()) }
                        FlatButton { text: "New issue"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally(about.repo + "/issues/new") }
                    }
                }
            }
            Rule { }
            SectionHeader {
                title: "Licence"
                subtitle: "harlo is free and open source, under the MIT licence. It's built with Qt (LGPLv3) and includes fonts, icons and logos under their own licences" + (about.fromSource ? "." : ", installed with harlo.")
            }
            RowLayout {
                Layout.topMargin: 10
                spacing: 8
                FlatButton { text: "MIT licence"; font.pixelSize: Theme.px(12); onClicked: Qt.openUrlExternally(about.repo + "/blob/main/LICENSE") }
            }
        }
    }

    Component {
        id: voicePage
        ColumnLayout {
            id: voice
            spacing: 0
            SectionHeader { title: "Dictation"; subtitle: "The microphone button in the composer (" + (Keymap.seq("dictate") || "no shortcut") + "): speak, then click it again or press Enter, and the text goes into your message. Speech is recognised on this computer (whisper.cpp, built in); nothing is sent anywhere. The first time, harlo downloads the model in use." }
            // Re-checked when a model arrives.
            readonly property var models: Dictation.modelsRevision >= 0 ? JSON.parse(Dictation.modelsJson()) : []
            property string model: Workspace.setting("dictation_model") || "base"
            property string language: Workspace.setting("dictation_language") || "auto"
            Group {
                Repeater {
                    model: voice.models
                    SettingRow {
                        id: modelRow
                        required property var modelData
                        required property int index
                        readonly property bool busy: Dictation.downloading === modelData.name
                        divider: index > 0
                        title: modelData.label.split(" · ")[0] + " model" + (voice.model === modelData.name ? " (in use)" : "")
                        detail: busy ? "Downloading… " + Math.round(Dictation.downloadProgress * 100) + "%" : modelData.label.split(" · ").slice(1).join(" · ")
                        icon: "archive-stack"
                        Row {
                            spacing: 6
                            FlatButton {
                                objectName: "modelDownload-" + modelRow.modelData.name
                                visible: !modelRow.modelData.ready
                                enabled: Dictation.downloading === "" || modelRow.busy
                                text: modelRow.busy ? "Cancel" : "Download"
                                font.pixelSize: Theme.px(12)
                                onClicked: modelRow.busy ? Dictation.cancelDownload() : Dictation.download(modelRow.modelData.name)
                            }
                            Pill {
                                objectName: "modelUse-" + modelRow.modelData.name
                                visible: modelRow.modelData.ready
                                text: voice.model === modelRow.modelData.name ? "In use" : "Use"
                                selected: voice.model === modelRow.modelData.name
                                onClicked: { voice.model = modelRow.modelData.name; Workspace.setSetting("dictation_model", voice.model) }
                            }
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Language"
                    detail: "What you speak. Auto-detect works for most languages."
                    icon: "chat-thread"
                    ComboField {
                        objectName: "dictationLanguage"
                        implicitWidth: 200
                        readonly property var codes: ["auto", "en", "tr", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "uk", "ja", "zh", "ko"]
                        model: ["Auto-detect", "English", "Turkish", "German", "French", "Spanish", "Italian", "Portuguese", "Dutch", "Polish", "Russian", "Ukrainian", "Japanese", "Chinese", "Korean"]
                        currentIndex: Math.max(0, codes.indexOf(voice.language))
                        onActivated: i => { voice.language = codes[i]; Workspace.setSetting("dictation_language", codes[i]) }
                    }
                }
            }
            Hint {
                visible: Dictation.downloadError !== ""
                Layout.topMargin: 8
                color: Theme.error
                text: Dictation.downloadError
            }
            Hint { Layout.topMargin: 8; text: "Models come from huggingface.co/ggerganov/whisper.cpp and are kept in " + Dictation.modelsDir() + "." }
        }
    }

    Component {
        id: shortcutsPage
        ColumnLayout {
            spacing: 0
            SectionHeader { title: "Keyboard shortcuts"; subtitle: "Click a key to change it: press the new keys (Esc cancels, Backspace removes it)." + (Keymap.seq("shortcuts") !== "" ? " Also shown with " + Keymap.seq("shortcuts") + "." : "") }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 8
                visible: Object.keys(Keymap.overrides).length > 0
                Item { Layout.fillWidth: true }
                FlatButton { text: "Reset all"; font.pixelSize: Theme.px(12); onClicked: Keymap.resetAll() }
            }
            ShortcutList {
                Layout.fillWidth: true
                Layout.topMargin: 16
                shortcuts: root.shortcuts
                editable: true
            }
        }
    }

    Component {
        id: schedulesPage
        ColumnLayout {
            id: schedules
            spacing: 0
            readonly property var list: { Workspace.schedulesRevision; return JSON.parse(Workspace.schedulesJson()) }
            readonly property var projects: Workspace.projectList().map(l => { const p = l.split("\t"); return { id: Number(p[0]), name: p[1] } })
            // "in 3 h", "in 12 min", "tomorrow 09:00"…
            function when(ts) {
                if (!ts) return ""
                const d = new Date(ts * 1000), now = new Date(), mins = Math.round((d - now) / 60000)
                const clock = d.toLocaleTimeString(Qt.locale(), "HH:mm")
                const day = d.toDateString() === now.toDateString() ? "today" : d.toDateString() === new Date(now.getTime() + 86400000).toDateString() ? "tomorrow" : d.toLocaleDateString(Qt.locale(), "ddd d MMM")
                const rel = mins < 1 ? "now" : mins < 60 ? "in " + mins + " min" : mins < 48 * 60 ? "in " + Math.round(mins / 60) + " h" : ""
                return day + " " + clock + (rel !== "" ? " (" + rel + ")" : "")
            }
            function ago(ts) {
                if (!ts) return "never"
                const d = new Date(ts * 1000), now = new Date()
                const day = d.toDateString() === now.toDateString() ? "today" : d.toLocaleDateString(Qt.locale(), "ddd d MMM")
                return day + " " + d.toLocaleTimeString(Qt.locale(), "HH:mm")
            }

            SectionHeader { title: "Scheduled prompts"; subtitle: "Send a prompt on a schedule, e.g. every weekday morning. Each run starts a new session in the project; you get the usual notification when it finishes. Schedules run while harlo is running (Settings → General → Start harlo when you log in)." }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 12
                Item { Layout.fillWidth: true }
                DialogButton { text: "New schedule"; accent: true; onClicked: editor.openFor(null) }
            }
            Hint { Layout.topMargin: 10; visible: schedules.list.length === 0; text: "No schedules yet." }
            Repeater {
                model: schedules.list
                Rectangle {
                    id: card
                    required property var modelData
                    readonly property var sch: modelData
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    implicitHeight: cardCol.implicitHeight + 24
                    radius: 10
                    color: Theme.elevated
                    border.color: Theme.border
                    opacity: sch.enabled ? 1 : 0.7
                    ColumnLayout {
                        id: cardCol
                        x: 14
                        y: 12
                        width: parent.width - 28
                        spacing: 4
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Icon { name: "history"; color: card.sch.enabled ? Theme.primary : Theme.muted; implicitWidth: 16; implicitHeight: 16 }
                            Label { Layout.fillWidth: true; text: card.sch.title; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(14); elide: Text.ElideRight }
                            AppSwitch {
                                objectName: "scheduleSwitch"
                                checked: card.sch.enabled
                                onToggled: Workspace.setScheduleEnabled(card.sch.id, checked)
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: [card.sch.describe, card.sch.projectName, card.sch.harness || "default harness", card.sch.model].filter(x => x).join(" · ")
                            color: Theme.muted
                            font.pixelSize: Theme.px(12)
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            text: card.sch.prompt
                            color: Theme.foreground
                            font.pixelSize: Theme.px(12)
                            wrapMode: Text.Wrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 4
                            spacing: 6
                            Label {
                                Layout.fillWidth: true
                                text: (card.sch.enabled ? (card.sch.nextRun ? "Next: " + schedules.when(card.sch.nextRun) : "No more runs") : "Off") + "  ·  Last: " + schedules.ago(card.sch.lastRun)
                                color: Theme.muted
                                font.pixelSize: Theme.px(11)
                                elide: Text.ElideRight
                            }
                            FlatButton { text: "Run now"; font.pixelSize: Theme.px(12); onClicked: Workspace.runScheduleNow(card.sch.id) }
                            FlatButton { text: "Edit"; font.pixelSize: Theme.px(12); onClicked: editor.openFor(card.sch) }
                            FlatButton { text: "Delete"; font.pixelSize: Theme.px(12); accent: Theme.error; onClicked: { confirmDelete.sch = card.sch; confirmDelete.open() } }
                        }
                    }
                }
            }

            AppDialog {
                id: confirmDelete
                property var sch: ({})
                title: "Delete “" + (sch.title || "") + "”?"
                standardButtons: Dialog.Discard | Dialog.Cancel
                Label { width: 360; wrapMode: Text.Wrap; color: Theme.muted; text: "It stops running. Sessions it started stay." }
                onDiscarded: { Workspace.deleteSchedule(sch.id); close() }
            }

            // ---- the editor ------------------------------------------------
            AppDialog {
                id: editor
                property var sch: null
                property string kind: "days"
                property int days: 0x1f
                property string harness: ""
                property string error: ""
                title: sch ? "Edit schedule" : "New schedule"
                width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
                function pad(n) { return (n < 10 ? "0" : "") + n }
                function openFor(s) {
                    sch = s
                    error = ""
                    titleField.text = s ? s.title : ""
                    promptField.text = s ? s.prompt : ""
                    projectBox.currentIndex = s ? Math.max(0, schedules.projects.findIndex(p => p.id === s.projectId) + 1) : 0
                    harness = s ? s.harness : ""
                    modelField.text = s ? s.model : ""
                    const r = s ? s.repeat : { kind: "days", days: 0x1f, hour: 9, minute: 0 }
                    kind = r.kind
                    days = r.kind === "days" ? r.days : 0x1f
                    timeField.text = r.kind === "days" ? pad(r.hour) + ":" + pad(r.minute) : "09:00"
                    everyField.text = r.kind === "every" ? String(r.minutes % 60 === 0 ? r.minutes / 60 : r.minutes) : "1"
                    everyHours.checked = r.kind === "every" ? r.minutes % 60 === 0 : true
                    const t = new Date(Date.now() + 86400000)
                    onceField.text = r.kind === "once" ? new Date(r.at * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm")
                                                       : t.toLocaleDateString(Qt.locale(), "yyyy-MM-dd") + " 09:00"
                    catchUp.checked = s ? s.catchUp : true
                    open()
                }
                function save() {
                    const p = projectBox.currentIndex > 0 ? schedules.projects[projectBox.currentIndex - 1].id : -1
                    const repeat = kind === "days" ? { kind: "days", days: days, time: timeField.text }
                                 : kind === "every" ? { kind: "every", minutes: Number(everyField.text) * (everyHours.checked ? 60 : 1) }
                                 : { kind: "once", at: onceField.text }
                    error = Workspace.saveSchedule(JSON.stringify({
                        id: sch ? sch.id : 0, title: titleField.text, prompt: promptField.text, projectId: p,
                        harness: harness, model: modelField.text, repeat: repeat, enabled: sch ? sch.enabled : true, catchUp: catchUp.checked
                    }))
                    if (error === "") close()
                }
                footer: Item {
                    implicitHeight: editorButtons.implicitHeight + 26
                    implicitWidth: editorButtons.implicitWidth + 40
                    Row {
                        id: editorButtons
                        anchors.right: parent.right
                        anchors.rightMargin: 20
                        anchors.top: parent.top
                        anchors.topMargin: 8
                        spacing: 8
                        DialogButton { text: "Cancel"; onClicked: editor.close() }
                        DialogButton { text: "Save"; accent: true; onClicked: editor.save() }
                    }
                }
                ColumnLayout {
                    width: parent.width
                    spacing: 8
                    AppTextField { id: titleField; objectName: "scheduleTitle"; Layout.fillWidth: true; placeholderText: "Name (optional: the prompt's first line)" }
                    ScrollView {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 90
                        TextArea {
                            id: promptField
                            objectName: "schedulePrompt"
                            wrapMode: TextEdit.Wrap
                            color: Theme.foreground
                            placeholderText: "The prompt to send, e.g. \"Summarize yesterday's commits and anything that looks unfinished.\""
                            placeholderTextColor: Theme.muted
                            selectionColor: Theme.selection
                            font.pixelSize: Theme.px(13)
                            background: Rectangle { radius: 6; color: Theme.background; border.color: promptField.activeFocus ? Theme.primary : Theme.border }
                            onTextChanged: editor.error = ""
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Label { text: "In"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                        ComboField { id: projectBox; Layout.fillWidth: true; model: ["Home folder"].concat(schedules.projects.map(p => p.name)) }
                        Repeater {
                            model: [["", "Default"], ["omp", "omp"], ["pi", "pi"]]
                            Pill { required property var modelData; text: modelData[1]; selected: editor.harness === modelData[0]; onClicked: editor.harness = modelData[0] }
                        }
                    }
                    AppTextField { id: modelField; Layout.fillWidth: true; placeholderText: "Model (optional, e.g. openai-codex/gpt-5.6-luna)"; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
                    RowLayout {
                        spacing: 6
                        Label { text: "When"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.rightMargin: 4 }
                        Repeater {
                            model: [["days", "On days"], ["every", "Every"], ["once", "Once"]]
                            Pill { required property var modelData; text: modelData[1]; selected: editor.kind === modelData[0]; onClicked: editor.kind = modelData[0] }
                        }
                    }
                    RowLayout {
                        visible: editor.kind === "days"
                        spacing: 4
                        Repeater {
                            model: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                            Pill {
                                required property string modelData
                                required property int index
                                text: modelData
                                selected: (editor.days & (1 << index)) !== 0
                                onClicked: editor.days = editor.days ^ (1 << index)
                            }
                        }
                        Label { text: "at"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.leftMargin: 6 }
                        AppTextField { id: timeField; objectName: "scheduleTime"; Layout.preferredWidth: 70; placeholderText: "09:00" }
                    }
                    RowLayout {
                        visible: editor.kind === "every"
                        spacing: 6
                        AppTextField { id: everyField; objectName: "scheduleEvery"; Layout.preferredWidth: 60; validator: IntValidator { bottom: 1; top: 9999 } }
                        Pill { id: everyHours; checkable: true; text: "hours"; selected: checked; onClicked: checked = true }
                        Pill { text: "minutes"; selected: !everyHours.checked; onClicked: everyHours.checked = false }
                    }
                    RowLayout {
                        visible: editor.kind === "once"
                        spacing: 6
                        AppTextField { id: onceField; objectName: "scheduleOnce"; Layout.preferredWidth: 160; placeholderText: "2026-10-13 09:00" }
                        Label { text: "(date and time)"; color: Theme.muted; font.pixelSize: Theme.px(11) }
                    }
                    AppCheckBox { id: catchUp; text: "If harlo was closed at the time, run it when harlo starts" }
                    Label {
                        Layout.fillWidth: true
                        visible: editor.error !== ""
                        text: editor.error
                        color: Theme.error
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

    Component {
        id: worktreesPage
        ColumnLayout {
            spacing: 0
            SectionHeader { title: "Location"; subtitle: "Where new worktree sessions get their checkout: <location>/<project>/<branch>. Existing worktrees stay where they are." }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 12
                spacing: 8
                AppTextField {
                    id: worktreeField
                    Layout.fillWidth: true
                    text: Workspace.setting("worktree_dir")
                    placeholderText: Workspace.defaultWorktreeDir()
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    onEditingFinished: { Workspace.setSetting("worktree_dir", text); text = Workspace.setting("worktree_dir") }
                    Connections { target: worktreeDialog; function onAccepted() { worktreeField.text = Workspace.setting("worktree_dir") } }
                }
                FlatButton { text: "Browse…"; onClicked: worktreeDialog.open() }
                FlatButton { text: "Reset"; visible: worktreeField.text !== ""; onClicked: { Workspace.setSetting("worktree_dir", ""); worktreeField.text = "" } }
            }
            Hint { Layout.topMargin: 6; text: "Empty uses harlo's data folder." }
        }
    }

    Component {
        id: gitPage
        ColumnLayout {
            id: gitSettings
            spacing: 0
            property var ignored: Workspace.ignoredRepoList()
            property string host: Workspace.setting("git_host") || "github"
            Component.onCompleted: Setup.refreshGit()
            SectionHeader { title: "Git hosting"; subtitle: "Where your repositories live. Signing in sets up git for that site (HTTPS pushes and pulls); SSH remotes keep using your SSH keys. The sign-in stays with the GitHub CLI, glab, or git's credential store; harlo keeps no token." }
            Repeater {
                model: ["github", "gitlab", "forgejo"]
                GitHostCard {
                    required property string modelData
                    service: modelData
                    chosen: gitSettings.host === modelData
                    Layout.fillWidth: true
                    Layout.topMargin: 12
                    onPick: { Workspace.setSetting("git_host", modelData); gitSettings.host = modelData }
                }
            }
            Rule { }
            SectionHeader { title: "Reviewing an agent's work" }
            Group {
                SettingRow {
                    title: "Undo a turn"
                    detail: "Before each message, harlo keeps a snapshot of the project's files (in git, off your branches), so a turn can be undone: click ↶ Undo under the agent's reply (earlier turns: ↶ Undo from here, where your next message starts; on the phone, long-press the reply). Off: no snapshots, and no Undo. Snapshots older than a month are removed; turn it off for a repository with very large untracked files."
                    icon: "history"
                    AppSwitch {
                        objectName: "undoSnapshotsSwitch"
                        checked: Workspace.setting("undo_snapshots") !== "0"
                        onToggled: Workspace.setSetting("undo_snapshots", checked ? "1" : "0")
                    }
                }
                SettingRow {
                    divider: true
                    title: "Comments on diff lines"
                    detail: "In a file's changes, click a line number to comment on that line; send your comments to the agent as one message. Off: no comment marks or bar."
                    icon: "chat-thread"
                    AppSwitch {
                        objectName: "reviewCommentsSwitch"
                        checked: Workspace.setting("review_comments") !== "0"
                        onToggled: Workspace.setSetting("review_comments", checked ? "1" : "0")
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Commit messages"; subtitle: "The ✦ button in the Changes panel has the model write the message for the staged changes (or all changes). It uses the session's harness in one-shot mode: no tools, nothing saved." }
            Group {
                SettingRow {
                    title: "Model"
                    detail: "Empty uses the harness's default model. A small, fast model is plenty, e.g. a \"mini\" or \"flash\" one."
                    icon: "sparkling"
                    AppTextField {
                        width: 240
                        text: Workspace.setting("commit_model")
                        placeholderText: "Harness default"
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(12)
                        onEditingFinished: Workspace.setSetting("commit_model", text)
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Change counts"; subtitle: "Session rows show how many files are uncommitted in their repository. These repositories are left out." }
            Group {
                Hint {
                    visible: gitSettings.ignored.length === 0
                    Layout.margins: 16
                    text: "None. Right-click a session and choose “Hide change count for …” to add its repository."
                }
                Repeater {
                    model: gitSettings.ignored
                    SettingRow {
                        required property string modelData
                        required property int index
                        divider: index > 0
                        icon: "git-repository"
                        title: modelData.split("/").pop()
                        detail: modelData
                        FlatButton { text: "Show changes"; onClicked: { Workspace.setRepoIgnored(modelData, false); gitSettings.ignored = Workspace.ignoredRepoList() } }
                    }
                }
            }
        }
    }

    Component {
        id: devicesPage
        ColumnLayout {
            id: devs
            spacing: 0
            readonly property var list: Phone.revision >= 0 ? JSON.parse(Phone.devicesJson()) : []
            SectionHeader { title: "Phone access"; subtitle: "Your sessions on your phone: open them, follow along, send messages, answer questions. Pair a phone by scanning a QR code; everything between it and harlo is end-to-end encrypted." }
            Group {
                SettingRow {
                    title: "Allow phones to connect"
                    detail: Phone.error !== "" ? Phone.error : Phone.running ? "Listening on port " + Phone.port + " (this computer's network and Tailscale)." : "Off: paired phones can't connect."
                    icon: "smartphone"
                    AppSwitch { objectName: "phoneAccess"; checked: Phone.enabled; onToggled: Phone.setAccess(checked) }
                }
                SettingRow {
                    divider: true
                    title: "Port"
                    detail: "If your phone can't connect, a firewall may block it: allow TCP " + Phone.port + ". If something else uses the port, pick another (then pair again)."
                    icon: "settings-3"
                    AppTextField {
                        implicitWidth: 90
                        text: String(Phone.port)
                        validator: IntValidator { bottom: 1024; top: 65535 }
                        onEditingFinished: Phone.setAccessPort(Number(text))
                    }
                }
            }
            RowLayout {
                Layout.topMargin: 10
                FlatButton { objectName: "pairDeviceSettings"; text: "Pair a device…"; accent: Theme.primary; font.pixelSize: Theme.px(12); onClicked: root.pairDeviceRequested() }
            }
            Rule { }
            SectionHeader { title: "From anywhere"; subtitle: "For phones away from home: mobile data, other Wi-Fi. Not needed if the phone has Tailscale (or another VPN) to this computer. Notifications arrive either way; these let the phone open sessions and send your answers back. Everything stays end-to-end encrypted." }
            Group {
                SettingRow {
                    title: "Connect to harlo's relay"
                    detail: !Phone.enabled ? "Turn on phone access first."
                          : Phone.relayError !== "" ? Phone.relayError
                          : Phone.relayState === "online" ? "Connected to " + Phone.relayAddress + ". Paired phones can reach harlo from anywhere."
                          : Phone.relayState === "connecting" ? "Connecting…"
                          : "Use your phone away from home without Tailscale or a VPN: harlo and your phones meet at " + Phone.relayAddress + ", which passes on messages it can't read and stores nothing."
                    detailColor: Phone.relayError !== "" ? Theme.error : Phone.enabled && Phone.relayState === "online" ? Theme.success : Theme.muted
                    icon: "global"
                    AppSwitch {
                        objectName: "relaySwitch"
                        checked: Phone.relayOn
                        onToggled: {
                            Phone.useRelay(checked)
                            checked = Qt.binding(() => Phone.relayOn)
                        }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Tailscale Funnel"
                    detail: !Phone.funnelAvailable && !Phone.funnelOn ? "Needs Tailscale on this computer (not on the phone). Nothing hosted: Tailscale gives harlo a public HTTPS address."
                          : !Phone.enabled ? "Turn on phone access first."
                          : Phone.funnelError !== "" ? Phone.funnelError
                          : Phone.funnelState === "online" ? "On at " + Phone.funnelUrl
                          : Phone.funnelState === "approval" ? "Tailscale needs you to allow Funnel for your tailnet (once)."
                          : Phone.funnelState === "starting" ? "Starting…"
                          : "A public HTTPS address from Tailscale, leading to harlo. Nothing hosted, no Tailscale app on the phone."
                    detailColor: Phone.funnelError !== "" ? Theme.error : Phone.enabled && Phone.funnelState === "online" ? Theme.success : Theme.muted
                    icon: "shield-check"
                    RowLayout {
                        spacing: 8
                        FlatButton {
                            visible: Phone.funnelOn && Phone.funnelState === "approval"
                            text: "Allow…"
                            font.pixelSize: Theme.px(12)
                            onClicked: Qt.openUrlExternally(Phone.funnelUrl)
                        }
                        AppSwitch {
                            objectName: "funnelSwitch"
                            enabled: Phone.funnelAvailable || Phone.funnelOn
                            checked: Phone.funnelOn
                            onToggled: { Phone.setFunnel(checked); checked = Qt.binding(() => Phone.funnelOn) }
                        }
                    }
                }
            }
            Rule { }
            // The iOS app's push notifications: through harlo's relay (it has
            // the app's APNs key), whether or not harlo is connected to it.
            SectionHeader { title: "iPhone notifications"; subtitle: "While the harlo app is closed: questions and approvals (Allow or Deny from the lock screen) and finished runs. They always go through harlo's relay to Apple, with or without the switch above; what they say is encrypted for your phone, so the relay and Apple only carry a placeholder." }
            Group {
                id: pushGroup
                readonly property var cfg: Phone.revision >= 0 ? JSON.parse(Phone.pushJson()) : ({})
                // (Also the left rail's phone button, once a phone is paired.)
                SettingRow {
                    title: "Send notifications"
                    detail: Phone.paused ? "Paused: nothing goes to your phones (the pairing stays). A test notification still does."
                                         : "Pause them while you work at the computer: click the phone button in the left rail, or here."
                    icon: "smartphone"
                    AppSwitch {
                        objectName: "pushPausedSwitch"
                        checked: !Phone.paused
                        onToggled: { Phone.setNotificationsPaused(!checked); checked = Qt.binding(() => !Phone.paused) }
                    }
                }
                SettingRow {
                    divider: true
                    title: "Test"
                    detail: Phone.pushNote === "sending" ? "Sending…"
                          : Phone.pushNote !== "" ? Phone.pushNote
                          : pushGroup.cfg.error ? "The last one didn't go: " + pushGroup.cfg.error
                          : "Sends one through " + Phone.relayAddress + " to every phone that allowed notifications in the harlo app."
                    detailColor: Phone.pushNote.startsWith("Sent") ? Theme.success
                               : (Phone.pushNote !== "" && Phone.pushNote !== "sending") || (Phone.pushNote === "" && pushGroup.cfg.error) ? Theme.error : Theme.muted
                    icon: "notification-3"
                    FlatButton {
                        objectName: "pushTest"
                        text: "Send a test notification"
                        enabled: Phone.enabled && Phone.pushNote !== "sending"
                        font.pixelSize: Theme.px(12)
                        onClicked: Phone.sendTestPush()
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Paired devices"; subtitle: "Removing one disconnects it and it can't come back without pairing again." }
            Hint { visible: devs.list.length === 0; Layout.topMargin: 8; text: "No phones paired yet." }
            Group {
                visible: devs.list.length > 0
                Repeater {
                    model: devs.list
                    SettingRow {
                        id: devRow
                        required property var modelData
                        required property int index
                        divider: index > 0
                        title: modelData.name
                        detail: "Paired " + new Date(modelData.pairedAt * 1000).toLocaleDateString() + (modelData.lastSeen ? " · last seen " + Theme.ago(modelData.lastSeen, Date.now() / 1000) : "") + (modelData.push ? " · notifications on" : "")
                        icon: "smartphone"
                        FlatButton { objectName: "removeDevice-" + devRow.modelData.name; text: "Remove"; accent: Theme.error; font.pixelSize: Theme.px(12); onClicked: Phone.removeDevice(devRow.modelData.id) }
                    }
                }
            }
        }
    }

    Component {
        id: projectsPage
        ColumnLayout {
            id: proj
            spacing: 0
            readonly property var list: Workspace.projectsRevision >= 0 ? JSON.parse(Workspace.projectsJson()) : []
            property int chosen: root.projectId
            readonly property var p: list.find(x => x.id === chosen) || (list.length > 0 ? list[0] : null)
            Connections { target: root; function onProjectIdChanged() { proj.chosen = root.projectId } }

            Hint { visible: proj.list.length === 0; text: "No projects yet. Add one with + Project in the sidebar." }
            // Which project.
            Flow {
                visible: proj.list.length > 1
                Layout.fillWidth: true
                Layout.bottomMargin: 18
                spacing: 6
                Repeater {
                    model: proj.list
                    Button {
                        id: pick
                        required property var modelData
                        readonly property bool on: proj.p !== null && modelData.id === proj.p.id
                        objectName: "projectPick-" + modelData.name
                        flat: true
                        implicitHeight: 30
                        leftPadding: 10
                        rightPadding: 12
                        onClicked: proj.chosen = modelData.id
                        contentItem: Row {
                            spacing: 7
                            ProjectIcon { anchors.verticalCenter: parent.verticalCenter; icon: pick.modelData.icon; color: pick.modelData.color; implicitWidth: 15; implicitHeight: 15 }
                            Label { anchors.verticalCenter: parent.verticalCenter; text: pick.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                        }
                        background: ControlSurface { control: pick; variant: pick.on ? "selected" : "outline"; radius: 8 }
                    }
                }
            }

            ColumnLayout {
                visible: proj.p !== null
                Layout.fillWidth: true
                spacing: 0
                // The project, as the sidebar shows it.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    Rectangle {
                        implicitWidth: 44
                        implicitHeight: 44
                        radius: 10
                        color: Theme.controlFill
                        border.color: Theme.controlBorder
                        ProjectIcon { anchors.centerIn: parent; icon: proj.p ? proj.p.icon : ""; color: proj.p ? proj.p.color : ""; fallback: Theme.foreground; implicitWidth: 24; implicitHeight: 24 }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label { text: proj.p ? proj.p.name : ""; color: Theme.foreground; font.pixelSize: Theme.px(16); font.bold: true; elide: Text.ElideRight; Layout.fillWidth: true }
                        Label { text: proj.p ? proj.p.path : ""; color: Theme.muted; font.family: Theme.mono; font.pixelSize: Theme.px(11.5); elide: Text.ElideMiddle; Layout.fillWidth: true }
                    }
                }
                Rule { }

                SectionHeader { title: "Name"; subtitle: "How the sidebar and tabs show it. Empty: the folder's name." }
                AppTextField {
                    objectName: "projectName"
                    Layout.topMargin: 8
                    Layout.preferredWidth: Math.min(380, proj.width)
                    text: proj.p ? proj.p.name : ""
                    placeholderText: proj.p ? proj.p.folder : ""
                    onEditingFinished: if (proj.p && text.trim() !== proj.p.name) Workspace.setProjectName(proj.p.id, text)
                }
                Rule { }

                SectionHeader { title: "Colour"; subtitle: "Its icon in this colour. They come from the theme, so they suit whichever you use." }
                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 8
                    Repeater {
                        model: [{ key: "", label: "None" }].concat(Theme.projectColors)
                        Button {
                            id: swatch
                            required property var modelData
                            readonly property bool on: proj.p !== null && proj.p.color === modelData.key
                            objectName: "projectColor-" + (modelData.key || "none")
                            flat: true
                            implicitWidth: 30
                            implicitHeight: 30
                            padding: 0
                            onClicked: Workspace.setProjectLook(proj.p.id, proj.p.icon, modelData.key)
                            contentItem: Item {
                                Rectangle {
                                    anchors.centerIn: parent
                                    width: 22
                                    height: 22
                                    radius: 6
                                    color: swatch.modelData.key ? swatch.modelData.color : "transparent"
                                    border.color: swatch.modelData.key ? Qt.darker(swatch.modelData.color, 1.15) : Theme.controlBorder
                                    Icon { anchors.centerIn: parent; visible: !swatch.modelData.key; name: "close"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                                }
                            }
                            background: Rectangle {
                                radius: 8
                                color: "transparent"
                                border.width: 2
                                border.color: swatch.on ? Theme.primary : swatch.hovered ? Theme.controlBorderHover : "transparent"
                            }
                            // Theme colours have no fixed names (a theme's "keyword" may be green).
                            ToolTip.visible: hovered && !modelData.key
                            ToolTip.delay: 400
                            ToolTip.text: "No colour"
                        }
                    }
                }
                Rule { }

                SectionHeader { title: "Icon"; subtitle: "Shown in the sidebar, in the project's colour." }
                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 6
                    Repeater {
                        model: [""].concat(root.projectIconPresets)
                        Button {
                            id: iconPick
                            required property string modelData
                            readonly property bool on: proj.p !== null && (modelData === "" ? proj.p.icon === "" : proj.p.icon === modelData)
                            objectName: "projectIconPick-" + (modelData || "none")
                            flat: true
                            implicitWidth: 32
                            implicitHeight: 32
                            padding: 0
                            onClicked: Workspace.setProjectLook(proj.p.id, modelData, proj.p.color)
                            contentItem: Item {
                                ProjectIcon {
                                    anchors.centerIn: parent
                                    icon: iconPick.modelData
                                    color: proj.p ? proj.p.color : ""
                                    fallback: iconPick.on ? Theme.foreground : Theme.muted
                                    implicitWidth: 17
                                    implicitHeight: 17
                                }
                            }
                            background: ControlSurface { control: iconPick; variant: iconPick.on ? "selected" : "outline"; radius: 8 }
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: modelData === "" ? "A folder (the default)" : modelData.replace(/-\d$/, "").replace("-", " ")
                        }
                    }
                }
                Rule { }

                SectionHeader { title: "New sessions" }
                Group {
                    SettingRow {
                        title: "Provider"
                        detail: "What + on the project starts (right-click it for the others)."
                        icon: "robot-2"
                        Row {
                            spacing: 6
                            Repeater {
                                model: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []
                                Pill {
                                    required property var modelData
                                    text: modelData.label
                                    provider: modelData.id
                                    selected: proj.p !== null && proj.p.harness === modelData.id
                                    onClicked: Workspace.setProjectHarness(proj.p.id, modelData.id)
                                }
                            }
                        }
                    }
                }
                Rule { }

                SectionHeader { title: "Worktree setup"; subtitle: "Commands run in each new worktree of this project, one per line (e.g. npm install)." }
                TextArea {
                    id: setupArea
                    objectName: "projectSetup"
                    Layout.fillWidth: true
                    Layout.topMargin: 8
                    Layout.preferredHeight: Math.max(70, implicitHeight)
                    text: proj.p ? proj.p.setup : ""
                    color: Theme.foreground
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    wrapMode: TextEdit.Wrap
                    placeholderText: "npm install"
                    placeholderTextColor: Theme.muted
                    background: Rectangle { radius: 6; color: Theme.background; border.color: setupArea.activeFocus ? Theme.primary : Theme.border }
                }
                RowLayout {
                    Layout.topMargin: 6
                    visible: proj.p !== null && setupArea.text !== proj.p.setup
                    FlatButton { text: "Save"; variant: "primary"; font.pixelSize: Theme.px(12); onClicked: Workspace.setProjectSetup(proj.p.id, setupArea.text) }
                    FlatButton { text: "Cancel"; outlined: false; font.pixelSize: Theme.px(12); onClicked: setupArea.text = proj.p.setup }
                }
                Rule { }

                RowLayout {
                    spacing: 10
                    FlatButton {
                        objectName: "projectRemove"
                        text: "Remove from harlo"
                        variant: "danger"
                        accent: Theme.error
                        font.pixelSize: Theme.px(12)
                        onClicked: { const id = proj.p.id; proj.chosen = -1; Workspace.removeProject(id) }
                    }
                    Hint { text: "Its folder and saved sessions stay; the sessions move to Other sessions." }
                }
            }
        }
    }

    Component {
        id: mcpPage
        McpPage {
            compact: root.compact
            footerHost: root.pageFooter
            onRunInTerminal: command => root.runInTerminalRequested(command)
        }
    }

    Component {
        id: orchestrationPage
        ColumnLayout {
            id: orch
            spacing: 0
            // (On a host it's looked at over SSH, in the background.)
            readonly property var info: (Workspace.orchestrationRevision, Workspace.instance, JSON.parse(Workspace.orchestrationJson()))
            readonly property string where: info.host === "" ? "" : " on " + info.host
            property string problem: ""
            readonly property string shownProblem: problem !== "" ? problem : info.problem
            readonly property bool allSkills: info.skills.every(s => s.installed)
            Component.onCompleted: Workspace.loadOrchestration()
            Connections { target: Workspace; function onInstanceChanged() { orch.problem = ""; Workspace.loadOrchestration() } }
            function reload() { Workspace.loadOrchestration() }
            function setSkills(names, on) {
                let err = ""
                for (const n of names) err = Workspace.setOrchestrationSkill(n, on) || err
                problem = err
                reload()
            }

            SectionHeader {
                title: "Agents drive harlo"
                subtitle: "Agents in harlo can start sessions with other agents, send them messages, read their replies and stop them. The sessions an agent starts show under its own, and each one's last reply goes back to it when it finishes."
            }
            Group {
                SettingRow {
                    title: "Orchestration"
                    detail: Workspace.orchestrationOn ? "On: agents get harlo's tools, and the composer has the Delegate button (hand off, second opinion, committee)."
                                                      : "Off: agents get no harlo tools, there's no Delegate button, and harlo's skills and bridges are removed (turning it on puts them back)."
                    icon: "ai-agent"
                    AppSwitch {
                        objectName: "orchEnabled"
                        checked: Workspace.orchestrationOn
                        onToggled: { orch.problem = Workspace.setOrchestrationEnabled(checked); orch.reload(); checked = Qt.binding(() => Workspace.orchestrationOn) }
                    }
                }
            }

            SectionHeader {
                visible: Workspace.orchestrationOn
                Layout.topMargin: 22
                title: "Skills" + orch.where
                subtitle: orch.info.loading ? "Looking at " + orch.info.host + "…"
                    : "Skills teach agents when and how to use these tools. harlo installs them into " + orch.info.skillDirs.join(" and ") + ", where Claude, OpenCode, pi and omp find them" + (orch.info.host === "" ? ", and keeps them up to date." : ".") + (orch.info.busy ? " Working…" : "")
            }
            Group {
                visible: Workspace.orchestrationOn
                SettingRow {
                    title: "All skills"
                    icon: "sparkling"
                    AppSwitch {
                        objectName: "orchSkillsAll"
                        checked: orch.allSkills
                        // (What's installed decides: a refused one stays off.)
                        onToggled: { orch.setSkills(orch.info.skills.map(s => s.name), checked); checked = Qt.binding(() => orch.allSkills) }
                    }
                }
                Repeater {
                    model: orch.info.skills
                    SettingRow {
                        id: skillRow
                        required property var modelData
                        divider: true
                        title: modelData.name
                        detail: modelData.summary
                        icon: "book-open"
                        AppSwitch {
                            objectName: "orchSkill-" + skillRow.modelData.name
                            checked: skillRow.modelData.installed
                            onToggled: { orch.setSkills([skillRow.modelData.name], checked); checked = Qt.binding(() => skillRow.modelData.installed) }
                        }
                    }
                }
            }

            SectionHeader {
                visible: Workspace.orchestrationOn
                Layout.topMargin: 22
                title: "Tools in each agent" + orch.where
                subtitle: orch.info.host === "" ? "The tools reach agents as an MCP server called harlo. Outside harlo it offers none."
                    : "The tools reach agents on " + orch.info.host + " through a small bridge there (it needs python3) and harlo's connection to it. Outside harlo they offer none."
            }
            Group {
                visible: Workspace.orchestrationOn
                Repeater {
                    model: orch.info.agents
                    SettingRow {
                        id: agentRow
                        required property var modelData
                        required property int index
                        divider: index > 0
                        title: modelData.name
                        icon: "harness-" + modelData.id
                        detail: modelData.bridge === "built-in" ? "Built in: harlo hands its tools to each session it starts." : "Adds the harlo server to " + modelData.config + "."
                        // (A Row: the hidden one takes no room.)
                        Row {
                            Label { visible: agentRow.modelData.bridge === "built-in"; text: "Always on"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            AppSwitch {
                                visible: agentRow.modelData.bridge !== "built-in"
                                objectName: "orchBridge-" + agentRow.modelData.id
                                checked: agentRow.modelData.bridge === true
                                onToggled: { orch.problem = Workspace.setOrchestrationBridge(agentRow.modelData.id, checked); orch.reload(); checked = Qt.binding(() => agentRow.modelData.bridge === true) }
                            }
                        }
                    }
                }
            }
            Hint { visible: orch.shownProblem !== ""; Layout.topMargin: 10; color: Theme.warning; text: orch.shownProblem; objectName: "orchProblem" }
        }
    }

    Component {
        id: remotePage
        ColumnLayout {
            id: remotes
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
                title: "Remote Instances"
                subtitle: "Work on other machines over SSH. Each one is an instance, with its own projects and sessions: switch to it from the instance menu at the top of the sidebar, then + Project adds its folders; its chats, terminal, git and files work there. harlo signs in with your SSH keys or agent. Each connected machine shows which agents it has (omp, pi, Claude, OpenCode), installs the missing ones for you, and opens a terminal there to sign Claude in."
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 12
                Label { text: remotes.hosts.length === 1 ? "1 host" : remotes.hosts.length + " hosts"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.fillWidth: true }
                FlatButton { objectName: "addRemoteHost"; text: "+ Add SSH connection"; font.pixelSize: Theme.px(12); accent: Theme.primary; visible: remotes.editing === null; onClicked: addHostDialog.open() }
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
                    FieldLabel { text: "Name" }
                    AppTextField { id: nameField; objectName: "editRemoteName"; Layout.fillWidth: true; placeholderText: "nas" }
                    FieldLabel { text: "SSH command" }
                    AppTextField {
                        id: sshField
                        objectName: "editRemoteSsh"
                        Layout.fillWidth: true
                        placeholderText: "user@host, a name from ~/.ssh/config, or ssh -p 2222 me@10.0.0.5"
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(12)
                        onAccepted: remotes.save()
                    }
                    Hint { visible: remotes.formError !== ""; color: Theme.error; text: remotes.formError }
                    RowLayout {
                        Layout.topMargin: 6
                        spacing: 8
                        Item { Layout.fillWidth: true }
                        FlatButton { text: "Cancel"; font.pixelSize: Theme.px(12); onClicked: remotes.editing = null }
                        FlatButton { objectName: "saveEditedHost"; text: "Save and connect"; font.pixelSize: Theme.px(12); accent: Theme.primary; onClicked: remotes.save() }
                    }
                }
            }
            Hint { visible: remotes.hosts.length === 0 && remotes.editing === null; Layout.topMargin: 8; text: "No remote machines yet." }
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
                            visible: !root.compact
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
                        visible: root.compact
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
                            model: ["omp", "pi", "claude", "opencode"]
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
                                        ToolTip.text: "Claude isn't signed in on " + agentChip.host.name + ". Opens Claude there in a terminal; sign in, then quit it (/exit)."
                                        onClicked: root.runOnHostRequested(agentsFlow.folder, "claude")
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
                                    onClicked: root.runInTerminalRequested(Remotes.installOutputCommand(hostBox.modelData.name, installRow.modelData))
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
    }

    Component {
        id: providersPage
        ColumnLayout {
            spacing: 0
            SectionHeader {
                title: Workspace.instance === "" ? "Agents" : "Agents on " + Workspace.instance
                subtitle: Workspace.instance === ""
                    ? "The coding agents harlo runs. Each uses its own login and settings; harlo finds them on PATH and in their usual install folders."
                    : "The coding agents on " + Workspace.instance + ", the instance shown: harlo runs them there, each with its own login and settings on that machine. The switches (which agents harlo offers) are the same for every instance."
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
                        model: ["omp", "pi", "claude", "opencode"]
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
                            detail: install && install.state === "running" ? install.message
                                : !found ? (install && install.state === "failed" ? install.message : "Not installed there")
                                : (version !== "" ? "Version " + version : "Installed") + (h[modelData] ? " · " + h[modelData] : "")
                                  + (modelData === "claude" && h.claudeAuth === "no" ? " · not signed in" : "")
                                  + (update && update.state === "available" ? " · " + update.latest + " available" : "")
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
                                    visible: hostAgent.found && hostAgent.modelData === "claude" && hostAgent.h.claudeAuth === "no"
                                    text: "Sign in"
                                    font.pixelSize: Theme.px(12)
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Opens Claude on " + Workspace.instance + " in a terminal; sign in, then quit it (/exit)."
                                    onClicked: root.runOnHostRequested("ssh://" + Workspace.instance + (hostAgent.h.home || ""), "claude")
                                }
                                AppSwitch {
                                    checked: hostAgent.local.enabled
                                    onToggled: Workspace.setProviderEnabled(hostAgent.modelData, checked)
                                }
                            }
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
                                        color: Theme.muted
                                        font.pixelSize: Theme.px(12.5)
                                        text: !prov.modelData.enabled ? "Disabled"
                                            : prov.modelData.state === "checking" ? "Checking…"
                                            : prov.modelData.state === "missing" ? "Not installed"
                                            : prov.modelData.state === "error" ? "Error"
                                            : "Available · " + prov.modelData.modelCount + (prov.modelData.modelCount === 1 ? " model" : " models")
                                              + (prov.modelData.canChat ? "" : " · sessions coming soon")
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
                                        id: pathField
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
                                    readonly property bool omp: prov.modelData.id === "omp"
                                    readonly property var st: Accounts.revision >= 0 ? JSON.parse(Accounts.status(prov.modelData.id)) : ({ accounts: [] })
                                    // omp: the providers it has models from (it lists no accounts), with
                                    // the account its usage report names.
                                    readonly property var ompFrom: omp ? [...new Set((prov.modelData.modelIds || []).map(m => m.split("/")[0]).filter(p => p !== ""))] : []
                                    readonly property var quota: omp ? (JSON.parse(Workspace.quotaJson || "{}").accounts || []) : []
                                    readonly property var rows: !omp ? (st.accounts || []) : ompFrom.map(p => {
                                        const q = quota.find(a => a.provider === p && a.account)
                                        return { id: p, label: p, detail: q ? q.account : "Signed in", signOut: false }
                                    })
                                    property string signOutError: ""
                                    onVisibleChanged: if (visible && !omp) Accounts.refresh(prov.modelData.id)
                                    Component.onCompleted: if (visible && !omp) Accounts.refresh(prov.modelData.id)
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
                                        visible: (acct.omp ? prov.modelData.state !== "checking" : acct.st.state === "ready" && (acct.st.error || "") === "") && acct.rows.length === 0
                                        Layout.fillWidth: true
                                        wrapMode: Text.Wrap
                                        text: acct.omp ? "No models yet: sign omp in to a provider." : "Not signed in to anything yet."
                                        color: Theme.warning
                                        font.pixelSize: Theme.px(12)
                                    }
                                    Label {
                                        visible: (acct.st.error || "") !== "" || acct.signOutError !== ""
                                        Layout.fillWidth: true
                                        wrapMode: Text.Wrap
                                        text: acct.signOutError !== "" ? acct.signOutError : (acct.st.error || "")
                                        color: Theme.error
                                        font.pixelSize: Theme.px(12)
                                    }
                                    RowLayout {
                                        spacing: 8
                                        FlatButton {
                                            objectName: "signIn-" + prov.modelData.id
                                            text: acct.rows.length > 0 ? "Sign in to another…" : "Sign in…"
                                            accent: Theme.primary
                                            font.pixelSize: Theme.px(12)
                                            onClicked: root.signInRequested(prov.modelData.id, prov.modelData.label)
                                        }
                                        Label {
                                            visible: acct.st.state === "loading" && !acct.omp
                                            text: "Checking…"
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(12)
                                        }
                                    }
                                }
                                Label {
                                    visible: !prov.modelData.canChat
                                    Layout.fillWidth: true
                                    wrapMode: Text.Wrap
                                    text: "harlo will start " + prov.modelData.label + " sessions in a coming update."
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(12)
                                }
                                Flow {
                                    visible: prov.modelData.models.length > 0
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
                                            Label { id: modelName; anchors.centerIn: parent; text: parent.modelData; color: Theme.muted; font.pixelSize: Theme.px(11) }
                                        }
                                    }
                                }
                                Label {
                                    visible: prov.modelData.models.length > 60
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
            SectionHeader { title: "omp" }
            Group {
                SettingRow {
                    title: "Goals continue on their own"
                    detail: "A goal (the compass button) keeps omp working turn after turn. omp only does that in its terminal UI unless told otherwise; harlo tells it through a config overlay for each omp session it starts (omp-overlays/ in harlo's data folder). Off: each goal turn needs a message from you. Applies to omp started afterwards."
                    icon: "compass-3"
                    AppSwitch {
                        checked: Workspace.setting("goal_continue") !== "0"
                        onToggled: Workspace.setSetting("goal_continue", checked ? "1" : "0")
                    }
                }
                SettingRow {
                    divider: true
                    title: "Subagents fall back to the session's model"
                    detail: "omp picks a subagent's model from its roles (smol, task, tiny) and fails when that model isn't available, whatever the session uses. On: each of those roles tries your configured model first, then the session's current model. Your omp config is not changed."
                    icon: "node-tree"
                    AppSwitch {
                        checked: Workspace.setting("subagent_fallback") !== "0"
                        onToggled: Workspace.setSetting("subagent_fallback", checked ? "1" : "0")
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Diagnostics"; subtitle: "For tracking down problems. harlo.log has this run's messages (the last run's is harlo.log.1); crashes leave a crash-….log with a backtrace." }
            Group {
                SettingRow {
                    title: "Log harness traffic"
                    detail: "Writes every message to and from pi/omp, and their error output, to logs/protocol/ (one file per agent process, the last 40 kept). Applies to agents started afterwards; the logs contain your prompts and the agents' answers. Also on with --debug-protocol."
                    icon: "bug"
                    AppSwitch {
                        checked: Workspace.setting("debug_protocol") === "1"
                        onToggled: Workspace.setSetting("debug_protocol", checked ? "1" : "0")
                    }
                }
                SettingRow {
                    divider: true
                    title: "Logs folder"
                    detail: Workspace.logsDir()
                    icon: "folder-open"
                    FlatButton { text: "Open"; onClicked: Qt.openUrlExternally("file://" + Workspace.logsDir()) }
                }
            }
        }
    }
}
