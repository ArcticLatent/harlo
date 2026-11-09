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
    /// Harlo signing in to an MCP server of `scope`.
    signal mcpSignInRequested(string scope, string server)
    /// MCP sign-in: run this in a terminal.
    signal runInTerminalRequested(string command)
    /// A file to open in harlo's editor (an agent's config, a log).
    signal openFileRequested(string path)
    /// A command in a terminal in `folder` (`ssh://host/…`: on that host).
    /// Where a page's floating action bar goes.
    readonly property Item pageFooter: footerHost
    property string page: "general"

    readonly property var pages: [
        { id: "general", title: "General", icon: "settings-3", group: "harlo", keywords: "setup guide welcome first model default harness new session startup reopen restore tabs tray close background drop-down quake yakuake hotkey shortcut graphics renderer gpu vulkan opengl software acceleration" },
        { id: "appearance", title: "Appearance", icon: "palette", group: "harlo", keywords: "glass blur opacity transparency shadow theme colour color dark light mode font monospace code text size prompt navigator rail jump claude subscription extra usage billing notice" },
        { id: "notifications", title: "Notifications", icon: "notification-3", group: "harlo", keywords: "notify desktop alert finished input" },
        { id: "devices", title: "Devices", icon: "smartphone", group: "harlo", keywords: "phone pair device mobile qr tailscale remote access" },
        { id: "voice", title: "Voice", icon: "mic", group: "harlo", keywords: "voice speech dictation microphone mic whisper transcribe speak" },
        { id: "shortcuts", title: "Shortcuts", icon: "command", group: "harlo", keywords: "keyboard keys hotkeys" },
        { id: "usage", title: "Usage", icon: "bar-chart-2", group: "harlo", keywords: "stats statistics tokens cost models tools" },
        { id: "projects", title: "Projects", icon: "folder-3", group: "workspace", keywords: "project icon colour color name harness setup remove specs spec plan steps commit review" },
        { id: "worktrees", title: "Worktrees", icon: "git-branch", group: "workspace", keywords: "worktree folder location branch settled shelf finished merged closed pull request quiet archive" },
        { id: "git", title: "Git", icon: "git-repository", group: "workspace", keywords: "undo snapshot turn revert rollback github gitlab forgejo codeberg gitea hosting sign in login token credentials changes count repository hidden sidebar uncommitted commit message model ai" },
        { id: "schedules", title: "Schedules", icon: "history", group: "workspace", keywords: "schedule scheduled prompt cron timer daily weekday every hour morning automatic run" },
        { id: "mcp", title: "MCP", icon: "node-tree", group: "agents", keywords: "mcp tool servers model context protocol tools stdio http oauth" },
        { id: "orchestration", title: "Orchestration", icon: "ai-agent", group: "agents", keywords: "orchestration handoff advisor committee second opinion delegate subagent agents start sessions mcp tools" },
        { id: "remote", title: "Remote Instances", icon: "server", group: "agents", keywords: "remote ssh server host machine nas instance connect" },
        { id: "about", title: "About Harlo", icon: "information", group: "about", keywords: "about version release licence license mit website source code github changelog what's new report problem issue bug logs" },
        { id: "providers", title: "Providers", icon: "robot-2", group: "agents", keywords: "providers harnesses harlo claude binary path program agent models version enable disable debug log logs protocol crash diagnostics sandbox bubblewrap sign in login log in account accounts api key oauth credentials code intelligence language server lsp rust-analyzer pyright gopls clangd typescript" },
        { id: "servers", title: "Model servers", icon: "server", group: "agents", keywords: "model servers local models ollama lm studio llama.cpp vllm sglang gateway openai compatible custom provider providers.json harlo basalt" },
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
    /// A search, or a link from another row, lands on the row: the page
    /// scrolls to the first row or section whose title (else description)
    /// says `text`, and it flashes.
    property string seek: ""
    signal seekRequested()
    onPageChanged: if (page === "usage") seek = ""
    function seekTo(id, text) {
        seek = text
        if (page === id) seekRequested()
        else page = id
    }
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
    // Narrow: the page list is icons; its search button opens it whole,
    // over the page, with names and the search field.
    property bool navOpen: false
    readonly property bool navWide: !compact || navOpen
    onCompactChanged: navOpen = false
    onClosed: navOpen = false
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
    // A description read at a glance: its first sentence, and "More" for
    // the rest (how, exceptions, where it applies). `more` gives the rest
    // outright; otherwise a long text splits after its first sentence.
    function splitLead(t) {
        t = String(t || "")
        if (t.length <= 110) return [t, ""]
        for (let i = 20; i < t.length - 2; i++) {
            const c = t[i]
            if ((c === "." || c === "?" || c === "!" || c === ")") && t[i + 1] === " " && (/[A-Z(]/.test(t[i + 2]) || t.startsWith("harlo", i + 2))) {
                if (c === ")" && t[i - 1] !== ".") continue
                if (/(e\.g|i\.e|etc|vs)\.$/.test(t.slice(Math.max(0, i - 4), i + 1))) continue
                return [t.slice(0, i + 1), t.slice(i + 2)]
            }
        }
        return [t, ""]
    }
    function escapeHtml(t) { return String(t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\n/g, "<br>") }
    component MoreText: Label {
        id: moreText
        property string full
        property string more: ""
        property bool open: false
        // (A short rest is shown whole: a link for a few words costs more
        // than the words.)
        readonly property var split: more !== "" ? [full, more] : root.splitLead(full)
        readonly property var parts: split[1] !== "" && split[1].length < 70 ? [split[0] + " " + split[1], ""] : split
        // (The words as shown, for tests and copying.)
        readonly property string plain: open && parts[1] !== "" ? parts[0] + " " + parts[1] : parts[0]
        // More reads as a quiet word (Text, Medium), underlined on hover.
        textFormat: Text.RichText
        text: root.escapeHtml(plain) + (parts[1] === "" ? "" : "&nbsp;&nbsp;<a href=\"more\" style=\"color:" + Theme.text + "; text-decoration:" + (hoveredLink !== "" ? "underline" : "none") + "; font-weight:500\">" + (open ? "Less" : "More") + "</a>")
        wrapMode: Text.Wrap
        onLinkActivated: open = !open
        HoverHandler { cursorShape: moreText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
    }
    // A section's heading, its one-line lede (More for the rest), and its
    // actions at the right of the heading ("Add …": every page adds the
    // same way).
    component SectionHeader: ColumnLayout {
        id: sectionHead
        readonly property bool seekable: true
        property string title
        property string subtitle
        property string more: ""
        default property alias actions: actionRow.data
        Layout.fillWidth: true
        spacing: 3
        // (None: a page's first section, under the page's own title.)
        GridLayout {
            visible: sectionHead.title !== "" || actionRow.children.length > 0
            Layout.fillWidth: true
            // (Narrow: the actions go under the heading rather than squeeze it.)
            columns: actionRow.implicitWidth > sectionHead.width * 0.55 ? 1 : 2
            columnSpacing: 8
            rowSpacing: 8
            Label { Layout.fillWidth: true; visible: sectionHead.title !== ""; text: sectionHead.title; color: Theme.text; font.pixelSize: Theme.px(16); font.weight: Font.DemiBold; font.letterSpacing: -0.1; wrapMode: Text.Wrap }
            Item { visible: sectionHead.title === ""; Layout.fillWidth: true }
            Row { id: actionRow; spacing: 6; Layout.alignment: Qt.AlignVCenter }
        }
        MoreText { visible: sectionHead.subtitle !== ""; full: sectionHead.subtitle; more: sectionHead.more; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true }
    }
    // The one way a page adds something: at its section's heading.
    component AddButton: FlatButton {
        font.pixelSize: Theme.px(12.5)
        padding: 4
        horizontalPadding: 10
    }
    // A section with nothing in it yet: said inside its card.
    component EmptyNote: Label {
        Layout.fillWidth: true
        Layout.margins: 16
        color: Theme.textMuted
        font.pixelSize: Theme.px(12.5)
        wrapMode: Text.Wrap
    }
    // A section that waits for phone access says so once, under its heading.
    component NeedsPhone: RowLayout {
        objectName: "needsPhone"
        visible: !Phone.enabled
        Layout.topMargin: 8
        spacing: 6
        Icon { name: "information"; color: Theme.warning; implicitWidth: 14; implicitHeight: 14 }
        Label { Layout.fillWidth: true; text: "Turn on phone access above first."; color: Theme.warning; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap }
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
        readonly property bool seekable: true
        property string title
        property string detail
        property string more: ""
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
            MoreText { visible: settingRow.detail !== ""; full: settingRow.detail; more: settingRow.more; color: settingRow.detailColor === Theme.muted ? Theme.textMuted : settingRow.detailColor; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true }
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

                // Page list (icons only in a narrow window, until opened)
                Item {
                    Layout.fillHeight: true
                    Layout.preferredWidth: root.compact ? 58 : 236
                    // (Opened, it lies over the page.)
                    z: 3
                // A shade under the opened list closes it.
                Rectangle {
                    visible: root.compact && root.navOpen
                    x: parent.width
                    width: card.width - parent.width
                    height: parent.height
                    color: Qt.rgba(0, 0, 0, 0.25)
                    TapHandler { onTapped: root.navOpen = false }
                }
                Rectangle {
                    objectName: "settingsNav"
                    height: parent.height
                    width: root.navWide ? 236 : parent.width
                    Behavior on width { enabled: root.compact; NumberAnimation { duration: Math.round(Theme.motion * 0.6); easing.type: Easing.OutCubic } }
                    clip: true
                    // A shade over the glass.
                    // (Opened over the page, it's solid.)
                    color: Theme.glass && !root.navOpen ? Qt.rgba(Theme.chrome.r, Theme.chrome.g, Theme.chrome.b, 0.6) : Theme.chrome
                    radius: 13
                    topRightRadius: 0
                    bottomRightRadius: 0
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hairline }
                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: root.navWide ? 14 : 8
                        anchors.rightMargin: root.navWide ? 15 : 9
                        spacing: 2
                        // Narrow: opens the list with its search field.
                        ItemDelegate {
                            id: navSearch
                            objectName: "settingsNavSearch"
                            visible: !root.navWide
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                            implicitHeight: 36
                            leftPadding: 12
                            onClicked: { root.navOpen = true; searchField.forceActiveFocus() }
                            contentItem: Icon { name: "search"; color: navSearch.hovered ? Theme.foreground : Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
                            background: Rectangle { radius: 9; color: navSearch.hovered ? Theme.rowHover : "transparent" }
                            ToolTip.visible: hovered
                            ToolTip.text: "Search settings"
                        }
                        AppTextField {
                            id: searchField
                            objectName: "settingsSearch"
                            visible: root.navWide
                            Layout.fillWidth: true
                            Layout.bottomMargin: 10
                            leftPadding: 32
                            placeholderText: "Search settings"
                            onTextChanged: root.query = text
                            Keys.onEscapePressed: event => {
                                if (text !== "") { text = ""; event.accepted = true }
                                else if (root.navOpen) { root.navOpen = false; event.accepted = true }
                                else event.accepted = false
                            }
                            // (Enter: the first page found.)
                            onAccepted: if (root.shownPages.length > 0) { root.seekTo(root.shownPages[0].id, text.trim()); root.navOpen = false }
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
                                    visible: root.navWide && groupBlock.modelData[1] !== ""
                                    text: groupBlock.modelData[1]
                                    color: Theme.textMuted
                                    font.pixelSize: Theme.px(12)
                                    font.weight: Font.DemiBold
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
                                        leftPadding: root.navWide ? 10 : 12
                                        rightPadding: root.navWide ? 8 : 0
                                        Layout.topMargin: !root.navWide && index === 0 ? 8 : 0
                                        ToolTip.visible: !root.navWide && hovered
                                        ToolTip.text: modelData.title
                                        contentItem: RowLayout {
                                            spacing: 10
                                            Icon { name: navItem.modelData.icon; color: navItem.current ? Theme.accent : Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
                                            Label {
                                                visible: root.navWide
                                                Layout.fillWidth: true
                                                text: navItem.modelData.title
                                                color: navItem.current ? Theme.text : Theme.textMuted
                                                font.pixelSize: Theme.px(14)
                                                font.weight: navItem.current ? Font.Medium : Font.Normal
                                                elide: Text.ElideRight
                                            }
                                        }
                                        background: Rectangle { radius: 9; color: navItem.current ? Theme.rowSelected : navItem.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
                                        // (From a search: to the row it matched.)
                                        onClicked: { if (root.query.trim() !== "") root.seekTo(modelData.id, root.query.trim()); else root.page = modelData.id; root.navOpen = false }
                                    }
                                }
                            }
                        }
                        Label {
                            visible: root.navWide && root.shownPages.length === 0
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
    // Every page starts at the same left edge; wide ones (Shortcuts, Usage)
    // only reach further right.
    function pageX(w) { return Math.max(28, Math.floor((w - 720) / 2)) }
    function pageWidth(w, wide) { return Math.min(w - pageX(w) - 58, wide ? 940 : 720) }
    component PageTitle: Label {
        text: root.current.title
        color: Theme.foreground
        font.pixelSize: Theme.px(20)
        font.bold: true
        Layout.bottomMargin: 18
    }

    Component {
        id: usagePage
        StatsView {
            id: usageStats
            color: "transparent"
            columnX: root.pageX(width)
            // (As wide as the other pages, so the edge stays put between them.)
            columnWidth: root.pageWidth(width, false)
            header: ColumnLayout {
                id: usageColumn
                spacing: 0
                PageTitle { }
                // Other machines: those connected, or whose limits were seen.
                readonly property var limitHosts: {
                    const seen = (() => { try { return JSON.parse(Workspace.hostLimitsJson || "{}") } catch (e) { return {} } })()
                    const hosts = Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()) : []
                    return hosts.filter(h => h.state === "connected" || seen[h.name] !== undefined).map(h => h.name)
                }
                PlanLimits {
                    Layout.fillWidth: true
                    Layout.bottomMargin: 8
                    title: usageColumn.limitHosts.length > 0 ? "Plan limits on this PC" : "Plan limits"
                }
                Repeater {
                    model: usageColumn.limitHosts
                    PlanLimits {
                        required property string modelData
                        host: modelData
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 8
                    }
                }
                // Spending limits: per session and per day, in dollars.
                ColumnLayout {
                    id: spendLimits
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    Layout.bottomMargin: 18
                    spacing: 0
                    readonly property var spend: Workspace.spendJson !== "" ? JSON.parse(Workspace.spendJson) : ({ limits: { session: 0, day: 0 }, today: 0 })
                    function amount(t) { const v = parseFloat(String(t).replace(/[$,\s]/g, "")); return isFinite(v) && v > 0 ? v : 0 }
                    function shown(v) { return v > 0 ? (v % 1 === 0 ? v.toFixed(0) : v.toFixed(2)) : "" }
                    function save() { Workspace.setSpendLimits(amount(sessionLimit.text), amount(dayLimit.text)) }
                    SectionHeader {
                        title: "Spending limits"
                        subtitle: "Today so far: $" + (spendLimits.spend.today || 0).toFixed(2) + " in harlo's sessions. Past a limit, harlo stops the run until you allow more."
                        more: "Costs are what Claude Code reports, checked at the end of each turn. Harlo's sessions report none: Settings → Providers → Harlo has a token budget per helper instead. Empty: no limit."
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
                SectionHeader { title: "Activity"; Layout.bottomMargin: 6 }
            }
        }
    }

    Component {
        id: scrolledPage
        Flickable {
            id: pageFlick
            objectName: "settingsPage"
            contentHeight: pageBody.implicitHeight + 64
            clip: true
            // (Another page opens at its top, not where the last one was;
            // or at the row a search or a link asked for.)
            Connections {
                target: root
                function onPageChanged() { pageFlick.contentY = 0; if (root.seek !== "") seekTimer.restart() }
                function onSeekRequested() { seekTimer.restart() }
            }
            // (Once the page is laid out.)
            Timer { id: seekTimer; interval: 80; onTriggered: pageFlick.seekNow() }
            function findRow(item, q, inDetail) {
                for (const c of item.children) {
                    if (!c.visible) continue
                    if (c.seekable === true) {
                        const t = inDetail ? (c.detail || c.subtitle || "") + " " + (c.more || "") : (c.title || "")
                        if (String(t).toLowerCase().includes(q)) return c
                    }
                    const f = findRow(c, q, inDetail)
                    if (f) return f
                }
                return null
            }
            function seekNow() {
                const q = root.seek.toLowerCase()
                root.seek = ""
                if (q === "") return
                const row = findRow(pageBody, q, false) || findRow(pageBody, q, true)
                if (!row) return
                const at = row.mapToItem(pageFlick.contentItem, 0, 0)
                contentY = Math.max(0, Math.min(contentHeight - height, at.y - 48))
                flash.x = at.x - 4
                flash.y = at.y - 4
                flash.width = row.width + 8
                flash.height = row.height + 8
                flashAnim.restart()
            }
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ColumnLayout {
                id: pageBody
                x: root.pageX(pageFlick.width)
                y: 26
                // Room for the close button on the right; shortcuts use two columns.
                width: root.pageWidth(pageFlick.width, root.page === "shortcuts")
                spacing: 0
                PageTitle { }
                Loader {
                    Layout.fillWidth: true
                    sourceComponent: ({
                        general: generalPage, appearance: appearancePage, notifications: notificationsPage, voice: voicePage, devices: devicesPage,
                        shortcuts: shortcutsPage, projects: projectsPage, worktrees: worktreesPage, git: gitPage, schedules: schedulesPage, providers: providersPage, servers: serversPage, remote: remotePage, mcp: mcpPage, orchestration: orchestrationPage,
                        about: aboutPage
                    })[root.page] || null
                }
            }
            // The row a search landed on, lit for a moment.
            Rectangle {
                id: flash
                objectName: "settingsSeekFlash"
                opacity: 0
                radius: 12
                color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.12)
                border.color: Theme.accent
                border.width: 1
                SequentialAnimation {
                    id: flashAnim
                    NumberAnimation { target: flash; property: "opacity"; to: 1; duration: 120 }
                    PauseAnimation { duration: 900 }
                    NumberAnimation { target: flash; property: "opacity"; to: 0; duration: 600; easing.type: Easing.OutCubic }
                }
            }
            // (Opened from Usage, which has no rows to seek.)
            Component.onCompleted: if (root.seek !== "") seekTimer.restart()
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
                        onClicked: root.page = "providers"
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
                        onToggled: root.trayToggled(checked)
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
                          : "Starts in the background, in the system tray."
                    more: autostartRow.error !== "" ? "" : "The window opens if there is no tray. Adds harlo to ~/.config/autostart."
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
                    detail: "Looks for new versions of harlo and your agents every 6 hours, and offers to update them."
                            + (Updates.checkStatus !== "" ? "\n" + Updates.checkStatus : "")
                    more: "Also at startup and when a remote host connects. harlo updates itself when it was installed from a package or an AppImage."
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
                        // (Compact: 31 of them make a long page.)
                        implicitHeight: 94
                        radius: 12
                        color: themeHover.hovered ? Theme.rowHover : Theme.raised
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        border.color: current ? Theme.accent : Theme.hairline
                        border.width: current ? 2 : 1
                        HoverHandler { id: themeHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Workspace.chooseTheme(themeCard.modelData.name) }
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 11
                            spacing: 12
                            Repeater {
                                model: ["light", "dark"]
                                Item {
                                    required property string modelData
                                    readonly property var cols: themeCard.modelData[modelData]
                                    // The variant showing now: ringed.
                                    readonly property bool showing: themeCard.current && Theme.variant === modelData
                                    width: 46
                                    height: 46
                                    visible: cols[0] !== ""
                                    Rectangle {
                                        visible: parent.showing
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
                            x: 14
                            y: parent.height - height - 10
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
                    id: glassRow
                    divider: true
                    title: "Glass"
                    // (Without acceleration it can't be drawn: off and
                    // disabled, the choice kept for when it can.)
                    readonly property bool noGpu: GraphicsInfo.api === GraphicsInfo.Software
                    detail: noGpu ? "Needs graphics acceleration; harlo is drawing in software, so menus and dialogs stay solid."
                        : "Menus, dialogs and the composer show a soft blur of what's behind them."
                    icon: "window"
                    RowLayout {
                        spacing: 8
                        FlatButton {
                            objectName: "glassRenderer"
                            visible: glassRow.noGpu
                            text: "Renderer…"
                            font.pixelSize: Theme.px(12)
                            onClicked: root.seekTo("general", "Renderer")
                        }
                        AppSwitch {
                            objectName: "glassSwitch"
                            enabled: !glassRow.noGpu
                            checked: Workspace.glass && !glassRow.noGpu
                            onToggled: { Workspace.chooseGlass(checked, Workspace.glassOpacity); checked = Qt.binding(() => Workspace.glass && !glassRow.noGpu) }
                        }
                    }
                }
                SettingRow {
                    divider: true
                    opacity: glassRow.noGpu ? 0.6 : 1
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
                            enabled: Workspace.glass && !glassRow.noGpu
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
                        { kind: "ui", title: "Interface font", detail: "Everything outside code and the terminal.", icon: "file-text", fixed: false },
                        { kind: "mono", title: "Code font", detail: "Code blocks, diffs, file previews, the editor and the terminal.", icon: "code-box", fixed: true },
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
                            // (As every row: its icon, its title and a line under it.)
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.columnSpan: root.compact ? 2 : 1
                                spacing: 12
                                Rectangle {
                                    implicitWidth: 32
                                    implicitHeight: 32
                                    radius: 9
                                    color: Theme.controlFill
                                    Icon { anchors.centerIn: parent; name: fontPick.modelData.icon; color: Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Label { text: fontPick.modelData.title; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium }
                                    Label { Layout.fillWidth: true; text: fontPick.modelData.detail; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap }
                                }
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
                { name: "completion", title: "Agent completion", detail: "A run finishes.", icon: "checkbox-circle" },
                { name: "subagent", title: "Helper finished", detail: "A helper finishes.", icon: "node-tree" },
                { name: "error", title: "Agent errors", detail: "A reply fails, the agent stops unexpectedly, or a helper fails.", icon: "alert" },
                { name: "question", title: "Agent questions", detail: "The agent asks you something or wants a tool approved.", icon: "question" },
                { name: "limits", title: "Plan limits", detail: "A plan limit (Claude's, ChatGPT's) is used up, and when one you'd nearly or fully used resets, with a sound of its own.", icon: "speed", fixed: true },
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
                    detail: "harlo's own, the same on any desktop."
                    icon: "volume-up"
                    AppSwitch { objectName: "notifySound"; checked: notify.cfg.sound; onToggled: notify.setTop("sound", checked) }
                }
                // (A row, as Devices' Test.)
                SettingRow {
                    visible: notify.cfg.enabled
                    divider: true
                    title: "Test"
                    detail: notify.testNote !== "" ? notify.testNote : "Shows one now, as the events below will."
                    icon: "notification-3"
                    FlatButton {
                        objectName: "notifyTest"
                        text: "Send a test notification"
                        font.pixelSize: Theme.px(12)
                        onClicked: { Workspace.sendTestNotification(); notify.testNote = "Sent. If nothing appeared, check that notify-send (libnotify) is installed." }
                    }
                }
            }

            Rule {}
            SectionHeader {
                title: "Quiet hours"
                subtitle: "No notifications here, and silent ones on your iPhone."
                more: "At night, or whenever you want peace. The iPhone's make no sound and leave the screen dark; they wait in Notification Center."
            }
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
                        icon: modelData.icon
                        AppSwitch {
                            objectName: "notifyEvent-" + evRow.modelData.name
                            checked: notify.cfg.events[evRow.modelData.name].on
                            onToggled: notify.setEvent(evRow.modelData.name, "on", checked)
                        }
                    }
                }
                // What a limit's reset plays (harlo plays it, whatever the
                // switch for sounds above says).
                SettingRow {
                    id: resetSoundRow
                    readonly property var sounds: notify.info.resetSounds || []
                    readonly property string current: notify.cfg.resetSound !== undefined ? notify.cfg.resetSound : "desktop-login"
                    visible: notify.cfg.events.limits && notify.cfg.events.limits.on
                    divider: true
                    title: "Sound when a limit resets"
                    detail: "Plays even with the sound switch above off."
                    icon: "volume-up"
                    RowLayout {
                        spacing: 8
                        FlatButton {
                            objectName: "resetSoundPlay"
                            text: "Play"
                            enabled: resetSoundRow.current !== ""
                            font.pixelSize: Theme.px(12)
                            onClicked: Workspace.playSound(resetSoundRow.current)
                        }
                        ComboField {
                            objectName: "resetSound"
                            implicitWidth: 150
                            model: resetSoundRow.sounds.map(s => s.label)
                            currentIndex: Math.max(0, resetSoundRow.sounds.findIndex(s => s.id === resetSoundRow.current))
                            onActivated: i => { notify.setTop("resetSound", resetSoundRow.sounds[i].id); if (resetSoundRow.sounds[i].id !== "") Workspace.playSound(resetSoundRow.sounds[i].id) }
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
                    model: notify.eventRows.filter(e => !e.fixed)
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
            SectionHeader {
                title: "Dictation"
                subtitle: "Speak into the composer's microphone (" + (Keymap.seq("dictate") || "no shortcut") + "); the text goes into your message."
                more: "Click it again or press Enter to finish. Speech is recognised on this computer (whisper.cpp, built in); nothing is sent anywhere. The first time, harlo downloads the chosen model."
            }
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
                        readonly property bool chosen: voice.model === modelData.name
                        // (In use only once it's here; chosen before that.)
                        title: modelData.label.split(" · ")[0] + " model" + (chosen && modelData.ready ? " (in use)" : "")
                        detail: busy ? "Downloading… " + Math.round(Dictation.downloadProgress * 100) + "%"
                              : modelData.label.split(" · ").slice(1).join(" · ") + (chosen && !modelData.ready ? ". Chosen, not downloaded yet: it downloads the first time you dictate." : "")
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

            SectionHeader {
                title: "Scheduled prompts"
                subtitle: "Send a prompt on a schedule, e.g. every weekday morning."
                more: "Each run starts a new session in the project; you get the usual notification when it finishes. Schedules run while harlo is running (Settings → General → Start harlo when you log in)."
                AddButton { objectName: "newSchedule"; text: "New schedule"; onClicked: editor.openFor(null) }
            }
            Group {
                visible: schedules.list.length === 0
                Layout.topMargin: 12
                EmptyNote { text: "No schedules yet." }
            }
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
                            model: [["", "Default"], ["harlo", "Harlo"], ["claude", "Claude"]]
                            Pill { required property var modelData; text: modelData[1]; selected: editor.harness === modelData[0]; onClicked: editor.harness = modelData[0] }
                        }
                    }
                    AppTextField { id: modelField; Layout.fillWidth: true; placeholderText: "Model (optional, provider/model)"; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
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
            Rule { }
            SectionHeader { title: "Finished worktree sessions" }
            Group {
                SettingRow {
                    title: "Put them on a Settled shelf"
                    detail: "Finished helpers and merged worktree sessions move under their project's sessions, out of the way."
                    more: "A helper goes on its project's Settled shelf (under its sessions in the sidebar) once it's done, and a worktree session when its branch's pull request is merged or closed (GitHub, GitLab), or after a week without activity. Working in one brings it back; right-click → Put back does too. Pinned sessions stay. Off: every session comes back."
                    icon: "archive-stack"
                    AppSwitch {
                        objectName: "settleSessions"
                        checked: Workspace.setting("settle_sessions") !== "0"
                        onToggled: Workspace.setSettling(checked)
                    }
                }
                SettingRow {
                    title: "Archive them after 30 days"
                    detail: "A session that's been on the Settled shelf for 30 days with no activity is archived (still searchable, and shown again with Show archived). Right-click the shelf to archive it all now."
                    icon: "time"
                    AppSwitch {
                        objectName: "archiveSettled"
                        checked: Workspace.setting("archive_settled") !== "0"
                        onToggled: Workspace.setSetting("archive_settled", checked ? "1" : "0")
                    }
                }
            }
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
                    detail: "Undo under the agent's reply puts the project's files back as they were before that turn."
                    more: "Before each message, harlo keeps a snapshot of the project's files (in git, off your branches). Earlier turns: Undo from here, on the line before your next message when you point at it; on the phone, long-press the reply. Off: no snapshots, and no Undo. Snapshots older than a month are removed; turn it off for a repository with very large untracked files."
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
            SectionHeader { title: "Commit messages"; subtitle: "The ✦ button in the Changes panel has a model write the message for the staged changes (or all changes): Harlo's, or Claude Code's when Harlo can't, with no tools and nothing saved." }
            Group {
                SettingRow {
                    id: commitModelRow
                    property string saved: Workspace.setting("commit_model")
                    readonly property var prov: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(p => p.id === "harlo") || null : null
                    readonly property var ids: prov ? prov.modelIds || [] : []
                    // (One it no longer lists stays a choice.)
                    readonly property var choices: saved === "" || ids.indexOf(saved) >= 0 ? ids : ids.concat([saved])
                    Component.onCompleted: Workspace.ensureProviders()
                    title: "Model"
                    detail: "One of Harlo's: a small, fast one is plenty. Claude Code writes it when Harlo can't, with Haiku." + (ids.length === 0 ? " Harlo lists its models once it's signed in." : "")
                    icon: "sparkling"
                    ComboField {
                        objectName: "commitModel"
                        width: 240
                        model: ["Harlo's default"].concat(commitModelRow.choices.map(id => { const i = commitModelRow.ids.indexOf(id); return i >= 0 ? Theme.modelLabel((commitModelRow.prov.models || [])[i] || id) : id }))
                        currentIndex: commitModelRow.choices.indexOf(commitModelRow.saved) + 1
                        onActivated: i => {
                            commitModelRow.saved = i === 0 ? "" : commitModelRow.choices[i - 1]
                            Workspace.setSetting("commit_model", commitModelRow.saved)
                        }
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
            Rule { }
            SectionHeader { title: "From anywhere"; subtitle: "For phones away from home: mobile data, other Wi-Fi. Not needed if the phone has Tailscale (or another VPN) to this computer. Notifications arrive either way; these let the phone open sessions and send your answers back. Everything stays end-to-end encrypted." }
            NeedsPhone { }
            Group {
                // (Waiting for phone access: dimmed, said once above.)
                opacity: Phone.enabled ? 1 : 0.6
                SettingRow {
                    title: "Connect to harlo's relay"
                    detail: Phone.relayError !== "" ? Phone.relayError
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
            NeedsPhone { }
            Group {
                id: pushGroup
                opacity: Phone.enabled ? 1 : 0.6
                readonly property var cfg: Phone.revision >= 0 ? JSON.parse(Phone.pushJson()) : ({})
                // (Also the left rail's phone button, once a phone is paired.)
                SettingRow {
                    title: "Send notifications"
                    // (Nothing goes with phone access off: said at the heading.)
                    detail: Phone.paused ? "Paused: nothing goes to your phones (the pairing stays). A test notification still does."
                          : "Pause them while you work at the computer: click the phone button in the left rail, or here."
                    icon: "smartphone"
                    AppSwitch {
                        objectName: "pushPausedSwitch"
                        enabled: Phone.enabled
                        checked: Phone.enabled && !Phone.paused
                        onToggled: { Phone.setNotificationsPaused(!checked); checked = Qt.binding(() => Phone.enabled && !Phone.paused) }
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
            SectionHeader {
                title: "Paired devices"
                subtitle: "Removing one disconnects it and it can't come back without pairing again."
                AddButton { objectName: "pairDeviceSettings"; text: "Pair a device…"; onClicked: root.pairDeviceRequested() }
            }
            Group { visible: devs.list.length === 0; EmptyNote { text: "No phones paired yet." } }
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
                    // Its agent's model here (each agent remembers its own).
                    SettingRow {
                        id: projModel
                        objectName: "projectModel"
                        visible: proj.p !== null
                        divider: true
                        title: "Model"
                        detail: "What " + (proj.p ? Theme.harnessLabel(proj.p.harness) : "the agent") + " starts with here."
                            + (proj.p && proj.p.remote ? (projModel.hostModels && projModel.hostModels.error ? " Couldn't read its models: " + projModel.hostModels.error : "") : "")
                        icon: "sparkling"
                        property int revision: 0
                        readonly property string key: proj.p ? "project_model_" + proj.p.id + "_" + proj.p.harness : ""
                        readonly property string current: revision >= 0 && key !== "" ? Workspace.setting(key) : ""
                        // Another machine's project: its agent's models there,
                        // asked when the menu opens (null until it answers).
                        property var hostModels: null
                        Connections {
                            target: Workspace
                            function onHostModelsReady(id, harness, json) {
                                if (proj.p && id === proj.p.id && harness === proj.p.harness) projModel.hostModels = JSON.parse(json)
                            }
                        }
                        readonly property var choices: {
                            if (proj.p && proj.p.remote) {
                                const list = hostModels && hostModels.models ? hostModels.models : []
                                return list.filter(m => m.local).concat(list.filter(m => !m.local))
                            }
                            const p = proj.p ? (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === proj.p.harness) : null
                            if (!p) return []
                            const all = (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 }))
                            return all.filter(m => m.local).concat(all.filter(m => !m.local))
                        }
                        function choose(id) { Workspace.setSetting(key, id); revision++ }
                        FlatButton {
                            id: projModelButton
                            readonly property var m: projModel.choices.find(x => x.id === projModel.current)
                            text: projModel.current === "" ? "Default model" : Theme.modelLabel(m ? m.name : projModel.current)
                            font.pixelSize: Theme.px(12)
                            onClicked: {
                                if (proj.p.remote) { projModel.hostModels = null; Workspace.requestHostModels(proj.p.id, proj.p.harness) }
                                else Workspace.ensureProviders()
                                projModelMenu.popup(projModelButton, 0, projModelButton.height + 4)
                            }
                            AppMenu {
                                id: projModelMenu
                                AppMenuItem { text: "Default model"; marked: projModel.current === ""; onTriggered: projModel.choose("") }
                                AppMenuItem {
                                    visible: proj.p !== null && proj.p.remote && projModel.hostModels === null
                                    enabled: false
                                    text: "Reading its models on " + (proj.p ? proj.p.host : "") + "…"
                                }
                                AppMenuSeparator { visible: projModel.choices.length > 0 }
                                Repeater {
                                    model: projModel.choices
                                    AppMenuItem {
                                        required property var modelData
                                        text: Theme.modelLabel(modelData.name)
                                        hint: modelData.local ? "Local" : Theme.providerOf(modelData.id)
                                        marked: projModel.current === modelData.id
                                        onTriggered: projModel.choose(modelData.id)
                                    }
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
                Rule { visible: proj.p !== null }
                // Specs and the shared plan.
                SectionHeader {
                    visible: proj.p !== null
                    title: "Specs and plans"
                    subtitle: "Tools that help this project's agents plan and keep track."
                    more: "Every agent gets them, through harlo's tools (Settings → Orchestration). Sessions started afterwards get them."
                }
                Group {
                    id: featureGroup
                    visible: proj.p !== null
                    // (Re-read when another project is picked, or a switch moves.)
                    readonly property int pid: proj.p ? proj.p.id : -1
                    property int rev: 0
                    function on(name) { return rev >= 0 && pid >= 0 && Workspace.setting(name + "_" + pid) === "1" }
                    function set(name, on) { Workspace.setSetting(name + "_" + pid, on ? "1" : "0"); rev++ }
                    SettingRow {
                        title: "Specs"
                        detail: "SPEC.md files beside the code say what each part is for, how it behaves and why. Agents read them before changing code and keep them true in the same change, so the project's intent outlives the chats. The Specs panel (right edge) lists them."
                        icon: "file-text"
                        AppSwitch {
                            objectName: "projectSpecs"
                            checked: featureGroup.on("specs")
                            onToggled: featureGroup.set("specs", checked)
                        }
                    }
                    SettingRow {
                        id: planRow
                        divider: true
                        title: "Shared plan"
                        detail: "For a bigger task the agent writes a plan of steps and works through it."
                        more: "You add, change, reorder or skip steps at any time (above the message box), and it reads them before each step. On by default in a git repository."
                        icon: "list-check-2"
                        // (Unset: on in a git repository.)
                        readonly property bool on: featureGroup.rev >= 0 && featureGroup.pid >= 0 && Workspace.planOn(featureGroup.pid)
                        AppSwitch {
                            objectName: "projectPlan"
                            checked: planRow.on
                            onToggled: featureGroup.set("plan", checked)
                        }
                    }
                    SettingRow {
                        divider: true
                        visible: planRow.on
                        title: "Commit each finished step"
                        detail: "When a step is done, harlo commits its changes with the step as the message."
                        more: "Only if they're provably the step's: not when a file it touched already had changes, the branch moved, or another session worked there meanwhile (then it says why and leaves them to you). A finished step can be reviewed by another agent. Off unless you turn it on (or the shared plan) here."
                        icon: "git-commit"
                        AppSwitch {
                            objectName: "projectPlanCommits"
                            checked: featureGroup.rev >= 0 && featureGroup.pid >= 0 && Workspace.planCommitsOn(featureGroup.pid)
                            onToggled: featureGroup.set("plan_commits", checked)
                        }
                    }
                }
                // Who the agents' helpers are (a plan's run, a split).
                SectionHeader {
                    visible: proj.p !== null && Workspace.orchestrationOn
                    Layout.topMargin: 22
                    title: "Team"
                    subtitle: "Who does the steps and who reviews them when a plan runs with helpers."
                    more: "Also who works when an agent splits a task. You can plan with one model, have another do the steps and a third check them, and still choose others for one run, or one step."
                }
                Group {
                    id: team
                    visible: proj.p !== null && Workspace.orchestrationOn
                    readonly property int pid: proj.p ? proj.p.id : -1
                    property int rev: 0
                    // { agent, model, thinking } ({}: the same as the session).
                    readonly property var value: {
                        if (rev < 0 || pid < 0) return ({})
                        try { return JSON.parse(Workspace.setting("team_helpers_" + pid) || "{}") } catch (e) { return ({}) }
                    }
                    readonly property var providers: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).filter(p => p.canChat && p.enabled && p.state !== "missing") : []
                    readonly property var provider: providers.find(p => p.id === value.agent) || null
                    readonly property var levels: ["", "low", "medium", "high"]
                    function save(v) { Workspace.setSetting("team_helpers_" + pid, v.agent ? JSON.stringify(v) : ""); rev++ }
                    function change(k, x) { const v = Object.assign({}, value); v[k] = x; save(v) }
                    // Ready-made teams: one agent for the steps, or one doing
                    // them and another reviewing them.
                    readonly property var reviewerProvider: value.reviewer ? providers.find(p => p.id === value.reviewer.agent) || null : null
                    onVisibleChanged: if (visible) Workspace.ensureProviders()
                    SettingRow {
                        title: "Helpers"
                        detail: team.provider ? "Helpers are " + team.provider.label + " sessions, with the model and thinking below."
                                              : "The same as the session that starts them: its agent, model and thinking."
                        icon: "node-tree"
                        ComboField {
                            id: teamAgent
                            objectName: "teamAgent"
                            width: 190
                            model: ["Same as the session"].concat(team.providers.map(p => p.label))
                            function at() { return team.provider ? team.providers.indexOf(team.provider) + 1 : 0 }
                            currentIndex: at()
                            onModelChanged: currentIndex = Qt.binding(() => at())
                            onActivated: i => {
                                team.save(i === 0 ? {} : { agent: team.providers[i - 1].id, model: "", thinking: "" })
                                currentIndex = Qt.binding(() => at())
                            }
                        }
                    }
                    SettingRow {
                        visible: team.provider !== null
                        divider: true
                        title: "Their model"
                        detail: team.provider && team.provider.state === "checking" ? "Finding " + team.provider.label + "'s models…" : ""
                        icon: "sparkling"
                        ComboField {
                            id: teamModel
                            objectName: "teamModel"
                            width: 190
                            readonly property var ids: team.provider ? team.provider.modelIds || [] : []
                            model: ["Default model"].concat(team.provider ? (team.provider.models || []).map(m => Theme.modelLabel(m)) : [])
                            function at() { return !team.value.model ? 0 : ids.indexOf(team.value.model) + 1 }
                            currentIndex: at()
                            onModelChanged: currentIndex = Qt.binding(() => at())
                            onActivated: i => {
                                team.change("model", i > 0 ? ids[i - 1] || "" : "")
                                currentIndex = Qt.binding(() => at())
                            }
                        }
                    }
                    SettingRow {
                        visible: team.provider !== null
                        divider: true
                        title: "Their thinking"
                        detail: "How hard they think: more is slower and costs more."
                        icon: "speed"
                        ComboField {
                            objectName: "teamThinking"
                            width: 190
                            model: ["Default", "Low", "Medium", "High"]
                            function at() { return Math.max(0, team.levels.indexOf(team.value.thinking || "")) }
                            currentIndex: at()
                            onActivated: i => {
                                team.change("thinking", team.levels[i])
                                currentIndex = Qt.binding(() => at())
                            }
                        }
                    }
                    SettingRow {
                        visible: team.provider !== null
                        divider: true
                        title: "Reviews by"
                        detail: team.reviewerProvider ? "Each finished step is reviewed by a " + team.reviewerProvider.label + " session."
                                                      : "Each finished step is reviewed by a session like its helper."
                        icon: "eye"
                        ComboField {
                            objectName: "teamReviewer"
                            width: 190
                            model: ["The helpers' agent"].concat(team.providers.map(p => p.label))
                            function at() { return team.reviewerProvider ? team.providers.indexOf(team.reviewerProvider) + 1 : 0 }
                            currentIndex: at()
                            onModelChanged: currentIndex = Qt.binding(() => at())
                            onActivated: i => {
                                team.change("reviewer", i === 0 ? undefined : { agent: team.providers[i - 1].id, model: "" })
                                currentIndex = Qt.binding(() => at())
                            }
                        }
                    }
                    SettingRow {
                        visible: team.provider !== null && team.reviewerProvider !== null
                        divider: true
                        title: "The reviewer's model"
                        icon: "sparkling"
                        ComboField {
                            objectName: "teamReviewerModel"
                            width: 190
                            readonly property var ids: team.reviewerProvider ? team.reviewerProvider.modelIds || [] : []
                            model: ["Default model"].concat(team.reviewerProvider ? (team.reviewerProvider.models || []).map(m => Theme.modelLabel(m)) : [])
                            function at() { return !team.value.reviewer || !team.value.reviewer.model ? 0 : ids.indexOf(team.value.reviewer.model) + 1 }
                            currentIndex: at()
                            onModelChanged: currentIndex = Qt.binding(() => at())
                            onActivated: i => {
                                team.change("reviewer", { agent: team.value.reviewer.agent, model: i > 0 ? ids[i - 1] || "" : "" })
                                currentIndex = Qt.binding(() => at())
                            }
                        }
                    }
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
                        onClicked: removeProject.ask(proj.p.id, proj.p.name)
                    }
                    Hint { text: "Its folder stays; you choose what happens to its sessions." }
                    RemoveProjectDialog { id: removeProject; onRemoved: proj.chosen = -1 }
                }
            }
        }
    }

    Component {
        id: mcpPage
        McpPage {
            compact: root.compact
            footerHost: root.pageFooter
            onSignInRequested: (scope, server) => root.mcpSignInRequested(scope, server)
        }
    }

    Component {
        id: orchestrationPage
        ColumnLayout {
            id: orch
            spacing: 0

            SectionHeader {
                title: "Agents drive harlo"
                subtitle: "Agents in harlo can start sessions with other agents, send them messages, read their replies and stop them. The sessions an agent starts show under its own as its helpers: each one reports back to it when it finishes, can ask it questions, and it's told when one waits for you. Any agent can also ask you to approve a step; only you can answer. harlo hands these tools to each session it starts (Claude Code's on a host need python3 there); outside harlo, agents don't have them."
            }
            Group {
                SettingRow {
                    title: "Orchestration"
                    detail: Workspace.orchestrationOn ? "On: agents get harlo's tools, and the composer has the Delegate button (hand off, second opinion, committee, split)."
                                                      : "Off: agents get no harlo tools, and there's no Delegate button."
                    icon: "ai-agent"
                    AppSwitch {
                        objectName: "orchEnabled"
                        checked: Workspace.orchestrationOn
                        onToggled: { Workspace.setOrchestrationEnabled(checked); checked = Qt.binding(() => Workspace.orchestrationOn) }
                    }
                }
                SettingRow {
                    id: selfStart
                    visible: Workspace.orchestrationOn
                    divider: true
                    // "1" on their own, "asked" only when asked, else they offer.
                    property string mode: Workspace.setting("orchestration_on_their_own")
                    readonly property int choice: mode === "asked" ? 0 : mode === "1" ? 2 : 1
                    title: "Agents start helpers"
                    detail: ["Only when you ask: for another agent or session, or with the Delegate button. Otherwise they do the work themselves.",
                             "When you ask, and they offer a split when a task has parts that could be done at the same time: you decide, and harlo starts the helpers if you agree.",
                             "On their own: when another agent's work would help (a second opinion, parallel work, another model)."][choice]
                        + " For sessions started from now on."
                    icon: "node-tree"
                    ComboField {
                        objectName: "orchSelfStart"
                        width: 190
                        model: ["Only when I ask", "Ask me first", "On their own"]
                        currentIndex: selfStart.choice
                        onActivated: i => {
                            selfStart.mode = ["asked", "", "1"][i]
                            Workspace.setSetting("orchestration_on_their_own", selfStart.mode)
                        }
                    }
                }
                SettingRow {
                    id: reviewSteps
                    visible: Workspace.orchestrationOn
                    divider: true
                    // ("" on, the default; "0" off.)
                    property bool reviews: Workspace.setting("plan_review") !== "0"
                    title: "Review each step"
                    detail: reviews ? "When a plan runs with helpers, another session of the same agent reviews each finished step before the next one starts. Problems go back to the step's helper once; if the review still finds some, the run pauses for you."
                               : "Off: a plan's steps follow each other as soon as their helpers report."
                    icon: "eye"
                    AppSwitch {
                        objectName: "orchReviewSteps"
                        checked: reviewSteps.reviews
                        onToggled: { Workspace.setSetting("plan_review", checked ? "" : "0"); reviewSteps.reviews = checked }
                    }
                }
            }

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
                title: "SSH connections"
                subtitle: "Work on other machines over SSH, each with its own projects and sessions."
                more: "Switch to one from the instance menu at the top of the sidebar, then + Project adds its folders; its chats, terminal, git and files work there. harlo signs in with your SSH keys or agent. Each connected machine shows whether it has Claude Code, installs it for you, and signs Claude in there; harlo brings Harlo itself."
                AddButton { objectName: "addRemoteHost"; text: "Add SSH connection"; enabled: remotes.editing === null; onClicked: addHostDialog.open() }
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
            Group { visible: remotes.hosts.length === 0 && remotes.editing === null; EmptyNote { text: "No remote machines yet." } }
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
                            model: ["claude"]
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
                                        ToolTip.text: "Claude isn't signed in on " + agentChip.host.name + "."
                                        onClicked: root.signInRequested("claude@" + agentChip.host.name, "Claude on " + agentChip.host.name)
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

    // Your own model servers, for Harlo (its providers.json).
    Component {
        id: serversPage
        ColumnLayout {
            id: srv
            spacing: 0
            property int revision: 0
            readonly property var agents: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : [])
                .filter(p => p.id === "harlo" && p.state !== "missing")
            // Other machines that are connected, and the servers their agents have.
            readonly property var hostNames: (Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()) : []).filter(h => h.state === "connected").map(h => h.name)
            readonly property var onHosts: Accounts.hostServers !== "" ? JSON.parse(Accounts.hostServers) : ({})
            // ...and the servers harlo set up on them, there.
            readonly property var hostEngines: Accounts.hostEngines !== "" ? JSON.parse(Accounts.hostEngines) : ({})
            function lookAtHosts() { for (const h of hostNames) { Accounts.loadHostServers(h); Accounts.loadHostEngines(h); Accounts.checkHostUpdates(h, false) } }
            // (Another machine's server added to an agent here, or taken out.)
            Connections { target: Accounts; function onLocalServersChanged() { Workspace.refreshProviders() } }
            onHostNamesChanged: if (visible) lookAtHosts()
            // One row per server, with the agents that have it.
            readonly property var list: {
                const rows = []
                if (revision < 0 || Accounts.revision < 0) return rows
                for (const a of agents) {
                    for (const sv of JSON.parse(Accounts.servers(a.id))) {
                        const same = rows.find(r => r.name === sv.name && r.baseUrl === sv.baseUrl)
                        if (same) { same.agents.push(a.label); same.agentIds.push(a.id) }
                        else rows.push({ name: sv.name, baseUrl: sv.baseUrl, models: sv.models, place: sv.place, managed: sv.managed, engine: sv.engine || "", contexts: sv.contexts || [], agents: [a.label], agentIds: [a.id] })
                    }
                }
                // Set up by harlo, in no agent yet.
                for (const sv of JSON.parse(Accounts.servers("")))
                    rows.push({ name: sv.name, baseUrl: sv.baseUrl, models: sv.models, place: sv.place, managed: true, engine: sv.engine || "", contexts: sv.contexts || [], agents: [], agentIds: [] })
                return rows
            }
            // What a row says it runs: engine · models · context, then where.
            // A server: its engine (and port when there are two of it), or
            // the name of one harlo didn't set up.
            function title(row) {
                if (!row.engine) return row.name
                const port = row.baseUrl.replace(/^https?:\/\/[^:\/]+:?/, "").replace(/\/.*$/, "")
                const twin = srv.list.filter(r => r.engine === row.engine).length > 1
                return row.engine + (twin && port ? " · " + port : "")
            }
            // Its models, one a line, with their context; then where it is
            // and the name agents list it under.
            function describe(row) {
                const lines = row.models.map((id, i) => {
                    const c = (row.contexts || [])[i]
                    // (Ollama says it only while running: harlo writes it down then.)
                    return Theme.modelLabel(id) + " · " + (c > 0 ? Math.round(c / 1024) + "k context" : row.managed ? "context set when it starts" : "context not given")
                })
                if (lines.length === 0) lines.push("No models yet")
                const where = row.baseUrl.replace(/^https?:\/\//, "").replace(/\/v1\/?$/, "")
                if (row.via) return lines.join("\n") + "\nOn " + row.via + ", through the SSH connection (" + where + " here)"
                return lines.join("\n") + "\n" + where + (row.engine ? " · in agents as “" + row.name + "”" : "")
            }
            ModelServerDialog { id: modelServerDialog; onAdded: { Accounts.loadServers(); Workspace.refreshProviders() } }
            // A server one agent has, added to another as it is there.
            function copy(row, agent) {
                const taken = [...new Set((agent.modelIds || []).map(m => m.split(/[\/:]/)[0]).filter(p => p !== ""))]
                // (From an agent that has it, else what harlo saved at its address.)
                const err = Accounts.copyServer(row.agentIds.length > 0 ? row.agentIds[0] : row.baseUrl, agent.id, row.name, JSON.stringify(taken))
                if (err !== "") { Workspace.showNotice(err, true); return false }
                Accounts.loadServers()
                Workspace.refreshProviders()
                return true
            }
            // Out of one agent (the others keep it).
            function uncopy(row, agent) {
                const err = Accounts.removeServerFrom(agent.id, row.name)
                if (err !== "") { Workspace.showNotice(err, true); return false }
                Workspace.refreshProviders()
                return true
            }
            // Removing one: from every agent that has it; one harlo set up
            // is stopped too, and its model file deleted if you like.
            AppDialog {
                id: removeServerDialog
                objectName: "removeServerDialog"
                property var row: null
                property var files: ({ files: [], size: 0 })
                // Removing (in the background): how far, until it's done.
                property bool removing: false
                // Done: what it did (and what didn't go), until it's closed.
                property bool finished: false
                property string summary: ""
                property string problems: ""
                readonly property var job: Accounts.removal !== "" ? JSON.parse(Accounts.removal) : null
                function ask(r) {
                    row = r
                    files = r.managed ? JSON.parse(Accounts.serverFiles(r.baseUrl)) : ({ files: [], size: 0 })
                    deleteFiles.checked = true
                    removing = false
                    finished = false
                    open()
                }
                title: !row ? "" : finished ? "Removed " + row.name : removing ? "Removing " + row.name : "Remove " + row.name + "?"
                width: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)
                closePolicy: removing ? Popup.NoAutoClose : Popup.CloseOnEscape
                footer: AppButtonBox {
                    DialogButton { text: removeServerDialog.finished ? "Close" : "Cancel"; enabled: !removeServerDialog.removing; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
                    DialogButton { objectName: "removeServerConfirm"; visible: !removeServerDialog.finished; text: "Remove"; enabled: !removeServerDialog.removing; DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole }
                }
                onDiscarded: {
                    removing = true
                    Accounts.removeServer(row.name, row.baseUrl, files.files.length > 0 && deleteFiles.checked)
                }
                Connections {
                    target: Accounts
                    function onRemovalChanged() {
                        const j = removeServerDialog.job
                        if (!removeServerDialog.removing || !j || j.state !== "done" || !removeServerDialog.row || j.url !== removeServerDialog.row.baseUrl) return
                        removeServerDialog.removing = false
                        removeServerDialog.finished = true
                        removeServerDialog.summary = j.summary || "Removed."
                        removeServerDialog.problems = j.error || ""
                        Workspace.refreshProviders()
                    }
                }
                ColumnLayout {
                    width: parent.width
                    spacing: 8
                    // Done: what it did.
                    Hint {
                        visible: removeServerDialog.finished
                        color: Theme.foreground
                        text: removeServerDialog.summary
                    }
                    Hint {
                        visible: removeServerDialog.finished && removeServerDialog.problems !== ""
                        color: Theme.error
                        text: removeServerDialog.problems
                    }
                    Hint {
                        visible: !removeServerDialog.finished
                        color: Theme.foreground
                        text: !removeServerDialog.row ? "" : removeServerDialog.row.agents.length === 0 ? "No agent uses it; harlo stops it and forgets how to start it."
                            : "harlo takes it out of " + removeServerDialog.row.agents.join(", ") + (removeServerDialog.row.agents.length === 1 ? "" : " (each one's own file)")
                              + (removeServerDialog.row.managed ? ", stops it and forgets how to start it." : ". The server itself keeps running where it is.")
                    }
                    AppCheckBox {
                        id: deleteFiles
                        objectName: "removeServerFiles"
                        visible: removeServerDialog.files.files.length > 0 && !removeServerDialog.finished
                        enabled: !removeServerDialog.removing
                        readonly property string size: removeServerDialog.files.size > 0 ? " (" + (removeServerDialog.files.size / 1e9).toFixed(1) + " GB)" : ""
                        text: removeServerDialog.files.ollama
                            ? "Also delete its " + (removeServerDialog.files.files.length === 1 ? "model" : removeServerDialog.files.files.length + " models") + " from Ollama" + size
                            : "Also delete its model " + (removeServerDialog.files.files.length === 1 ? "file" : "files") + size
                    }
                    Hint {
                        visible: removeServerDialog.row !== null && removeServerDialog.row.managed && !removeServerDialog.removing && !removeServerDialog.finished
                        text: "The engine stays, for your other servers; setting this one up again downloads the model again if you delete it."
                    }
                    // What it's doing now, and how many of how many.
                    ColumnLayout {
                        visible: removeServerDialog.removing && removeServerDialog.job !== null
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 6
                        Label {
                            Layout.fillWidth: true
                            readonly property var j: removeServerDialog.job || ({})
                            text: (j.step || "Removing it") + (j.total > 1 ? " (" + (j.done + 1) + " of " + j.total + ")" : "")
                            color: Theme.foreground
                            font.pixelSize: Theme.px(12.5)
                            elide: Text.ElideMiddle
                        }
                        AppProgressBar {
                            Layout.fillWidth: true
                            readonly property var j: removeServerDialog.job || ({})
                            indeterminate: !(j.total > 1)
                            value: j.total > 1 ? j.done / j.total : 0
                        }
                    }
                }
            }
            SetupServerDialog { id: setupServer; onAdded: { Accounts.loadServers(); Workspace.refreshProviders(); srv.lookAtHosts() } }
            // Which are up: now, and every few seconds while this is open.
            readonly property var states: Accounts.serverStates !== "" ? JSON.parse(Accounts.serverStates) : ({})
            function check() { Accounts.checkServers(JSON.stringify([...new Set(srv.list.filter(r => r.place !== "online").map(r => r.baseUrl))])) }
            // (What harlo read last shows at once; read again in the background.)
            Component.onCompleted: { Accounts.loadServers(); check(); lookAtEngines(); if (visible) lookAtHosts() }
            onVisibleChanged: if (visible) { lookAtEngines(); lookAtHosts() }
            // The engines on this computer, and whether they have a newer version.
            function lookAtEngines() {
                // (This computer's, unless a setup is going on elsewhere.)
                if (srv.jobRunning || setupServer.visible) return
                if (ServerSetup.host !== "") ServerSetup.host = ""
                ServerSetup.refresh()
                ServerSetup.checkUpdates(false)
            }
            readonly property var engines: setupServer.cat.host ? [] : (setupServer.cat.engines || []).filter(e => e.installed !== null)
            readonly property var updates: ServerSetup.updates !== "" ? JSON.parse(ServerSetup.updates) : ({})
            readonly property bool jobRunning: setupServer.job !== null && setupServer.job.state === "running"
            Connections { target: Accounts; function onRevisionChanged() { srv.check() } }
            Timer { interval: 3000; repeat: true; running: srv.visible; onTriggered: srv.check() }
            SectionHeader {
                title: "Your servers"
                subtitle: "Models you run yourself or your company serves, for Harlo."
                more: "They show in the model lists (under Local when they run on this computer or your network). Claude Code only uses Anthropic's models. harlo starts and stops the servers it set up; they stop when harlo quits. Tick an agent to add a server to its config, untick it to take it out."
                AddButton {
                    objectName: "setupModelServer"
                    visible: srv.agents.length > 0
                    text: "Set up a local model…"
                    onClicked: setupServer.openOn("")
                    ToolTip.visible: hovered
                    ToolTip.text: "harlo installs an engine, downloads a model that fits your graphics card, starts it and adds it to your agents."
                    ToolTip.delay: 400
                }
                AddButton {
                    objectName: "addModelServer"
                    visible: srv.agents.length > 0
                    text: "Connect…"
                    onClicked: modelServerDialog.open()
                    ToolTip.visible: hovered
                    ToolTip.text: "One you run yourself or your company serves (Ollama, LM Studio, vLLM, a gateway…): its address, and harlo finds its models."
                    ToolTip.delay: 400
                }
            }
            // Setting one up, in the background (the dialog can be hidden).
            RowLayout {
                readonly property var job: ServerSetup.job !== "" ? JSON.parse(ServerSetup.job) : null
                visible: job !== null && job.state === "running"
                Layout.topMargin: 12
                spacing: 8
                BusyIndicator { implicitWidth: 18; implicitHeight: 18; running: parent.visible }
                Label { Layout.fillWidth: true; text: parent.job ? (ServerSetup.host !== "" ? ServerSetup.host + ": " : "") + parent.job.step : ""; color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight }
                FlatButton { text: "Show"; font.pixelSize: Theme.px(12); onClicked: setupServer.open() }
            }
            Group { visible: srv.agents.length === 0; EmptyNote { text: "Model servers are for Harlo, which isn't installed." } }
            Group {
                visible: srv.list.length > 0
                Repeater {
                    model: srv.list
                    // A server: its engine's mark with whether it runs under it,
                    // what it serves and where, the agents that have it; its
                    // buttons down the right.
                    Item {
                        id: serverItem
                        required property var modelData
                        required property int index
                        readonly property var st: srv.states[modelData.baseUrl] || ({ state: "", message: "" })
                        readonly property bool running: st.state === "running"
                        readonly property bool local: modelData.place !== "online"
                        Layout.fillWidth: true
                        implicitHeight: Math.max(info.implicitHeight, buttons.implicitHeight, mark.implicitHeight) + 28
                        Rectangle { visible: serverItem.index > 0; width: parent.width; height: 1; color: Theme.hairline }
                        Column {
                            id: mark
                            x: 16
                            y: 14
                            spacing: 8
                            Rectangle {
                                width: 32
                                height: 32
                                radius: 9
                                color: Theme.controlFill
                                // (llama.cpp has no logo: a server; one harlo didn't set up too.)
                                ProviderLogo {
                                    anchors.centerIn: parent
                                    implicitWidth: 18
                                    implicitHeight: 18
                                    provider: serverItem.modelData.engine ? serverItem.modelData.engine.toLowerCase() : "local"
                                    color: Theme.textMuted
                                }
                            }
                            Rectangle {
                                objectName: "serverState-" + serverItem.modelData.name
                                visible: serverItem.local
                                anchors.horizontalCenter: parent.horizontalCenter
                                width: 9
                                height: 9
                                radius: 4.5
                                color: serverItem.running ? Theme.success : serverItem.st.state === "starting" ? Theme.warning : serverItem.st.state === "failed" ? Theme.error : Theme.muted
                                HoverHandler { id: stateHover }
                                ToolTip.visible: stateHover.hovered
                                ToolTip.text: ({ running: serverItem.st.harlo ? "Running (started by harlo)" : "Running", starting: "Starting…", failed: "Didn't start", stopped: "Stopped" })[serverItem.st.state] || "Checking…"
                                ToolTip.delay: 300
                            }
                        }
                        ColumnLayout {
                            id: info
                            x: 60
                            y: 14
                            width: parent.width - x - buttons.width - 32
                            spacing: 3
                            Label {
                                Layout.fillWidth: true
                                text: srv.title(serverItem.modelData)
                                color: Theme.text
                                font.pixelSize: Theme.px(14)
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                text: srv.describe(serverItem.modelData) + (serverItem.modelData.place === "online" ? "\nOnline service" : "")
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                            }
                            // Starting, or why it didn't.
                            Label {
                                Layout.fillWidth: true
                                visible: text !== ""
                                text: serverItem.st.message || ""
                                color: serverItem.st.state === "failed" ? Theme.error : Theme.muted
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                            }
                            // The agents: ticked ones have it in their config;
                            // ticking adds it, unticking takes it out.
                            Flow {
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                spacing: 16
                                Repeater {
                                    model: srv.agents
                                AppCheckBox {
                                    id: agentBox
                                    required property var modelData
                                    readonly property bool has: serverItem.modelData.agentIds.includes(modelData.id)
                                    objectName: "serverAgent-" + serverItem.modelData.name + "-" + modelData.id
                                    text: modelData.label
                                    font.pixelSize: Theme.px(12)
                                    // Changing: what was asked for, locked, with a
                                    // spinner until the list shows it (the row is
                                    // made again then).
                                    property bool busy: false
                                    property bool want: false
                                    checked: busy ? want : has
                                    enabled: !busy
                                    onToggled: {
                                        const want = checked
                                        checked = Qt.binding(() => agentBox.busy ? agentBox.want : agentBox.has)
                                        agentBox.want = want
                                        agentBox.busy = true
                                        // (After the spinner is drawn.)
                                        change.start()
                                    }
                                    Timer {
                                        id: change
                                        interval: 30
                                        onTriggered: {
                                            const ok = agentBox.want ? srv.copy(serverItem.modelData, agentBox.modelData) : srv.uncopy(serverItem.modelData, agentBox.modelData)
                                            if (ok) giveUp.start()
                                            else agentBox.busy = false
                                        }
                                    }
                                    // (The list didn't come back: unlocked anyway.)
                                    Timer { id: giveUp; interval: 10000; onTriggered: agentBox.busy = false }
                                    BusyIndicator {
                                        visible: agentBox.busy
                                        running: visible
                                        x: agentBox.width + 2
                                        anchors.verticalCenter: parent.verticalCenter
                                        implicitWidth: 14
                                        implicitHeight: 14
                                    }
                                }
                                }
                            }
                            // Other machines' agents: one on this computer is
                            // reached there over the SSH connection.
                            Repeater {
                                model: serverItem.local && !serverItem.modelData.via ? srv.hostNames : []
                                ColumnLayout {
                                    id: hostRow
                                    required property string modelData
                                    readonly property var there: srv.onHosts[modelData] || ({ state: "loading", agents: {} })
                                    readonly property var agentIds: Object.keys(there.agents || {})
                                    Layout.fillWidth: true
                                    Layout.topMargin: 4
                                    spacing: 2
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 16
                                        Label {
                                            text: "On " + hostRow.modelData + ":"
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(12)
                                            height: implicitHeight + 6
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        Label {
                                            visible: hostRow.there.state !== "ready"
                                            text: hostRow.there.state === "error" ? "couldn't read its agents' files" : "reading…"
                                            color: hostRow.there.state === "error" ? Theme.error : Theme.textFaint
                                            font.pixelSize: Theme.px(12)
                                            height: implicitHeight + 6
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        Label {
                                            visible: hostRow.there.state === "ready" && hostRow.agentIds.length === 0
                                            text: "no Harlo there"
                                            color: Theme.textFaint
                                            font.pixelSize: Theme.px(12)
                                            height: implicitHeight + 6
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        Repeater {
                                            model: hostRow.there.state === "ready" ? hostRow.agentIds : []
                                            AppCheckBox {
                                                required property string modelData
                                                readonly property bool has: (hostRow.there.agents[modelData] || []).some(sv => sv.name === serverItem.modelData.name && sv.baseUrl === serverItem.modelData.baseUrl)
                                                objectName: "serverHostAgent-" + serverItem.modelData.name + "-" + hostRow.modelData + "-" + modelData
                                                text: Theme.harnessLabel(modelData)
                                                font.pixelSize: Theme.px(12)
                                                checked: has
                                                enabled: !hostRow.there.busy
                                                onToggled: {
                                                    const want = checked
                                                    checked = Qt.binding(() => has)
                                                    if (want) Accounts.addServerOn(hostRow.modelData, modelData, serverItem.modelData.agentIds.length > 0 ? serverItem.modelData.agentIds[0] : serverItem.modelData.baseUrl, serverItem.modelData.name)
                                                    else Accounts.removeServerOn(hostRow.modelData, modelData, serverItem.modelData.name)
                                                }
                                                ToolTip.visible: hovered
                                                ToolTip.text: serverItem.modelData.place === "here"
                                                    ? "Its sessions on " + hostRow.modelData + " reach this computer's server through the SSH connection, while harlo is open."
                                                    : "Its sessions on " + hostRow.modelData + " use the same address; it has to reach it from there."
                                                ToolTip.delay: 400
                                            }
                                        }
                                        BusyIndicator {
                                            visible: !!hostRow.there.busy
                                            running: visible
                                            implicitWidth: 14
                                            implicitHeight: 14
                                        }
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        visible: text !== ""
                                        text: hostRow.there.note || (hostRow.there.state === "error" ? hostRow.there.error || "" : "")
                                        color: Theme.error
                                        font.pixelSize: Theme.px(12)
                                        wrapMode: Text.Wrap
                                    }
                                }
                            }
                        }
                        // Its buttons, down the right, as wide as each other.
                        Column {
                            id: buttons
                            anchors.right: parent.right
                            anchors.rightMargin: 16
                            y: 14
                            width: 112
                            spacing: 4
                            // Set up by harlo: Start, Stop while it runs.
                            FlatButton {
                                objectName: (serverItem.running ? "serverStop-" : "serverStart-") + serverItem.modelData.name
                                visible: serverItem.modelData.managed && serverItem.local
                                width: parent.width
                                enabled: serverItem.st.state !== "starting" && serverItem.st.state !== ""
                                text: serverItem.running ? "Stop" : serverItem.st.state === "starting" ? "Starting…" : "Start"
                                // (Stop in red, as Remove.)
                                variant: serverItem.running ? "danger" : "primary"
                                accent: serverItem.running ? Theme.error : Theme.foreground
                                font.pixelSize: Theme.px(12)
                                onClicked: serverItem.running ? Accounts.stopServer(serverItem.modelData.baseUrl) : Accounts.startServer(serverItem.modelData.baseUrl)
                            }
                            FlatButton {
                                readonly property string log: serverItem.modelData.managed && Accounts.revision >= 0 && serverItem.st.state !== "" ? Accounts.serverLog(serverItem.modelData.baseUrl) : ""
                                visible: log !== ""
                                width: parent.width
                                text: "Log"
                                font.pixelSize: Theme.px(12)
                                onClicked: root.openFileRequested(log)
                            }
                            // An Ollama or llama.cpp harlo set up: more models for it.
                            FlatButton {
                                objectName: "serverAddModel-" + serverItem.modelData.name
                                visible: serverItem.modelData.managed && (serverItem.modelData.engine === "Ollama" || serverItem.modelData.engine === "llama.cpp")
                                width: parent.width
                                text: "Add a model…"
                                font.pixelSize: Theme.px(12)
                                onClicked: setupServer.addModel(serverItem.modelData)
                            }
                            FlatButton {
                                objectName: "serverRemove-" + serverItem.modelData.name
                                width: parent.width
                                text: "Remove"
                                variant: "danger"
                                accent: Theme.error
                                font.pixelSize: Theme.px(12)
                                onClicked: removeServerDialog.ask(serverItem.modelData)
                            }
                        }
                    }
                }
            }
            Group {
                visible: srv.agents.length > 0 && srv.list.length === 0
                EmptyNote { text: "No model servers yet. Set up a local model: harlo installs an engine, downloads a model that fits your graphics card and starts it. Start and Stop it here or from the left bar." }
            }
            Rule { visible: srv.engines.length > 0 }
            SectionHeader { visible: srv.engines.length > 0; title: "Engines on this computer"; subtitle: "What runs the servers harlo set up here." }
            Group {
                visible: srv.engines.length > 0
                Repeater {
                    model: srv.engines
                    // An engine: its version, whether a newer one is out, and Update.
                    Item {
                        id: engineItem
                        required property var modelData
                        required property int index
                        readonly property var latest: srv.updates[modelData.id] || null
                        readonly property bool updatable: modelData.installed.updatable === true
                        readonly property bool newer: updatable && latest !== null && latest.available === true
                        readonly property string version: modelData.installed.version.replace(/^v/, "")
                        // (Built on this computer: several minutes.)
                        readonly property bool built: modelData.id === "basalt" || modelData.id === "ninfer"
                        Layout.fillWidth: true
                        implicitHeight: 56
                        objectName: "engineRow-" + modelData.id
                        Rectangle { visible: engineItem.index > 0; width: parent.width; height: 1; color: Theme.hairline }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 16
                            anchors.rightMargin: 16
                            spacing: 12
                            Rectangle {
                                implicitWidth: 32
                                implicitHeight: 32
                                radius: 9
                                color: Theme.controlFill
                                ProviderLogo {
                                    anchors.centerIn: parent
                                    implicitWidth: 18
                                    implicitHeight: 18
                                    provider: engineItem.modelData.name.toLowerCase()
                                    color: Theme.textMuted
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label { text: engineItem.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(13); font.weight: Font.Medium }
                                Label {
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    text: engineItem.newer ? engineItem.version + " → " + engineItem.latest.latest
                                        : !engineItem.updatable ? engineItem.version + " · installed outside harlo"
                                        : engineItem.latest !== null ? engineItem.version + " · up to date"
                                        : engineItem.version
                                    color: engineItem.newer ? Theme.accent : Theme.textMuted
                                    font.pixelSize: Theme.px(12)
                                }
                            }
                            FlatButton {
                                objectName: "engineUpdate-" + engineItem.modelData.id
                                visible: engineItem.newer
                                enabled: !srv.jobRunning
                                text: "Update"
                                variant: "primary"
                                font.pixelSize: Theme.px(12)
                                onClicked: setupServer.updateEngine(engineItem.modelData.id)
                                ToolTip.visible: hovered
                                ToolTip.delay: 400
                                ToolTip.text: (engineItem.built ? "harlo builds the new version here (several minutes)" : "harlo downloads the new version")
                                    + ". Its servers that are running stop and start again, so a message on them now would be cut off."
                            }
                        }
                    }
                }
            }
            // Each machine harlo is connected to: the servers harlo set up
            // there (running there, for its agents there), and setting one up.
            Repeater {
                model: srv.hostNames
                ColumnLayout {
                    id: hostBlock
                    required property string modelData
                    readonly property string host: modelData
                    readonly property var e: srv.hostEngines[host] || ({ state: "loading", list: [], busy: {} })
                    readonly property var there: srv.onHosts[host] || ({ state: "loading", agents: {} })
                    readonly property var agentIds: ["harlo"].filter(id => (there.agents || {})[id] !== undefined)
                    // Rows as this computer's: models by id, their contexts.
                    readonly property var rows: (e.list || []).map(r => ({
                        name: r.name, baseUrl: r.baseUrl, engine: r.engine, running: r.running, log: r.log, managed: true, here: r.here || "",
                        models: (r.models || []).map(m => m.id), contexts: (r.models || []).map(m => m.context || 0), rawModels: r.models || [],
                        agentIds: ["harlo"].filter(id => ((there.agents || {})[id] || []).some(sv => sv.baseUrl === r.baseUrl)),
                    }))
                    Layout.fillWidth: true
                    spacing: 0
                    Rule { }
                    SectionHeader { title: "On " + hostBlock.host }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        spacing: 8
                        Hint {
                            Layout.fillWidth: true
                            text: hostBlock.e.state === "error" ? "Couldn't read its servers: " + hostBlock.e.error
                                : hostBlock.e.state === "loading" && hostBlock.rows.length === 0 ? "Reading its servers…"
                                : hostBlock.rows.length === 0 ? "harlo can install an engine on " + hostBlock.host + ", download a model that fits it and run it there, for its agents."
                                : "They run on " + hostBlock.host + " for its agents there, and keep running when harlo quits."
                            color: hostBlock.e.state === "error" ? Theme.error : Theme.muted
                        }
                        FlatButton {
                            objectName: "setupModelServerOn-" + hostBlock.host
                            text: "Set up a model on " + hostBlock.host + "…"
                            font.pixelSize: Theme.px(12)
                            enabled: !srv.jobRunning || ServerSetup.host === hostBlock.host
                            onClicked: setupServer.openOn(hostBlock.host)
                            ToolTip.visible: hovered && !enabled
                            ToolTip.text: "A setup is going on; this can start once it's done."
                            ToolTip.delay: 300
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        visible: text !== ""
                        text: hostBlock.e.note || ""
                        color: Theme.error
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                    Group {
                        visible: hostBlock.rows.length > 0
                        Layout.topMargin: 8
                        Repeater {
                            model: hostBlock.rows
                            Item {
                                id: hostServer
                                required property var modelData
                                required property int index
                                readonly property string busy: (hostBlock.e.busy || {})[modelData.baseUrl] || ""
                                Layout.fillWidth: true
                                implicitHeight: Math.max(hInfo.implicitHeight, hButtons.implicitHeight, 32) + 28
                                Rectangle { visible: hostServer.index > 0; width: parent.width; height: 1; color: Theme.hairline }
                                Column {
                                    x: 16
                                    y: 14
                                    spacing: 8
                                    Rectangle {
                                        width: 32
                                        height: 32
                                        radius: 9
                                        color: Theme.controlFill
                                        ProviderLogo {
                                            anchors.centerIn: parent
                                            implicitWidth: 18
                                            implicitHeight: 18
                                            provider: hostServer.modelData.engine.toLowerCase()
                                            color: Theme.textMuted
                                        }
                                    }
                                    Rectangle {
                                        objectName: "hostServerState-" + hostBlock.host + "-" + hostServer.modelData.name
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        width: 9
                                        height: 9
                                        radius: 4.5
                                        color: hostServer.busy !== "" ? Theme.warning : hostServer.modelData.running ? Theme.success : Theme.muted
                                        HoverHandler { id: hostStateHover }
                                        ToolTip.visible: hostStateHover.hovered
                                        ToolTip.text: hostServer.busy !== "" ? hostServer.busy.charAt(0).toUpperCase() + hostServer.busy.slice(1) + "…" : hostServer.modelData.running ? "Running" : "Stopped"
                                        ToolTip.delay: 300
                                    }
                                }
                                ColumnLayout {
                                    id: hInfo
                                    x: 60
                                    y: 14
                                    width: parent.width - x - hButtons.width - 32
                                    spacing: 3
                                    Label {
                                        Layout.fillWidth: true
                                        text: hostServer.modelData.engine + (hostBlock.rows.filter(r => r.engine === hostServer.modelData.engine).length > 1 ? " · " + hostServer.modelData.baseUrl.replace(/^https?:\/\/[^:\/]+:?/, "").replace(/\/.*$/, "") : "")
                                        color: Theme.text
                                        font.pixelSize: Theme.px(14)
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        text: srv.describe(hostServer.modelData)
                                        color: Theme.muted
                                        font.pixelSize: Theme.px(12)
                                        wrapMode: Text.Wrap
                                    }
                                    // Its agents there: ticked ones have it.
                                    Flow {
                                        Layout.fillWidth: true
                                        Layout.topMargin: 6
                                        spacing: 16
                                        Label {
                                            visible: hostBlock.there.state === "ready" && hostBlock.agentIds.length === 0
                                            text: "No Harlo on " + hostBlock.host
                                            color: Theme.textFaint
                                            font.pixelSize: Theme.px(12)
                                            height: implicitHeight + 6
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        Repeater {
                                            model: hostBlock.there.state === "ready" ? hostBlock.agentIds : []
                                            AppCheckBox {
                                                required property string modelData
                                                readonly property bool has: hostServer.modelData.agentIds.includes(modelData)
                                                objectName: "hostServerAgent-" + hostBlock.host + "-" + hostServer.modelData.name + "-" + modelData
                                                text: Theme.harnessLabel(modelData)
                                                font.pixelSize: Theme.px(12)
                                                checked: has
                                                enabled: !hostBlock.there.busy
                                                onToggled: {
                                                    const want = checked
                                                    checked = Qt.binding(() => has)
                                                    if (want) Accounts.addHostEngine(hostBlock.host, JSON.stringify({ name: hostServer.modelData.name, baseUrl: hostServer.modelData.baseUrl, models: hostServer.modelData.rawModels, agents: [modelData] }))
                                                    else Accounts.removeServerOn(hostBlock.host, modelData, hostServer.modelData.name)
                                                }
                                            }
                                        }
                                        BusyIndicator {
                                            visible: !!hostBlock.there.busy
                                            running: visible
                                            implicitWidth: 14
                                            implicitHeight: 14
                                        }
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        visible: text !== ""
                                        text: hostBlock.there.note || ""
                                        color: Theme.error
                                        font.pixelSize: Theme.px(12)
                                        wrapMode: Text.Wrap
                                    }
                                    // This computer's agents: through the SSH connection.
                                    Flow {
                                        Layout.fillWidth: true
                                        visible: srv.agents.length > 0
                                        spacing: 16
                                        Label {
                                            text: "Here:"
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(12)
                                            height: implicitHeight + 6
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        Repeater {
                                            model: srv.agents
                                            AppCheckBox {
                                                required property var modelData
                                                readonly property bool has: hostServer.modelData.here !== "" && srv.list.some(r => r.baseUrl === hostServer.modelData.here && r.agentIds.includes(modelData.id))
                                                objectName: "hostServerHere-" + hostBlock.host + "-" + hostServer.modelData.name + "-" + modelData.id
                                                text: modelData.label
                                                font.pixelSize: Theme.px(12)
                                                checked: has
                                                enabled: hostServer.busy === ""
                                                onToggled: {
                                                    const want = checked
                                                    checked = Qt.binding(() => has)
                                                    Accounts.useHostEngine(hostBlock.host, JSON.stringify({ name: hostServer.modelData.name, baseUrl: hostServer.modelData.baseUrl, models: hostServer.modelData.rawModels }), modelData.id, want)
                                                }
                                                ToolTip.visible: hovered
                                                ToolTip.text: "Its sessions on this computer reach the server on " + hostBlock.host + " through the SSH connection; harlo starts it there first when it's stopped."
                                                ToolTip.delay: 400
                                            }
                                        }
                                        BusyIndicator {
                                            visible: hostServer.busy === "here"
                                            running: visible
                                            implicitWidth: 14
                                            implicitHeight: 14
                                        }
                                    }
                                }
                                Column {
                                    id: hButtons
                                    anchors.right: parent.right
                                    anchors.rightMargin: 16
                                    y: 14
                                    width: 112
                                    spacing: 4
                                    FlatButton {
                                        objectName: (hostServer.modelData.running ? "hostServerStop-" : "hostServerStart-") + hostBlock.host + "-" + hostServer.modelData.name
                                        width: parent.width
                                        enabled: hostServer.busy === ""
                                        text: hostServer.busy === "starting" ? "Starting…" : hostServer.busy === "stopping" ? "Stopping…" : hostServer.modelData.running ? "Stop" : "Start"
                                        variant: hostServer.modelData.running ? "danger" : "primary"
                                        accent: hostServer.modelData.running ? Theme.error : Theme.foreground
                                        font.pixelSize: Theme.px(12)
                                        onClicked: hostServer.modelData.running ? Accounts.stopHostEngine(hostBlock.host, hostServer.modelData.baseUrl) : Accounts.startHostEngine(hostBlock.host, hostServer.modelData.baseUrl)
                                    }
                                    FlatButton {
                                        visible: hostServer.modelData.log !== ""
                                        width: parent.width
                                        text: "Log"
                                        font.pixelSize: Theme.px(12)
                                        onClicked: root.openFileRequested("ssh://" + hostBlock.host + hostServer.modelData.log)
                                    }
                                    FlatButton {
                                        visible: hostServer.modelData.engine === "Ollama" || hostServer.modelData.engine === "llama.cpp"
                                        width: parent.width
                                        enabled: !srv.jobRunning
                                        text: "Add a model…"
                                        font.pixelSize: Theme.px(12)
                                        onClicked: setupServer.openOn(hostBlock.host, hostServer.modelData)
                                    }
                                    FlatButton {
                                        objectName: "hostServerRemove-" + hostBlock.host + "-" + hostServer.modelData.name
                                        width: parent.width
                                        enabled: hostServer.busy === ""
                                        text: hostServer.busy === "removing" ? "Removing…" : "Remove"
                                        variant: "danger"
                                        accent: Theme.error
                                        font.pixelSize: Theme.px(12)
                                        onClicked: removeHostServerDialog.ask(hostBlock.host, hostServer.modelData)
                                    }
                                }
                            }
                        }
                    }
                    // The engines there: their version, and Update.
                    Flow {
                        visible: (hostBlock.e.engines || []).length > 0
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        spacing: 14
                        Label { text: "Engines there:"; color: Theme.muted; font.pixelSize: Theme.px(12); height: 26; verticalAlignment: Text.AlignVCenter; rightPadding: 4 }
                        Repeater {
                            model: hostBlock.e.engines || []
                            RowLayout {
                                id: hostEngine
                                required property var modelData
                                readonly property var latest: (hostBlock.e.updates || {})[modelData.id] || null
                                readonly property bool newer: modelData.updatable && latest !== null && latest.available === true
                                readonly property string version: (modelData.version || "").replace(/^v/, "")
                                height: 26
                                spacing: 4
                                Label {
                                    text: hostEngine.modelData.name + " " + hostEngine.version + (hostEngine.newer ? " → " + hostEngine.latest.latest : "")
                                    color: hostEngine.newer ? Theme.accent : Theme.foreground
                                    font.pixelSize: Theme.px(12)
                                }
                                FlatButton {
                                    objectName: "hostEngineUpdate-" + hostBlock.host + "-" + hostEngine.modelData.id
                                    visible: hostEngine.newer
                                    enabled: !srv.jobRunning
                                    text: "Update"
                                    outlined: false
                                    accent: Theme.accent
                                    font.pixelSize: Theme.px(12)
                                    onClicked: setupServer.updateEngine(hostEngine.modelData.id, hostBlock.host)
                                    ToolTip.visible: hovered
                                    ToolTip.delay: 400
                                    ToolTip.text: "harlo updates it on " + hostBlock.host + ". Its servers there that are running stop and start again, so a message on them now would be cut off."
                                }
                            }
                        }
                    }
                }
            }
            // Removing a server from another machine: out of its agents
            // there, stopped and forgotten, its model files deleted if you like.
            AppDialog {
                id: removeHostServerDialog
                property string host: ""
                property var row: null
                function ask(h, r) {
                    host = h
                    row = r
                    deleteHostFiles.checked = true
                    open()
                }
                title: row ? "Remove " + row.name + " from " + host + "?" : ""
                width: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)
                footer: AppButtonBox {
                    DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
                    DialogButton { text: "Remove"; DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole }
                }
                onDiscarded: {
                    Accounts.removeHostEngine(host, row.baseUrl, row.name, deleteHostFiles.checked)
                    close()
                }
                ColumnLayout {
                    width: parent.width
                    spacing: 8
                    Hint {
                        color: Theme.foreground
                        text: !removeHostServerDialog.row ? "" : (removeHostServerDialog.row.agentIds.length > 0 ? "harlo takes it out of its agents there, stops it" : "harlo stops it") + " and forgets how to start it."
                    }
                    AppCheckBox { id: deleteHostFiles; text: "Also delete its model files on " + removeHostServerDialog.host }
                    Hint { text: "The engine stays there, for its other servers." }
                }
            }
            // The agents' own files, to read.
            Flow {
                // (Only when an agent has a file to show.)
                visible: srv.agents.length > 0 && Accounts.revision >= 0 && srv.agents.some(a => Accounts.serversFile(a.id) !== "")
                Layout.fillWidth: true
                Layout.topMargin: 18
                spacing: 4
                Label { text: "Config files:"; color: Theme.muted; font.pixelSize: Theme.px(12); height: 26; verticalAlignment: Text.AlignVCenter; rightPadding: 4 }
                Repeater {
                    model: srv.agents
                    FlatButton {
                        required property var modelData
                        readonly property string file: Accounts.revision >= 0 ? Accounts.serversFile(modelData.id) : ""
                        visible: file !== ""
                        text: modelData.label
                        outlined: false
                        accent: Theme.accent
                        font.pixelSize: Theme.px(12)
                        onClicked: root.openFileRequested(file)
                        ToolTip.visible: hovered
                        ToolTip.text: file
                        ToolTip.delay: 400
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
                            detail: modelData === "harlo" ? (found ? "harlo puts its agent there as sessions start"
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
                                    onClicked: root.signInRequested("claude@" + Workspace.instance, "Claude on " + Workspace.instance)
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
                            onClicked: root.signInRequested(hostAccounts.agent, Theme.harnessLabel(hostAgents.open) + " on " + Workspace.instance)
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
                                        onClicked: { providerList.open = prov.modelData.id; root.signInRequested(prov.modelData.id, prov.modelData.label) }
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
                                            visible: acct.st.state === "loading"
                                            text: "Checking…"
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(12)
                                        }
                                    }
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
                                            Label { id: modelName; anchors.centerIn: parent; text: Theme.modelLabel(parent.modelData); color: Theme.muted; font.pixelSize: Theme.px(11) }
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
            SectionHeader {
                title: "Harlo"
                subtitle: harloModels.ids.length === 0 ? "Its model choices fill in once it's signed in (Sign in… above)." : ""
            }
            Group {
                SettingRow {
                    title: "Sandbox commands"
                    detail: "Harlo's commands can change the project and nothing else, and can't see your keys or sign-ins."
                    more: "The system and your home folder are read-only: commands can change the project, /tmp and tool caches, and can't see SSH and GPG keys, credentials, sign-ins or browser profiles. Harlo's file tools keep to the same. A command that needs more asks you before it runs outside the sandbox. Needs bubblewrap (bwrap) on the machine the session runs on; MCP servers and harlo's tools aren't sandboxed. Applies to Harlo sessions started afterwards."
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
                // The language servers Harlo finds here (it doesn't install them).
                SettingRow {
                    id: codeIntel
                    divider: true
                    readonly property var info: Accounts.languageServers === "" ? ({}) : JSON.parse(Accounts.languageServers)
                    title: "Code intelligence"
                    detail: "Harlo asks a language server where code is defined, used and called from."
                    more: "For when a text search can't tell same-named things apart, for far fewer tokens on those questions. It uses the servers installed on this computer: install one for a language you work in, then start a new session."
                        + (codeIntel.info.state === "failed" ? "\n" + codeIntel.info.error : "")
                    icon: "code-box"
                    Component.onCompleted: Accounts.loadLanguageServers()
                    FlatButton {
                        objectName: "harloLanguageServersCheck"
                        text: codeIntel.info.state === "loading" ? "Checking…" : "Check again"
                        enabled: codeIntel.info.state !== "loading"
                        font.pixelSize: Theme.px(12)
                        onClicked: Accounts.loadLanguageServers()
                    }
                }
                ColumnLayout {
                    objectName: "harloLanguageServers"
                    Layout.fillWidth: true
                    Layout.leftMargin: 60
                    Layout.rightMargin: 16
                    Layout.bottomMargin: 14
                    spacing: 10
                    // Copies through a hidden TextEdit (QML has no clipboard API).
                    TextEdit {
                        id: lspCopier
                        visible: false
                        function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" }
                    }
                    Repeater {
                        model: codeIntel.info.list || []
                        // Each language: its name, then where its server is or how to install it.
                        ColumnLayout {
                            id: lsRow
                            required property var modelData
                            readonly property bool ready: modelData.state === "ready"
                            readonly property bool broken: modelData.state === "broken"
                            readonly property bool command: !ready && modelData.install.indexOf("Install ") !== 0
                            Layout.fillWidth: true
                            spacing: 2
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Icon {
                                    name: lsRow.ready ? "checkbox-circle" : "close-circle"
                                    color: lsRow.ready ? Theme.success : lsRow.broken ? Theme.warning : Theme.textMuted
                                    implicitWidth: 14
                                    implicitHeight: 14
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: lsRow.modelData.language + (lsRow.ready ? "" : lsRow.broken ? " · doesn't start" : " · not installed")
                                    color: Theme.text
                                    font.pixelSize: Theme.px(12.5)
                                    elide: Text.ElideRight
                                }
                                // (Hidden, not removed, so every row is as tall.)
                                FlatButton {
                                    opacity: lsRow.command ? 1 : 0
                                    enabled: lsRow.command
                                    text: "Copy"
                                    font.pixelSize: Theme.px(11.5)
                                    onClicked: lspCopier.copyText(lsRow.modelData.install)
                                }
                            }
                            // Why, in its first clause; all of it (and the
                            // program's path) on hover.
                            Label {
                                id: lsWhy
                                objectName: "lspWhy"
                                visible: lsRow.broken
                                Layout.fillWidth: true
                                Layout.leftMargin: 22
                                readonly property string full: lsRow.modelData.path + ": " + lsRow.modelData.why.replace(/\.+$/, "") + "."
                                text: lsRow.modelData.why.split(/[,;:] | because /)[0].replace(/\.+$/, "") + "."
                                color: Theme.warning
                                font.pixelSize: Theme.px(12)
                                elide: Text.ElideRight
                                HoverHandler { id: lsWhyHover }
                                ToolTip.visible: lsWhyHover.hovered
                                ToolTip.text: lsWhy.full
                                ToolTip.delay: 300
                            }
                            Label {
                                Layout.fillWidth: true
                                Layout.leftMargin: 22
                                text: lsRow.ready ? lsRow.modelData.path : lsRow.modelData.install
                                color: Theme.textMuted
                                font.family: lsRow.ready || lsRow.command ? Theme.mono : Theme.uiFont
                                font.pixelSize: Theme.px(12)
                                // (A path shortened in the middle; a command in full.)
                                elide: lsRow.ready ? Text.ElideMiddle : Text.ElideNone
                                wrapMode: lsRow.ready ? Text.NoWrap : Text.Wrap
                                ToolTip.visible: lsHover.hovered && truncated
                                ToolTip.text: text
                                HoverHandler { id: lsHover }
                            }
                        }
                    }
                }
            }
            Rule { }
            SectionHeader { title: "Diagnostics"; subtitle: "For tracking down problems. harlo.log has this run's messages (the last run's is harlo.log.1); crashes leave a crash-….log with a backtrace." }
            Group {
                SettingRow {
                    title: "Log agent traffic"
                    detail: "Writes every message to and from the agents to logs/protocol/."
                    more: "Their error output too, one file per agent process, the last 40 kept. Applies to agents started afterwards; the logs contain your prompts and the agents' answers. Also on with --debug-protocol."
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
