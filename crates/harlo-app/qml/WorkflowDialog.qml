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
    property alias approvalBox: approvalBox
    property alias flowBox: flowBox
    readonly property var approvalModes: ["always-ask", "write", "yolo"]
    property alias problem: problem

    function show(id, name, harness) {
        projectId = id
        title = qsTr("Run a workflow in %1").arg(name)
        providers = JSON.parse(Workspace.chatProvidersJson())
        flows = JSON.parse(Workspace.workflowDefinitionsJson())
        const last = Workspace.workflowChoice(id)
        flowBox.currentIndex = Math.max(0, flows.findIndex(f => f.id === last))
        const index = agent => Math.max(0, providers.findIndex(p => p.id === agent))
        implementerBox.currentIndex = index(providers.some(p => p.id === "harlo") ? "harlo" : harness)
        reviewerBox.currentIndex = index(providers.some(p => p.id === "claude") ? "claude" : harness)
        testField.text = Workspace.workflowTestCommand(id)
        approvalBox.currentIndex = 1
        problem.text = ""
        open()
        taskArea.forceActiveFocus()
    }

    function start() {
        const agent = box => providers[box.currentIndex] ? providers[box.currentIndex].id : ""
        const err = Workspace.startWorkflow(projectId, JSON.stringify({
            workflow: flow ? flow.id : "", task: taskArea.text, criteria: criteriaArea.text, testCommand: testField.text,
            implementer: agent(implementerBox), reviewer: agent(reviewerBox),
            approval: approvalModes[approvalBox.currentIndex]
        }))
        problem.text = err
        if (err === "")
            close()
    }

    objectName: "workflowDialog"
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 520
    footer: AppButtonBox {
        DialogButton {
            objectName: "workflowStart"
            text: qsTr("Start")
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
            AppTextField { id: testField; visible: !dialog.flow || dialog.flow.needsTestCommand; Layout.fillWidth: true; font.family: Theme.mono; placeholderText: qsTr("The command that runs them, e.g. cargo test") }
            Label { text: qsTr("Implements") }
            ComboField { id: implementerBox; Layout.fillWidth: true; model: dialog.providers.map(p => p.label); marks: dialog.providers.map(p => p.id) }
            Label { text: qsTr("It may") }
            ComboField { id: approvalBox; Layout.fillWidth: true; model: [qsTr("Ask before every change"), qsTr("Edit files freely, ask before commands"), qsTr("Run anything without asking")] }
            Label { visible: reviewerBox.visible; text: qsTr("Reviews") }
            ComboField { id: reviewerBox; visible: !dialog.flow || dialog.flow.needsReviewer; Layout.fillWidth: true; model: dialog.providers.map(p => p.label); marks: dialog.providers.map(p => p.id) }
        }

        FormNote {
            Layout.fillWidth: true
            text: qsTr("The change is made in a new worktree. After each attempt harlo runs the checks there and the reviews read the diff; a failure, or a review asking for changes, goes back to the same session to fix, and each fix goes through every check and review again.")
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
