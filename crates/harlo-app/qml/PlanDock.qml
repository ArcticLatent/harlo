import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The session's shared plan (a project with the plan on): progress and
// the step in hand on one line; expanded, every step to edit, reorder,
// skip or remove, a step to add, and each finished step's commit, which
// another agent can review. The plan can also run with helpers: harlo
// gives each step to a helper session, stage by stage (a step marked
// "alongside" runs at the same time as the one before, in a worktree).
Rectangle {
    id: root
    property ChatSession session
    readonly property var plan: session && session.key !== "" && Workspace.plansRevision >= 0 ? JSON.parse(Workspace.planJson(session.key)) : ({ enabled: false, steps: [] })
    readonly property var steps: plan.steps || []
    readonly property int finished: steps.filter(s => s.status === "done" || s.status === "skipped").length
    readonly property var current: steps.find(s => s.status === "active") || null
    readonly property var run: plan.run || null
    readonly property bool paused: !!run && (run.paused || "") !== ""
    property bool expanded: false
    /// The step being edited (its id; -1 none, 0 a new one).
    property int editing: -1

    visible: plan.enabled && steps.length > 0
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    border.color: Theme.border

    function edit(op) { Workspace.planEdit(session.key, JSON.stringify(op)) }
    function mark(status) {
        switch (status) {
        case "done": return { icon: "checkbox-circle", color: Theme.success }
        case "active": return { icon: "arrow-right-s", color: Theme.primary }
        case "skipped": return { icon: "close", color: Theme.muted }
        default: return { icon: "", color: Theme.muted }
        }
    }
    // Agents that can review a step (not the one that did it, first).
    readonly property var reviewers: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : [])
        .filter(p => p.id !== "demo")
        .sort((a, b) => (a.id === root.session.harness) - (b.id === root.session.harness))

    ColumnLayout {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 4

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "list-check-2"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label {
                text: "Plan · " + root.finished + "/" + root.steps.length
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
                font.bold: true
            }
            Rectangle {
                implicitWidth: 60
                implicitHeight: 4
                radius: 2
                color: Theme.subtle
                Rectangle { width: parent.width * (root.steps.length ? root.finished / root.steps.length : 0); height: parent.height; radius: 2; color: Theme.success }
            }
            // Running with helpers (or paused, waiting for you).
            Rectangle {
                objectName: "planRunChip"
                visible: !!root.run
                implicitWidth: runChip.implicitWidth + 12
                implicitHeight: 18
                radius: 9
                readonly property color tint: root.paused ? Theme.warning : Theme.primary
                color: Qt.rgba(tint.r, tint.g, tint.b, 0.15)
                Label { id: runChip; anchors.centerIn: parent; text: root.paused ? "Paused" : "Running with helpers"; color: parent.tint; font.pixelSize: Theme.px(10.5); font.bold: true }
            }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.current ? root.current.title : (root.finished === root.steps.length ? "All done" : "")
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                elide: Text.ElideRight
            }
            Button {
                id: toggle
                objectName: "planToggle"
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                onClicked: { root.expanded = !root.expanded; root.editing = -1 }
                contentItem: Icon { name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
                ToolTip.visible: hovered
                ToolTip.text: root.expanded ? "Hide the plan" : "Show and edit the plan"
            }
        }

        Repeater {
            model: root.expanded ? root.steps : []
            ColumnLayout {
                id: stepRow
                required property var modelData
                required property int index
                readonly property var m: root.mark(modelData.status)
                readonly property bool open: modelData.status === "pending" || modelData.status === "active"
                Layout.fillWidth: true
                spacing: 2
                HoverHandler { id: stepHover }
                RowLayout {
                    Layout.fillWidth: true
                    visible: root.editing !== stepRow.modelData.id
                    spacing: 8
                    Item {
                        implicitWidth: 14
                        implicitHeight: 14
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: 2
                        Icon { anchors.fill: parent; visible: stepRow.m.icon !== ""; name: stepRow.m.icon; color: stepRow.m.color }
                        Rectangle { visible: stepRow.m.icon === ""; anchors.centerIn: parent; width: 10; height: 10; radius: 5; color: "transparent"; border.color: Theme.muted }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1
                        // Runs at the same time as the step before (with helpers).
                        RowLayout {
                            visible: !!stepRow.modelData.alongside
                            spacing: 4
                            Icon { name: "layout-column"; color: Theme.info; implicitWidth: 11; implicitHeight: 11 }
                            Label { text: "With the step before"; color: Theme.info; font.pixelSize: Theme.px(10.5) }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: stepRow.modelData.title + (stepRow.modelData.editedByUser ? "  ·  yours" : "")
                            color: stepRow.modelData.status === "active" ? Theme.foreground : Theme.muted
                            font.pixelSize: Theme.px(12)
                            font.strikeout: stepRow.modelData.status === "skipped"
                            wrapMode: Text.Wrap
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: text !== ""
                            text: stepRow.modelData.summary || stepRow.modelData.detail || ""
                            color: Theme.textFaint
                            font.pixelSize: Theme.px(11.5)
                            wrapMode: Text.Wrap
                        }
                        // Its helper (a run), and the worktree it works in.
                        RowLayout {
                            visible: (stepRow.modelData.helper || "") !== "" || (stepRow.modelData.branch || "") !== ""
                            Layout.fillWidth: true
                            spacing: 6
                            Icon { name: "node-tree"; color: Theme.muted; implicitWidth: 12; implicitHeight: 12 }
                            Label {
                                Layout.fillWidth: true
                                text: (stepRow.modelData.status === "active" ? "A helper is on it" : "Done by a helper") + ((stepRow.modelData.branch || "") !== "" ? " · ⎇ " + stepRow.modelData.branch : "")
                                color: Theme.muted
                                font.pixelSize: Theme.px(11)
                                elide: Text.ElideRight
                            }
                            FlatButton {
                                objectName: "planOpenHelper-" + stepRow.modelData.id
                                visible: (stepRow.modelData.helper || "") !== "" && stepRow.modelData.status === "active"
                                text: "Open"
                                padding: 2
                                font.pixelSize: Theme.px(11)
                                onClicked: Workspace.open(stepRow.modelData.helper)
                            }
                        }
                        // Its commit, or why there isn't one.
                        RowLayout {
                            visible: (stepRow.modelData.commit || "") !== "" || (stepRow.modelData.commitNote || "") !== ""
                            Layout.fillWidth: true
                            spacing: 6
                            Icon { visible: (stepRow.modelData.commit || "") !== ""; name: "git-commit"; color: Theme.success; implicitWidth: 12; implicitHeight: 12 }
                            Label {
                                Layout.fillWidth: true
                                text: (stepRow.modelData.commit || "") !== "" ? "Committed " + stepRow.modelData.commit.substring(0, 8) : (stepRow.modelData.commitNote || "")
                                color: (stepRow.modelData.commit || "") !== "" ? Theme.success : (stepRow.modelData.commitNote || "").startsWith("Not committed") ? Theme.warning : Theme.muted
                                font.family: (stepRow.modelData.commit || "") !== "" ? Theme.mono : Theme.uiFont
                                font.pixelSize: Theme.px(11)
                                wrapMode: Text.Wrap
                            }
                            FlatButton {
                                objectName: "planReview-" + stepRow.modelData.id
                                visible: (stepRow.modelData.commit || "") !== ""
                                text: "Review…"
                                padding: 2
                                font.pixelSize: Theme.px(11)
                                onClicked: reviewMenu.popup()
                                AppMenu {
                                    id: reviewMenu
                                    Repeater {
                                        model: root.reviewers
                                        AppMenuItem {
                                            required property var modelData
                                            text: "Review with " + modelData.label
                                            onTriggered: {
                                                const err = Workspace.reviewStep(root.session.key, stepRow.modelData.id, modelData.id)
                                                if (err !== "") Workspace.showNotice(err, true)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Your edits (on hover; their room is kept, so the text
                    // doesn't rewrap when they show).
                    Row {
                        Layout.alignment: Qt.AlignTop
                        opacity: stepHover.hovered ? 1 : 0
                        enabled: stepHover.hovered
                        spacing: 0
                        StepButton { iconName: "arrow-up-s"; tip: "Move up"; enabled: stepRow.index > 0; onClicked: root.edit({ op: "shift", id: stepRow.modelData.id, by: -1 }) }
                        StepButton { iconName: "arrow-down-s"; tip: "Move down"; enabled: stepRow.index < root.steps.length - 1; onClicked: root.edit({ op: "shift", id: stepRow.modelData.id, by: 1 }) }
                        StepButton {
                            visible: stepRow.index > 0 && stepRow.modelData.status === "pending"
                            iconName: "layout-column"
                            tip: stepRow.modelData.alongside ? "Run it after the step before" : "Run it at the same time as the step before (with helpers)"
                            onClicked: root.edit({ op: "alongside", id: stepRow.modelData.id, alongside: !stepRow.modelData.alongside })
                        }
                        StepButton { visible: stepRow.open; iconName: "edit"; tip: "Change it"; onClicked: { root.editing = stepRow.modelData.id; editTitle.text = stepRow.modelData.title; editDetail.text = stepRow.modelData.detail || "" } }
                        StepButton {
                            visible: stepRow.modelData.status !== "done"
                            iconName: stepRow.modelData.status === "skipped" ? "arrow-go-back" : "close"
                            tip: stepRow.modelData.status === "skipped" ? "Don't skip it" : "Skip it"
                            onClicked: root.edit({ op: "skip", id: stepRow.modelData.id, skipped: stepRow.modelData.status !== "skipped" })
                        }
                        StepButton { visible: stepRow.modelData.status === "pending"; iconName: "delete-bin"; tip: "Remove it"; onClicked: root.edit({ op: "remove", id: stepRow.modelData.id }) }
                        StepButton { visible: stepRow.modelData.status === "pending" || stepRow.modelData.status === "active"; iconName: "add"; tip: "Add a step after it"; onClicked: { root.editing = 0; addAfter.value = stepRow.modelData.id; editTitle.text = ""; editDetail.text = "" } }
                    }
                }
            }
        }

        // Changing a step, or a new one.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 4
            visible: root.expanded && root.editing >= 0
            spacing: 4
            QtObject { id: addAfter; property int value: -1 }
            AppTextField {
                id: editTitle
                objectName: "planStepTitle"
                Layout.fillWidth: true
                placeholderText: "The step's outcome"
                font.pixelSize: Theme.px(12)
                onAccepted: save.clicked()
            }
            AppTextField {
                id: editDetail
                Layout.fillWidth: true
                placeholderText: "How, or what to watch for (optional)"
                font.pixelSize: Theme.px(12)
                onAccepted: save.clicked()
            }
            RowLayout {
                Item { Layout.fillWidth: true }
                FlatButton { text: "Cancel"; outlined: false; font.pixelSize: Theme.px(11.5); onClicked: root.editing = -1 }
                FlatButton {
                    id: save
                    objectName: "planStepSave"
                    text: root.editing === 0 ? "Add" : "Save"
                    enabled: editTitle.text.trim() !== ""
                    accent: Theme.primary
                    font.pixelSize: Theme.px(11.5)
                    onClicked: {
                        if (root.editing === 0)
                            root.edit({ op: "add", title: editTitle.text, detail: editDetail.text, after: addAfter.value >= 0 ? addAfter.value : undefined })
                        else
                            root.edit({ op: "edit", id: root.editing, title: editTitle.text, detail: editDetail.text })
                        root.editing = -1
                    }
                }
            }
        }
        // Why the run paused.
        Label {
            Layout.fillWidth: true
            Layout.topMargin: 2
            visible: root.expanded && root.paused
            text: root.run ? root.run.paused : ""
            color: Theme.warning
            font.pixelSize: Theme.px(11.5)
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.fillWidth: true
            visible: root.expanded && root.editing < 0
            spacing: 8
            // Each step to a helper session, stage by stage.
            FlatButton {
                objectName: "planRun"
                visible: (!root.run || root.paused) && root.finished < root.steps.length
                text: root.paused ? "Run again" : "Run with helpers"
                accent: Theme.primary
                padding: 3
                font.pixelSize: Theme.px(11.5)
                onClicked: runMenu.popup()
                ToolTip.visible: hovered
                ToolTip.text: "harlo gives each step to a helper session: in order, and steps marked to run with the one before at the same time, each in a worktree."
                ToolTip.delay: 500
                AppMenu {
                    id: runMenu
                    Repeater {
                        model: root.reviewers.slice().sort((a, b) => (b.id === root.session.harness) - (a.id === root.session.harness))
                        AppMenuItem {
                            required property var modelData
                            text: "Helpers: " + modelData.label + (modelData.id === root.session.harness ? " (this session's)" : "")
                            onTriggered: {
                                const err = Workspace.runPlan(root.session.key, modelData.id, "")
                                if (err !== "") Workspace.showNotice(err, true)
                            }
                        }
                    }
                }
            }
            FlatButton {
                objectName: "planStopRun"
                visible: !!root.run
                text: "Stop the run"
                outlined: false
                accent: Theme.muted
                padding: 3
                font.pixelSize: Theme.px(11.5)
                onClicked: Workspace.stopPlanRun(root.session.key)
                ToolTip.visible: hovered
                ToolTip.text: "No more steps start; helpers at work finish theirs."
                ToolTip.delay: 500
            }
            FlatButton {
                objectName: "planAddStep"
                text: "Add a step"
                padding: 3
                font.pixelSize: Theme.px(11.5)
                onClicked: { root.editing = 0; addAfter.value = -1; editTitle.text = ""; editDetail.text = ""; editTitle.forceActiveFocus() }
            }
            Label {
                Layout.fillWidth: true
                text: root.run ? "" : "The agent reads your changes before each step."
                color: Theme.textFaint
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
            }
            FlatButton {
                text: "Clear"
                outlined: false
                accent: Theme.muted
                padding: 3
                font.pixelSize: Theme.px(11.5)
                onClicked: root.edit({ op: "clear" })
            }
        }
    }

    component StepButton: Button {
        id: sb
        property string iconName
        property string tip
        flat: true
        implicitWidth: 22
        implicitHeight: 22
        contentItem: Icon { name: sb.iconName; color: !sb.enabled ? Theme.textFaint : sb.hovered ? Theme.foreground : Theme.muted; implicitWidth: 13; implicitHeight: 13 }
        background: Rectangle { radius: 5; color: sb.hovered ? Theme.hover : "transparent" }
        ToolTip.visible: hovered
        ToolTip.text: tip
        ToolTip.delay: 500
    }
}
