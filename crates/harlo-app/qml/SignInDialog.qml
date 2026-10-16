import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Signing an agent in to a model provider (Settings → Providers →
// Accounts): pick what to sign in to, then follow the agent's own sign-in
// here: the page to open (harlo opens it), a device code, what it asks for
// (an API key, a code from the page). The agent keeps the credential.
AppDialog {
    id: dlg
    objectName: "signInDialog"
    width: Math.min(520, (Overlay.overlay ? Overlay.overlay.width : 560) - 40)
    title: page === "pick" ? "Sign in · " + agentLabel : (chosen ? chosen.name : "Sign in")
    standardButtons: Dialog.NoButton

    property string agent: ""
    property string agentLabel: ""
    /// "pick", "form" (OpenCode: a key and its fields first) or "flow".
    property string page: "pick"
    property var chosen: null
    property var method: null
    property string search: ""
    property var answers: ({})
    /// Links already opened in the browser (by address).
    property var openedUrls: ({})

    readonly property var providers: dlg.visible && Accounts.revision >= 0 ? JSON.parse(Accounts.providers(agent)) : ({ state: "loading", list: [] })
    readonly property var shown: (providers.list || []).filter(p => search === "" || (p.name + " " + p.id).toLowerCase().indexOf(search.toLowerCase()) >= 0)
    readonly property var flow: Accounts.flow !== "" ? JSON.parse(Accounts.flow) : null
    readonly property bool running: flow !== null && flow.state === "running"

    function openFor(id, label) {
        agent = id
        agentLabel = label
        page = "pick"
        chosen = null
        method = null
        search = ""
        Accounts.loadProviders(id)
        open()
    }
    function begin(provider, m) {
        chosen = provider
        method = m
        answers = ({})
        const fields = (m.form || []).filter(f => !f.hidden)
        if (m.type === "key" || fields.length > 0) { page = "form"; return }
        go({ type: m.type })
    }
    function go(extra) {
        openedUrls = ({})
        page = "flow"
        Accounts.start(agent, chosen.id, method.id, JSON.stringify(Object.assign({ type: method.type }, extra)))
    }
    function formAnswer() {
        const a = {}
        for (const f of (method.form || [])) {
            if (f.type === "external") continue
            const v = answers[f.key] !== undefined ? answers[f.key] : f.default
            if (v !== undefined && v !== "") a[f.key] = v
        }
        return a
    }
    function shows(f) {
        if (f.hidden) return false
        return (f.when || []).every(c => {
            const v = answers[c.key]
            if (v === undefined) return false
            return c.op === "eq" ? String(v) === String(c.value) : String(v) !== String(c.value)
        })
    }
    onClosed: {
        if (running) Accounts.cancel()
        if (flow && flow.state === "done") Workspace.refreshProviders()
    }
    onFlowChanged: {
        if (!flow || !visible) return
        for (const s of flow.steps) {
            if (s.kind === "link" && s.open && !openedUrls[s.url]) {
                openedUrls[s.url] = true
                Qt.openUrlExternally(s.url)
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 10

        // Copies through a hidden TextEdit (QML has no clipboard API).
        TextEdit {
            id: clip
            visible: false
            function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" }
        }

        // ---- what to sign in to -------------------------------------------------
        ColumnLayout {
            visible: dlg.page === "pick"
            Layout.fillWidth: true
            spacing: 8
            AppTextField {
                objectName: "signInSearch"
                visible: (dlg.providers.list || []).length > 6
                Layout.fillWidth: true
                placeholderText: "Search providers"
                text: dlg.search
                onTextEdited: dlg.search = text
            }
            Label {
                visible: dlg.providers.state === "loading"
                text: "Asking " + dlg.agentLabel + "…"
                color: Theme.muted
                font.pixelSize: Theme.px(12.5)
            }
            Label {
                visible: (dlg.providers.error || "") !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dlg.providers.error || ""
                color: Theme.error
                font.pixelSize: Theme.px(12.5)
            }
            ListView {
                id: providerList
                objectName: "signInProviders"
                visible: dlg.providers.state === "ready"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 380)
                clip: true
                spacing: 2
                model: dlg.shown
                ScrollBar.vertical: ScrollBar { policy: providerList.contentHeight > providerList.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                delegate: ColumnLayout {
                    id: row
                    required property var modelData
                    required property int index
                    width: providerList.width - 8
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 8
                        Label {
                            text: row.modelData.name
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13.5)
                            font.weight: Font.Medium
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        Label {
                            visible: row.modelData.connected === true
                            text: "Signed in"
                            color: Theme.success
                            font.pixelSize: Theme.px(11.5)
                        }
                        Repeater {
                            model: row.modelData.methods
                            FlatButton {
                                required property var modelData
                                objectName: "signInMethod-" + row.modelData.id + "-" + modelData.id
                                text: row.modelData.methods.length > 1 ? modelData.label : (modelData.type === "key" ? "Add key" : "Sign in")
                                enabled: modelData.blocked !== true
                                font.pixelSize: Theme.px(11.5)
                                padding: 4
                                onClicked: dlg.begin(row.modelData, modelData)
                            }
                        }
                    }
                    Label {
                        visible: (row.modelData.note || "") !== ""
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: row.modelData.note || ""
                        color: row.modelData.methods.some(m => m.blocked) ? Theme.warning : Theme.muted
                        font.pixelSize: Theme.px(11.5)
                    }
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: 4; implicitHeight: 1; color: Theme.border; visible: row.index < dlg.shown.length - 1 }
                }
            }
            Label {
                visible: dlg.providers.state === "ready" && dlg.shown.length === 0 && (dlg.providers.error || "") === ""
                text: "Nothing matches."
                color: Theme.muted
                font.pixelSize: Theme.px(12.5)
            }
        }

        // ---- a key, and what else the provider needs (OpenCode) ------------------
        ColumnLayout {
            visible: dlg.page === "form" && dlg.method !== null
            Layout.fillWidth: true
            spacing: 8
            Label {
                visible: dlg.method !== null && dlg.method.type === "key"
                text: dlg.method ? dlg.method.label : ""
                color: Theme.foreground
                font.pixelSize: Theme.px(12.5)
            }
            AppTextField {
                id: keyField
                objectName: "signInKey"
                visible: dlg.method !== null && dlg.method.type === "key"
                Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: "Paste the API key"
            }
            Repeater {
                model: dlg.method ? (dlg.method.form || []) : []
                ColumnLayout {
                    id: field
                    required property var modelData
                    visible: dlg.shows(modelData)
                    Layout.fillWidth: true
                    spacing: 4
                    Label {
                        text: (field.modelData.title || field.modelData.key) + (field.modelData.required ? "" : " (optional)")
                        color: Theme.foreground
                        font.pixelSize: Theme.px(12.5)
                    }
                    Label {
                        visible: (field.modelData.description || "") !== ""
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: field.modelData.description || ""
                        color: Theme.muted
                        font.pixelSize: Theme.px(11.5)
                    }
                    FlatButton {
                        visible: field.modelData.type === "external"
                        text: "Open " + (field.modelData.title || "the page")
                        font.pixelSize: Theme.px(12)
                        onClicked: Qt.openUrlExternally(field.modelData.url)
                    }
                    ComboBox {
                        visible: field.modelData.type === "string" && (field.modelData.options || []).length > 0
                        Layout.fillWidth: true
                        model: (field.modelData.options || []).map(o => o.label || o.value)
                        onActivated: i => { dlg.answers[field.modelData.key] = field.modelData.options[i].value; dlg.answersChanged() }
                        Component.onCompleted: if ((field.modelData.options || []).length > 0 && dlg.answers[field.modelData.key] === undefined) {
                            dlg.answers[field.modelData.key] = field.modelData.default || field.modelData.options[0].value
                            dlg.answersChanged()
                        }
                    }
                    AppTextField {
                        visible: (field.modelData.type === "string" && (field.modelData.options || []).length === 0) || field.modelData.type === "number" || field.modelData.type === "integer"
                        Layout.fillWidth: true
                        placeholderText: field.modelData.placeholder || ""
                        text: field.modelData.default !== undefined ? String(field.modelData.default) : ""
                        onTextEdited: {
                            dlg.answers[field.modelData.key] = field.modelData.type === "string" ? text : Number(text)
                            dlg.answersChanged()
                        }
                    }
                    AppSwitch {
                        visible: field.modelData.type === "boolean"
                        checked: field.modelData.default === true
                        onToggled: { dlg.answers[field.modelData.key] = checked; dlg.answersChanged() }
                    }
                }
            }
        }

        // ---- the sign-in going on -----------------------------------------------
        ColumnLayout {
            visible: dlg.page === "flow" && dlg.flow !== null
            Layout.fillWidth: true
            spacing: 8
            Repeater {
                objectName: "signInSteps"
                model: dlg.flow ? dlg.flow.steps : []
                ColumnLayout {
                    id: step
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 4
                    Label {
                        visible: step.modelData.kind === "text" || ((step.modelData.text || "") !== "" && step.modelData.kind === "link")
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: step.modelData.text || ""
                        color: step.modelData.kind === "text" ? Theme.foreground : Theme.muted
                        font.pixelSize: Theme.px(12.5)
                    }
                    RowLayout {
                        visible: step.modelData.kind === "link"
                        Layout.fillWidth: true
                        spacing: 6
                        FlatButton {
                            objectName: "signInOpen"
                            text: "Open in browser"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            onClicked: Qt.openUrlExternally(step.modelData.url)
                        }
                        FlatButton {
                            text: "Copy link"
                            font.pixelSize: Theme.px(12)
                            onClicked: clip.copyText(step.modelData.url)
                        }
                        Label {
                            Layout.fillWidth: true
                            text: step.modelData.url || ""
                            elide: Text.ElideMiddle
                            color: Theme.muted
                            font.family: Theme.mono
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    // A device code: typed on the provider's page.
                    ColumnLayout {
                        visible: step.modelData.kind === "device"
                        Layout.fillWidth: true
                        spacing: 6
                        Label { text: "Enter this code on the page:"; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                        RowLayout {
                            spacing: 10
                            Label {
                                objectName: "signInDeviceCode"
                                text: step.modelData.code || ""
                                color: Theme.foreground
                                font.family: Theme.mono
                                font.pixelSize: Theme.px(22)
                                font.bold: true
                            }
                            FlatButton { text: "Copy"; font.pixelSize: Theme.px(12); onClicked: clip.copyText(step.modelData.code) }
                            FlatButton {
                                text: "Open the page"
                                accent: Theme.primary
                                font.pixelSize: Theme.px(12)
                                onClicked: { clip.copyText(step.modelData.code); Qt.openUrlExternally(step.modelData.url) }
                            }
                        }
                    }
                }
            }
            // What it asks for.
            ColumnLayout {
                visible: dlg.flow !== null && dlg.flow.ask !== null && dlg.flow.ask !== undefined
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 6
                readonly property var ask: dlg.flow && dlg.flow.ask ? dlg.flow.ask : ({})
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: (parent.ask.message || "") + (parent.ask.optional ? " (only if the page shows one)" : "")
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12.5)
                }
                RowLayout {
                    visible: parent.ask.kind !== "select"
                    Layout.fillWidth: true
                    spacing: 6
                    AppTextField {
                        id: askField
                        objectName: "signInAnswer"
                        Layout.fillWidth: true
                        echoMode: parent.parent.ask.kind === "secret" ? TextInput.Password : TextInput.Normal
                        placeholderText: parent.parent.ask.placeholder || ""
                        onAccepted: sendAnswer.clicked()
                    }
                    FlatButton {
                        id: sendAnswer
                        objectName: "signInSend"
                        text: "Continue"
                        accent: Theme.primary
                        enabled: askField.text.trim() !== ""
                        font.pixelSize: Theme.px(12)
                        onClicked: { Accounts.answer(askField.text); askField.text = "" }
                    }
                }
                Flow {
                    visible: parent.ask.kind === "select"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: parent.parent.ask.options || []
                        FlatButton { required property var modelData; text: modelData.label; font.pixelSize: Theme.px(12); onClicked: Accounts.answer(modelData.id) }
                    }
                }
            }
            RowLayout {
                visible: dlg.running
                spacing: 8
                BusyIndicator { implicitWidth: 18; implicitHeight: 18; running: dlg.running }
                Label { text: "Waiting for the sign-in…"; color: Theme.muted; font.pixelSize: Theme.px(12) }
            }
            Label {
                objectName: "signInOutcome"
                visible: dlg.flow !== null && dlg.flow.state !== "running"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: !dlg.flow ? "" : dlg.flow.state === "done" ? "✓ " + (dlg.flow.message || "Signed in.")
                    : dlg.flow.state === "cancelled" ? "Cancelled." : (dlg.flow.message || "It didn't work.")
                color: !dlg.flow ? Theme.muted : dlg.flow.state === "done" ? Theme.success : dlg.flow.state === "failed" ? Theme.error : Theme.muted
                font.pixelSize: Theme.px(13)
            }
        }
    }

    footer: Item {
        implicitHeight: buttons.implicitHeight + 26
        implicitWidth: buttons.implicitWidth + 40
        Row {
            id: buttons
            anchors.right: parent.right
            anchors.rightMargin: 20
            anchors.top: parent.top
            anchors.topMargin: 8
            spacing: 8
            DialogButton {
                visible: dlg.page !== "pick" && !dlg.running
                text: "Back"
                onClicked: { dlg.page = "pick"; Accounts.loadProviders(dlg.agent) }
            }
            DialogButton {
                visible: dlg.page === "form"
                objectName: "signInSave"
                text: dlg.method && dlg.method.type === "key" ? "Save key" : "Continue"
                accent: true
                enabled: dlg.method !== null && (dlg.method.type !== "key" || keyField.text.trim() !== "")
                onClicked: { dlg.go({ key: keyField.text.trim(), answer: dlg.formAnswer() }); keyField.text = "" }
            }
            DialogButton {
                objectName: "signInClose"
                text: dlg.running ? "Cancel" : dlg.flow && dlg.flow.state === "done" && dlg.page === "flow" ? "Done" : "Close"
                accent: dlg.flow !== null && dlg.flow.state === "done" && dlg.page === "flow"
                onClicked: { if (dlg.running) Accounts.cancel(); else dlg.close() }
            }
        }
    }
}
