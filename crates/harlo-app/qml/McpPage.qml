import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Settings → MCP: the tool servers your agents (omp, pi, Claude, OpenCode)
// can call, globally or in a project. A grid of servers with their status
// (harlo tests them: the agents don't report one), and a page per server
// to edit it.
ColumnLayout {
    id: page
    /// Settings is narrow.
    property bool compact: false
    /// Sign in from a terminal (Main opens one).
    signal runInTerminal(string command)
    /// Where the server page's action bar floats (Settings' page bottom).
    property Item footerHost: null
    property bool showAllTools: false
    spacing: 0

    // ---- scope: global (this computer's, or the instance host's), or a
    // project of the instance ------------------------------------------------
    readonly property string globalScope: Workspace.instance === "" ? "global" : "host:" + Workspace.instance
    readonly property var scopes: {
        const list = [{ id: page.globalScope, label: Workspace.instance === "" ? "Global (every project)" : "Global on " + Workspace.instance + " (every project there)" }]
        for (const line of Workspace.projectList()) {
            const p = line.split("\t")
            const root = Workspace.projectRoots(Number(p[0]))[0] || ""
            if (root !== "") list.push({ id: root, label: p[1] + (root.startsWith("ssh://") ? "  (" + Remotes.displayPath(root).split(":")[0] + ")" : "") })
        }
        return list
    }
    property string scope: globalScope
    onGlobalScopeChanged: scope = globalScope
    readonly property var listing: Mcp.revision >= 0 ? JSON.parse(Mcp.listJson(scope)) : { servers: [], errors: [], files: {} }
    readonly property var status: Mcp.revision >= 0 ? JSON.parse(Mcp.statusJson(scope)) : ({})
    onScopeChanged: { editing = null; Mcp.testAll(scope) }
    Component.onCompleted: Mcp.testAll(scope)

    // ---- the server being edited (null: the grid) ---------------------------
    /// { oldName, form, harnesses, isNew }
    property var editing: null
    property string formError: ""
    property bool showSecrets: false
    readonly property var emptyForm: ({ name: "", enabled: true, transport: "stdio", command: [], url: "", headers: [], env: [], cwd: "", timeoutSecs: null, exposure: "", description: "" })
    function freeName() {
        let n = "new-server", i = 1
        while (listing.servers.some(s => s.name === n)) n = "new-server-" + i++
        return n
    }
    function openNew(form) {
        const f = Object.assign({}, emptyForm, form || {})
        if (f.name === "" || f.name === "imported") f.name = freeName()
        editing = { oldName: "", form: f, harnesses: page.defaultAgents(), isNew: true }
        formError = ""
        showSecrets = false
    }
    function openServer(s) {
        editing = { oldName: s.name, form: JSON.parse(JSON.stringify(s.form)), harnesses: s.harnesses.slice(), isNew: false, differs: s.differs }
        formError = ""
        showSecrets = false
        showAllTools = false
    }
    function setField(key, value) {
        const e = Object.assign({}, editing)
        e.form = Object.assign({}, e.form)
        e.form[key] = value
        editing = e
    }
    /// The agents that are on (Settings → Providers) and have MCP servers.
    readonly property var agents: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).filter(p => ["omp", "pi", "claude", "opencode", "hermes"].includes(p.id))
    /// A new server is for the installed ones (all of them if none is).
    function defaultAgents() {
        const ids = agents.map(a => a.id), installed = ids.filter(id => Mcp.installedOn(id, page.scope))
        return installed.length > 0 ? installed : ids
    }
    function toggleHarness(h, on) {
        const e = Object.assign({}, editing)
        e.harnesses = on ? e.harnesses.filter(x => x !== h).concat([h]) : e.harnesses.filter(x => x !== h)
        editing = e
    }
    function save() {
        const err = Mcp.save(scope, editing.oldName, JSON.stringify(editing.form), JSON.stringify(editing.harnesses))
        if (err !== "") { formError = err; return false }
        const name = editing.form.name
        editing = null
        Mcp.test(scope, name)
        return true
    }
    function statusText(s, enabled) {
        if (!enabled) return "Disabled"
        if (!s) return ""
        if (s.state === "testing") return "Testing…"
        if (s.state === "connected") return "Connected · " + s.tools.length + (s.tools.length === 1 ? " tool" : " tools")
        if (s.state === "needs_auth") return "Needs sign-in"
        return "Failed"
    }
    function statusColor(s, enabled) {
        if (!enabled || !s || s.state === "testing") return Theme.muted
        return s.state === "connected" ? Theme.success : s.state === "needs_auth" ? Theme.warning : Theme.error
    }

    component Pill_: Rectangle {
        property string text
        property color tint: Theme.muted
        implicitWidth: pillLabel.implicitWidth + 14
        implicitHeight: 20
        radius: 10
        color: Qt.rgba(tint.r, tint.g, tint.b, 0.13)
        Label { id: pillLabel; anchors.centerIn: parent; text: parent.text; color: parent.tint; font.pixelSize: Theme.px(11) }
    }
    component Badge_: Rectangle {
        property string text
        implicitWidth: badgeLabel.implicitWidth + 10
        implicitHeight: 18
        radius: 5
        color: Theme.subtle
        border.color: Theme.border
        Label { id: badgeLabel; anchors.centerIn: parent; text: parent.text; color: Theme.muted; font.pixelSize: Theme.px(10.5) }
    }
    component SectionTitle_: Label {
        color: Theme.foreground
        font.bold: true
        font.pixelSize: Theme.px(13)
        Layout.topMargin: 16
    }
    component Hint_: Label {
        color: Theme.muted
        font.pixelSize: Theme.px(12)
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    /// Key / value rows (environment variables, headers); values hidden.
    component PairsEditor_: ColumnLayout {
        id: pe
        property var rows: []
        property string keyPlaceholder: "KEY"
        property bool upperKeys: false
        signal edited(var rows)
        spacing: 6
        function update(i, k, v) { const r = rows.map(x => x.slice()); r[i] = [k, v]; edited(r) }
        Repeater {
            model: pe.rows
            RowLayout {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                spacing: 6
                AppTextField {
                    Layout.preferredWidth: 180
                    text: modelData[0]
                    placeholderText: pe.keyPlaceholder
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    onEditingFinished: pe.update(index, pe.upperKeys ? text.toUpperCase().replace(/[^A-Z0-9_]/g, "_") : text.trim(), modelData[1])
                }
                AppTextField {
                    Layout.fillWidth: true
                    text: modelData[1]
                    placeholderText: "value"
                    echoMode: page.showSecrets ? TextInput.Normal : TextInput.Password
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    onEditingFinished: pe.update(index, modelData[0], text)
                }
                FlatButton { text: "×"; outlined: false; accent: Theme.muted; onClicked: pe.edited(pe.rows.filter((_, i) => i !== index)) }
            }
        }
    }

    // ---- header -------------------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        Label {
            Layout.fillWidth: true
            text: page.editing === null ? "Tool servers your agents can call. Open one to see its status and settings." : ""
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        ComboField {
            objectName: "mcpScope"
            implicitWidth: 220
            model: page.scopes.map(s => s.label)
            currentIndex: Math.max(0, page.scopes.findIndex(s => s.id === page.scope))
            onActivated: i => page.scope = page.scopes[i].id
        }
        FlatButton {
            objectName: "mcpRefresh"
            visible: page.editing === null
            outlined: false
            implicitWidth: 30
            contentItem: Icon { name: "refresh"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
            onClicked: Mcp.testAll(page.scope)
            ToolTip.visible: hovered
            ToolTip.text: "Test the servers again"
        }
    }
    Hint_ {
        Layout.topMargin: 6
        text: "In " + page.agents.map(a => page.listing.files ? page.listing.files[a.id] : "").filter(x => x).join(", ") + ". Changes apply to new sessions."
              + (!page.scope.startsWith("host:") && page.scope !== "global" && page.agents.some(a => a.id === "pi") ? " pi reads a project's servers only in projects you trust (/trust)." : "")
    }
    Repeater {
        model: page.listing.errors
        Hint_ { required property string modelData; Layout.topMargin: 6; color: Theme.error; text: modelData }
    }

    // ---- grid ----------------------------------------------------------------
    GridLayout {
        visible: page.editing === null
        Layout.fillWidth: true
        Layout.topMargin: 14
        columns: page.compact ? 1 : 2
        columnSpacing: 12
        rowSpacing: 12
        // Add
        Rectangle {
            objectName: "mcpAdd"
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            implicitHeight: 96
            radius: 10
            color: addHover.hovered ? Theme.hover : "transparent"
            border.color: Theme.border
            border.width: 1
            HoverHandler { id: addHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: page.openNew() }
            Column {
                anchors.centerIn: parent
                spacing: 6
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 30; height: 30; radius: 8
                    color: Theme.subtle
                    Icon { anchors.centerIn: parent; name: "add"; color: Theme.foreground; implicitWidth: 16; implicitHeight: 16 }
                }
                Label { anchors.horizontalCenter: parent.horizontalCenter; text: "Add MCP server"; color: Theme.foreground; font.pixelSize: Theme.px(13) }
                Label { anchors.horizontalCenter: parent.horizontalCenter; text: "Local command or remote URL"; color: Theme.muted; font.pixelSize: Theme.px(11.5) }
            }
        }
        Repeater {
            model: page.listing.servers
            Rectangle {
                id: card
                required property var modelData
                readonly property var st: page.status[modelData.name]
                readonly property bool local: modelData.form.transport === "stdio"
                objectName: "mcpServer-" + modelData.name
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                implicitHeight: 96
                radius: 10
                color: cardHover.hovered ? Theme.hover : Theme.elevated
                border.color: Theme.border
                opacity: modelData.form.enabled ? 1 : 0.6
                HoverHandler { id: cardHover; cursorShape: Qt.PointingHandCursor }
                // Under the switch, which takes its own clicks.
                MouseArea { anchors.fill: parent; onClicked: page.openServer(card.modelData) }
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Icon { name: card.local ? "server" : "global"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                        Label { Layout.fillWidth: true; text: card.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(13); font.weight: Font.Medium; elide: Text.ElideRight }
                        Pill_ {
                            objectName: "mcpStatus-" + card.modelData.name
                            visible: text !== ""
                            text: page.statusText(card.st, card.modelData.form.enabled)
                            tint: page.statusColor(card.st, card.modelData.form.enabled)
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        text: card.local ? card.modelData.form.command.join(" ") : card.modelData.form.url
                        color: Theme.muted
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(11.5)
                        elide: Text.ElideRight
                    }
                    Item { Layout.fillHeight: true }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 5
                        Label { text: card.local ? "Local server" : "Remote server"; color: Theme.muted; font.pixelSize: Theme.px(11) }
                        Repeater { model: card.modelData.harnesses; Badge_ { required property string modelData; text: Theme.harnessLabel(modelData) } }
                        Badge_ { visible: card.modelData.differs; text: "copies differ" }
                        Item { Layout.fillWidth: true }
                        AppSwitch {
                            objectName: "mcpToggle-" + card.modelData.name
                            checked: card.modelData.form.enabled
                            onToggled: {
                                const err = Mcp.setEnabled(page.scope, card.modelData.name, checked)
                                if (err !== "") page.formError = err
                            }
                            ToolTip.visible: hovered
                            ToolTip.text: checked ? "On: sessions start it. Click to turn it off." : "Off. Click to turn it on."
                        }
                    }
                }
            }
        }
    }
    Hint_ { visible: page.editing === null && page.listing.servers.length === 0; Layout.topMargin: 10; text: "No MCP servers here yet." }

    // ---- one server ----------------------------------------------------------
    ColumnLayout {
        id: detail
        visible: page.editing !== null
        Layout.fillWidth: true
        Layout.topMargin: 6
        spacing: 6
        readonly property var f: page.editing ? page.editing.form : page.emptyForm
        readonly property var st: page.editing ? page.status[page.editing.oldName] : undefined
        readonly property bool local: f.transport === "stdio"
        readonly property bool hasPi: page.editing !== null && page.editing.harnesses.includes("pi")

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            FlatButton { objectName: "mcpBack"; text: "‹ All servers"; outlined: false; font.pixelSize: Theme.px(12); onClicked: page.editing = null }
            Label {
                Layout.fillWidth: true
                text: page.editing && page.editing.isNew ? "New MCP server" : detail.f.name
                color: Theme.foreground
                font.bold: true
                font.pixelSize: Theme.px(15)
                elide: Text.ElideRight
            }
            Pill_ { visible: text !== "" && page.editing !== null && !page.editing.isNew; text: page.statusText(detail.st, detail.f.enabled); tint: page.statusColor(detail.st, detail.f.enabled) }
            FlatButton { objectName: "mcpImport"; visible: page.editing !== null && page.editing.isNew; text: "Import JSON…"; font.pixelSize: Theme.px(12); onClicked: importDialog.open() }
        }
        Hint_ { visible: page.editing !== null && page.editing.differs === true; color: Theme.warning; text: "The agents' copies of this server differ; saving writes this version to each." }

        SectionTitle_ { text: "Server" }
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            AppTextField {
                objectName: "mcpName"
                Layout.fillWidth: true
                text: detail.f.name
                placeholderText: "my-server"
                onEditingFinished: page.setField("name", text.trim().replace(/[^A-Za-z0-9_-]/g, "-"))
            }
            AppCheckBox { objectName: "mcpEnabled"; text: "Enabled"; checked: detail.f.enabled; onToggled: page.setField("enabled", checked) }
        }
        RowLayout {
            spacing: 14
            Label { text: "For"; color: Theme.muted; font.pixelSize: Theme.px(12) }
            Repeater {
                model: page.agents
                AppCheckBox {
                    required property var modelData
                    objectName: "mcpFor" + ({ omp: "Omp", pi: "Pi", claude: "Claude", opencode: "OpenCode" })[modelData.id]
                    text: modelData.label
                    checked: page.editing !== null && page.editing.harnesses.includes(modelData.id)
                    onToggled: page.toggleHarness(modelData.id, checked)
                }
            }
        }
        Hint_ {
            visible: page.editing !== null && page.editing.harnesses.includes("claude") && !detail.f.enabled
            text: "Claude has no off switch: it keeps the server on."
        }

        SectionTitle_ { text: "How to reach it" }
        Hint_ { text: "Paste the command that starts it, or the address of a hosted server." }
        RowLayout {
            spacing: 6
            Repeater {
                model: [["stdio", "Command"], ["http", "Link"], ["sse", "Link (SSE, not pi)"]]
                FlatButton {
                    required property var modelData
                    objectName: "mcpTransport-" + modelData[0]
                    text: modelData[1]
                    font.pixelSize: Theme.px(12)
                    accent: detail.f.transport === modelData[0] ? Theme.primary : Theme.muted
                    onClicked: page.setField("transport", modelData[0])
                }
            }
        }
        TextArea {
            id: commandArea
            objectName: "mcpCommand"
            visible: detail.local
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(70, contentHeight + 16)
            text: detail.f.command.join("\n")
            placeholderText: "npx\n-y\n@modelcontextprotocol/server-filesystem\n~/Documents"
            placeholderTextColor: Theme.muted
            color: Theme.foreground
            font.family: Theme.mono
            font.pixelSize: Theme.px(12)
            wrapMode: TextEdit.Wrap
            background: Rectangle { radius: 6; color: Theme.background; border.color: commandArea.activeFocus ? Theme.primary : Theme.border }
            function commit() {
                const lines = text.split("\n").map(l => l.trim()).filter(l => l !== "")
                // A whole command on one line: split it.
                const words = lines.length === 1 ? JSON.parse(Mcp.splitCommand(lines[0])) : lines
                if (words.length === 1 && /^https?:\/\//.test(words[0])) { page.setField("url", words[0]); page.setField("transport", "http"); return }
                page.setField("command", words)
            }
            onActiveFocusChanged: if (!activeFocus) commit()
        }
        Hint_ { visible: detail.local; text: "One argument per line; a whole command pasted on one line is split for you. It runs on " + (page.scope.startsWith("ssh://") ? "the project's host." : page.scope.startsWith("host:") ? page.scope.substring(5) + "." : "this computer.") }
        AppTextField {
            objectName: "mcpUrl"
            visible: !detail.local
            Layout.fillWidth: true
            text: detail.f.url
            placeholderText: "https://mcp.example.com/mcp"
            font.family: Theme.mono
            font.pixelSize: Theme.px(12)
            onEditingFinished: page.setField("url", text.trim())
        }

        SectionTitle_ { text: detail.local ? "Environment variables" : "Request headers" }
        RowLayout {
            Layout.fillWidth: true
            Hint_ { text: detail.local ? "Values the server needs, such as an API key. ${VAR} reads one from your environment (OpenCode: {env:VAR}); omp and pi also run !command." : "Such as Authorization: Bearer …" }
            FlatButton { text: page.showSecrets ? "Hide values" : "Show values"; outlined: false; font.pixelSize: Theme.px(12); onClicked: page.showSecrets = !page.showSecrets }
        }
        PairsEditor_ {
            Layout.fillWidth: true
            rows: detail.local ? detail.f.env : detail.f.headers
            upperKeys: detail.local
            keyPlaceholder: detail.local ? "API_KEY" : "Header-Name"
            onEdited: rows => page.setField(detail.local ? "env" : "headers", rows)
        }
        FlatButton {
            objectName: "mcpAddPair"
            text: detail.local ? "+ Add variable" : "+ Add header"
            font.pixelSize: Theme.px(12)
            onClicked: page.setField(detail.local ? "env" : "headers", (detail.local ? detail.f.env : detail.f.headers).concat([["", ""]]))
        }
        Hint_ { text: "Stored as plain text in the config files." }

        SectionTitle_ { text: "Advanced" }
        GridLayout {
            Layout.fillWidth: true
            columns: page.compact ? 1 : 2
            columnSpacing: 12
            rowSpacing: 6
            ColumnLayout {
                Layout.fillWidth: true
                Label { text: "Timeout (seconds)"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                AppTextField {
                    objectName: "mcpTimeout"
                    Layout.fillWidth: true
                    text: detail.f.timeoutSecs === null || detail.f.timeoutSecs === undefined ? "" : String(detail.f.timeoutSecs)
                    placeholderText: "default"
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: page.setField("timeoutSecs", text.trim() === "" ? null : Number(text))
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                visible: detail.local
                Label { text: "Working folder"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                AppTextField {
                    Layout.fillWidth: true
                    text: detail.f.cwd
                    placeholderText: "the session's folder"
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    onEditingFinished: page.setField("cwd", text.trim())
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                visible: detail.hasPi
                Label { text: "How pi shows its tools"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                ComboField {
                    Layout.fillWidth: true
                    readonly property var values: ["", "codemode", "deferred", "direct", "hidden"]
                    model: ["Default (code mode)", "Code mode: called from scripts", "Deferred: found with tool search", "Direct: like built-in tools", "Hidden"]
                    currentIndex: Math.max(0, values.indexOf(detail.f.exposure))
                    onActivated: i => page.setField("exposure", values[i])
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                visible: detail.hasPi
                Label { text: "Description (pi tells the model)"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                AppTextField { Layout.fillWidth: true; text: detail.f.description; onEditingFinished: page.setField("description", text) }
            }
        }

        // Status and tools (saved servers).
        SectionTitle_ { visible: page.editing !== null && !page.editing.isNew; text: "Status" }
        RowLayout {
            visible: page.editing !== null && !page.editing.isNew
            Layout.fillWidth: true
            spacing: 8
            Hint_ {
                text: !detail.st ? "Not tested yet."
                    : detail.st.state === "testing" ? "Testing…"
                    : detail.st.state === "connected" ? "Connected" + (detail.st.server ? " to " + detail.st.server : "") + "."
                    : detail.st.state === "needs_auth" ? "The server wants you to sign in. If you have signed in with pi or omp, sessions still work: harlo's test can't use that sign-in."
                    : detail.st.error
                color: detail.st && detail.st.state === "failed" ? Theme.error : Theme.muted
            }
            FlatButton { objectName: "mcpTest"; text: "Test"; font.pixelSize: Theme.px(12); enabled: !(detail.st && detail.st.state === "testing"); onClicked: Mcp.test(page.scope, page.editing.oldName) }
        }
        RowLayout {
            visible: page.editing !== null && !page.editing.isNew && !detail.local
            spacing: 8
            Label { text: "Sign in:"; color: Theme.muted; font.pixelSize: Theme.px(12) }
            FlatButton { visible: detail.hasPi; text: "with pi"; font.pixelSize: Theme.px(12); onClicked: page.runInTerminal(Mcp.signInCommand("pi", detail.f.name)) }
            FlatButton {
                visible: page.editing !== null && page.editing.harnesses.includes("omp")
                text: "with omp"
                font.pixelSize: Theme.px(12)
                onClicked: page.runInTerminal(Mcp.signInCommand("omp", detail.f.name))
                ToolTip.visible: hovered
                ToolTip.text: "Opens omp in a terminal; type /mcp reauth " + detail.f.name
            }
        }
        Repeater {
            model: detail.st && detail.st.state === "connected" ? (page.showAllTools ? detail.st.tools : detail.st.tools.slice(0, 12)) : []
            RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: 8
                Label { text: modelData.name; color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
                Label { Layout.fillWidth: true; text: modelData.description.split("\n")[0]; color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight }
            }
        }

        FlatButton {
            objectName: "mcpShowAllTools"
            visible: detail.st !== undefined && detail.st.state === "connected" && detail.st.tools.length > 12
            text: page.showAllTools ? "Show fewer" : "Show all " + (detail.st ? detail.st.tools.length : 0) + " tools"
            outlined: false
            font.pixelSize: Theme.px(12)
            onClicked: page.showAllTools = !page.showAllTools
        }

        Hint_ { visible: page.formError !== ""; Layout.topMargin: 10; color: Theme.error; text: page.formError }
        // Room for the floating action bar.
        Item { implicitHeight: 64 }
    }

    // Save, Cancel, Delete: floating at the bottom of the page.
    Rectangle {
        id: actionBar
        objectName: "mcpActionBar"
        parent: page.footerHost
        visible: page.editing !== null
        width: parent ? parent.width : 0
        height: 58
        color: Theme.background
        Rectangle { width: parent.width; height: 1; color: Theme.border }
        RowLayout {
            anchors.verticalCenter: parent.verticalCenter
            x: Math.max(28, (parent.width - width) / 2)
            width: Math.min(parent.width - 56 - 30, 720)
            spacing: 8
            FlatButton {
                objectName: "mcpDelete"
                visible: page.editing !== null && !page.editing.isNew
                text: "Delete"
                accent: Theme.error
                font.pixelSize: Theme.px(12)
                onClicked: confirmDelete.open()
            }
            Label {
                Layout.fillWidth: true
                visible: page.formError !== ""
                text: page.formError
                color: Theme.error
                font.pixelSize: Theme.px(12)
                elide: Text.ElideRight
            }
            Item { Layout.fillWidth: page.formError === "" }
            FlatButton { objectName: "mcpCancel"; text: "Cancel"; font.pixelSize: Theme.px(12); onClicked: page.editing = null }
            FlatButton {
                objectName: "mcpSave"
                text: page.editing && page.editing.isNew ? "Create" : "Save"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                // Fields commit when they lose focus; take it first.
                onClicked: { forceActiveFocus(); Qt.callLater(page.save) }
            }
        }
    }

    AppDialog {
        id: confirmDelete
        title: "Delete " + (page.editing ? page.editing.oldName : "") + "?"
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label { width: 360; wrapMode: Text.Wrap; color: Theme.muted; text: "It's removed from every agent's config here." }
        onDiscarded: {
            const err = Mcp.remove(page.scope, page.editing.oldName)
            if (err !== "") page.formError = err
            else page.editing = null
            close()
        }
    }

    AppDialog {
        id: importDialog
        objectName: "mcpImportDialog"
        title: "Import from JSON"
        standardButtons: Dialog.Cancel
        width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
        property string error: ""
        onAboutToShow: { importText.text = ""; error = "" }
        ColumnLayout {
            width: parent.width
            spacing: 8
            Hint_ { text: "Paste an mcpServers snippet from Claude, Cursor, VS Code, pi or omp (or OpenCode's mcp)." }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 180
                TextArea {
                    id: importText
                    objectName: "mcpImportText"
                    color: Theme.foreground
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    wrapMode: TextEdit.Wrap
                    placeholderText: '{ "mcpServers": { "files": { "command": "npx", "args": ["-y", "…"] } } }'
                    placeholderTextColor: Theme.muted
                    background: Rectangle { radius: 6; color: Theme.background; border.color: Theme.border }
                }
            }
            Hint_ { visible: importDialog.error !== ""; color: Theme.error; text: importDialog.error }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                DialogButton {
                    objectName: "mcpImportGo"
                    text: "Import"
                    accent: true
                    onClicked: {
                        const r = JSON.parse(Mcp.parseImport(importText.text))
                        if (r.error !== "") { importDialog.error = r.error; return }
                        if (r.forms.length === 1) { page.openNew(r.forms[0]); importDialog.close(); return }
                        // Several: each is added for the agents a new one is for.
                        const failed = []
                        for (const f of r.forms) {
                            const err = Mcp.save(page.scope, "", JSON.stringify(f), JSON.stringify(page.defaultAgents()))
                            if (err !== "") failed.push(f.name + ": " + err)
                        }
                        if (failed.length) { importDialog.error = failed.join("\n"); return }
                        page.editing = null
                        Mcp.testAll(page.scope)
                        importDialog.close()
                    }
                }
            }
        }
    }
}
