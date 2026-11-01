import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Sets up a model server (Settings → Model servers): an engine that suits
// this computer, a model, then harlo installs, starts and tests it, and adds
// it to the agents chosen.
AppDialog {
    id: dlg
    objectName: "setupServerDialog"
    signal added()

    readonly property var cat: ServerSetup.catalog !== "" ? JSON.parse(ServerSetup.catalog) : ({ state: "loading", engines: [] })
    readonly property var job: ServerSetup.job !== "" ? JSON.parse(ServerSetup.job) : null
    readonly property var gg: ServerSetup.ggufs !== "" ? JSON.parse(ServerSetup.ggufs) : ({ state: "", files: [] })
    readonly property var hm: ServerSetup.hfModels !== "" ? JSON.parse(ServerSetup.hfModels) : ({ state: "", models: [] })
    readonly property var agents: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : [])
        .filter(p => ["pi", "omp", "opencode", "hermes"].includes(p.id) && p.state !== "missing")

    // "engine" → "model" → "run".
    property string page: "engine"
    property var engine: null
    property bool useInstalled: true
    property string model: ""
    property real modelSize: 0
    /// The Hugging Face repository picked (Ollama, llama.cpp), and its file.
    property string repo: ""
    property string file: ""
    /// llama.cpp's context (tokens; 0: as much as fits) and the model's own.
    property int context: 0
    property int maxContext: 0
    readonly property var contexts: maxContext === 0 ? [0, 16384, 32768, 65536, 131072]
        : [0].concat([16384, 32768, 65536, 131072, 262144, 524288].filter(c => c < maxContext), [maxContext])
    readonly property bool fromHf: engine !== null && (engine.id === "ollama" || engine.id === "llamacpp" || engine.id === "vllm")
    /// vLLM: a repository as it is (no GGUF sizes to pick from).
    readonly property bool wholeRepo: engine !== null && engine.id === "vllm"
    /// The files Ollama can take: one file each, not in a folder.
    readonly property var files: gg.repo === repo ? (gg.files || []).filter(f => !(engine && engine.id === "ollama" && f.split)) : []
    property var chosen: []
    property string error: ""
    readonly property real memoryGb: cat.hardware ? (Math.max(0, ...(cat.hardware.gpus || []).map(g => g.vramMb), 0) + cat.hardware.ramMb) / 1024 : 0
    readonly property real vramGb: cat.hardware ? Math.max(0, ...(cat.hardware.gpus || []).map(g => g.vramMb), 0) / 1024 : 0

    /// The server's name in the agents: its engine, and its port when that
    /// isn't the engine's usual one (two of an engine side by side).
    property bool nameEdited: false
    function defaultName() {
        if (!engine) return ""
        const usual = engine.id === "ollama" ? 11434 : engine.id === "vllm" ? 8000 : 8080
        const port = parseInt(portField.text)
        return engine.id + (port && port !== usual ? "-" + port : "")
    }
    function gb(bytes) { return (bytes / 1e9).toFixed(bytes < 10e9 ? 1 : 0) + " GB" }
    function count(n) { return n >= 1e9 ? +(n / 1e9).toFixed(n < 10e9 ? 1 : 0) + "B" : n >= 1e6 ? +(n / 1e6).toFixed(n < 10e6 ? 1 : 0) + "M" : n >= 1e3 ? Math.round(n / 1e3) + "k" : "" + n }
    /// Tokens, as models count them: 128k, 1M.
    function ctxText(n) { return n >= 1048576 ? +(n / 1048576).toFixed(1) + "M" : Math.round(n / 1024) + "k" }
    function fitText(fit) { return fit === "gpu" ? "Fits your graphics card" : fit === "ram" ? "Partly in RAM (slower)" : fit === "no" ? "Too big for this computer" : "" }
    function fitColor(fit) { return fit === "gpu" ? Theme.success : fit === "ram" ? Theme.warning : Theme.muted }
    function chooseRepo(m) {
        repo = m.repo
        maxContext = m.context || 0
        if (!contexts.includes(context)) context = 0
        chooseFile(null)
        // (vLLM: the repository is the model; its size, its weights.)
        if (wholeRepo) {
            file = m.repo
            modelSize = m.q4Size || 0
            model = m.repo
            return
        }
        ServerSetup.listGgufs(m.repo)
    }
    function chooseFile(f) {
        file = f ? f.file : ""
        modelSize = f ? f.size : 0
        model = !f ? "" : engine.id === "ollama" ? "hf.co/" + repo + ":" + f.quant : f.file
    }
    function pick(e) {
        engine = e
        useInstalled = e.installed !== null
        repo = ""
        maxContext = 0
        context = 0
        chooseFile(null)
        error = ""
        modelSearch.text = ""
        if (e.id === "ollama" || e.id === "llamacpp" || e.id === "vllm") ServerSetup.findModels("", e.id)
        portField.text = ServerSetup.freePort(e.id === "ollama" ? 11434 : e.id === "vllm" ? 8000 : 8080)
        nameEdited = false
        nameField.text = defaultName()
        page = "model"
    }
    /// Adding a model to a server set up before (an Ollama row's "Add a
    /// model…"): its row; its engine, address and name as they are.
    property var addTo: null
    function addModel(row) {
        addTo = row
        open()
    }
    function go() {
        error = ""
        ServerSetup.start(JSON.stringify({
            engine: engine.id, useInstalled: addTo !== null || (useInstalled && engine.installed !== null), model: model, repo: repo, size: modelSize,
            port: parseInt(portField.text), context: context, name: nameField.text.trim() || engine.id, addModel: addTo !== null,
        }))
        page = "run"
    }
    /// Oh My Pi finds Ollama on its usual port by itself ("ollama", with all
    /// its models): nothing to add there.
    readonly property bool ompFindsIt: !!job && !!job.result && engine !== null && engine.id === "ollama" && /:11434\/v1$/.test(job.result.baseUrl)
    /// Hermes refuses models with under 64k tokens of context.
    readonly property bool hermesFits: !job || !job.result || job.result.models.length === 0 || job.result.models.some(m => !m.context || m.context >= 64000)
    function addToAgents() {
        const r = job.result
        for (const id of chosen.filter(x => (x !== "hermes" || hermesFits) && !(x === "omp" && ompFindsIt))) {
            const a = agents.find(x => x.id === id)
            const taken = [...new Set((a.modelIds || []).map(m => m.split(/[\/:]/)[0]).filter(p => p !== ""))]
            const err = Accounts.addServer(id, r.name, r.baseUrl, "", JSON.stringify(r.models), JSON.stringify(taken))
            if (err !== "" && err !== "OPENCODE_BUSY") { error = (chosen.length > 1 ? a.label + ": " : "") + err; return }
        }
        close()
        added()
    }

    title: addTo !== null ? "Add a model to " + (engine ? engine.name : "the server") : page === "engine" ? "Set up a model server" : engine ? "Set up " + engine.name : ""
    standardButtons: Dialog.NoButton
    width: Math.min(640, (Overlay.overlay ? Overlay.overlay.width : 700) - 40)
    onAboutToShow: {
        page = (job && job.state === "running") ? "run" : "engine"
        // Adding a model: straight to the models, for the agents that use it.
        if (page === "engine" && addTo !== null) {
            pick(addTo.engine === "llama.cpp" ? { id: "llamacpp", name: "llama.cpp", installed: {} } : { id: "ollama", name: "Ollama", installed: {} })
            portField.text = addTo.baseUrl.replace(/^https?:\/\/[^:\/]+:?/, "").replace(/\/.*$/, "")
            nameEdited = true
            nameField.text = addTo.name
            chosen = addTo.agentIds.slice()
            return
        }
        if (page === "engine") ServerSetup.refresh()
        chosen = agents.map(a => a.id)
    }
    onClosed: if (!job || job.state !== "running") addTo = null
    Connections {
        target: ServerSetup
        // The file that suits this computer, picked to start with.
        function onGgufsChanged() {
            if (dlg.gg.state !== "ready" || dlg.gg.repo !== dlg.repo || dlg.file !== "") return
            const pick = dlg.engine && dlg.engine.id === "ollama" ? dlg.gg.pickWhole : dlg.gg.pick
            const f = dlg.files.find(x => x.file === pick)
            if (f) dlg.chooseFile(f)
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: 8

        // ---- 1. the engine ----
        ColumnLayout {
            visible: dlg.page === "engine"
            Layout.fillWidth: true
            spacing: 8
            Note { text: dlg.cat.hardware ? "This computer: " + dlg.cat.hardware.summary : "Looking at this computer…" }
            Repeater {
                model: dlg.cat.engines || []
                ItemDelegate {
                    id: card
                    required property var modelData
                    objectName: "engine-" + modelData.id
                    Layout.fillWidth: true
                    enabled: modelData.fit !== "no"
                    hoverEnabled: true
                    padding: 12
                    background: Rectangle { radius: 10; color: card.hovered ? Theme.hover : Theme.raised; border.color: Theme.hairline }
                    contentItem: ColumnLayout {
                        spacing: 3
                        RowLayout {
                            spacing: 8
                            Label { text: card.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(14); font.weight: Font.DemiBold }
                            Label {
                                text: card.modelData.fit === "ok" ? "Runs here" : card.modelData.fit === "warn" ? "Runs, with a catch" : "Doesn't run here"
                                color: card.modelData.fit === "ok" ? Theme.success : card.modelData.fit === "warn" ? Theme.warning : Theme.muted
                                font.pixelSize: Theme.px(11.5)
                            }
                            Item { Layout.fillWidth: true }
                            Label {
                                visible: card.modelData.installed !== null
                                text: card.modelData.installed ? "Installed" + (card.modelData.installed.version ? " · " + card.modelData.installed.version : "") : ""
                                color: Theme.muted
                                font.pixelSize: Theme.px(11.5)
                            }
                        }
                        Label { Layout.fillWidth: true; text: card.modelData.tagline; color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap }
                        Label { Layout.fillWidth: true; visible: text !== ""; text: card.modelData.why; color: card.modelData.fit === "no" ? Theme.muted : Theme.warning; font.pixelSize: Theme.px(11.5); wrapMode: Text.Wrap }
                    }
                    onClicked: dlg.pick(modelData)
                }
            }
            Note { visible: dlg.cat.state === "loading" && (dlg.cat.engines || []).length === 0; text: "Checking the graphics card and what's installed…" }
            Note { text: "More engines (NInfer, Basalt, Avalon, vLLM, LM Studio, Unsloth) come in later updates." }
        }

        // ---- 2. the model and settings ----
        ColumnLayout {
            visible: dlg.page === "model" && dlg.engine !== null
            Layout.fillWidth: true
            spacing: 6
            // (Its version; where it is, on hover: a path is too long here.)
            AppCheckBox {
                visible: dlg.addTo === null && dlg.engine !== null && dlg.engine.installed !== null
                Layout.fillWidth: true
                Layout.maximumWidth: parent.width
                text: dlg.engine && dlg.engine.installed ? "Use the installed one" + (dlg.engine.installed.version ? " (" + dlg.engine.installed.version + ")" : "") : ""
                checked: dlg.useInstalled
                onToggled: dlg.useInstalled = checked
                ToolTip.visible: hovered && !!dlg.engine && !!dlg.engine.installed
                ToolTip.text: dlg.engine && dlg.engine.installed ? dlg.engine.installed.program || "" : ""
                ToolTip.delay: 400
            }
            Note { visible: dlg.addTo === null && dlg.engine !== null && (dlg.engine.installed === null || !dlg.useInstalled)
                text: dlg.wholeRepo ? "harlo installs uv (checked against its published checksum), then a Python of its own with vLLM and PyTorch for your card, in harlo's folder: several GB, a few minutes."
                    : "harlo downloads the latest release for this computer's card, checks it against its published checksum and keeps it in its own folder." }

            // Ollama and llama.cpp: a model from Hugging Face, then its size.
            ColumnLayout {
                visible: dlg.fromHf
                Layout.fillWidth: true
                spacing: 6
                FieldName { text: "Model" }
                AppTextField {
                    id: modelSearch
                    objectName: "setupModelSearch"
                    Layout.fillWidth: true
                    placeholderText: "Search Hugging Face, or pick a popular one"
                    font.pixelSize: Theme.px(12)
                    onTextEdited: searchTimer.restart()
                    onAccepted: { searchTimer.stop(); ServerSetup.findModels(text, dlg.engine.id) }
                    Timer { id: searchTimer; interval: 400; onTriggered: ServerSetup.findModels(modelSearch.text, dlg.engine.id) }
                }
                ListView {
                    id: modelList
                    objectName: "setupModels"
                    visible: dlg.hm.models.length > 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.min(contentHeight, 210)
                    clip: true
                    model: dlg.hm.models
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { }
                    delegate: ItemDelegate {
                        id: mRow
                        required property var modelData
                        width: ListView.view.width
                        padding: 6
                        enabled: modelData.fit !== "no"
                        highlighted: dlg.repo === modelData.repo
                        background: Rectangle { radius: 6; color: mRow.highlighted ? Theme.rowSelected : mRow.hovered ? Theme.hover : "transparent" }
                        contentItem: RowLayout {
                            spacing: 8
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 1
                                Label { Layout.fillWidth: true; text: mRow.modelData.name; color: mRow.enabled ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(12.5); font.weight: Font.Medium; elide: Text.ElideRight }
                                Label {
                                    Layout.fillWidth: true
                                    text: [mRow.modelData.repo.split("/")[0],
                                           mRow.modelData.params ? dlg.count(mRow.modelData.params) + " parameters" : "",
                                           mRow.modelData.context ? dlg.ctxText(mRow.modelData.context) + " context" : "",
                                           dlg.count(mRow.modelData.downloads) + " downloads"].filter(x => x !== "").join(" · ")
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(11)
                                    elide: Text.ElideRight
                                }
                            }
                            Label {
                                visible: mRow.modelData.fit !== ""
                                text: dlg.fitText(mRow.modelData.fit) + (mRow.modelData.fit === "no" ? "" : " · ~" + dlg.gb(mRow.modelData.q4Size))
                                color: dlg.fitColor(mRow.modelData.fit)
                                font.pixelSize: Theme.px(11)
                            }
                        }
                        onClicked: dlg.chooseRepo(modelData)
                    }
                }
                Note {
                    visible: text !== ""
                    color: dlg.hm.state === "failed" ? Theme.error : Theme.muted
                    text: dlg.hm.state === "failed" ? dlg.hm.error
                        : dlg.hm.state === "finding" && dlg.hm.models.length === 0 ? "Finding models…"
                        : dlg.hm.state === "ready" && dlg.hm.models.length === 0 ? "No GGUF models match “" + dlg.hm.search + "”."
                        : ""
                }

                // Its sizes (quantizations): smaller is faster and less exact.
                FieldName { visible: dlg.repo !== "" && !dlg.wholeRepo; Layout.topMargin: 4; text: "Size" }
                Note { visible: dlg.repo !== "" && !dlg.wholeRepo && dlg.gg.state === "listing"; text: "Listing its files…" }
                Note { visible: dlg.repo !== "" && dlg.gg.repo === dlg.repo && dlg.gg.state === "failed"; color: Theme.error; text: dlg.gg.error || "" }
                Note {
                    visible: dlg.repo !== "" && dlg.gg.repo === dlg.repo && dlg.gg.state === "ready" && dlg.files.length === 0
                    text: (dlg.gg.files || []).length > 0 ? "Its files come in parts, which Ollama can't load: pick another model, or use llama.cpp." : "It has no GGUF files."
                }
                Flow {
                    visible: dlg.files.length > 0
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: dlg.files
                        ItemDelegate {
                            id: qChip
                            required property var modelData
                            objectName: "quant-" + modelData.quant
                            padding: 6
                            horizontalPadding: 10
                            enabled: modelData.fit !== "no"
                            highlighted: dlg.file === modelData.file
                            background: Rectangle {
                                radius: 6
                                color: qChip.highlighted ? Theme.rowSelected : qChip.hovered ? Theme.hover : "transparent"
                                border.color: qChip.highlighted ? Theme.primary : Theme.hairline
                            }
                            contentItem: Row {
                                spacing: 6
                                Label { text: qChip.modelData.quant; color: qChip.enabled ? Theme.foreground : Theme.muted; font.family: Theme.mono; font.pixelSize: Theme.px(11.5) }
                                Label { text: dlg.gb(qChip.modelData.size); color: dlg.fitColor(qChip.modelData.fit); font.pixelSize: Theme.px(11.5) }
                            }
                            ToolTip.visible: hovered
                            ToolTip.text: dlg.fitText(modelData.fit)
                            onClicked: dlg.chooseFile(modelData)
                        }
                    }
                }
                Note {
                    visible: dlg.file !== "" && !dlg.wholeRepo
                    readonly property var f: dlg.files.find(x => x.file === dlg.file)
                    text: f ? dlg.fitText(f.fit) + ". Q4 and up are close to the full model; Q2 and Q3 lose some quality." : ""
                }
                // vLLM: what the repository holds, and what it needs.
                Note {
                    visible: dlg.wholeRepo
                    text: dlg.repo === "" ? "Models in Hugging Face's own format: full, or quantized (AWQ, GPTQ, FP8: smaller, nearly as good). vLLM keeps the whole model on your card, with room for its context."
                        : "About " + dlg.gb(dlg.modelSize) + " to download. The first start takes a minute or two while vLLM prepares it for your card."
                }
            }

            // llama.cpp and Ollama: how much it reads at once (each llama.cpp
            // model its own; Ollama's is the server's, not asked when adding).
            ColumnLayout {
                visible: dlg.fromHf && !(dlg.addTo !== null && dlg.engine.id === "ollama")
                Layout.fillWidth: true
                spacing: 6
                FieldName { Layout.topMargin: 4; text: "Context" }
                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: dlg.contexts
                        ItemDelegate {
                            id: cChip
                            required property int modelData
                            objectName: "context-" + modelData
                            padding: 6
                            horizontalPadding: 10
                            highlighted: dlg.context === modelData
                            background: Rectangle {
                                radius: 6
                                color: cChip.highlighted ? Theme.rowSelected : cChip.hovered ? Theme.hover : "transparent"
                                border.color: cChip.highlighted ? Theme.primary : Theme.hairline
                            }
                            contentItem: Label {
                                text: cChip.modelData === 0 ? "As much as fits" : dlg.ctxText(cChip.modelData) + (cChip.modelData === dlg.maxContext ? " (all of it)" : "")
                                color: Theme.foreground
                                font.pixelSize: Theme.px(11.5)
                            }
                            onClicked: dlg.context = modelData
                        }
                    }
                }
                Note {
                    text: dlg.context === 0
                        ? "How much of the conversation and files the model keeps in mind, in tokens. " + (dlg.wholeRepo
                            ? "vLLM gives it as much as fits on your card next to the model" + (dlg.maxContext ? ", up to its own " + dlg.ctxText(dlg.maxContext) : ", up to the model's own") + "."
                            : dlg.engine && dlg.engine.id === "ollama"
                            ? "Ollama picks it from your graphics memory (32k on cards with 24 GB or more), up to the model's own" + (dlg.maxContext ? " " + dlg.ctxText(dlg.maxContext) : "") + "."
                            : "llama.cpp gives it as much as fits on your card next to the model" + (dlg.maxContext ? ", up to its own " + dlg.ctxText(dlg.maxContext) : "") + ", and not less than 16k.")
                        : dlg.wholeRepo ? "How much of the conversation and files the model keeps in mind, in tokens. vLLM keeps it all on your card with the model: if they don't fit together, it says so when it starts."
                        : "How much of the conversation and files the model keeps in mind, in tokens. If the model and " + dlg.ctxText(dlg.context) + " don't fit on your card together, " + (dlg.engine && dlg.engine.id === "ollama" ? "Ollama" : "llama.cpp") + " keeps part of the model in RAM, which is slower."
                            + (dlg.engine && dlg.engine.id === "ollama" ? " (It's Ollama's for every model it serves.)" : "")
                }
            }

            // Strata: Qwen3.8-Flash-Next's sizes.
            ColumnLayout {
                visible: dlg.engine !== null && dlg.engine.id === "strata"
                Layout.fillWidth: true
                spacing: 4
                FieldName { text: "Qwen3.8-Flash-Next, in the size that fits" }
                Repeater {
                    model: dlg.cat.strataModels || []
                    ItemDelegate {
                        id: sRow
                        required property var modelData
                        readonly property bool fits: modelData.needGb <= dlg.memoryGb
                        Layout.fillWidth: true
                        padding: 6
                        enabled: fits
                        highlighted: dlg.model === modelData.id
                        background: Rectangle { radius: 6; color: sRow.highlighted ? Theme.rowSelected : sRow.hovered ? Theme.hover : "transparent" }
                        contentItem: RowLayout {
                            spacing: 8
                            Label { text: sRow.modelData.id.toUpperCase(); color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(12); Layout.preferredWidth: 80 }
                            Label { Layout.fillWidth: true; text: sRow.modelData.what; color: Theme.muted; font.pixelSize: Theme.px(12) }
                            Label { text: "needs " + sRow.modelData.needGb + " GB" + (sRow.fits ? "" : " (too big here)"); color: sRow.fits ? Theme.muted : Theme.error; font.pixelSize: Theme.px(11.5) }
                        }
                        onClicked: dlg.model = modelData.id
                    }
                }
                Note { text: "RAM and graphics memory together; Strata downloads the model the first time it starts (tens of GB), and tunes it for your card." }
            }

            Note {
                visible: dlg.addTo !== null
                text: !dlg.addTo ? "" : dlg.engine && dlg.engine.id === "llamacpp"
                    ? "It joins the models of this llama.cpp server (at " + dlg.addTo.baseUrl + "). The server keeps one model in graphics memory at a time and switches when a session asks for another (that takes a few seconds)."
                    : "It joins the models this Ollama serves (at " + dlg.addTo.baseUrl + "); its context is the server's own setting."
            }
            RowLayout {
                visible: dlg.addTo === null
                Layout.topMargin: 6
                spacing: 8
                FieldName { text: "Name" }
                AppTextField { id: nameField; Layout.preferredWidth: 180; placeholderText: "my-server"; font.family: Theme.mono; font.pixelSize: Theme.px(12); onTextEdited: dlg.nameEdited = true }
                FieldName { text: "Port" }
                AppTextField {
                    id: portField
                    Layout.preferredWidth: 80
                    font.pixelSize: Theme.px(12)
                    validator: IntValidator { bottom: 1024; top: 65535 }
                    onTextChanged: if (!dlg.nameEdited) nameField.text = dlg.defaultName()
                }
            }
            Note { visible: dlg.addTo === null; text: "On this computer only (127.0.0.1)." }
            RowLayout {
                Layout.topMargin: 6
                FlatButton { visible: dlg.addTo === null; text: "Back"; outlined: false; font.pixelSize: Theme.px(12); onClicked: dlg.page = "engine" }
                Item { Layout.fillWidth: true }
                FlatButton { text: "Cancel"; outlined: false; accent: Theme.muted; font.pixelSize: Theme.px(12); onClicked: dlg.close() }
                FlatButton {
                    objectName: "setupGo"
                    text: dlg.addTo !== null ? "Download and add" : dlg.engine && dlg.engine.installed !== null && dlg.useInstalled ? "Start it" : "Install and start"
                    enabled: dlg.model !== "" && portField.text !== ""
                    accent: Theme.primary
                    font.pixelSize: Theme.px(12)
                    onClicked: dlg.go()
                }
            }
        }

        // ---- 3. installing, starting, testing; then the agents ----
        ColumnLayout {
            visible: dlg.page === "run" && dlg.job !== null
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: !dlg.job ? "" : dlg.job.state === "done" ? "It works." : dlg.job.state === "failed" ? "It didn't work." : dlg.job.state === "cancelled" ? "Cancelled." : dlg.job.step
                color: dlg.job && dlg.job.state === "failed" ? Theme.error : Theme.foreground
                font.pixelSize: Theme.px(13.5)
                font.weight: Font.Medium
                wrapMode: Text.Wrap
            }
            AppProgressBar {
                visible: dlg.job !== null && dlg.job.state === "running"
                Layout.fillWidth: true
                Layout.topMargin: 2
                indeterminate: !dlg.job || dlg.job.progress < 0
                value: dlg.job && dlg.job.progress >= 0 ? dlg.job.progress : 0
            }
            RowLayout {
                visible: detailText.text !== "" || percent.visible
                Layout.fillWidth: true
                spacing: 8
                Label {
                    id: detailText
                    Layout.fillWidth: true
                    text: dlg.job ? (dlg.job.detail || "") : ""
                    color: dlg.job && dlg.job.state === "failed" ? Theme.error : Theme.textMuted
                    font.pixelSize: Theme.px(12)
                    wrapMode: Text.Wrap
                }
                Label {
                    id: percent
                    visible: dlg.job !== null && dlg.job.state === "running" && dlg.job.progress >= 0
                    text: dlg.job ? Math.floor(dlg.job.progress * 100) + "%" : ""
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12)
                    font.weight: Font.DemiBold
                    font.features: { "tnum": 1 }
                }
            }
            // Done: the test, then the agents.
            ColumnLayout {
                visible: dlg.job !== null && dlg.job.state === "done"
                Layout.fillWidth: true
                spacing: 6
                Note {
                    readonly property var r: dlg.job && dlg.job.result ? dlg.job.result : ({})
                    text: r.baseUrl ? "It answered “" + (r.answer || "") + "” at " + (r.speed || 0).toFixed(0) + " tokens/s" + (r.context ? ", with " + dlg.ctxText(r.context) + " tokens of context" : "") + ". It runs at " + r.baseUrl + "." : ""
                    color: Theme.foreground
                }
                FieldName { text: "Use it in" }
                Row {
                    spacing: 14
                    Repeater {
                        model: dlg.agents
                        AppCheckBox {
                            required property var modelData
                            text: modelData.label
                            readonly property bool builtIn: modelData.id === "omp" && dlg.ompFindsIt
                            enabled: !builtIn && (modelData.id !== "hermes" || dlg.hermesFits)
                            checked: builtIn || (enabled && dlg.chosen.includes(modelData.id))
                            ToolTip.visible: hovered && builtIn
                            ToolTip.text: "Oh My Pi finds Ollama by itself, with all its models"
                            ToolTip.delay: 300
                            onToggled: dlg.chosen = checked ? dlg.chosen.concat([modelData.id]) : dlg.chosen.filter(x => x !== modelData.id)
                        }
                    }
                }
                Note {
                    visible: !dlg.hermesFits && dlg.agents.some(a => a.id === "hermes")
                    text: dlg.hermesFits ? "" : "Not Hermes: it needs at least 64k tokens of context, and this serves " + dlg.ctxText(Math.max(...dlg.job.result.models.map(m => m.context || 0))) + "."
                }
                Note { visible: dlg.ompFindsIt && dlg.agents.some(a => a.id === "omp"); text: "Oh My Pi finds Ollama on this port by itself (as “ollama”), with all its models: nothing to add there." }
                Note { visible: dlg.error !== ""; color: Theme.error; text: dlg.error }
            }
            RowLayout {
                Layout.topMargin: 6
                FlatButton { visible: dlg.job !== null && dlg.job.state !== "running" && dlg.job.state !== "done"; text: "Back"; outlined: false; font.pixelSize: Theme.px(12); onClicked: dlg.page = "model" }
                Item { Layout.fillWidth: true }
                FlatButton { visible: dlg.job !== null && dlg.job.state === "running"; text: "Cancel setup"; accent: Theme.error; font.pixelSize: Theme.px(12); onClicked: ServerSetup.cancel() }
                FlatButton { visible: dlg.job !== null && dlg.job.state === "running"; text: "Hide"; outlined: false; accent: Theme.muted; font.pixelSize: Theme.px(12); onClicked: dlg.close() }
                FlatButton { visible: dlg.job !== null && dlg.job.state !== "running"; text: "Close"; outlined: false; accent: Theme.muted; font.pixelSize: Theme.px(12); onClicked: dlg.close() }
                FlatButton {
                    objectName: "setupAdd"
                    visible: dlg.job !== null && dlg.job.state === "done"
                    text: "Add to the agents"
                    enabled: dlg.chosen.some(x => (x !== "hermes" || dlg.hermesFits) && !(x === "omp" && dlg.ompFindsIt))
                    accent: Theme.primary
                    font.pixelSize: Theme.px(12)
                    onClicked: dlg.addToAgents()
                }
            }
        }
    }

    component Note: Label { color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
    component FieldName: Label { color: Theme.foreground; font.pixelSize: Theme.px(13) }
}
