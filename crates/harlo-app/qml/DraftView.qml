import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Effects
import harlo

// The new-session page: pick a project, where to work (current checkout or
// a new worktree) and the harness, write the first message or pick a
// starter. Nothing starts until the first message is sent.
Rectangle {
    id: root
    color: Theme.background

    property int projectId: -1
    property string harness: ""          // "" = the project's default
    property bool useWorktree: false
    property string worktreeName: ""
    property string starter: ""          // starter id
    property var attachments: []
    property bool askFirst: false
    // The shield: what runs without asking ("" the mode it starts in).
    property string approval: ""
    readonly property string startApproval: Workspace.modesRevision >= 0 && !parallel ? Workspace.draftApproval(effectiveHarness, projectId) : ""
    readonly property string shownApproval: startApproval === "" ? "" : approval !== "" ? approval : startApproval
    function cycleApproval() {
        const modes = ["always-ask", "write", "yolo"]
        approval = modes[(modes.indexOf(shownApproval) + 1) % modes.length]
    }
    // omp: start the session as a goal (the text is its objective).
    property bool asGoal: false
    property alias goalBudget: goalBudgetField.text
    /// "200k", "1.5m", "50000" → tokens; 0 for none, -1 when unreadable.
    function parseBudget(t) {
        const m = t.trim().toLowerCase().replace(/[, _]/g, "").match(/^(\d+(?:\.\d+)?)([km]?)$/)
        if (t.trim() === "") return 0
        if (!m) return -1
        return Math.round(parseFloat(m[1]) * (m[2] === "k" ? 1e3 : m[2] === "m" ? 1e6 : 1))
    }
    /// Less than this is gone in a single turn (the system prompt alone
    /// is several thousand tokens); the budget field warns.
    readonly property int minUsefulBudget: 10000
    readonly property bool goalOn: asGoal && !parallel && (effectiveHarness === "omp" || effectiveHarness === "demo")
    onAsGoalChanged: if (asGoal) parallel = false
    // Run in parallel: one prompt to several lanes (harness + model each).
    property bool parallel: false
    property var lanes: [{ harness: "omp", model: "" }, { harness: "omp", model: "" }]
    property bool laneWorktrees: true
    property var knownModels: []
    onParallelChanged: {
        if (parallel && knownModels.length === 0) knownModels = JSON.parse(Workspace.knownModels())
        if (parallel) asGoal = false
    }
    function setLane(i, field, value) {
        const next = lanes.map(l => Object.assign({}, l))
        next[i][field] = value
        lanes = next
    }
    signal startedSession(string key, bool askFirst)
    signal addProjectRequested()

    // Re-read when the page opens: projects and branches change elsewhere.
    property var projects: []
    property var branches: []
    function refresh() {
        projects = JSON.parse(Workspace.projectsJson())
        defaultAgent = Workspace.defaultAgent()
        if (model === "") model = defaultModel()
        branches = []
        // (Read off the UI thread: a remote project's over SSH.)
        if (projects.some(p => p.id === projectId)) Workspace.requestBranches(projectId)
    }
    Connections {
        target: Workspace
        function onBranchesReady(id, list) { if (id === root.projectId) root.branches = list }
        function onProvidersRevisionChanged() { root.defaultAgent = Workspace.defaultAgent() }
    }
    onVisibleChanged: if (visible) refresh()
    Component.onCompleted: refresh()
    readonly property var project: projects.find(p => p.id === projectId) || null
    readonly property string currentBranch: branches.length > 0 ? branches[0] : ""
    readonly property bool onDefaultBranch: currentBranch === "main" || currentBranch === "master"
    // (What harlo starts when nothing is picked: Settings' choice, or the
    // first provider on and installed.)
    property string defaultAgent: Workspace.defaultAgent()
    readonly property string effectiveHarness: harness !== "" ? harness : project ? project.harness : defaultAgent
    /// The model to start with (`provider/model` as the agent takes it; ""
    /// the agent's own), from what the agent offers here (Settings →
    /// Providers reads it); the agent's default model from Settings (the
    /// setup guide) until one is picked. Not for a remote project (its host's
    /// agent).
    property string model: ""
    function defaultModel() { return project && project.remote ? "" : Workspace.setting("default_model_" + effectiveHarness) }
    onEffectiveHarnessChanged: model = defaultModel()
    readonly property var modelChoices: {
        if (project && project.remote) return []
        const p = (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === effectiveHarness)
        if (!p) return []
        // Models served on this computer or the local network last, under
        // "Local" in the list.
        const all = (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 }))
        return all.filter(m => !m.local).concat(all.filter(m => m.local))
    }
    readonly property string modelName: { const m = modelChoices.find(x => x.id === model); return m ? m.name : model }
    readonly property var starters: JSON.parse(Workspace.starters())
    // Your templates (a set-up and a first message, saved under a name).
    readonly property var templates: Workspace.templatesJson !== "" ? JSON.parse(Workspace.templatesJson) : []
    /// Fills the draft from a template: nothing starts until it's sent.
    function applyTemplate(t) {
        parallel = false
        starter = ""
        harness = t.harness
        model = t.model !== "" ? t.model : defaultModel()
        approval = t.approval
        askFirst = t.askFirst
        asGoal = t.goal
        useWorktree = t.worktree && currentBranch !== ""
        if (useWorktree && worktreeName === "") worktreeName = Workspace.suggestBranch()
        input.text = t.text
        input.forceActiveFocus()
        input.cursorPosition = input.length
    }
    /// The draft as a template (what it would start with now).
    function draftTemplate() {
        return { name: "", harness: effectiveHarness, model: model, approval: shownApproval, askFirst: askFirst,
                 goal: goalOn, worktree: useWorktree, text: input.text }
    }
    /// "Claude · claude-opus-4 · edits · goal · new worktree"
    function templateSummary(t) {
        const modes = { "always-ask": "asks before changes", "write": "edits freely", "yolo": "runs without asking" }
        return [t.harness ? Theme.harnessLabel(t.harness) : "", (t.model || "").replace(/^.*\//, ""), modes[t.approval] || "",
                t.goal ? "goal" : "", t.worktree ? "new worktree" : "", t.askFirst ? "Ask first" : ""].filter(x => x !== "").join(" · ")
    }
    readonly property var starterInfo: starters.find(s => s.id === starter) || null
    readonly property bool canSend: goalOn ? (input.text.trim() !== "" || starter !== "") && parseBudget(goalBudget) >= 0
                                           : input.text.trim() !== "" || starter !== "" || attachments.length > 0

    // Opens the page for a project (-1: home folder), keeping any draft text.
    function openFor(id) {
        if (id !== projectId) {
            projectId = id
            harness = ""
            useWorktree = false
        }
        refresh()
        input.forceActiveFocus()
    }
    function reset() {
        input.text = ""
        starter = ""
        attachments = []
        useWorktree = false
    }
    function send() {
        if (!canSend)
            return
        if (parallel) {
            Workspace.startRun(JSON.stringify({
                projectId: projectId, prompt: input.text, starter: starter, attachments: attachments, askFirst: askFirst,
                worktrees: laneWorktrees && currentBranch !== "", base: currentBranch, lanes: lanes
            }))
            reset()
            parallel = false
            startedSession("", false)
            return
        }
        if (useWorktree && currentBranch === "")
            useWorktree = false
        if (useWorktree && worktreeName.trim() === "")
            worktreeName = Workspace.suggestBranch()
        const key = Workspace.startDraft(JSON.stringify({
            projectId: projectId, harness: effectiveHarness, worktreeBranch: useWorktree ? worktreeName.trim() : "",
            base: currentBranch, starter: starter, text: input.text, attachments: attachments, askFirst: askFirst,
            approval: startApproval === "" ? "" : approval, goal: goalOn, goalBudget: Math.max(0, parseBudget(goalBudget)), model: model
        }))
        const ask = askFirst && !goalOn
        reset()
        asGoal = false
        goalBudget = ""
        startedSession(key, ask)
    }
    // Test hooks.
    property alias text: input.text
    readonly property alias input: input
    readonly property alias templateMenu: templateMenu
    readonly property alias templateDialog: templateDialog

    FileDialog {
        id: attachDialog
        title: "Attach files"
        fileMode: FileDialog.OpenFiles
        currentFolder: root.project && !Workspace.projectRoots(root.projectId)[0].startsWith("ssh://") ? "file://" + Workspace.projectRoots(root.projectId)[0] : ""
        onAccepted: {
            const add = selectedFiles.map(f => decodeURIComponent(f.toString().replace(/^file:\/\//, "")))
            root.attachments = root.attachments.concat(add.filter(p => !root.attachments.includes(p)))
        }
    }

    AppMenu {
        id: projectMenu
        Repeater {
            model: root.projects
            AppMenuItem {
                required property var modelData
                text: modelData.name
                marked: root.projectId === modelData.id
                onTriggered: root.openFor(modelData.id)
            }
        }
        AppMenuItem { text: "Home folder"; marked: root.projectId === -1; onTriggered: root.openFor(-1) }
        AppMenuSeparator { }
        AppMenuItem { text: "Add project…"; onTriggered: root.addProjectRequested() }
    }
    AppMenu {
        id: branchMenu
        AppMenuItem { text: "Current checkout (" + root.currentBranch + ")"; marked: !root.useWorktree; onTriggered: root.useWorktree = false }
        AppMenuItem {
            text: "New worktree"
            marked: root.useWorktree
            onTriggered: { root.useWorktree = true; if (root.worktreeName === "") root.worktreeName = Workspace.suggestBranch() }
        }
    }
    AppMenu {
        id: harnessMenu
        Repeater {
            // (A remote project: the agents its host has.)
            model: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).filter(p => !(root.project && root.project.remote) || Theme.offeredOnHost(root.project.host, p.id))
            AppMenuItem {
                required property var modelData
                text: modelData.label
                provider: modelData.id
                marked: root.effectiveHarness === modelData.id
                onTriggered: root.harness = modelData.id
            }
        }
    }

    AppMenu {
        id: templateMenu
        property var template: null
        AppMenuItem { text: "Rename…"; onTriggered: templateDialog.edit(templateMenu.template, false) }
        AppMenuItem { objectName: "templateUpdate"; text: "Replace with this draft"; onTriggered: templateDialog.edit(templateMenu.template, true) }
        AppMenuSeparator { }
        AppMenuItem { objectName: "templateDelete"; text: "Delete"; onTriggered: Workspace.deleteTemplate(templateMenu.template.id) }
    }
    // Saves the draft as a template, renames one, or replaces one with the draft.
    AppDialog {
        id: templateDialog
        objectName: "templateDialog"
        property var base: ({})
        title: base.id ? "Edit template" : "Save as template"
        width: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)
        function saveNew() {
            base = root.draftTemplate()
            templateName.text = root.input.text.trim().split(/\s+/).slice(0, 5).join(" ").slice(0, 40)
            open()
            templateName.forceActiveFocus()
            templateName.selectAll()
        }
        function edit(t, fromDraft) {
            base = fromDraft ? Object.assign(root.draftTemplate(), { id: t.id }) : t
            templateName.text = t.name
            open()
            templateName.forceActiveFocus()
        }
        footer: AppButtonBox {
            DialogButton { objectName: "templateSave"; text: "Save"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole; enabled: templateName.text.trim() !== "" }
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onAccepted: Workspace.saveTemplate(JSON.stringify(Object.assign({}, base, { name: templateName.text.trim() })))
        ColumnLayout {
            width: parent.width
            spacing: 8
            AppTextField {
                id: templateName
                objectName: "templateName"
                Layout.fillWidth: true
                placeholderText: "Name, e.g. Review a pull request"
                onAccepted: if (text.trim() !== "") templateDialog.accept()
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                text: (root.templateSummary(templateDialog.base) || "The project's agent") + "\n" + "Its first message: " + ((templateDialog.base.text || "").trim() === "" ? "none" : "“" + templateDialog.base.text.trim().slice(0, 140) + (templateDialog.base.text.trim().length > 140 ? "…" : "") + "”")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
                text: "Picking it fills the new-session page (and the iPhone app's New Session); you look it over and send. The same name replaces a template."
            }
        }
    }

    component Chip: Button {
        id: chip
        property string iconName
        property color iconColor: Theme.muted
        /// A project's icon instead (as ProjectIcon takes it).
        property bool projectLook: false
        property string projectIcon: ""
        property string projectColor: ""
        property bool arrow: true
        flat: true
        implicitHeight: 30
        leftPadding: 10
        rightPadding: 8
        font.pixelSize: Theme.px(13)
        contentItem: Row {
            spacing: 7
            Icon { visible: !chip.projectLook; anchors.verticalCenter: parent.verticalCenter; name: chip.iconName; color: chip.iconColor; implicitWidth: 15; implicitHeight: 15 }
            ProjectIcon { visible: chip.projectLook; anchors.verticalCenter: parent.verticalCenter; icon: chip.projectIcon; color: chip.projectColor; fallback: Theme.info; implicitWidth: 15; implicitHeight: 15 }
            Label { anchors.verticalCenter: parent.verticalCenter; text: chip.text; color: Theme.foreground; font: chip.font }
            Icon { visible: chip.arrow; anchors.verticalCenter: parent.verticalCenter; name: "arrow-down-s"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
        }
        background: ControlSurface {
            control: chip
            Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.color: Theme.primary; visible: chip.visualFocus }
        }
    }

    Flickable {
        anchors.fill: parent
        contentHeight: Math.max(height, page.implicitHeight + 80)
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: page
            width: Math.min(parent.width - 48, 780)
            x: (parent.width - width) / 2
            y: Math.max(40, (parent.height - implicitHeight) / 2 - 40)
            spacing: 14

            Label {
                Layout.fillWidth: true
                Layout.bottomMargin: 8
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.StyledText
                text: "What are we working on" + (root.project ? " in <b>" + root.project.name + "</b>" : "") + "?"
                color: Theme.foreground
                font.pixelSize: Theme.px(26)
                wrapMode: Text.Wrap
            }

            // Project, where, harness
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Chip {
                    iconName: "home"
                    iconColor: Theme.info
                    projectLook: root.project !== null
                    projectIcon: root.project ? root.project.icon : ""
                    projectColor: root.project ? root.project.color : ""
                    text: root.project ? root.project.name : "Home folder"
                    onClicked: projectMenu.popup(this, 0, height + 4)
                }
                Chip {
                    visible: root.currentBranch !== ""
                    iconName: root.useWorktree ? "git-branch" : root.onDefaultBranch ? "alert" : "git-branch"
                    iconColor: root.useWorktree ? Theme.success : root.onDefaultBranch ? Theme.warning : Theme.muted
                    text: root.useWorktree ? "New worktree" : root.currentBranch
                    onClicked: branchMenu.popup(this, 0, height + 4)
                    ToolTip.visible: hovered && root.onDefaultBranch && !root.useWorktree
                    ToolTip.text: "Working directly on " + root.currentBranch + ". Pick “New worktree” to keep it untouched."
                    ToolTip.delay: 400
                }
                AppTextField {
                    visible: root.useWorktree
                    width: 220
                    implicitHeight: 30
                    text: root.worktreeName
                    placeholderText: "branch name"
                    font.family: Theme.mono
                    font.pixelSize: Theme.px(12)
                    onTextEdited: root.worktreeName = text
                }
                Chip {
                    objectName: "draftHarness"
                    iconName: "harness-" + root.effectiveHarness
                    iconColor: Theme.foreground
                    text: Theme.harnessLabel(root.effectiveHarness)
                    onClicked: harnessMenu.popup(this, 0, height + 4)
                }
                Chip {
                    id: modelChip
                    objectName: "draftModel"
                    visible: !root.parallel && !(root.project && root.project.remote)
                    iconName: "sparkling"
                    iconColor: root.model !== "" ? Theme.primary : Theme.muted
                    text: root.model !== "" ? root.modelName : "Default model"
                    // (The agents' models are read when first asked for.)
                    onClicked: { Workspace.ensureProviders(); modelPopup.open() }
                    ToolTip.visible: hovered && root.model === ""
                    ToolTip.text: Theme.harnessLabel(root.effectiveHarness) + " starts with its own default model. Pick one to start with another."
                    ToolTip.delay: 400
                    // The models the agent offers here, filtered as you type.
                    Popup {
                        id: modelPopup
                        y: parent.height + 4
                        width: 400
                        height: Math.min(380, 60 + Math.max(1, draftModels.count) * 30 + (root.modelChoices.some(m => m.local) ? 30 : 0))
                        padding: 8
                        background: GlassSurface { radius: 10 }
                        onOpened: { draftModelFilter.text = ""; draftModelFilter.forceActiveFocus() }
                        ColumnLayout {
                            anchors.fill: parent
                            AppTextField {
                                id: draftModelFilter
                                objectName: "draftModelFilter"
                                Layout.fillWidth: true
                                placeholderText: "Filter models"
                                Keys.onDownPressed: draftModels.incrementCurrentIndex()
                                Keys.onUpPressed: draftModels.decrementCurrentIndex()
                                onAccepted: draftModels.pick(draftModels.currentIndex)
                            }
                            Label {
                                visible: root.modelChoices.length === 0
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                text: Workspace.providersJson().indexOf('"checking"') >= 0 ? "Reading " + Theme.harnessLabel(root.effectiveHarness) + "'s models…"
                                    : Theme.harnessLabel(root.effectiveHarness) + " has no models to pick here. Sign it in to a provider in Settings → Providers."
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                            }
                            ListView {
                                id: draftModels
                                objectName: "draftModels"
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                clip: true
                                highlightMoveDuration: 0
                                model: [{ id: "", name: "Default model" }].concat(root.modelChoices)
                                    .filter(m => m.id === "" ? draftModelFilter.text === "" : (m.id + " " + m.name).toLowerCase().includes(draftModelFilter.text.toLowerCase()))
                                function pick(i) {
                                    const m = model[i]
                                    if (m) { root.model = m.id; modelPopup.close() }
                                }
                                delegate: ItemDelegate {
                                    id: dmRow
                                    required property var modelData
                                    required property int index
                                    objectName: "draftModel-" + modelData.id
                                    // The first local model: "Local" above it.
                                    readonly property real heading: !!modelData.local && (index === 0 || !ListView.view.model[index - 1].local) ? 30 : 0
                                    width: ListView.view.width
                                    height: 30 + heading
                                    topInset: heading
                                    topPadding: heading + bottomPadding
                                    highlighted: ListView.isCurrentItem
                                    background: Rectangle { radius: 4; color: dmRow.highlighted ? Theme.selected : dmRow.hovered ? Theme.hover : "transparent" }
                                    Label {
                                        visible: dmRow.heading > 0
                                        x: 8
                                        y: dmRow.heading - height - 3
                                        text: "LOCAL"
                                        color: Theme.label
                                        font.pixelSize: Theme.px(11)
                                        font.weight: Font.Medium
                                        font.letterSpacing: Theme.px(11) * 0.07
                                    }
                                    contentItem: RowLayout {
                                        Label {
                                            text: dmRow.modelData.name + (dmRow.modelData.id === root.model ? "  ✓" : "")
                                            color: dmRow.modelData.id === "" ? Theme.muted : Theme.foreground
                                            font.pixelSize: Theme.px(13)
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }
                                        Label {
                                            visible: dmRow.modelData.id.indexOf("/") > 0
                                            text: dmRow.modelData.id.split("/")[0]
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(11)
                                        }
                                    }
                                    onClicked: draftModels.pick(index)
                                }
                            }
                        }
                    }
                }
            }

            // Lanes of a parallel run
            Rectangle {
                visible: root.parallel
                Layout.fillWidth: true
                implicitHeight: lanesCol.implicitHeight + 20
                radius: 12
                color: Theme.elevated
                border.color: Theme.border
                ColumnLayout {
                    id: lanesCol
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 6
                    RowLayout {
                        spacing: 8
                        Icon { name: "node-tree"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
                        Label { text: "Run in parallel"; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true }
                        Label { text: "the same prompt, one session per lane"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                    }
                    Repeater {
                        model: root.lanes
                        RowLayout {
                            id: laneRow
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: 6
                            Rectangle {
                                implicitWidth: 22
                                implicitHeight: 22
                                radius: 6
                                color: Theme.selected
                                Label { anchors.centerIn: parent; text: String.fromCharCode(65 + laneRow.index); color: Theme.primary; font.pixelSize: Theme.px(12); font.bold: true }
                            }
                            Repeater {
                                model: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).map(p => p.id)
                                Button {
                                    id: hb
                                    required property string modelData
                                    readonly property bool on: laneRow.modelData.harness === modelData
                                    flat: true
                                    implicitHeight: 26
                                    leftPadding: 9
                                    rightPadding: 9
                                    onClicked: root.setLane(laneRow.index, "harness", modelData)
                                    contentItem: Row {
                                        spacing: 5
                                        ProviderMark { anchors.verticalCenter: parent.verticalCenter; provider: hb.modelData; tip: ""; implicitWidth: 12; implicitHeight: 12; color: hb.on ? Theme.foreground : Theme.muted }
                                        Label { anchors.verticalCenter: parent.verticalCenter; text: Theme.harnessLabel(hb.modelData); color: hb.on ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(12) }
                                    }
                                    background: ControlSurface { control: hb; variant: hb.on ? "selected" : "outline"; radius: 7 }
                                }
                            }
                            AppTextField {
                                id: modelField
                                Layout.fillWidth: true
                                implicitHeight: 28
                                text: laneRow.modelData.model
                                placeholderText: "model (harness default)"
                                font.family: Theme.mono
                                font.pixelSize: Theme.px(12)
                                onTextEdited: root.setLane(laneRow.index, "model", text)
                            }
                            FlatButton {
                                text: "▾"
                                padding: 4
                                font.pixelSize: Theme.px(12)
                                onClicked: modelMenu.popup(this, 0, height + 2)
                                ToolTip.visible: hovered
                                ToolTip.text: "Models you've used"
                                AppMenu {
                                    id: modelMenu
                                    AppMenuItem { text: "Harness default"; onTriggered: root.setLane(laneRow.index, "model", "") }
                                    Repeater {
                                        model: root.knownModels.filter(m => m.harness === laneRow.modelData.harness)
                                        AppMenuItem {
                                            required property var modelData
                                            text: modelData.model
                                            marked: laneRow.modelData.model === modelData.model
                                            onTriggered: root.setLane(laneRow.index, "model", modelData.model)
                                        }
                                    }
                                }
                            }
                            Button {
                                id: removeLane
                                visible: root.lanes.length > 2
                                flat: true
                                implicitWidth: 24
                                implicitHeight: 24
                                onClicked: root.lanes = root.lanes.filter((_, i) => i !== laneRow.index)
                                contentItem: Icon { name: "close"; color: removeLane.hovered ? Theme.foreground : Theme.muted }
                                background: Rectangle { radius: 5; color: removeLane.hovered ? Theme.hover : "transparent" }
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        FlatButton {
                            visible: root.lanes.length < 4
                            text: "+ Add lane"
                            font.pixelSize: Theme.px(12)
                            onClicked: root.lanes = root.lanes.concat([{ harness: root.lanes[root.lanes.length - 1].harness, model: "" }])
                        }
                        AppCheckBox {
                            visible: root.currentBranch !== ""
                            text: "Each lane in its own worktree"
                            checked: root.laneWorktrees
                            onToggled: root.laneWorktrees = checked
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            // Composer
            Rectangle {
                id: draftCard
                Layout.fillWidth: true
                implicitHeight: box.implicitHeight + 20
                radius: 10
                color: Theme.elevated
                border.color: input.activeFocus ? Theme.primary : Theme.border
                // The chat composer's shadow.
                RectangularShadow {
                    z: -1
                    anchors.fill: parent
                    radius: draftCard.radius
                    offset.y: Theme.isLight ? 12 : 14
                    blur: Theme.isLight ? 28 : 32
                    spread: -18
                    color: Qt.rgba(0, 0, 0, Theme.isLight ? 0.4 : 0.75)
                }
                ColumnLayout {
                    id: box
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 8
                    // Chosen starter and attachments
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        visible: root.starterInfo !== null || root.attachments.length > 0
                        Rectangle {
                            visible: root.starterInfo !== null
                            implicitHeight: 26
                            implicitWidth: starterRow.implicitWidth + 14
                            radius: 13
                            color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.14)
                            border.color: Theme.primary
                            Row {
                                id: starterRow
                                anchors.centerIn: parent
                                spacing: 6
                                Icon { anchors.verticalCenter: parent.verticalCenter; name: root.starterInfo ? root.starterInfo.icon : ""; color: Theme.primary; implicitWidth: 14; implicitHeight: 14 }
                                Label { anchors.verticalCenter: parent.verticalCenter; text: root.starterInfo ? root.starterInfo.label : ""; color: Theme.foreground; font.pixelSize: Theme.px(12) }
                                Icon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "close"
                                    color: Theme.muted
                                    implicitWidth: 13
                                    implicitHeight: 13
                                    TapHandler { onTapped: root.starter = "" }
                                }
                            }
                        }
                        Repeater {
                            model: root.attachments
                            AttachmentChip {
                                required property string modelData
                                required property int index
                                path: modelData
                                removable: true
                                onRemoveClicked: root.attachments = root.attachments.filter((_, i) => i !== index)
                            }
                        }
                    }
                    TextArea {
                        id: input
                        objectName: "draftInput"
                        Layout.fillWidth: true
                        Layout.minimumHeight: 56
                        wrapMode: TextEdit.Wrap
                        color: Theme.foreground
                        selectionColor: Theme.selection
                        font.pixelSize: Theme.px(14)
                        placeholderTextColor: Theme.muted
                        placeholderText: root.goalOn ? "The goal: what should be true when it's done? omp works on it turn after turn."
                            : root.starterInfo
                            ? root.starterInfo.prompt + "  Add details, or just send."
                            : "Plan, build, or ask."
                        background: null
                        padding: 4
                        Keys.onPressed: event => {
                            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                                root.send()
                                event.accepted = true
                            }
                        }
                    }
                    // The same bar as the chat's composer: icon buttons on the left,
                    // Send on the right.
                    RowLayout {
                        id: draftBar
                        Layout.fillWidth: true
                        spacing: 6
                        readonly property bool omp: root.effectiveHarness === "omp" || root.effectiveHarness === "demo"
                        ComposerIconButton {
                            objectName: "draftAttachButton"
                            iconName: "add-circle"
                            tip: "Attach files"
                            onClicked: attachDialog.open()
                        }
                        // omp: start as a goal it keeps working toward across turns.
                        ComposerIconButton {
                            objectName: "draftGoalButton"
                            visible: draftBar.omp && !root.parallel
                            iconName: root.asGoal ? "target-fill" : "target"
                            tint: root.asGoal ? Theme.info : Theme.foreground
                            on: root.asGoal
                            tip: root.asGoal ? "Goal is on: omp keeps working toward it, turn after turn, until it's done"
                                             : "Start as a goal: omp keeps working toward it, turn after turn, until it's done"
                            onClicked: root.asGoal = !root.asGoal
                        }
                        // omp: ask about unclear points before starting.
                        ComposerIconButton {
                            objectName: "draftAskButton"
                            visible: root.effectiveHarness === "omp"
                            iconName: "question"
                            tint: root.askFirst ? Theme.primary : Theme.foreground
                            on: root.askFirst
                            tip: root.askFirst ? "Ask first is on: the agent asks about unclear points and decisions before it starts"
                                               : "Ask first: have the agent ask about unclear points and decisions before it starts"
                            onClicked: root.askFirst = !root.askFirst
                        }
                        // What runs without asking, as the chat's shield; click cycles.
                        ComposerIconButton {
                            objectName: "draftPermissionsButton"
                            visible: root.shownApproval !== ""
                            iconName: root.shownApproval === "always-ask" ? "shield-user" : root.shownApproval === "write" ? "shield-star" : "shield-check"
                            tint: root.shownApproval === "always-ask" ? Theme.foreground : root.shownApproval === "write" ? Theme.success : Theme.info
                            tip: (root.shownApproval === "always-ask" ? "Permissions: ask — the agent asks before it edits files or runs commands"
                                  : root.shownApproval === "write" ? "Permissions: edits — the agent edits files freely, asks before running commands"
                                  : "Permissions: auto — the agent edits and runs commands without asking")
                                 + "\nClick to switch"
                            onClicked: root.cycleApproval()
                        }
                        ComposerIconButton {
                            objectName: "draftParallelButton"
                            iconName: "node-tree"
                            tint: root.parallel ? Theme.primary : Theme.foreground
                            on: root.parallel
                            tip: root.parallel ? "Run in parallel is on: the prompt goes to every lane above"
                                               : "Run in parallel: send the prompt to several models at once and compare"
                            onClicked: root.parallel = !root.parallel
                        }
                        AppTextField {
                            id: goalBudgetField
                            objectName: "goalBudget"
                            visible: root.goalOn
                            Layout.preferredWidth: 110
                            implicitHeight: 26
                            font.pixelSize: Theme.px(12)
                            placeholderText: "budget, e.g. 200k"
                            color: root.parseBudget(text) < 0 ? Theme.error : Theme.foreground
                            ToolTip.visible: hovered
                            ToolTip.text: "Optional: omp stops the goal after this many tokens (200k, 1.5m…)"
                            Keys.onReturnPressed: root.send()
                        }
                        // What the budget field means, and a warning for tiny budgets.
                        Label {
                            readonly property int budget: root.parseBudget(goalBudgetField.text)
                            visible: root.goalOn && goalBudgetField.text.trim() !== ""
                            text: budget < 0 ? "use e.g. 200k" : budget < root.minUsefulBudget ? budget.toLocaleString(Qt.locale("en_US"), "f", 0) + " tokens: too small for one turn" : budget.toLocaleString(Qt.locale("en_US"), "f", 0) + " tokens"
                            color: budget < 0 ? Theme.error : budget < root.minUsefulBudget ? Theme.warning : Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                        // Fills the gap; shortens (then vanishes) before Send moves.
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            horizontalAlignment: Text.AlignRight
                            elide: Text.ElideRight
                            text: root.parallel ? "Enter starts " + root.lanes.length + " lanes" : "Enter to start · Shift+Enter for a new line"
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                        ComposerIconButton {
                            objectName: "draftSendButton"
                            iconName: "send-plane-2"
                            tint: Theme.primary
                            enabled: root.canSend
                            tip: root.parallel ? "Start " + root.lanes.length + " lanes (Enter)" : "Start the session (Enter)"
                            onClicked: root.send()
                        }
                    }
                }
            }

            // Starters
            Flow {
                id: starterFlow
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 8
                // Centre the chips line by line.
                leftPadding: Math.max(0, (width - starterRowWidth) / 2)
                readonly property real starterRowWidth: {
                    let w = 0
                    for (let i = 0; i < starterRepeater.count; i++) {
                        const it = starterRepeater.itemAt(i)
                        if (!it) continue
                        if (w + it.width > width) break
                        w += it.width + spacing
                    }
                    return Math.max(0, w - spacing)
                }
                Repeater {
                    id: starterRepeater
                    model: root.starters
                    Button {
                        id: starterChip
                        required property var modelData
                        readonly property bool chosen: root.starter === modelData.id
                        flat: true
                        implicitHeight: 32
                        leftPadding: 12
                        rightPadding: 13
                        onClicked: { root.starter = chosen ? "" : modelData.id; input.forceActiveFocus() }
                        contentItem: Row {
                            spacing: 7
                            Icon { anchors.verticalCenter: parent.verticalCenter; name: starterChip.modelData.icon; color: starterChip.chosen ? Theme.primary : Theme.muted; implicitWidth: 15; implicitHeight: 15 }
                            Label { anchors.verticalCenter: parent.verticalCenter; text: starterChip.modelData.label; color: starterChip.chosen || starterChip.hovered ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(13) }
                        }
                        background: ControlSurface { control: starterChip; variant: starterChip.chosen ? "selected" : "outline"; radius: 16 }
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: "“" + starterChip.modelData.prompt + "”"
                    }
                }
            }

            // Your templates, and saving the draft as one.
            Flow {
                id: templateFlow
                objectName: "draftTemplates"
                Layout.fillWidth: true
                Layout.topMargin: 2
                visible: root.templates.length > 0 || input.text.trim() !== ""
                spacing: 8
                leftPadding: Math.max(0, (width - implicitRowWidth) / 2)
                readonly property real implicitRowWidth: {
                    let w = 0
                    for (let i = 0; i < children.length; i++) {
                        const it = children[i]
                        if (!it.visible || it.width === 0 || it === templateRepeater) continue
                        if (w + it.width > width) break
                        w += it.width + spacing
                    }
                    return Math.max(0, w - spacing)
                }
                Repeater {
                    id: templateRepeater
                    model: root.templates
                    Button {
                        id: templateChip
                        required property var modelData
                        objectName: "template-" + modelData.name
                        flat: true
                        implicitHeight: 32
                        leftPadding: 12
                        rightPadding: 13
                        onClicked: root.applyTemplate(modelData)
                        contentItem: Row {
                            spacing: 7
                            Icon { anchors.verticalCenter: parent.verticalCenter; name: "bookmark"; color: Theme.primary; implicitWidth: 14; implicitHeight: 14 }
                            Label { anchors.verticalCenter: parent.verticalCenter; text: templateChip.modelData.name; color: templateChip.hovered ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(13) }
                        }
                        background: ControlSurface { control: templateChip; variant: "outline"; radius: 16 }
                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            onTapped: { templateMenu.template = templateChip.modelData; templateMenu.popup() }
                        }
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: (root.templateSummary(modelData) || "The project's agent") + (modelData.text.trim() !== "" ? "\n“" + modelData.text.trim().slice(0, 160) + "”" : "") + "\nRight-click to rename, replace or delete"
                    }
                }
                Button {
                    id: saveTemplateChip
                    objectName: "saveTemplate"
                    visible: input.text.trim() !== "" && !root.parallel
                    flat: true
                    implicitHeight: 32
                    leftPadding: 11
                    rightPadding: 13
                    onClicked: templateDialog.saveNew()
                    contentItem: Row {
                        spacing: 6
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: "add"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                        Label { anchors.verticalCenter: parent.verticalCenter; text: "Save as template"; color: saveTemplateChip.hovered ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(13) }
                    }
                    background: ControlSurface { control: saveTemplateChip; radius: 16 }
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: "Save this agent, model, permissions and message under a name, to start from again"
                }
            }
        }
    }
}
