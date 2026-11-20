import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// The form that starts a workflow in a project: one agent implements the
// task in a new worktree, then harlo runs the workflow's checks there and
// other agents review the diff against the acceptance criteria; failures go
// back to the same implementation session. Shown in a dialog (a project's
// menu) and on the New workflow page.
ColumnLayout {
    id: form
    property int projectId: -1
    property var providers: []
    property var flows: []
    readonly property var flow: flows[flowBox.currentIndex] || null
    signal editRequested()
    // A run started (Start).
    signal started()
    /// "Run a workflow in <project>", for the dialog's title.
    property string title: ""
    /// What Start needs: a task, and a repository (or leave to make one).
    readonly property bool canStart: taskArea.text.trim() !== "" && (repoState !== "no-git" || makeRepo)
    // The project's folder, git-wise: "ready", "no-commits" (an empty first
    // commit is made at the start) or "no-git" (it's made a repository only
    // when you say so).
    property string repoState: "ready"
    /// Not ready and holding files: the run's branch would start without them.
    property bool folderFiles: false
    property alias makeRepo: makeRepoSwitch.checked
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
        const known = defaultModelOf(agent)
        return [waiting ? qsTr("Reading models…") : known !== "" ? qsTr("Default (%1)").arg(Theme.modelLabel(known)) : qsTr("Default model")]
            .concat(models.map(m => Theme.modelLabel(m.name)))
    }
    /// The model a run's session gets when none is picked, as a new
    /// session's: the project's for that agent, else (on this computer)
    /// Settings' default for it; "" the agent's own.
    function defaultModelOf(agent) {
        if (agent === "") return ""
        const own = Workspace.setting("project_model_" + projectId + "_" + agent)
        return own !== "" || host !== "" ? own : Workspace.setting("default_model_" + agent)
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

    /// Fills it in for a project: its last choices, and the workflow
    /// remembered for it.
    function load(id, name, harness) {
        const repo = JSON.parse(Workspace.projectRepoState(id))
        repoState = repo.state
        folderFiles = repo.files
        makeRepoSwitch.checked = false
        // (Another project's task and criteria don't carry over.)
        if (id !== projectId) {
            taskArea.text = ""
            criteriaArea.text = ""
        }
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
        implementerBox.currentIndex = index(has(impl.agent) ? impl.agent : harness)
        reviewerBox.currentIndex = index(has(rev.agent) ? rev.agent : has("pi") ? "pi" : harness)
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
            if (id !== form.projectId) return
            const next = Object.assign({}, form.hostModels)
            next[harness] = JSON.parse(json)
            form.hostModels = next
        }
    }

    function start() {
        const reviewing = !flow || flow.needsReviewer
        const err = Workspace.startWorkflow(projectId, JSON.stringify({
            workflow: flow ? flow.id : "", task: taskArea.text, criteria: criteriaArea.text, testCommand: testField.text,
            implementer: implementerAgent, implementerModel: implementerModel,
            reviewer: reviewerAgent, reviewerModel: reviewing ? reviewerModel : "",
            approval: approvalModes[approvalBox.currentIndex],
            prepare: repoState === "no-commits" || repoState === "no-git" && makeRepo
        }))
        problem.text = err
        if (err === "") {
            Workspace.setSetting("workflow_implementer_" + projectId, implementerAgent + "\t" + implementerModel)
            if (reviewing)
                Workspace.setSetting("workflow_reviewer_" + projectId, reviewerAgent + "\t" + reviewerModel)
            taskArea.text = ""
            criteriaArea.text = ""
            started()
        }
    }

    spacing: 6

    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        ComboField {
            id: flowBox
            objectName: "workflowPick"
            Layout.fillWidth: true
            model: form.flows.map(f => f.name)
            onActivated: form.useFlowAgent()
        }
        FlatButton { objectName: "workflowEditList"; text: qsTr("Edit workflows…"); font.pixelSize: Theme.px(12); onClicked: form.editRequested() }
    }
    // Its stages, in order.
    PlainLabel {
        objectName: "workflowPickStages"
        Layout.fillWidth: true
        // (A check of the project's tests is left out while there's no
        // test command.)
        readonly property var leftOut: form.flow && form.testField.text.trim() === ""
            ? form.flow.stages.filter(s => s.kind === "check" && s.command.trim() === "") : []
        text: !form.flow ? ""
            : form.flow.stages.filter(s => leftOut.indexOf(s) < 0).map(s => s.name).join("  →  ")
              + (leftOut.length > 0 ? "    " + qsTr("(%1 left out: no test command)").arg(leftOut.map(s => s.name).join(", ")) : "")
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
        background: Rectangle { radius: 2; color: Theme.background; border.color: taskArea.activeFocus ? Theme.primary : Theme.border }
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
        background: Rectangle { radius: 2; color: Theme.background; border.color: criteriaArea.activeFocus ? Theme.primary : Theme.border }
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
        AppTextField { id: testField; objectName: "workflowTests"; visible: !form.flow || form.flow.needsTestCommand; Layout.fillWidth: true; font.family: Theme.mono; placeholderText: qsTr("e.g. cargo test, or leave it empty"); onActiveFocusChanged: if (!activeFocus) cursorPosition = 0; onTextChanged: if (!activeFocus) cursorPosition = 0 }
        Label { text: qsTr("Agent") }
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            ComboField {
                id: implementerBox
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                model: form.providers.map(p => p.label)
                marks: form.providers.map(p => p.id)
                onActivated: { form.pendingModels = Object.assign({}, form.pendingModels, { implementer: "" }); form.askHost(form.implementerAgent) }
            }
            ComboField {
                id: implementerModelBox
                objectName: "workflowImplementerModel"
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                model: form.modelNames(form.implementerAgent, form.implementerModels)
                onActivated: form.pendingModels = Object.assign({}, form.pendingModels, { implementer: form.implementerModel })
            }
        }
        Label { text: qsTr("Permissions") }
        ComboField { id: approvalBox; Layout.fillWidth: true; model: [qsTr("Ask before every change"), qsTr("Edit files freely, ask before commands"), qsTr("Run anything without asking")] }
        // What the choice means for a run left alone, and the way to
        // one that never waits.
        Item { implicitWidth: 1; implicitHeight: 1 }
        Label {
            id: approvalNote
            objectName: "workflowApprovalNote"
            readonly property string plain: approvalBox.currentIndex === 2
                ? qsTr("Nothing asks. The run works in its own worktree; nothing reaches your branch until you merge it.")
                : approvalBox.currentIndex === 0
                ? qsTr("Each agent asks before every change and command, and the run waits until you answer.")
                : qsTr("Each agent asks before running a command, and the run waits until you answer.")
            Layout.fillWidth: true
            Layout.topMargin: -4
            textFormat: Text.RichText
            text: plain.replace(/&/g, "&amp;").replace(/</g, "&lt;") + (approvalBox.currentIndex === 2 ? ""
                : "&nbsp;&nbsp;<a href=\"yolo\" style=\"color:" + Theme.text + "; text-decoration:" + (hoveredLink !== "" ? "underline" : "none") + "; font-weight:500\">" + qsTr("Run anything instead") + "</a>")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            onLinkActivated: approvalBox.currentIndex = 2
            HoverHandler { cursorShape: approvalNote.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
        }
        Label { visible: reviewerRow.visible; text: qsTr("Reviewer") }
        RowLayout {
            id: reviewerRow
            visible: !form.flow || form.flow.needsReviewer
            Layout.fillWidth: true
            spacing: 6
            ComboField {
                id: reviewerBox
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                model: form.providers.map(p => p.label)
                marks: form.providers.map(p => p.id)
                onActivated: { form.pendingModels = Object.assign({}, form.pendingModels, { reviewer: "" }); form.askHost(form.reviewerAgent) }
            }
            ComboField {
                id: reviewerModelBox
                objectName: "workflowReviewerModel"
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                model: form.modelNames(form.reviewerAgent, form.reviewerModels)
                onActivated: form.pendingModels = Object.assign({}, form.pendingModels, { reviewer: form.reviewerModel })
            }
        }
    }

    // A folder that isn't a repository: made one only when you say so.
    ColumnLayout {
        objectName: "workflowNoGit"
        visible: form.repoState === "no-git"
        Layout.fillWidth: true
        Layout.topMargin: 2
        spacing: 6
        MoreText {
            Layout.fillWidth: true
            full: qsTr("This folder isn't a git repository yet, and a run needs one.")
            more: qsTr("Harlo runs git init here and makes an empty first commit; the run works in a worktree branched from it.")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: form.folderFiles
            text: qsTr("The files here won't be in the run unless you commit them first.")
            color: Theme.warning
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            PlainLabel {
                Layout.fillWidth: true
                text: qsTr("Make it a git repository")
                font.pixelSize: Theme.px(13)
                TapHandler { onTapped: makeRepoSwitch.toggle() }
            }
            AppSwitch {
                id: makeRepoSwitch
                objectName: "workflowMakeRepo"
                Accessible.name: qsTr("Make it a git repository")
            }
        }
    }
    ColumnLayout {
        objectName: "workflowNoCommits"
        visible: form.repoState === "no-commits"
        Layout.fillWidth: true
        spacing: 4
        MoreText {
            Layout.fillWidth: true
            full: qsTr("Nothing is committed here yet: Harlo starts the run from an empty first commit.")
            more: qsTr("The run's branch needs a commit to start from; Harlo makes one with no files in it.")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: form.folderFiles
            text: qsTr("The files here won't be in the run unless you commit them first.")
            color: Theme.warning
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
    }
    MoreText {
        objectName: "workflowNote"
        Layout.fillWidth: true
        full: qsTr("The agent works on its own copy of the project; anything that fails goes back to it until every check and review passes.")
        more: qsTr("The copy is a git worktree on a branch of its own: nothing reaches your branch until you merge it. After each attempt Harlo runs the checks there and the reviews read the diff. A failure, or a review asking for changes, goes back to the same session to fix, and each fix goes through every check and review again.")
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
