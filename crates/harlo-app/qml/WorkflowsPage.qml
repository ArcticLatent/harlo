pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Workflows: the built-in one and yours. A workflow is its
// implementation, then checks (a command harlo runs) and reviews (another
// agent reads the diff), in order; each failure goes back to the
// implementation a set number of times.
ColumnLayout {
    id: page
    objectName: "workflowsPage"
    spacing: 0
    // Test hook.
    property alias editor: editor
    readonly property var list: Workspace.workflowsRevision >= 0 ? JSON.parse(Workspace.workflowDefinitionsJson()) : []
    readonly property var providers: JSON.parse(Workspace.chatProvidersJson())

    function agentLabel(id) {
        const p = providers.find(x => x.id === id)
        return p ? p.label : id
    }
    function stageIcon(kind) { return kind === "implement" ? "edit" : kind === "check" ? "terminal-box" : "search-eye" }
    // "`cargo test`", "Claude Code", "the reviewer you pick".
    function stageWhat(s) {
        if (s.kind === "implement") return qsTr("the agent you pick")
        if (s.kind === "check") return s.command !== "" ? s.command : qsTr("the project's tests")
        return s.agent.harness !== "" ? agentLabel(s.agent.harness) + (s.agent.model !== "" ? " · " + s.agent.model : "") : qsTr("the reviewer you pick")
    }

    SectionHeader {
        title: qsTr("Tasks in stages")
        subtitle: qsTr("Take a task through an implementation, checks and reviews, in a worktree of its own.")
        more: qsTr("Start one from a project's menu (Run a workflow…). A failed check, or a review asking for changes, goes back to the same implementation session, and each fix goes through every check and review again. A run keeps its own copy of its workflow: changing one here doesn't change runs under way.")
        AddButton { objectName: "newWorkflow"; text: qsTr("New workflow"); onClicked: editor.openFor(null, false) }
    }

    Repeater {
        model: page.list
        Rectangle {
            id: card
            required property var modelData
            readonly property var flow: modelData
            Layout.fillWidth: true
            Layout.topMargin: 10
            implicitHeight: cardCol.implicitHeight + 24
            radius: 10
            color: Theme.elevated
            border.color: Theme.border
            ColumnLayout {
                id: cardCol
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Icon { name: "list-check-2"; color: Theme.primary; implicitWidth: 16; implicitHeight: 16 }
                    Label { Layout.fillWidth: true; text: card.flow.name; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(14); elide: Text.ElideRight }
                    Label { visible: card.flow.builtin; text: qsTr("Built in"); color: Theme.muted; font.pixelSize: Theme.px(11) }
                }
                // The stages, in order.
                Flow {
                    objectName: "workflowStagesLine"
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: card.flow.stages
                        Row {
                            id: chip
                            required property var modelData
                            required property int index
                            spacing: 6
                            Icon { visible: chip.index > 0; anchors.verticalCenter: parent.verticalCenter; name: "arrow-right-s"; color: Theme.muted; implicitWidth: 12; implicitHeight: 12 }
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                implicitWidth: chipRow.implicitWidth + 14
                                implicitHeight: chipRow.implicitHeight + 6
                                radius: 6
                                color: Theme.background
                                border.color: Theme.border
                                Row {
                                    id: chipRow
                                    anchors.centerIn: parent
                                    spacing: 5
                                    Icon { anchors.verticalCenter: parent.verticalCenter; name: page.stageIcon(chip.modelData.kind); color: Theme.muted; implicitWidth: 13; implicitHeight: 13 }
                                    Label { anchors.verticalCenter: parent.verticalCenter; text: chip.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(12) }
                                    Label {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: page.stageWhat(chip.modelData)
                                        color: Theme.muted
                                        font.pixelSize: Theme.px(11)
                                        font.family: chip.modelData.kind === "check" && chip.modelData.command !== "" ? Theme.mono : Theme.uiFont
                                    }
                                }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Item { Layout.fillWidth: true }
                    FlatButton { visible: !card.flow.builtin; objectName: "workflowEdit"; text: qsTr("Edit"); font.pixelSize: Theme.px(12); onClicked: editor.openFor(card.flow, false) }
                    FlatButton { objectName: "workflowDuplicate"; text: qsTr("Duplicate"); font.pixelSize: Theme.px(12); onClicked: editor.openFor(card.flow, true) }
                    FlatButton {
                        visible: !card.flow.builtin
                        objectName: "workflowDelete"
                        text: qsTr("Delete")
                        font.pixelSize: Theme.px(12)
                        accent: Theme.error
                        onClicked: { confirmDelete.flow = card.flow; confirmDelete.open() }
                    }
                }
            }
        }
    }

    AppDialog {
        id: confirmDelete
        property var flow: ({})
        title: qsTr("Delete “%1”?").arg(flow.name || "")
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label { width: 360; wrapMode: Text.Wrap; color: Theme.muted; text: qsTr("Runs that used it go on with their own copy.") }
        onDiscarded: { Workspace.deleteWorkflowDefinition(flow.id); close() }
    }

    // ---- the editor ------------------------------------------------
    AppDialog {
        id: editor
        objectName: "workflowEditor"
        property string flowId: ""
        property string error: ""
        // Test hooks.
        property alias nameField: nameField
        property alias stageModel: stageModel
        title: flowId === "" ? qsTr("New workflow") : qsTr("Edit workflow")
        implicitWidth: Math.min(620, (Overlay.overlay ? Overlay.overlay.width : 660) - 40)

        function openFor(flow, copy) {
            error = ""
            flowId = flow && !copy ? flow.id : ""
            nameField.text = flow ? (copy ? qsTr("%1 (copy)").arg(flow.name) : flow.name) : ""
            stageModel.clear()
            const stages = flow ? flow.stages : [
                { kind: "implement", name: "Implement", command: "", agent: { harness: "", model: "" }, focus: "", tries: 0 },
                { kind: "check", name: "Tests", command: "", agent: { harness: "", model: "" }, focus: "", tries: 3 },
                { kind: "review", name: "Review", command: "", agent: { harness: "", model: "" }, focus: "", tries: 2 }
            ]
            for (const s of stages)
                stageModel.append({ kind: s.kind, name: s.name, command: s.command, harness: s.agent.harness, agentModel: s.agent.model, reviewFocus: s.focus, tries: s.tries })
            open()
            nameField.forceActiveFocus()
        }
        function add(kind) {
            stageModel.append({ kind: kind, name: kind === "check" ? qsTr("Check") : qsTr("Review"), command: "", harness: "", agentModel: "", reviewFocus: "", tries: kind === "check" ? 3 : 2 })
        }
        function save() {
            const stages = []
            for (let i = 0; i < stageModel.count; i++) {
                const s = stageModel.get(i)
                stages.push({ kind: s.kind, name: s.name, command: s.command, agent: { harness: s.harness, model: s.agentModel }, focus: s.reviewFocus, tries: s.tries })
            }
            const answer = JSON.parse(Workspace.saveWorkflowDefinition(JSON.stringify({ id: flowId, name: nameField.text, stages: stages })))
            error = answer.error
            if (error === "") close()
        }

        ListModel { id: stageModel }

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
                DialogButton { text: qsTr("Cancel"); onClicked: editor.close() }
                DialogButton { objectName: "workflowSave"; text: qsTr("Save"); accent: true; onClicked: editor.save() }
            }
        }

        ColumnLayout {
            width: parent.width
            spacing: 8
            AppTextField { id: nameField; objectName: "workflowName"; Layout.fillWidth: true; placeholderText: qsTr("Name, e.g. Careful change"); onTextEdited: editor.error = "" }
            FormLabel { text: qsTr("Stages, in order") }
            ScrollView {
                id: stagesScroll
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(stagesCol.implicitHeight, (Overlay.overlay ? Overlay.overlay.height : 700) - 300)
                contentWidth: availableWidth
                clip: true
                ColumnLayout {
                    id: stagesCol
                    width: stagesScroll.availableWidth
                    spacing: 6
                    Repeater {
                        model: stageModel
                        Rectangle {
                            id: stage
                            required property int index
                            required property string kind
                            required property string name
                            required property string command
                            required property string harness
                            required property string agentModel
                            required property string reviewFocus
                            required property int tries
                            readonly property bool first: kind === "implement"
                            objectName: "workflowStage" + index
                            Layout.fillWidth: true
                            implicitHeight: stageCol.implicitHeight + 16
                            radius: 8
                            color: Theme.background
                            border.color: Theme.border
                            ColumnLayout {
                                id: stageCol
                                x: 10
                                y: 8
                                width: parent.width - 20
                                spacing: 6
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 6
                                    Icon { name: page.stageIcon(stage.kind); color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                                    AppTextField {
                                        objectName: "stageName"
                                        Layout.fillWidth: true
                                        Layout.minimumWidth: 60
                                        text: stage.name
                                        enabled: !stage.first
                                        onTextEdited: stageModel.setProperty(stage.index, "name", text)
                                    }
                                    Label { visible: !stage.first; text: qsTr("Fixes"); color: Theme.muted; font.pixelSize: Theme.px(12) }
                                    AppTextField {
                                        objectName: "stageTries"
                                        visible: !stage.first
                                        Layout.preferredWidth: 44
                                        horizontalAlignment: Text.AlignHCenter
                                        text: String(stage.tries)
                                        validator: IntValidator { bottom: 0; top: 10 }
                                        onTextEdited: stageModel.setProperty(stage.index, "tries", Number(text) || 0)
                                        ToolTip.visible: hovered
                                        ToolTip.text: qsTr("Times a failure here goes back to the implementation before the run pauses for you")
                                    }
                                    ToolButton {
                                        objectName: "stageUp"
                                        visible: !stage.first
                                        enabled: stage.index > 1
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        contentItem: Icon { name: "arrow-up-s"; color: parent.enabled ? Theme.foreground : Theme.border; implicitWidth: 16; implicitHeight: 16 }
                                        background: Rectangle { radius: 5; color: parent.hovered ? Theme.hover : "transparent" }
                                        onClicked: stageModel.move(stage.index, stage.index - 1, 1)
                                        Accessible.name: qsTr("Move up")
                                    }
                                    ToolButton {
                                        visible: !stage.first
                                        enabled: stage.index < stageModel.count - 1
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        contentItem: Icon { name: "arrow-down-s"; color: parent.enabled ? Theme.foreground : Theme.border; implicitWidth: 16; implicitHeight: 16 }
                                        background: Rectangle { radius: 5; color: parent.hovered ? Theme.hover : "transparent" }
                                        onClicked: stageModel.move(stage.index, stage.index + 1, 1)
                                        Accessible.name: qsTr("Move down")
                                    }
                                    ToolButton {
                                        objectName: "stageRemove"
                                        visible: !stage.first
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        contentItem: Icon { name: "delete-bin"; color: parent.hovered ? Theme.error : Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                                        background: Rectangle { radius: 5; color: parent.hovered ? Theme.hover : "transparent" }
                                        onClicked: stageModel.remove(stage.index)
                                        Accessible.name: qsTr("Remove this stage")
                                    }
                                }
                                PlainLabel {
                                    visible: stage.first
                                    Layout.fillWidth: true
                                    text: qsTr("The agent you pick when you start a run makes the change in a new worktree.")
                                    color: Theme.muted
                                    font.pixelSize: Theme.px(12)
                                    wrapMode: Text.Wrap
                                }
                                AppTextField {
                                    objectName: "stageCommand"
                                    visible: stage.kind === "check"
                                    Layout.fillWidth: true
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.px(12)
                                    placeholderText: qsTr("Command, e.g. cargo clippy -- -D warnings (empty: the project's tests)")
                                    text: stage.command
                                    onTextEdited: stageModel.setProperty(stage.index, "command", text)
                                }
                                RowLayout {
                                    visible: stage.kind === "review"
                                    Layout.fillWidth: true
                                    spacing: 6
                                    ComboField {
                                        objectName: "stageAgent"
                                        Layout.preferredWidth: 200
                                        model: [qsTr("The reviewer you pick")].concat(page.providers.map(p => p.label))
                                        marks: [""].concat(page.providers.map(p => p.id))
                                        currentIndex: stage.harness === "" ? 0 : Math.max(0, page.providers.findIndex(p => p.id === stage.harness) + 1)
                                        onActivated: i => stageModel.setProperty(stage.index, "harness", i === 0 ? "" : page.providers[i - 1].id)
                                    }
                                    AppTextField {
                                        visible: stage.harness !== ""
                                        Layout.fillWidth: true
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.px(12)
                                        placeholderText: qsTr("Model (optional)")
                                        text: stage.agentModel
                                        onTextEdited: stageModel.setProperty(stage.index, "agentModel", text)
                                    }
                                    Item { visible: stage.harness === ""; Layout.fillWidth: true }
                                }
                                AppTextField {
                                    objectName: "stageFocus"
                                    visible: stage.kind === "review"
                                    Layout.fillWidth: true
                                    placeholderText: qsTr("Look in particular at… (optional, e.g. security, error handling)")
                                    text: stage.reviewFocus
                                    onTextEdited: stageModel.setProperty(stage.index, "reviewFocus", text)
                                }
                            }
                        }
                    }
                }
            }
            RowLayout {
                spacing: 8
                FlatButton { objectName: "addCheck"; text: qsTr("Add check"); font.pixelSize: Theme.px(12); onClicked: editor.add("check") }
                FlatButton { objectName: "addReview"; text: qsTr("Add review"); font.pixelSize: Theme.px(12); onClicked: editor.add("review") }
            }
            PlainLabel {
                objectName: "workflowEditorError"
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
