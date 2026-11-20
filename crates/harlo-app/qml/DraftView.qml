pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Effects
import harlo

// The new-session page: pick a project, where to work (current checkout or
// a new worktree) and the model, and write the first message. Nothing
// starts until the first message is sent.
Rectangle {
    id: root
    color: Theme.background

    property int projectId: -1
    property string harness: ""          // "" = the project's default
    property bool useWorktree: false
    property string worktreeName: ""
    property var attachments: []
    // The shield: what runs without asking ("" the mode it starts in).
    property string approval: ""
    readonly property string startApproval: Workspace.modesRevision >= 0 ? Workspace.draftApproval(effectiveHarness, projectId) : ""
    readonly property string shownApproval: startApproval === "" ? "" : approval !== "" ? approval : startApproval
    function cycleApproval() {
        const modes = ["always-ask", "write", "yolo"]
        approval = modes[(modes.indexOf(shownApproval) + 1) % modes.length]
    }
    // (Another machine: its models are asked for again.)
    onHostChanged: hostModels = null
    signal startedSession(string key)
    signal addProjectRequested()
    /// Sign this agent in on that host.
    signal signInOnHost(string agent, string host)

    // Re-read when the page opens: projects and branches change elsewhere.
    property var projects: []
    property var branches: []
    function refresh() {
        projects = JSON.parse(Workspace.projectsJson())
        defaultAgent = Workspace.defaultAgent()
        if (model === "") setModel(defaultModel())
        branches = []
        // (Read off the UI thread: a remote project's over SSH.)
        if (projects.some(p => p.id === projectId)) Workspace.requestBranches(projectId)
    }
    Connections {
        target: Workspace
        function onBranchesReady(id, list) { if (id === root.projectId) root.branches = list }
        function onProjectsRevisionChanged() {
            if (!root.visible) return
            root.projects = JSON.parse(Workspace.projectsJson())
        }
        function onProvidersRevisionChanged() { root.defaultAgent = Workspace.defaultAgent() }
        // Another machine: its projects, its home folder, its agents.
        function onInstanceChanged() {
            root.projects = JSON.parse(Workspace.projectsJson())
            root.projectId = -1
            root.harness = ""
            root.useWorktree = false
            root.hostModels = null
            root.setModel(root.defaultModel())
            root.refresh()
        }
        function onHostModelsReady(id, harness, json) {
            if (id !== root.projectId) return
            if (harness === root.effectiveHarness) root.hostModels = JSON.parse(json)
        }
    }
    onVisibleChanged: if (visible) refresh()
    Component.onCompleted: refresh()
    readonly property var project: projects.find(p => p.id === projectId) || null
    // The machine the session would run on ("": this one): the home folder
    // is the home folder of the instance shown, so it can be a host too.
    readonly property string host: project ? project.host : Workspace.instance
    readonly property bool runsOnHost: host !== ""
    readonly property string folder: (Remotes.revision, Workspace.instance, Workspace.draftFolder(projectId))
    // What the host's agent offers: asked there when the menu opens (null
    // until it answers; `error` if it couldn't).
    property var hostModels: null
    function loadHostModels() {
        hostModels = null
        Workspace.requestHostModels(projectId, effectiveHarness)
    }
    readonly property string currentBranch: branches.length > 0 ? branches[0] : ""
    readonly property string homeLabel: Workspace.instance !== "" ? "Home folder on " + Workspace.instance : "Home folder"
    readonly property bool atDefaultBranch: currentBranch === "main" || currentBranch === "master"
    // (What harlo starts when nothing is picked: Settings' choice, or the
    // first provider on and installed.)
    property string defaultAgent: Workspace.defaultAgent()
    readonly property string startAgent: project ? project.harness : defaultAgent
    // (On a host: one that's there, if the usual one isn't.)
    readonly property string effectiveHarness: {
        if (harness !== "") return harness
        if (!runsOnHost || Theme.offeredOnHost(host, startAgent)) return startAgent
        const there = (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []).find(p => Theme.offeredOnHost(host, p.id))
        return there ? there.id : startAgent
    }
    /// The model to start with (`provider/model` as the agent takes it; ""
    /// the agent's own), from what the agent offers here (Settings →
    /// Providers reads it); the agent's default model from Settings (the
    /// setup guide) until one is picked. Not for a remote project (its host's
    /// agent).
    property string model: ""
    // A picked model belongs to the agent and machine it was picked for: it
    // goes no further (another machine's agent may not even have it).
    property string modelFor: ""
    readonly property string activeModel: modelFor === host + "\t" + effectiveHarness ? model : ""
    function setModel(id) {
        model = id
        modelFor = host + "\t" + effectiveHarness
    }
    function defaultModel() {
        // The project's for this agent (Add a project, Settings → Projects).
        const own = project ? Workspace.setting("project_model_" + project.id + "_" + effectiveHarness) : ""
        // (On another machine only its own: this computer's default may not be there.)
        if (runsOnHost) return own
        return own !== "" ? own : Workspace.setting("default_model_" + effectiveHarness)
    }
    onEffectiveHarnessChanged: { hostModels = null; setModel(defaultModel()) }
    readonly property var modelChoices: {
        if (runsOnHost) return hostModels && hostModels.models ? hostModels.models.filter(m => m.local).concat(hostModels.models.filter(m => !m.local)) : []
        const p = (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === effectiveHarness)
        if (!p) return []
        // Models served on this computer or the local network first, under
        // "Local" in the list; then the rest, under "Online".
        const all = (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 }))
        return all.filter(m => m.local).concat(all.filter(m => !m.local))
    }
    readonly property string modelName: { const m = modelChoices.find(x => x.id === model); return m ? m.name : model }
    // Your templates (a set-up and a first message, saved under a name).
    readonly property var templates: Workspace.templatesJson !== "" ? JSON.parse(Workspace.templatesJson) : []
    /// Fills the draft from a template: nothing starts until it's sent.
    function applyTemplate(t) {
        harness = t.harness
        setModel(t.model !== "" ? t.model : defaultModel())
        approval = t.approval
        useWorktree = t.worktree && currentBranch !== ""
        if (useWorktree && worktreeName === "") worktreeName = Workspace.suggestBranch()
        input.text = t.text
        input.forceActiveFocus()
        input.cursorPosition = input.length
    }
    /// The draft as a template (what it would start with now).
    function draftTemplate() {
        return { name: "", harness: effectiveHarness, model: model, approval: shownApproval,
                 worktree: useWorktree, text: input.text }
    }
    /// "Claude · claude-opus-4 · edits · new worktree"
    function templateSummary(t) {
        const modes = { "always-ask": "asks before changes", "write": "edits freely", "yolo": "runs without asking" }
        return [t.harness ? Theme.harnessLabel(t.harness) : "", (t.model || "").replace(/^.*\//, ""), modes[t.approval] || "",
                t.worktree ? "new worktree" : ""].filter(x => x !== "").join(" · ")
    }
    readonly property bool canSend: folder !== "" && (input.text.trim() !== "" || attachments.length > 0)

    // Opens the page for a project (-1: home folder), keeping any draft text.
    function openFor(id) {
        // (A project of the machine shown, or its home folder.)
        projects = JSON.parse(Workspace.projectsJson())
        if (!projects.some(p => p.id === id)) id = -1
        const other = id !== projectId
        if (other) {
            projectId = id
            harness = ""
            useWorktree = false
        }
        refresh()
        if (other) setModel(defaultModel())
        input.forceActiveFocus()
    }
    function reset() {
        input.text = ""
        attachments = []
        useWorktree = false
    }
    function send() {
        if (!canSend)
            return
        if (useWorktree && currentBranch === "")
            useWorktree = false
        if (useWorktree && worktreeName.trim() === "")
            worktreeName = Workspace.suggestBranch()
        const key = Workspace.startDraft(JSON.stringify({
            projectId: project ? projectId : -1, harness: effectiveHarness, worktreeBranch: useWorktree ? worktreeName.trim() : "",
            base: currentBranch, text: input.text, attachments: attachments,
            approval: startApproval === "" ? "" : approval, model: activeModel
        }))
        reset()
        startedSession(key)
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
        AppMenuItem { text: root.homeLabel; marked: root.projectId === -1; onTriggered: root.openFor(-1) }
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
            Rectangle { anchors.fill: parent; radius: 3; color: "transparent"; border.color: Theme.primary; visible: chip.visualFocus }
        }
    }

    Flickable {
        anchors.fill: parent
        contentHeight: Math.max(height, page.implicitHeight + 80)
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: page
            // (The session's own width: the box you type in keeps its size
            // when the session starts.)
            width: Math.min(parent.width - 48, Theme.px(Math.min(Theme.chatMaxWidth, 1180)))
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

            // Project, where, model: centred line by line, as the title above.
            Flow {
                objectName: "draftPickers"
                Layout.fillWidth: true
                spacing: 4
                leftPadding: Math.max(0, (width - pickerRowWidth) / 2)
                readonly property real pickerRowWidth: {
                    let w = 0
                    for (let i = 0; i < children.length; i++) {
                        const it = children[i]
                        if (!it.visible || it.width === 0) continue
                        if (w + it.width > width) break
                        w += it.width + spacing
                    }
                    return Math.max(0, w - spacing)
                }
                Chip {
                    iconName: "home"
                    iconColor: Theme.info
                    projectLook: root.project !== null
                    projectIcon: root.project ? root.project.icon : ""
                    projectColor: root.project ? root.project.color : ""
                    text: root.project ? root.project.name : root.folder === "" ? "Connecting to " + Workspace.instance + "…" : root.homeLabel
                    onClicked: projectMenu.popup(this, 0, height + 4)
                }
                Chip {
                    visible: root.currentBranch !== ""
                    // (On the default branch it says so in words: the
                    // warning tint alone needed a hover to explain.)
                    iconName: "git-branch"
                    iconColor: root.useWorktree ? Theme.success : root.atDefaultBranch ? Theme.warning : Theme.muted
                    text: root.useWorktree ? "New worktree" : root.atDefaultBranch ? "Directly on " + root.currentBranch : root.currentBranch
                    onClicked: branchMenu.popup(this, 0, height + 4)
                    ToolTip.visible: hovered && root.atDefaultBranch && !root.useWorktree
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
                    objectName: "draftModel"
                    iconName: "sparkling"
                    iconColor: root.activeModel !== "" ? Theme.primary : Theme.muted
                    text: root.activeModel !== "" ? Theme.modelLabel(root.modelName) : "Default model"
                    // (The agents' models are read when first asked for.)
                    onClicked: {
                        if (root.runsOnHost) root.loadHostModels(); else Workspace.ensureProviders()
                        modelPopup.open()
                    }
                    ToolTip.visible: hovered && root.activeModel === ""
                    ToolTip.text: Theme.harnessLabel(root.effectiveHarness) + " starts with its own default model. Pick one to start with another."
                    ToolTip.delay: 400
                    // The models the agent offers here, filtered as you type.
                    Popup {
                        id: modelPopup
                        y: parent.height + 4
                        // (Kept inside the window, however far right the chip is.)
                        margins: 8
                        width: 400
                        height: Math.min(380, 60 + Math.max(1, draftModels.count) * 30 + (root.modelChoices.some(m => m.local) ? 30 : 0) + (root.modelChoices.length === 0 ? 70 : 0))
                        padding: 8
                        background: GlassSurface { radius: 4 }
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
                                text: root.runsOnHost
                                    ? (root.hostModels === null ? "Reading " + Theme.harnessLabel(root.effectiveHarness) + "'s models on " + root.host + "…"
                                       : root.hostModels.error ? "Couldn't read " + Theme.harnessLabel(root.effectiveHarness) + "'s models on " + root.host + ": " + root.hostModels.error
                                       : "No models. " + Theme.harnessLabel(root.effectiveHarness) + " needs its own login or API key on " + root.host + ".")
                                    : Workspace.providersJson().indexOf('"checking"') >= 0 ? "Reading " + Theme.harnessLabel(root.effectiveHarness) + "'s models…"
                                    : Theme.harnessLabel(root.effectiveHarness) + " has no models to pick here. Add a login in Settings → Agents, or an API key in Settings → Models."
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                            }
                            RowLayout {
                                visible: root.runsOnHost && root.hostModels !== null && root.modelChoices.length === 0
                                spacing: 6
                                FlatButton {
                                    objectName: "draftModelsSignIn"
                                    text: "Sign in on " + root.host
                                    onClicked: { modelPopup.close(); root.signInOnHost(root.effectiveHarness, root.host) }
                                }
                                FlatButton {
                                    objectName: "draftModelsCheckAgain"
                                    text: "Check again"
                                    onClicked: root.loadHostModels()
                                }
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
                                    if (m) {
                                        root.setModel(m.id)
                                        modelPopup.close()
                                    }
                                }
                                delegate: ItemDelegate {
                                    id: dmRow
                                    required property var modelData
                                    required property int index
                                    objectName: "draftModel-" + modelData.id
                                    // The first of each group: its name above it (when
                                    // there are local models; "Default model" has none).
                                    // (The row before it: gone for a moment while the
                                    // filter changes the list.)
                                    readonly property var before: index > 0 && ListView.view && ListView.view.model ? ListView.view.model[index - 1] : undefined
                                    readonly property real heading: root.modelChoices.length > 0 && root.modelChoices[0].local && modelData.id !== ""
                                        && (index === 0 || !before || before.id === "" || !!before.local !== !!modelData.local) ? 30 : 0
                                    width: ListView.view.width
                                    height: 30 + heading
                                    topInset: heading
                                    topPadding: heading + bottomPadding
                                    highlighted: ListView.isCurrentItem
                                    background: Rectangle { radius: 2; color: dmRow.highlighted ? Theme.selected : dmRow.hovered ? Theme.hover : "transparent" }
                                    Label {
                                        visible: dmRow.heading > 0
                                        x: 8
                                        y: dmRow.heading - height - 3
                                        text: dmRow.modelData.local ? "LOCAL" : "ONLINE"
                                        color: Theme.label
                                        font.pixelSize: Theme.px(11)
                                        font.weight: Font.Medium
                                        font.letterSpacing: Theme.px(11) * 0.07
                                    }
                                    contentItem: RowLayout {
                                        Label {
                                            text: (dmRow.modelData.id === "" ? dmRow.modelData.name : Theme.modelLabel(dmRow.modelData.name)) + (dmRow.modelData.id === root.model ? "  ✓" : "")
                                            color: dmRow.modelData.id === "" ? Theme.muted : Theme.foreground
                                            font.pixelSize: Theme.px(13)
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }
                                        // (A local model: its engine.)
                                        Label {
                                            readonly property string engine: dmRow.modelData.local ? Theme.modelEngine(root.effectiveHarness, dmRow.modelData.id) : ""
                                            visible: engine !== "" || dmRow.modelData.id.indexOf("/") > 0
                                            text: engine || Theme.providerOf(dmRow.modelData.id)
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

            // Composer
            Rectangle {
                id: draftCard
                Layout.fillWidth: true
                implicitHeight: box.implicitHeight + 20
                radius: 4
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
                    color: Theme.shadowStrong
                }
                ColumnLayout {
                    id: box
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 8
                    // Attachments
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        visible: root.attachments.length > 0
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
                        placeholderText: "Ask, or describe what to build."
                        background: null
                        padding: 4
                        Keys.onPressed: event => {
                            // An image or files on the clipboard: attached, not typed.
                            if (event.matches(StandardKey.Paste)) {
                                const files = Workspace.pasteFiles()
                                for (let i = 0; i < files.length; i++)
                                    if (!root.attachments.includes(files[i])) root.attachments = root.attachments.concat([files[i]])
                                if (files.length > 0) event.accepted = true
                                return
                            }
                            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                                root.send()
                                event.accepted = true
                            }
                        }
                    }
                    // The same bar as the chat's composer: icon buttons on the left,
                    // Send on the right.
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 6
                        ComposerIconButton {
                            objectName: "draftAttachButton"
                            iconName: "add-circle"
                            tip: "Attach files"
                            onClicked: attachDialog.open()
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
                        // Fills the gap; shortens (then vanishes) before Send moves.
                        Label {
                            id: startHint
                            objectName: "draftStartHint"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            Layout.preferredWidth: 0
                            horizontalAlignment: Text.AlignRight
                            elide: Text.ElideRight
                            // (Narrow: the short form, rather than one cut mid-word.)
                            readonly property string full: "Enter to start · Shift+Enter for a new line"
                            TextMetrics { id: startHintFull; font: startHint.font; text: startHint.full }
                            TextMetrics { id: startHintShort; font: startHint.font; text: "Enter to start" }
                            // (No room even for that: nothing, rather than over the buttons.)
                            text: width >= startHintFull.advanceWidth ? full : width >= startHintShort.advanceWidth ? "Enter to start" : ""
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                        ComposerIconButton {
                            objectName: "draftSendButton"
                            iconName: "send-plane-2"
                            tint: Theme.primary
                            enabled: root.canSend
                            tip: "Start the session (Enter)"
                            onClicked: root.send()
                        }
                    }
                }
            }

            // Your templates, and saving the draft as one.
            Flow {
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
                        background: ControlSurface { control: templateChip; variant: "outline"; radius: 6 }
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
                    visible: input.text.trim() !== ""
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
                    background: ControlSurface { control: saveTemplateChip; radius: 6 }
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: "Save this agent, model, permissions and message under a name, to start from again"
                }
            }
        }
    }
}
