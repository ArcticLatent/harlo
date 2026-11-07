import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Adds your own model server to the agents chosen (Settings → Model
// servers): any OpenAI-compatible server. Find models asks it what it
// serves; the models picked go into each chosen agent's own config.
AppDialog {
    id: dlg
    objectName: "modelServerDialog"
    /// The agents it can go to: Harlo, pi and Oh My Pi, where installed.
    readonly property var agents: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : [])
        .filter(p => ["pi", "omp", "harlo"].includes(p.id) && p.state !== "missing")
    /// The ones chosen (ids).
    property var chosen: []
    /// A server was added.
    signal added()
    readonly property var search: Accounts.serverSearch !== "" ? JSON.parse(Accounts.serverSearch) : ({ state: "", models: [] })
    readonly property bool found: search.state === "ready"
    property var picked: ({})
    property string error: ""
    readonly property int pickedCount: search.models.filter(m => picked[m.id] !== false).length

    function findModels() {
        error = ""
        Accounts.findModels(address.text, key.text)
    }
    function add() {
        const models = search.models.filter(m => picked[m.id] !== false)
        for (const id of chosen) {
            const a = agents.find(x => x.id === id)
            // Provider names it has already.
            const taken = [...new Set((a.modelIds || []).map(m => m.split(/[\/:]/)[0]).filter(p => p !== ""))]
            const err = Accounts.addServer(id, name.text, search.baseUrl, key.text, JSON.stringify(models), JSON.stringify(taken))
            if (err !== "") { error = (chosen.length > 1 ? a.label + ": " : "") + err; return }
        }
        close()
        added()
    }
    function contextText(n) { return n >= 1000 ? Math.round(n / 1000) + "k context" : n + " context" }

    title: "Add a model server"
    standardButtons: Dialog.Cancel
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
    onAboutToShow: {
        address.text = ""
        key.text = ""
        name.text = ""
        picked = ({})
        error = ""
        chosen = agents.map(a => a.id)
        Accounts.findModels("", "")
        address.forceActiveFocus()
    }
    Connections {
        target: Accounts
        function onServerSearchChanged() {
            if (dlg.search.state === "ready") {
                dlg.picked = ({})
                name.text = dlg.search.name
            }
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: 6
        Note { text: "Any OpenAI-compatible server: Ollama, LM Studio, llama.cpp, vLLM, SGLang, or a company gateway. Its models show in harlo's model lists (under Local when the server is on this computer or your network) and in the agents themselves." }
        FieldName { text: "Address" }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            AppTextField {
                id: address
                objectName: "modelServerAddress"
                Layout.fillWidth: true
                placeholderText: "http://localhost:11434"
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                onAccepted: dlg.findModels()
            }
            FlatButton {
                objectName: "modelServerFind"
                text: dlg.search.state === "finding" ? "Finding…" : "Find models"
                enabled: address.text.trim() !== "" && dlg.search.state !== "finding"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                onClicked: dlg.findModels()
            }
        }
        FieldName { text: "Key" }
        AppTextField {
            id: key
            objectName: "modelServerKey"
            Layout.fillWidth: true
            placeholderText: "Only if the server wants one: the key, or $NAME to read it from the environment"
            echoMode: text.startsWith("$") ? TextInput.Normal : TextInput.Password
            font.pixelSize: Theme.px(12)
            onAccepted: dlg.findModels()
        }
        Note {
            visible: dlg.search.state === "failed"
            color: Theme.error
            text: dlg.search.error || ""
        }
        // What it serves.
        ColumnLayout {
            visible: dlg.found
            Layout.fillWidth: true
            spacing: 6
            FieldName { text: dlg.search.models.length === 0 ? "Models" : "Models (" + dlg.pickedCount + " of " + dlg.search.models.length + ")" }
            Note { visible: dlg.search.models.length === 0; text: "The server answered but lists no models. Load or pull one there, then find models again." }
            ListView {
                objectName: "modelServerModels"
                visible: dlg.search.models.length > 0
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 220)
                clip: true
                model: dlg.search.models
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                delegate: RowLayout {
                    required property var modelData
                    width: ListView.view.width
                    spacing: 8
                    AppCheckBox {
                        Layout.fillWidth: true
                        text: modelData.id
                        checked: dlg.picked[modelData.id] !== false
                        onToggled: {
                            const p = Object.assign({}, dlg.picked)
                            p[modelData.id] = checked
                            dlg.picked = p
                        }
                    }
                    Label {
                        visible: !!modelData.context
                        text: modelData.context ? dlg.contextText(modelData.context) : ""
                        color: Theme.muted
                        font.pixelSize: Theme.px(11.5)
                    }
                }
            }
            FieldName { text: "Name" }
            AppTextField {
                id: name
                objectName: "modelServerName"
                Layout.fillWidth: true
                placeholderText: "ollama"
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
            }
            FieldName { text: "For" }
            Row {
                spacing: 14
                Repeater {
                    model: dlg.agents
                    AppCheckBox {
                        required property var modelData
                        text: modelData.label
                        checked: dlg.chosen.includes(modelData.id)
                        onToggled: dlg.chosen = checked ? dlg.chosen.concat([modelData.id]) : dlg.chosen.filter(x => x !== modelData.id)
                    }
                }
            }
            Note { text: "Models show as " + (name.text.trim() || "name") + "/<model>, saved in each agent's own config." }
            Note { visible: dlg.error !== ""; color: Theme.error; text: dlg.error }
            RowLayout {
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                FlatButton {
                    objectName: "modelServerAdd"
                    text: dlg.pickedCount === 1 ? "Add 1 model" : "Add " + dlg.pickedCount + " models"
                    enabled: dlg.pickedCount > 0 && name.text.trim() !== "" && dlg.chosen.length > 0
                    accent: Theme.primary
                    font.pixelSize: Theme.px(12)
                    onClicked: dlg.add()
                }
            }
        }
    }

    component Note: Label { color: Theme.muted; font.pixelSize: Theme.px(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
    component FieldName: Label { color: Theme.foreground; font.pixelSize: Theme.px(13); Layout.topMargin: 8 }
}
