pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Local models: local and online servers the agents use.
ColumnLayout {
    id: srv
    signal openFileRequested(string path)
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
                else rows.push({ name: sv.name, baseUrl: sv.baseUrl, models: sv.models, place: sv.place, managed: sv.managed, free: !!sv.free, engine: sv.engine || "", contexts: sv.contexts || [], prices: sv.prices || [], agents: [a.label], agentIds: [a.id] })
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
            const p = (row.prices || [])[i]
            // (Ollama says it only while running: harlo writes it down then.)
            return Theme.modelLabel(id) + " · " + (c > 0 ? Math.round(c / 1024) + "k context" : row.managed ? "context set when it starts" : "context not given")
                + (p ? " · $" + srv.rate(p[0]) + " in, $" + srv.rate(p[1]) + " out per million tokens" : "")
        })
        if (lines.length === 0) lines.push("No models yet")
        const where = row.baseUrl.replace(/^https?:\/\//, "").replace(/\/v1\/?$/, "")
        if (row.via) return lines.join("\n") + "\nOn " + row.via + ", through the SSH connection (" + where + " here)"
        return lines.join("\n") + "\n" + where + (row.engine ? " · in agents as “" + row.name + "”" : "")
    }
    // A price as typed: "3", "0.25", "1.10".
    function rate(v) { return Number(v) % 1 === 0 ? String(Number(v)) : String(Number(Number(v).toFixed(4))) }
    function amount(t) { const v = parseFloat(String(t).replace(/[$,\s]/g, "")); return isFinite(v) && v > 0 ? v : 0 }
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
        implicitWidth: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)
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
            FormNote {
                visible: removeServerDialog.finished
                color: Theme.foreground
                text: removeServerDialog.summary
            }
            FormNote {
                visible: removeServerDialog.finished && removeServerDialog.problems !== ""
                color: Theme.error
                text: removeServerDialog.problems
            }
            FormNote {
                visible: !removeServerDialog.finished
                color: Theme.foreground
                text: !removeServerDialog.row ? "" : removeServerDialog.row.agents.length === 0 ? "No agent uses it; Harlo stops it and forgets how to start it."
                    : "Harlo takes it out of " + removeServerDialog.row.agents.join(", ") + (removeServerDialog.row.agents.length === 1 ? "" : " (each one's own file)")
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
            FormNote {
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
    readonly property var serverStates: Accounts.serverStates !== "" ? JSON.parse(Accounts.serverStates) : ({})
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
        more: "They show in the model lists (under Local when they run on this computer or your network). Claude Code only uses Anthropic's models. Harlo starts and stops the servers it set up; they stop when Harlo quits. Tick an agent to add a server to its config, untick it to take it out."
        AddButton {
            objectName: "setupModelServer"
            visible: srv.agents.length > 0
            text: "Set up a local model…"
            onClicked: setupServer.openOn("")
            ToolTip.visible: hovered
            ToolTip.text: "Harlo installs an engine, downloads a model that fits your graphics card, starts it and adds it to your agents."
            ToolTip.delay: 400
        }
        AddButton {
            objectName: "addModelServer"
            visible: srv.agents.length > 0
            text: "Connect…"
            onClicked: modelServerDialog.open()
            ToolTip.visible: hovered
            ToolTip.text: "One you run yourself or your company serves (Ollama, LM Studio, vLLM, a gateway…): its address, and Harlo finds its models."
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
    Group { visible: srv.agents.length === 0; EmptyNote { text: "Local models are for Harlo, which isn't installed." } }
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
                readonly property var st: srv.serverStates[modelData.baseUrl] || ({ state: "", message: "" })
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
                        ToolTip.text: ({ running: serverItem.st.harlo ? "Running (started by Harlo)" : "Running", starting: "Starting…", failed: "Didn't start", stopped: "Stopped" })[serverItem.st.state] || "Checking…"
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
                    // One on this computer or its network (Harlo's):
                    // free when it runs on your own hardware (a
                    // proxy here may pass requests on to a paid
                    // service).
                    AppCheckBox {
                        objectName: "serverFree-" + serverItem.modelData.name
                        visible: serverItem.local && serverItem.modelData.agentIds.includes("harlo")
                        Layout.topMargin: 4
                        text: "Free: it runs on hardware you own"
                        checked: !!serverItem.modelData.free
                        onToggled: {
                            const err = Accounts.setServerFree("harlo", serverItem.modelData.name, checked)
                            if (err !== "") Workspace.showNotice(err, true)
                            Accounts.loadServers()
                        }
                    }
                    // A server's price (Harlo's), when it isn't
                    // free: what its list says, or one you give;
                    // without one, what its sessions cost isn't known.
                    RowLayout {
                        id: priceRow
                        objectName: "serverPrice-" + serverItem.modelData.name
                        visible: (!serverItem.modelData.free || (serverItem.modelData.prices || []).some(p => p)) && serverItem.modelData.agentIds.includes("harlo")
                        Layout.topMargin: 6
                        spacing: 6
                        readonly property var prices: serverItem.modelData.prices || []
                        // (The same for every model: shown to change.)
                        readonly property var shared: prices.length > 0 && prices.every(p => JSON.stringify(p) === JSON.stringify(prices[0])) ? prices[0] : null
                        function save() {
                            const err = Accounts.setServerPrice("harlo", serverItem.modelData.name, srv.amount(priceIn.text), srv.amount(priceOut.text))
                            if (err !== "") Workspace.showNotice(err, true)
                            Accounts.loadServers()
                        }
                        Label { text: "Price per million tokens"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                        AppTextField {
                            id: priceIn
                            objectName: "serverPriceIn-" + serverItem.modelData.name
                            implicitWidth: 76
                            placeholderText: "in"
                            text: priceRow.shared ? srv.rate(priceRow.shared[0]) : ""
                            leftPadding: 22
                            Label { x: 10; anchors.verticalCenter: parent.verticalCenter; text: "$"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            onEditingFinished: if (srv.amount(text) !== (priceRow.shared ? priceRow.shared[0] : 0)) priceRow.save()
                        }
                        AppTextField {
                            id: priceOut
                            objectName: "serverPriceOut-" + serverItem.modelData.name
                            implicitWidth: 76
                            placeholderText: "out"
                            text: priceRow.shared ? srv.rate(priceRow.shared[1]) : ""
                            leftPadding: 22
                            Label { x: 10; anchors.verticalCenter: parent.verticalCenter; text: "$"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            onEditingFinished: if (srv.amount(text) !== (priceRow.shared ? priceRow.shared[1] : 0)) priceRow.save()
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: priceRow.visible
                        text: priceRow.prices.some(p => p) ? (priceRow.shared ? "Harlo counts what its sessions cost at this price." : "Each model at the price its server lists; a price here sets them all.")
                            : "No price: what Harlo's sessions on it cost isn't known, so spending limits can't count it."
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
                                            ? "Its sessions on " + hostRow.modelData + " reach this computer's server through the SSH connection, while Harlo is open."
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
                        onClicked: srv.openFileRequested(log)
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
        EmptyNote { text: "No model servers yet. Set up a local model: Harlo installs an engine, downloads a model that fits your graphics card and starts it. Start and Stop it here or from the left bar." }
    }
    Rule { visible: srv.engines.length > 0 }
    SectionHeader { visible: srv.engines.length > 0; title: "Engines on this computer"; subtitle: "What runs the servers Harlo set up here." }
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
                                : !engineItem.updatable ? engineItem.version + " · installed outside Harlo"
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
                        ToolTip.text: (engineItem.built ? "Harlo builds the new version here (several minutes)" : "Harlo downloads the new version")
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
                FormNote {
                    Layout.fillWidth: true
                    text: hostBlock.e.state === "error" ? "Couldn't read its servers: " + hostBlock.e.error
                        : hostBlock.e.state === "loading" && hostBlock.rows.length === 0 ? "Reading its servers…"
                        : hostBlock.rows.length === 0 ? "Harlo can install an engine on " + hostBlock.host + ", download a model that fits it and run it there, for its agents."
                        : "They run on " + hostBlock.host + " for its agents there, and keep running when Harlo quits."
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
                                        ToolTip.text: "Its sessions on this computer reach the server on " + hostBlock.host + " through the SSH connection; Harlo starts it there first when it's stopped."
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
                                onClicked: srv.openFileRequested("ssh://" + hostBlock.host + hostServer.modelData.log)
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
                            ToolTip.text: "Harlo updates it on " + hostBlock.host + ". Its servers there that are running stop and start again, so a message on them now would be cut off."
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
        implicitWidth: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)
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
            FormNote {
                color: Theme.foreground
                text: !removeHostServerDialog.row ? "" : (removeHostServerDialog.row.agentIds.length > 0 ? "Harlo takes it out of its agents there, stops it" : "Harlo stops it") + " and forgets how to start it."
            }
            AppCheckBox { id: deleteHostFiles; text: "Also delete its model files on " + removeHostServerDialog.host }
            FormNote { text: "The engine stays there, for its other servers." }
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
                onClicked: srv.openFileRequested(file)
                ToolTip.visible: hovered
                ToolTip.text: file
                ToolTip.delay: 400
            }
        }
    }
}
