import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Starts a workflow in a project: one agent implements the task in a new
// worktree, then harlo runs the workflow's checks there and other agents
// review the diff against the acceptance criteria; failures go back to the
// same implementation session.
AppDialog {
    id: dialog
    property int projectId: -1
    property var providers: []
    property var flows: []
    readonly property var flow: flows[flowBox.currentIndex] || null
    signal editRequested()
    // Test hooks.
    property alias taskArea: taskArea
    property alias criteriaArea: criteriaArea
    property alias testField: testField
    property alias implementerBox: implementerBox
    property alias reviewerBox: reviewerBox
    property alias implementerModelBox: implementerModelBox
    property alias reviewerModelBox: reviewerModelBox
    property alias approvalBox: approvalBox
    property alias flowBox: flowBox
    readonly property var approvalModes: ["always-ask", "write", "yolo"]
    property alias problem: problem
    // The project's machine ("": this one), and what its agents offer
    // there (by agent, asked when one is picked).
    property string host: ""
    property var hostModels: ({})
    readonly property string implementerAgent: providers[implementerBox.currentIndex] ? providers[implementerBox.currentIndex].id : ""
    readonly property string reviewerAgent: providers[reviewerBox.currentIndex] ? providers[reviewerBox.currentIndex].id : ""
    readonly property var implementerModels: modelsFor(implementerAgent)
    readonly property var reviewerModels: modelsFor(reviewerAgent)

    /// An agent's models, this computer's and the local network's first
    /// (a host's: once it has answered).
    function modelsFor(agent) {
        let all = []
        if (agent === "") return all
        if (host !== "") {
            const got = hostModels[agent]
            all = got && got.models ? got.models : []
        } else {
            const p = (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === agent)
            all = p ? (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 })) : []
        }
        return all.filter(m => m.local).concat(all.filter(m => !m.local))
    }
    function askHost(agent) {
        if (host !== "" && agent !== "" && hostModels[agent] === undefined)
            Workspace.requestHostModels(projectId, agent)
    }
    // A role's agent and model, as last started in this project.
    function remembered(role) {
        const v = Workspace.setting("workflow_" + role + "_" + projectId)
        const tab = v.indexOf("\t")
        return tab < 0 ? { agent: v, model: "" } : { agent: v.slice(0, tab), model: v.slice(tab + 1) }
    }
    /// A model menu's lines: the agent's default, then its models (while a
    /// host is still asked, a line saying so).
    function modelNames(agent, models) {
        const waiting = host !== "" && agent !== "" && hostModels[agent] === undefined
        return [waiting ? qsTr("Reading models…") : qsTr("Default model")].concat(models.map(m => Theme.modelLabel(m.name)))
    }
    function pickModel(box, models, model) {
        box.currentIndex = Math.max(0, models.findIndex(m => m.id === model) + 1)
    }

    // A workflow that names its implementation's agent (and model) starts
    // with them chosen.
    function useFlowAgent() {
        const own = flow && flow.stages.length > 0 ? flow.stages[0].agent : null
        if (!own || own.harness === "" || !providers.some(p => p.id === own.harness)) return
        implementerBox.currentIndex = providers.findIndex(p => p.id === own.harness)
        pendingModels = Object.assign({}, pendingModels, { implementer: own.model })
        askHost(own.harness)
        Qt.callLater(pickImplementerModel)
    }

    function show(id, name, harness) {
        projectId = id
        title = qsTr("Run a workflow in %1").arg(name)
        const project = JSON.parse(Workspace.projectsJson()).find(p => p.id === id)
        host = project && project.host ? project.host : ""
        hostModels = ({})
        providers = JSON.parse(Workspace.chatProvidersJson())
        flows = JSON.parse(Workspace.workflowDefinitionsJson())
        const last = Workspace.workflowChoice(id)
        flowBox.currentIndex = Math.max(0, flows.findIndex(f => f.id === last))
        const index = agent => Math.max(0, providers.findIndex(p => p.id === agent))
        const has = agent => agent !== "" && providers.some(p => p.id === agent)
        const impl = remembered("implementer"), rev = remembered("reviewer")
        implementerBox.currentIndex = index(has(impl.agent) ? impl.agent : has("harlo") ? "harlo" : harness)
        reviewerBox.currentIndex = index(has(rev.agent) ? rev.agent : has("claude") ? "claude" : harness)
        pendingModels = { implementer: has(impl.agent) ? impl.model : "", reviewer: has(rev.agent) ? rev.model : "" }
        useFlowAgent()
        Qt.callLater(pickImplementerModel)
        Qt.callLater(pickReviewerModel)
        // (Read when first needed, here as in a new session's menu.)
        if (host === "")
            Workspace.ensureProviders()
        askHost(implementerAgent)
        askHost(reviewerAgent)
        testField.text = Workspace.workflowTestCommand(id)
        approvalBox.currentIndex = 1
        problem.text = ""
        open()
        taskArea.forceActiveFocus()
    }

    // The remembered models, until their agent's list is there to pick
    // them from (a host's comes later).
    property var pendingModels: ({})
    readonly property string implementerModel: implementerModelBox.currentIndex > 0 && implementerModels[implementerModelBox.currentIndex - 1] ? implementerModels[implementerModelBox.currentIndex - 1].id : ""
    readonly property string reviewerModel: reviewerModelBox.currentIndex > 0 && reviewerModels[reviewerModelBox.currentIndex - 1] ? reviewerModels[reviewerModelBox.currentIndex - 1].id : ""
    // (Once the menu has its new lines: a new model resets its choice.)
    function pickImplementerModel() { pickModel(implementerModelBox, implementerModels, pendingModels.implementer || "") }
    function pickReviewerModel() { pickModel(reviewerModelBox, reviewerModels, pendingModels.reviewer || "") }
    onImplementerModelsChanged: Qt.callLater(pickImplementerModel)
    onReviewerModelsChanged: Qt.callLater(pickReviewerModel)

    Connections {
        target: Workspace
        function onHostModelsReady(id, harness, json) {
            if (id !== dialog.projectId) return
            const next = Object.assign({}, dialog.hostModels)
            next[harness] = JSON.parse(json)
            dialog.hostModels = next
        }
    }

    function start() {
        const reviewing = !flow || flow.needsReviewer
        const err = Workspace.startWorkflow(projectId, JSON.stringify({
            workflow: flow ? flow.id : "", task: taskArea.text, criteria: criteriaArea.text, testCommand: testField.text,
            implementer: implementerAgent, implementerModel: implementerModel,
            reviewer: reviewerAgent, reviewerModel: reviewing ? reviewerModel : "",
            approval: approvalModes[approvalBox.currentIndex]
        }))
        problem.text = err
        if (err === "") {
            Workspace.setSetting("workflow_implementer_" + projectId, implementerAgent + "\t" + implementerModel)
            if (reviewing)
                Workspace.setSetting("workflow_reviewer_" + projectId, reviewerAgent + "\t" + reviewerModel)
            close()
        }
    }

    objectName: "workflowDialog"
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 520
    footer: AppButtonBox {
        DialogButton {
            objectName: "workflowStart"
            text: qsTr("Start")
            accent: true
            enabled: taskArea.text.trim() !== "" && (!dialog.flow || !dialog.flow.needsTestCommand || testField.text.trim() !== "")
            onClicked: dialog.start()
        }
        DialogButton { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
    }

    ColumnLayout {
        width: parent.width
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            ComboField {
                id: flowBox
                objectName: "workflowPick"
                Layout.fillWidth: true
                model: dialog.flows.map(f => f.name)
                onActivated: dialog.useFlowAgent()
            }
            FlatButton { objectName: "workflowEditList"; text: qsTr("Edit workflows…"); font.pixelSize: Theme.px(12); onClicked: { dialog.close(); dialog.editRequested() } }
        }
        // Its stages, in order.
        PlainLabel {
            objectName: "workflowPickStages"
            Layout.fillWidth: true
            text: dialog.flow ? dialog.flow.stages.map(s => s.name).join("  →  ") : ""
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }

        FormLabel { text: qsTr("Task"); Layout.topMargin: 4 }
        TextArea {
            id: taskArea
            Layout.fillWidth: true
            Layout.preferredHeight: 96
            placeholderText: qsTr("What to implement")
            placeholderTextColor: Theme.muted
            wrapMode: TextEdit.Wrap
            color: Theme.foreground
            selectionColor: Theme.selection
            background: Rectangle { radius: 6; color: Theme.background; border.color: taskArea.activeFocus ? Theme.primary : Theme.border }
            KeyNavigation.tab: criteriaArea
        }

        FormLabel { text: qsTr("Acceptance criteria") }
        TextArea {
            id: criteriaArea
            Layout.fillWidth: true
            Layout.preferredHeight: 72
            placeholderText: qsTr("What the reviewer checks, one per line (optional)")
            placeholderTextColor: Theme.muted
            wrapMode: TextEdit.Wrap
            color: Theme.foreground
            selectionColor: Theme.selection
            background: Rectangle { radius: 6; color: Theme.background; border.color: criteriaArea.activeFocus ? Theme.primary : Theme.border }
            KeyNavigation.tab: testField
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.topMargin: 4
            columns: 2
            columnSpacing: 10
            rowSpacing: 8
            Label { visible: testField.visible; text: qsTr("Tests") }
            // (Left: back to its start, so a long command shows where it begins.)
            AppTextField { id: testField; objectName: "workflowTests"; visible: !dialog.flow || dialog.flow.needsTestCommand; Layout.fillWidth: true; font.family: Theme.mono; placeholderText: qsTr("The command that runs them, e.g. cargo test"); onActiveFocusChanged: if (!activeFocus) cursorPosition = 0; onTextChanged: if (!activeFocus) cursorPosition = 0 }
            Label { text: qsTr("Agent") }
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                ComboField {
                    id: implementerBox
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    model: dialog.providers.map(p => p.label)
                    marks: dialog.providers.map(p => p.id)
                    onActivated: { dialog.pendingModels = Object.assign({}, dialog.pendingModels, { implementer: "" }); dialog.askHost(dialog.implementerAgent) }
                }
                ComboField {
                    id: implementerModelBox
                    objectName: "workflowImplementerModel"
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    model: dialog.modelNames(dialog.implementerAgent, dialog.implementerModels)
                    onActivated: dialog.pendingModels = Object.assign({}, dialog.pendingModels, { implementer: dialog.implementerModel })
                }
            }
            Label { text: qsTr("Permissions") }
            ComboField { id: approvalBox; Layout.fillWidth: true; model: [qsTr("Ask before every change"), qsTr("Edit files freely, ask before commands"), qsTr("Run anything without asking")] }
            Label { visible: reviewerRow.visible; text: qsTr("Reviewer") }
            RowLayout {
                id: reviewerRow
                visible: !dialog.flow || dialog.flow.needsReviewer
                Layout.fillWidth: true
                spacing: 6
                ComboField {
                    id: reviewerBox
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    model: dialog.providers.map(p => p.label)
                    marks: dialog.providers.map(p => p.id)
                    onActivated: { dialog.pendingModels = Object.assign({}, dialog.pendingModels, { reviewer: "" }); dialog.askHost(dialog.reviewerAgent) }
                }
                ComboField {
                    id: reviewerModelBox
                    objectName: "workflowReviewerModel"
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    model: dialog.modelNames(dialog.reviewerAgent, dialog.reviewerModels)
                    onActivated: dialog.pendingModels = Object.assign({}, dialog.pendingModels, { reviewer: dialog.reviewerModel })
                }
            }
        }

        MoreText {
            objectName: "workflowNote"
            Layout.fillWidth: true
            full: qsTr("It works in a worktree of its own; what fails goes back to the agent until every check and review passes.")
            more: qsTr("After each attempt Harlo runs the checks in the worktree and the reviews read the diff. A failure, or a review asking for changes, goes back to the same session to fix, and each fix goes through every check and review again.")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
        }
        PlainLabel {
            id: problem
            objectName: "workflowProblem"
            Layout.fillWidth: true
            visible: text !== ""
            color: Theme.error
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
    }
}
