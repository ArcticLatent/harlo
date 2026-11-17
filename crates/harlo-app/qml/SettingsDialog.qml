pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
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
    /// Straight to signing `agent` in to `provider` (OpenRouter).
    signal providerSignInRequested(string agent, string provider, string name)
    /// Harlo signing in to an MCP server of `scope`.
    signal mcpSignInRequested(string scope, string server)
    /// MCP sign-in: run this in a terminal.
    signal runInTerminalRequested(string command)
    /// A file to open in harlo's editor (an agent's config, a log).
    signal openFileRequested(string path)
    // Settings → Shortcuts' "Show every key".
    signal shortcutsSheetRequested()
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
        { id: "projects", title: "Projects", icon: "folder-3", group: "workspace", keywords: "project icon colour color name harness setup remove wiki openwiki docs plan steps commit review" },
        { id: "wiki", title: "Wiki", icon: "book-open", group: "workspace", keywords: "wiki openwiki writer model agent thinking create update continue cheaper cheap cost tokens spend" },
        { id: "worktrees", title: "Worktrees", icon: "git-branch", group: "workspace", keywords: "worktree folder location branch settled shelf finished merged closed pull request quiet archive" },
        { id: "git", title: "Git", icon: "git-repository", group: "workspace", keywords: "undo snapshot turn revert rollback github gitlab forgejo codeberg gitea hosting sign in login token credentials changes count repository hidden sidebar uncommitted commit message model ai" },
        { id: "workflows", title: "Workflows", icon: "list-check-2", group: "workspace", keywords: "workflow workflows implement verify check checks tests review reviewer stages run runs pipeline lint clippy security retry tries fixes" },
        { id: "schedules", title: "Schedules", icon: "history", group: "workspace", keywords: "schedule scheduled prompt cron timer daily weekday every hour morning automatic run" },
        { id: "mcp", title: "MCP", icon: "node-tree", group: "agents", keywords: "mcp tool servers model context protocol tools stdio http oauth" },
        { id: "orchestration", title: "Orchestration", icon: "ai-agent", group: "agents", keywords: "orchestration handoff advisor committee second opinion delegate subagent agents start sessions mcp tools" },
        { id: "remote", title: "Remote Instances", icon: "server", group: "agents", keywords: "remote ssh server host machine nas instance connect" },
        { id: "about", title: "About Harlo", icon: "information", group: "about", keywords: "about version release licence license mit website source code github changelog what's new report problem issue bug logs" },
        { id: "providers", title: "Providers", icon: "robot-2", group: "agents", keywords: "providers harnesses harlo claude binary path program agent models version enable disable debug log logs protocol crash diagnostics sandbox bubblewrap sign in login log in account accounts api key oauth credentials" },
        { id: "lsp", title: "LSP", icon: "code-box", group: "agents", keywords: "lsp code intelligence language server servers definition references calls rust-analyzer pyright gopls clangd typescript" },
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

    FolderDialog {
        id: worktreeDialog
        title: "Folder for new worktrees"
        onAccepted: Workspace.setSetting("worktree_dir", decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, "")))
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
                    color: Theme.scrim
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
                            verticalPadding: 0  // (the style's 12 px would push the content low)
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
                                        // (The style's 12 px above and below would leave
                                        // the icon and name a 12 px band, and they'd sit
                                        // low on the highlight.)
                                        verticalPadding: 0
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
                        anchors.fill: parent
                        sourceComponent: root.page === "usage" ? usagePage : scrolledPage
                        // (Built while Settings is open.)
                        active: root.visible
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
    function pageWidth(w) { return Math.min(w - pageX(w) - 58, 720) }
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
            color: "transparent"
            columnX: root.pageX(width)
            // (As wide as the other pages, so the edge stays put between them.)
            columnWidth: root.pageWidth(width)
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
                        subtitle: "Today so far, since midnight: $" + (spendLimits.spend.today || 0).toFixed(2) + " in harlo's sessions. Past a limit, harlo stops the run until you allow more."
                        more: "Costs are what the agents report: Claude Code at the end of each turn, Harlo after each request. Harlo on a ChatGPT plan or on a model server marked free (it runs on your own hardware) costs nothing extra. On a model with no price its cost isn't known, so the limits can't count it: give the server a price in Model servers. Empty: no limit."
                    }
                    // (Sessions open now whose costs the limits can't count.)
                    FormNote {
                        objectName: "spendUnpriced"
                        visible: (spendLimits.spend.unpriced || 0) > 0
                        Layout.bottomMargin: 8
                        text: (spendLimits.spend.unpriced === 1 ? "One session open now runs" : spendLimits.spend.unpriced + " sessions open now run")
                            + " on a model with no price, so the limits don't count what it costs. Give its server a price in Model servers, or mark it free there if it runs on your own hardware."
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
                SectionHeader {
                    objectName: "usageActivity"
                    title: "Activity"
                    // (Its total is the period's, not today's, which the
                    // spending limits count.)
                    subtitle: "Every session over the period you pick, read from their files; the spending limits above count today only."
                    Layout.bottomMargin: 6
                }
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
                // Room for the close button on the right.
                width: root.pageWidth(pageFlick.width)
                spacing: 0
                PageTitle { }
                Loader {
                    Layout.fillWidth: true
                    sourceComponent: ({
                        general: generalPage, appearance: appearancePage, notifications: notificationsPage, voice: voicePage, devices: devicesPage,
                        shortcuts: shortcutsPage, projects: projectsPage, wiki: wikiPage, worktrees: worktreesPage, git: gitPage, schedules: schedulesPage, workflows: workflowsPage, providers: providersPage, lsp: lspPage, servers: serversPage, remote: remotePage, mcp: mcpPage, orchestration: orchestrationPage,
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
        GeneralPage {
            onSetupRequested: root.setupRequested()
            onTrayToggled: on => root.trayToggled(on)
            onPageRequested: pageId => root.page = pageId
        }
    }

    Component {
        id: appearancePage
        AppearancePage {
            compact: root.compact
            onPromptRailToggled: on => root.promptRailToggled(on)
            onRowRequested: (page, text) => root.seekTo(page, text)
        }
    }

    Component {
        id: notificationsPage
        NotificationsPage {
            compact: root.compact
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
                            ChoicePill {
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
            FormNote {
                visible: Dictation.downloadError !== ""
                Layout.topMargin: 8
                color: Theme.error
                text: Dictation.downloadError
            }
            FormNote { Layout.topMargin: 8; text: "Models come from huggingface.co/ggerganov/whisper.cpp and are kept in " + Dictation.modelsDir() + "." }
        }
    }

    Component {
        id: shortcutsPage
        ShortcutSettings {
            shortcuts: root.shortcuts
            onSheetRequested: root.shortcutsSheetRequested()
        }
    }

    Component {
        id: schedulesPage
        SchedulesPage { }
    }

    Component {
        id: workflowsPage
        WorkflowsPage { }
    }

    Component {
        id: wikiPage
        ColumnLayout {
            id: wikiWriters
            spacing: 0
            property int rev: 0
            SectionHeader {
                title: "Creating a wiki"
                subtitle: "Reads the whole repository and plans every page: the biggest job, where a stronger model pays off."
                more: "Create, and Continue on a first run that stopped, is a session of this agent spending its model's tokens. Unset, a project's own agent and model write it. On another machine, the project's model writes it (the one chosen here may not be there). Turning a project's wiki on or off is in Projects."
            }
            WikiWriter { op: "create"; objectPrefix: "wikiCreate"; rev: wikiWriters.rev; onSaved: wikiWriters.rev++ }
            SectionHeader {
                Layout.topMargin: 28
                title: "Updating it"
                subtitle: "Rewrites only the pages whose cited code changed: focused work a cheaper or local model does well."
                more: "Update first checks, with no model, whether any fact's code changed or files were added; when none did, harlo records the update itself and no session starts. Otherwise a session of this agent updates the pages that need it."
            }
            WikiWriter { op: "update"; objectPrefix: "wikiWriter"; rev: wikiWriters.rev; onSaved: wikiWriters.rev++ }
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
            FormNote { Layout.topMargin: 6; text: "Empty uses harlo's data folder." }
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
                FormNote {
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
        ProjectsPage {
            projectId: root.projectId
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
        RemotePage {
            compact: root.compact
            onRunInTerminalRequested: command => root.runInTerminalRequested(command)
            onSignInRequested: (agent, label) => root.signInRequested(agent, label)
        }
    }

    // Your own model servers, for Harlo (its providers.json).
    Component {
        id: serversPage
        ServersPage {
            onOpenFileRequested: path => root.openFileRequested(path)
        }
    }

    Component {
        id: lspPage
        ColumnLayout {
            spacing: 0
            SectionHeader {
                title: "Language servers"
                subtitle: "The ones installed on this computer; harlo finds them, it doesn't install them."
            }
            Group {
                // The language servers found here (it doesn't install them).
                SettingRow {
                    id: codeIntel
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
        }
    }

    Component {
        id: providersPage
        ProvidersPage {
            onSignInRequested: (agent, label) => root.signInRequested(agent, label)
            onProviderSignInRequested: (agent, provider, name) => root.providerSignInRequested(agent, provider, name)
        }
    }
}
