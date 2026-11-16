pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
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
    /// Run with helpers, but other ones than the team's (this run only).
    signal chooseHelpers()
    /// Who does one step in a run with helpers (its agent and model).
    signal chooseStepHelper(int id, string agent, string model)
    // Who the helpers are (read as the menu opens): the team's, else as
    // the session.
    property var helpers: ({})
    readonly property string helpersLabel: !helpers.agent ? "" : helpers.agentName
        + (helpers.model ? " · " + Theme.modelLabel(helpers.model.split("/").pop()) : "")
        + (helpers.thinking ? " · " + helpers.thinking + " thinking" : "")
    readonly property var plan: session && session.key !== "" && Workspace.plansRevision >= 0 ? JSON.parse(Workspace.planJson(session.key)) : ({ enabled: false, steps: [] })
    readonly property var steps: plan.steps || []
    readonly property int finished: steps.filter(s => s.status === "done" || s.status === "skipped").length
    readonly property var current: steps.find(s => s.status === "active") || null
    // The session's agent is working (through the plan, or anything else).
    readonly property bool agentBusy: !!session && (session.status === "busy" || session.status === "starting")
    readonly property var run: plan.run || null
    readonly property bool paused: !!run && (run.paused || "") !== ""
    // The step a pause is about ("step 2 (…) …"), else 0.
    readonly property int pausedStep: paused ? Number((/^step (\d+) /.exec(run.paused) || [0, 0])[1]) : 0
    // What the run's helpers (its steps' and their reviews') used, together.
    readonly property string runUsed: !!session && session.key !== "" && Workspace.orchestrationRevision >= 0
        ? Theme.usedLabel(Theme.usedTotal(JSON.parse(Workspace.orchestratedJson(session.key)).filter(h => (h.step !== null && h.step !== undefined) || (h.review !== null && h.review !== undefined)).map(h => h.used || null)))
        : ""
    // The session is in Plan: the plan waits for you to build it.
    readonly property bool planning: !!session && session.key !== "" && Workspace.modesRevision >= 0 && Workspace.planMode(session.key)
    /// Build: out of Plan, and the agent works through the plan.
    function build() {
        Workspace.setPlanMode(session.key, false)
        session.planMode = false
        session.send("Build it: work through the plan, step by step.", "normal")
    }
    // Folded when a session opens: its header says the step in hand. It
    // opens by hand, or when the agent makes the plan while you watch, and
    // a finished one folds to "All done" (as its last step ends); folding
    // or opening it by hand holds for that session.
    property bool expanded: false
    readonly property bool allDone: steps.length > 0 && finished === steps.length
    readonly property string sessionKey: session ? session.key : ""
    /// By session: true folded by hand, false opened by hand.
    property var foldedByHand: ({})
    /// By session: its plan's steps when last seen; true once a plan was
    /// made in it while it was on screen.
    property var seenSteps: ({})
    property var madeHere: ({})
    function settleFold() {
        const k = sessionKey
        expanded = k in foldedByHand ? !foldedByHand[k] : madeHere[k] === true && !allDone
        editing = -1
    }
    // (Read from harlo, not `steps`: that may still be the last session's.)
    function noteSteps() {
        const k = sessionKey
        if (k === "") return
        const n = (JSON.parse(Workspace.planJson(k)).steps || []).length
        const seen = seenSteps
        if (k in seen && seen[k] === 0 && n > 0) {
            const made = madeHere
            made[k] = true
            madeHere = made
            settleFold()
        }
        seen[k] = n
        seenSteps = seen
    }
    /// A session opened: its plan folded (unless opened by hand), even one
    /// made while you watched it before.
    function sessionOpened() {
        showFinished = false
        madeHere = ({})
        settleFold()
        noteSteps()
    }
    onSessionKeyChanged: sessionOpened()
    onStepsChanged: noteSteps()
    onAllDoneChanged: settleFold()
    Component.onCompleted: { settleFold(); noteSteps() }
    /// The most its steps take before they scroll.
    property real maxStepsHeight: 168
    /// The finished steps before the first one to do, folded to a line.
    readonly property int leadFinished: {
        const i = steps.findIndex(s => s.status === "pending" || s.status === "active")
        return i < 0 ? 0 : i
    }
    property bool showFinished: false
    // (Folding the card folds them again: the next open is quick.)
    onExpandedChanged: if (!expanded) showFinished = false
    /// The step in hand, else the next one to do.
    readonly property var next: current || steps.find(s => s.status === "pending") || null
    /// The step being edited (its id; -1 none, 0 a new one).
    property int editing: -1

    visible: plan.enabled && steps.length > 0
    implicitHeight: col.implicitHeight + 22
    radius: 10
    color: Theme.elevated
    border.color: Theme.border

    function agentName(id) { return ({ harlo: "Harlo", claude: "Claude Code" })[id] || id }
    // A model's name as its agent lists it (else its id, shortened).
    function modelName(agent, model) {
        const p = Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(x => x.id === agent) : null
        const i = p ? (p.modelIds || []).indexOf(model) : -1
        return i >= 0 && p.models[i] ? Theme.modelLabel(p.models[i]) : Theme.modelLabel(String(model).split("/").pop())
    }
    function edit(op) { Workspace.planEdit(session.key, JSON.stringify(op)) }
    /// The step whose "Review…" menu is open.
    property int reviewStepId: -1
    // Agents that can review a step (not the one that did it, first).
    readonly property var reviewers: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : [])
        .filter(p => p.id !== "demo")
        .sort((a, b) => (a.id === root.session.harness) - (b.id === root.session.harness))

    ColumnLayout {
        id: col
        x: 14
        y: 11
        width: parent.width - 28
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "list-check-2"; color: Theme.textMuted; implicitWidth: 15; implicitHeight: 15 }
            Label {
                text: "Plan"
                color: Theme.text
                font.pixelSize: Theme.px(13.5)
                font.weight: Font.DemiBold
            }
            Label {
                objectName: "planProgress"
                text: root.finished + " of " + root.steps.length + " done"
                color: Theme.textMuted
                font.pixelSize: Theme.px(12)
                font.features: { "tnum": 1 }
            }
            Rectangle {
                visible: root.width >= 480
                implicitWidth: 72
                implicitHeight: 4
                radius: 2
                color: Theme.hairline
                Rectangle { width: parent.width * (root.steps.length ? root.finished / root.steps.length : 0); height: parent.height; radius: 2; color: Theme.textMuted }
            }
            // Running with helpers (or paused, waiting for you); what they
            // used is in its tooltip.
            Label {
                objectName: "planRunChip"
                visible: !!root.run
                text: "· " + (root.paused ? "paused" : root.width >= 480 ? "running with helpers" : "running")
                color: root.paused ? Theme.warning : Theme.primary
                font.pixelSize: Theme.px(12)
                font.weight: Font.Medium
                ToolTip.visible: runUsedHover.hovered && root.runUsed !== ""
                ToolTip.text: "This plan's helpers and their reviews used " + root.runUsed + "."
                ToolTip.delay: 500
                HoverHandler { id: runUsedHover }
                readonly property string used: root.runUsed
            }
            // Folded: the step in hand, or the next one, at the right by the
            // toggle (open, the list shows them).
            RowLayout {
                id: planFolded
                objectName: "planFoldedStep"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.leftMargin: 4
                spacing: 6
                visible: !root.expanded && (root.next !== null || root.allDone)
                Item { Layout.fillWidth: true }
                Rectangle {
                    visible: !!root.current
                    implicitWidth: 6
                    implicitHeight: 6
                    radius: 3
                    color: Theme.primary
                }
                Label {
                    visible: !root.current && !root.allDone
                    text: "Next"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                PlainLabel {
                    // (Its own width while it fits; a long title elides.)
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Layout.maximumWidth: implicitWidth
                    text: root.allDone ? "All done" : root.next ? root.next.title : ""
                    color: root.allDone ? Theme.textMuted : Theme.text
                    font.pixelSize: Theme.px(12.5)
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }
            }
            Item { visible: !planFolded.visible; Layout.fillWidth: true }
            // In Plan: switch to Build and start on the plan.
            FlatButton {
                objectName: "planBuild"
                visible: root.planning && !root.run && root.finished < root.steps.length
                enabled: !root.agentBusy
                text: "Build"
                variant: "primary"
                onClicked: root.build()
                ToolTip.visible: hovered
                ToolTip.text: root.agentBusy ? "Once the agent has finished planning" : "Switch to Build and have the agent work through the plan"
                ToolTip.delay: 500
            }
            Button {
                id: toggle
                objectName: "planToggle"
                flat: true
                implicitWidth: 24
                implicitHeight: 24
                padding: 0
                onClicked: {
                    root.expanded = !root.expanded
                    root.editing = -1
                    const hand = root.foldedByHand
                    hand[root.sessionKey] = !root.expanded
                    root.foldedByHand = hand
                }
                contentItem: Item { Icon { anchors.centerIn: parent; name: "arrow-down-s"; rotation: root.expanded ? 180 : 0; color: toggle.hovered ? Theme.foreground : Theme.muted; implicitWidth: 18; implicitHeight: 18 } }
                background: Rectangle { radius: 5; color: toggle.hovered ? Theme.hover : "transparent" }
                ToolTip.visible: hovered
                ToolTip.text: root.expanded ? "Hide the plan" : "Show and edit the plan"
            }
        }

        // Its steps (a long plan scrolls, so the chat keeps its room).
        Flickable {
            id: stepsView
            Layout.fillWidth: true
            // (Rows reach toward the card's edges, their marks under the
            // header's icon.)
            Layout.leftMargin: -6
            Layout.rightMargin: -6
            Layout.preferredHeight: Math.min(stepsCol.implicitHeight, root.maxStepsHeight)
            visible: root.expanded
            contentHeight: stepsCol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: stepsView.contentHeight > stepsView.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            ColumnLayout {
                id: stepsCol
                width: stepsView.width
                spacing: 2
                // The finished steps before the first to do, in a line that
                // shows them.
                Button {
                    id: finishedToggle
                    objectName: "planFinishedToggle"
                    visible: root.leadFinished > 0
                    Layout.fillWidth: true
                    implicitHeight: 26
                    flat: true
                    padding: 0
                    leftPadding: 6
                    onClicked: root.showFinished = !root.showFinished
                    Accessible.name: (root.showFinished ? "Hide " : "Show ") + root.leadFinished + " finished steps"
                    contentItem: RowLayout {
                        spacing: 8
                        Icon {
                            name: "arrow-right-s"
                            rotation: root.showFinished ? 90 : 0
                            color: finishedToggle.hovered ? Theme.foreground : Theme.textMuted
                            implicitWidth: 14
                            implicitHeight: 14
                        }
                        Label {
                            Layout.fillWidth: true
                            text: root.leadFinished === 1 ? "1 finished step" : root.leadFinished + " finished steps"
                            color: finishedToggle.hovered ? Theme.foreground : Theme.textMuted
                            font.pixelSize: Theme.px(12)
                            font.features: { "tnum": 1 }
                        }
                    }
                    background: Rectangle { radius: 6; color: finishedToggle.hovered ? Theme.hover : "transparent" }
                }
                Repeater {
                    // (The folded finished steps aren't made until shown: a
                    // long plan's are most of it.)
                    model: !root.expanded ? [] : root.showFinished ? root.steps : root.steps.slice(root.leadFinished)
                    Item {
                        id: stepRow
                        required property var modelData
                        required property int index
                        readonly property bool open: modelData.status === "pending" || modelData.status === "active"
                        // (The session reviewing it, while its review runs.)
                        readonly property string reviewer: root.run && root.run.reviewing ? (root.run.reviewing[String(modelData.id)] || "") : ""
                        objectName: "planStepRow"
                        // Its place in the plan.
                        readonly property int at: index + (root.showFinished ? 0 : root.leadFinished)
                        readonly property bool pausedHere: root.pausedStep === at + 1
                        readonly property bool active: modelData.status === "active"
                        readonly property bool finishedStep: modelData.status === "done" || modelData.status === "skipped"
                        Layout.fillWidth: true
                        implicitHeight: rowCol.implicitHeight > 0 ? rowCol.implicitHeight + 8 : 0
                        HoverHandler { id: stepHover }
                        // The step in hand stands out; the others take the hover.
                        Rectangle {
                            anchors.fill: parent
                            radius: 6
                            color: stepRow.active ? Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, Theme.isLight ? 0.07 : 0.09)
                                : stepHover.hovered ? Theme.hover : "transparent"
                        }
                        ColumnLayout {
                            id: rowCol
                            x: 6
                            y: 4
                            width: parent.width - 12
                            spacing: 2
                            RowLayout {
                                Layout.fillWidth: true
                                visible: root.editing !== stepRow.modelData.id
                                spacing: 8
                                // Its mark on the plan's line (as a run's steps):
                                // done a check, the one in hand an accent dot,
                                // to do a ring, skipped a cross.
                                Item {
                                    implicitWidth: 14
                                    implicitHeight: 14
                                    Layout.fillHeight: true
                                    Layout.alignment: Qt.AlignTop
                                    Layout.topMargin: 2
                                    Rectangle {
                                        // (To the next step's mark, across the gap.)
                                        visible: stepRow.at < root.steps.length - 1
                                        x: 6.5
                                        y: 16
                                        width: 1
                                        height: Math.max(0, parent.height - 6)
                                        color: Theme.border
                                    }
                                    Rectangle {
                                        visible: stepRow.active
                                        anchors.centerIn: mark
                                        width: 14
                                        height: 14
                                        radius: 7
                                        color: Theme.accentSoft
                                    }
                                    Rectangle {
                                        id: mark
                                        x: 2
                                        y: 2
                                        width: 10
                                        height: 10
                                        radius: 5
                                        color: stepRow.active ? Theme.primary : "transparent"
                                        border.width: stepRow.modelData.status === "pending" ? 1.5 : 0
                                        border.color: Theme.textMuted
                                    }
                                    Icon {
                                        visible: stepRow.finishedStep
                                        anchors.centerIn: mark
                                        name: stepRow.modelData.status === "done" ? "check" : "close"
                                        color: stepRow.modelData.status === "done" ? Theme.textMuted : Theme.textFaint
                                        implicitWidth: 13
                                        implicitHeight: 13
                                    }
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
                                    PlainLabel {
                                        Layout.fillWidth: true
                                        text: stepRow.modelData.title + (stepRow.modelData.editedByUser ? "  ·  yours" : "")
                                        // (To do reads as the text it is; done steps step back.)
                                        color: stepRow.finishedStep ? Theme.textMuted : Theme.text
                                        font.pixelSize: Theme.px(12.5)
                                        font.weight: stepRow.active ? Font.Medium : Font.Normal
                                        font.strikeout: stepRow.modelData.status === "skipped"
                                        wrapMode: Text.Wrap
                                    }
                                    PlainLabel {
                                        id: stepText
                                        Layout.fillWidth: true
                                        visible: text !== ""
                                        // (A helper's report can run long: two lines, a click for all.)
                                        property bool full: false
                                        // (Its paragraphs as lines: no blank line in two.)
                                        text: String(stepRow.modelData.summary || stepRow.modelData.detail || "").replace(/\n\s*\n/g, "\n").trim()
                                        // The step the run stopped at: what went wrong, once, here.
                                        color: stepRow.pausedHere ? Theme.warning : stepRow.finishedStep ? Theme.textFaint : Theme.textMuted
                                        font.pixelSize: Theme.px(11.5)
                                        wrapMode: Text.Wrap
                                        maximumLineCount: full ? 1000 : 2
                                        elide: Text.ElideRight
                                        HoverHandler { enabled: stepText.truncated || stepText.full; cursorShape: Qt.PointingHandCursor }
                                        TapHandler { enabled: stepText.truncated || stepText.full; onTapped: stepText.full = !stepText.full }
                                    }
                                    // Its helper (a run), and the worktree it works in.
                                    // (This and the rows below are made only for
                                    // the steps that have them: a long plan opens
                                    // at once.)
                                    Loader {
                                        Layout.fillWidth: true
                                        active: (stepRow.modelData.helper || "") !== "" || (stepRow.modelData.branch || "") !== ""
                                        visible: active
                                        sourceComponent: RowLayout {
                                            spacing: 6
                                            Icon { name: "node-tree"; color: Theme.muted; implicitWidth: 12; implicitHeight: 12 }
                                            Label {
                                                Layout.fillWidth: true
                                                text: (stepRow.reviewer !== "" ? "Done, being reviewed" : stepRow.modelData.status === "active" ? "A helper is on it" : "Done by a helper") + ((stepRow.modelData.branch || "") !== "" ? " · ⎇ " + stepRow.modelData.branch : "")
                                                color: Theme.muted
                                                font.pixelSize: Theme.px(11)
                                                elide: Text.ElideRight
                                            }
                                            FlatButton {
                                                objectName: "planOpenHelper-" + stepRow.modelData.id
                                                // (Under review: the review's, below.)
                                                visible: (stepRow.modelData.helper || "") !== "" && stepRow.modelData.status === "active" && stepRow.reviewer === ""
                                                text: "Open"
                                                padding: 3
                                                font.pixelSize: Theme.px(12)
                                                onClicked: Workspace.open(stepRow.modelData.helper)
                                            }
                                            FlatButton {
                                                objectName: "planOpenReview-" + stepRow.modelData.id
                                                visible: stepRow.reviewer !== ""
                                                text: "Open review"
                                                padding: 3
                                                font.pixelSize: Theme.px(12)
                                                onClicked: Workspace.open(stepRow.reviewer)
                                            }
                                        }
                                    }
                                    // Who you chose to do it (a run with helpers).
                                    Loader {
                                        Layout.fillWidth: true
                                        active: (stepRow.modelData.agent || "") !== "" && stepRow.modelData.status === "pending"
                                        visible: active
                                        sourceComponent: RowLayout {
                                            spacing: 6
                                            Icon { name: "robot-2"; color: Theme.muted; implicitWidth: 12; implicitHeight: 12 }
                                            Label {
                                                objectName: "planStepWho-" + stepRow.modelData.id
                                                Layout.fillWidth: true
                                                Layout.maximumWidth: implicitWidth
                                                Layout.minimumWidth: 0
                                                text: "Done by " + root.agentName(stepRow.modelData.agent || "") + (stepRow.modelData.model ? " · " + root.modelName(stepRow.modelData.agent, stepRow.modelData.model) : "")
                                                color: Theme.muted
                                                font.pixelSize: Theme.px(11.5)
                                                elide: Text.ElideRight
                                            }
                                            // Undo the choice, right by it (on hover).
                                            FlatButton {
                                                objectName: "planStepWhoClear-" + stepRow.modelData.id
                                                opacity: stepHover.hovered || hovered || activeFocus ? 1 : 0
                                                text: "Use the run's helpers"
                                                outlined: false
                                                accent: Theme.textMuted
                                                padding: 2
                                                font.pixelSize: Theme.px(11.5)
                                                onClicked: root.edit({ op: "helper", id: stepRow.modelData.id, agent: "", model: "" })
                                            }
                                            Item { Layout.fillWidth: true }
                                        }
                                    }
                                    // Its commit, or why there isn't one.
                                    Loader {
                                        Layout.fillWidth: true
                                        active: (stepRow.modelData.commit || "") !== "" || (stepRow.modelData.commitNote || "") !== ""
                                        visible: active
                                        sourceComponent: RowLayout {
                                            spacing: 6
                                            Icon { visible: (stepRow.modelData.commit || "") !== ""; name: "git-commit"; color: Theme.textMuted; implicitWidth: 12; implicitHeight: 12 }
                                            Label {
                                                Layout.fillWidth: true
                                                text: (stepRow.modelData.commit || "") !== "" ? "Committed " + stepRow.modelData.commit.substring(0, 8) : (stepRow.modelData.commitNote || "")
                                                color: (stepRow.modelData.commit || "") !== "" ? Theme.textMuted : (stepRow.modelData.commitNote || "").startsWith("Not committed") ? Theme.warning : Theme.muted
                                                font.family: (stepRow.modelData.commit || "") !== "" ? Theme.mono : Theme.uiFont
                                                font.pixelSize: Theme.px(11)
                                                wrapMode: Text.Wrap
                                            }
                                            FlatButton {
                                                objectName: "planReview-" + stepRow.modelData.id
                                                visible: (stepRow.modelData.commit || "") !== ""
                                                text: "Review…"
                                                padding: 3
                                                font.pixelSize: Theme.px(12)
                                                onClicked: { root.reviewStepId = stepRow.modelData.id; reviewMenu.popup() }
                                            }
                                        }
                                    }
                                }
                                // Your edits, on hover: over the step's right end (they
                                // take no room, so the text doesn't rewrap and the
                                // step's own buttons stay at the right edge).
                                Item {
                                    Layout.preferredWidth: 0
                                    Layout.alignment: Qt.AlignTop
                                    implicitHeight: 24
                                    Loader {
                                        anchors.right: parent.right
                                        anchors.top: parent.top
                                        anchors.topMargin: -2
                                        z: 2
                                        // (Made while the step is hovered.)
                                        active: stepHover.hovered
                                        sourceComponent: Rectangle {
                                            width: editRow.width + 8
                                            height: editRow.height + 4
                                            radius: 7
                                            color: root.color
                                            border.color: Theme.border
                                            Row {
                                                id: editRow
                                                objectName: "planStepEdits"
                                                anchors.centerIn: parent
                                                spacing: 0
                                                StepButton { iconName: "arrow-up-s"; tip: "Move up"; enabled: stepRow.at > 0; onClicked: root.edit({ op: "shift", id: stepRow.modelData.id, by: -1 }) }
                                                StepButton { iconName: "arrow-down-s"; tip: "Move down"; enabled: stepRow.at < root.steps.length - 1; onClicked: root.edit({ op: "shift", id: stepRow.modelData.id, by: 1 }) }
                                                StepButton {
                                                    visible: stepRow.at > 0 && stepRow.modelData.status === "pending"
                                                    iconName: "layout-column"
                                                    tip: stepRow.modelData.alongside ? "Run it after the step before" : "Run it at the same time as the step before (with helpers)"
                                                    onClicked: root.edit({ op: "alongside", id: stepRow.modelData.id, alongside: !stepRow.modelData.alongside })
                                                }
                                                StepButton {
                                                    objectName: "planStepHelper-" + stepRow.modelData.id
                                                    visible: stepRow.modelData.status === "pending" && Workspace.orchestrationOn
                                                    iconName: "robot-2"
                                                    tip: "Who does it, when the plan runs with helpers"
                                                    onClicked: root.chooseStepHelper(stepRow.modelData.id, stepRow.modelData.agent || "", stepRow.modelData.model || "")
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
                            }
                        }
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
            objectName: "planPaused"
            // About a step: that step says what went wrong; this says what to do.
            text: root.pausedStep > 0 ? "Paused at step " + root.pausedStep + ". Run it again, or skip or change the step first."
                : root.run && root.run.paused ? "Paused: " + root.run.paused + "." : ""
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.fillWidth: true
            visible: root.expanded && root.editing < 0
            spacing: 8
            // Each step to a helper session, stage by stage.
            FlatButton {
                objectName: "planRun"
                // (Helpers are sessions harlo starts: orchestration's.)
                visible: Workspace.orchestrationOn && (!root.run || root.paused) && root.finished < root.steps.length
                text: root.paused ? "Run again" : "Run with helpers"
                // (Not while the agent works through it itself: two at once.)
                enabled: !root.agentBusy
                variant: "primary"
                onClicked: {
                    root.helpers = JSON.parse(Workspace.helpersJson(root.session.key))
                    runMenu.popup()
                }
                ToolTip.visible: hovered
                ToolTip.text: "harlo gives each step to a helper session: in order, and steps marked to run with the one before at the same time, each in a worktree."
                ToolTip.delay: 500
                AppMenu {
                    id: runMenu
                    AppMenuItem {
                        objectName: "planRunTeam"
                        text: "Helpers: " + root.helpersLabel
                        hint: root.helpers.team ? "the team" : "as this session"
                        onTriggered: {
                            const err = Workspace.runPlan(root.session.key, "", "")
                            if (err !== "") Workspace.showNotice(err, true)
                        }
                    }
                    AppMenuSeparator { }
                    AppMenuItem {
                        objectName: "planRunOther"
                        text: "Other agent or model…"
                        hint: "this run"
                        onTriggered: root.chooseHelpers()
                    }
                }
            }
            FlatButton {
                objectName: "planStopRun"
                visible: !!root.run
                text: "Stop the run"
                onClicked: Workspace.stopPlanRun(root.session.key)
                ToolTip.visible: hovered
                ToolTip.text: "No more steps start; helpers at work finish theirs."
                ToolTip.delay: 500
            }
            FlatButton {
                objectName: "planAddStep"
                text: "Add a step"
                onClicked: { root.editing = 0; addAfter.value = -1; editTitle.text = ""; editDetail.text = ""; editTitle.forceActiveFocus() }
            }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.run ? "" : root.agentBusy && root.finished < root.steps.length ? "Run with helpers once the agent stops (or when you stop it)."
                    : "The agent reads your changes before each step."
                color: Theme.textFaint
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
            }
            FlatButton {
                text: "Clear"
                outlined: false
                accent: Theme.textMuted
                onClicked: root.edit({ op: "clear" })
            }
        }
    }

    // A done step's "Review…": one menu for every step.
    AppMenu {
        id: reviewMenu
        Repeater {
            model: root.reviewers
            AppMenuItem {
                required property var modelData
                text: "Review with " + modelData.label
                onTriggered: {
                    const err = Workspace.reviewStep(root.session.key, root.reviewStepId, modelData.id)
                    if (err !== "") Workspace.showNotice(err, true)
                }
            }
        }
    }

    component StepButton: Button {
        id: sb
        property string iconName
        property string tip
        flat: true
        implicitWidth: 24
        implicitHeight: 24
        // (The style's padding would leave the icon a few pixels.)
        padding: 0
        contentItem: Item { Icon { anchors.centerIn: parent; name: sb.iconName; color: !sb.enabled ? Theme.textFaint : sb.hovered ? Theme.foreground : Theme.muted; implicitWidth: 15; implicitHeight: 15 } }
        background: Rectangle { radius: 5; color: sb.hovered ? Theme.hover : "transparent" }
        ToolTip.visible: hovered
        ToolTip.text: tip
        ToolTip.delay: 500
    }
}
