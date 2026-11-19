pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Projects: each project's name, icon, colour and harness, its wiki, code lookup and plan.
ColumnLayout {
    id: proj
    property int projectId: -1
    readonly property var projectIconPresets: ["code-box", "terminal-box", "rocket", "flask", "gamepad", "briefcase", "home", "global", "leaf", "shield", "palette",
        "server", "smartphone", "database-2", "lightbulb", "music", "camera", "book-open", "heart"]
    spacing: 0
    readonly property var list: Workspace.projectsRevision >= 0 ? JSON.parse(Workspace.projectsJson()) : []
    property int chosen: proj.projectId
    readonly property var p: list.find(x => x.id === chosen) || (list.length > 0 ? list[0] : null)
    onProjectIdChanged: chosen = projectId

    FormNote { visible: proj.list.length === 0; text: "No projects yet. Add one with + Project in the sidebar." }
    // Which project.
    Flow {
        visible: proj.list.length > 1
        Layout.fillWidth: true
        Layout.bottomMargin: 18
        spacing: 6
        Repeater {
            model: proj.list
            Button {
                id: pick
                required property var modelData
                readonly property bool on: proj.p !== null && modelData.id === proj.p.id
                objectName: "projectPick-" + modelData.name
                flat: true
                implicitHeight: 30
                leftPadding: 10
                rightPadding: 12
                onClicked: proj.chosen = modelData.id
                contentItem: Row {
                    spacing: 7
                    ProjectIcon { anchors.verticalCenter: parent.verticalCenter; icon: pick.modelData.icon; color: pick.modelData.color; implicitWidth: 15; implicitHeight: 15 }
                    Label { anchors.verticalCenter: parent.verticalCenter; text: pick.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                }
                background: ControlSurface { control: pick; variant: pick.on ? "selected" : "outline"; radius: 8 }
            }
        }
    }

    ColumnLayout {
        visible: proj.p !== null
        Layout.fillWidth: true
        spacing: 0
        // The project, as the sidebar shows it.
        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Rectangle {
                implicitWidth: 44
                implicitHeight: 44
                radius: 10
                color: Theme.controlFill
                border.color: Theme.controlBorder
                ProjectIcon { anchors.centerIn: parent; icon: proj.p ? proj.p.icon : ""; color: proj.p ? proj.p.color : ""; fallback: Theme.foreground; implicitWidth: 24; implicitHeight: 24 }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label { text: proj.p ? proj.p.name : ""; color: Theme.foreground; font.pixelSize: Theme.px(16); font.bold: true; elide: Text.ElideRight; Layout.fillWidth: true }
                Label { text: proj.p ? proj.p.path : ""; color: Theme.muted; font.family: Theme.mono; font.pixelSize: Theme.px(11.5); elide: Text.ElideMiddle; Layout.fillWidth: true }
            }
        }
        Rule { }

        SectionHeader { title: "Name"; subtitle: "How the sidebar and tabs show it. Empty: the folder's name." }
        AppTextField {
            objectName: "projectName"
            Layout.topMargin: 8
            Layout.preferredWidth: Math.min(380, proj.width)
            text: proj.p ? proj.p.name : ""
            placeholderText: proj.p ? proj.p.folder : ""
            onEditingFinished: if (proj.p && text.trim() !== proj.p.name) Workspace.setProjectName(proj.p.id, text)
        }
        Rule { }

        SectionHeader { title: "Colour"; subtitle: "Its icon in this colour. They come from the theme, so they suit whichever you use." }
        Flow {
            Layout.fillWidth: true
            Layout.topMargin: 10
            spacing: 8
            Repeater {
                model: [{ key: "", label: "None" }].concat(Theme.projectColors)
                Button {
                    id: swatch
                    required property var modelData
                    readonly property bool on: proj.p !== null && proj.p.color === modelData.key
                    objectName: "projectColor-" + (modelData.key || "none")
                    flat: true
                    implicitWidth: 30
                    implicitHeight: 30
                    padding: 0
                    onClicked: Workspace.setProjectLook(proj.p.id, proj.p.icon, modelData.key)
                    contentItem: Item {
                        Rectangle {
                            anchors.centerIn: parent
                            width: 22
                            height: 22
                            radius: 6
                            color: swatch.modelData.key ? swatch.modelData.color : "transparent"
                            border.color: swatch.modelData.key ? Qt.darker(swatch.modelData.color, 1.15) : Theme.controlBorder
                            Icon { anchors.centerIn: parent; visible: !swatch.modelData.key; name: "close"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                        }
                    }
                    background: Rectangle {
                        radius: 8
                        color: "transparent"
                        border.width: 2
                        border.color: swatch.on ? Theme.primary : swatch.hovered ? Theme.controlBorderHover : "transparent"
                    }
                    // Theme colours have no fixed names (a theme's "keyword" may be green).
                    ToolTip.visible: hovered && !modelData.key
                    ToolTip.delay: 400
                    ToolTip.text: "No colour"
                }
            }
        }
        Rule { }

        SectionHeader { title: "Icon"; subtitle: "Shown in the sidebar, in the project's colour." }
        Flow {
            Layout.fillWidth: true
            Layout.topMargin: 10
            spacing: 6
            Repeater {
                model: [""].concat(proj.projectIconPresets)
                Button {
                    id: iconPick
                    required property string modelData
                    readonly property bool on: proj.p !== null && (modelData === "" ? proj.p.icon === "" : proj.p.icon === modelData)
                    objectName: "projectIconPick-" + (modelData || "none")
                    flat: true
                    implicitWidth: 32
                    implicitHeight: 32
                    padding: 0
                    onClicked: Workspace.setProjectLook(proj.p.id, modelData, proj.p.color)
                    contentItem: Item {
                        ProjectIcon {
                            anchors.centerIn: parent
                            icon: iconPick.modelData
                            color: proj.p ? proj.p.color : ""
                            fallback: iconPick.on ? Theme.foreground : Theme.muted
                            implicitWidth: 17
                            implicitHeight: 17
                        }
                    }
                    background: ControlSurface { control: iconPick; variant: iconPick.on ? "selected" : "outline"; radius: 8 }
                    ToolTip.visible: hovered
                    ToolTip.delay: 400
                    ToolTip.text: modelData === "" ? "A folder (the default)" : modelData.replace(/-\d$/, "").replace("-", " ")
                }
            }
        }
        Rule { }

        SectionHeader { title: "New sessions" }
        Group {
            SettingRow {
                title: "Provider"
                detail: "What + on the project starts (right-click it for the others)."
                icon: "robot-2"
                Row {
                    spacing: 6
                    Repeater {
                        model: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : []
                        ChoicePill {
                            required property var modelData
                            text: modelData.label
                            provider: modelData.id
                            selected: proj.p !== null && proj.p.harness === modelData.id
                            onClicked: Workspace.setProjectHarness(proj.p.id, modelData.id)
                        }
                    }
                }
            }
            // Its agent's model here (each agent remembers its own).
            SettingRow {
                id: projModel
                objectName: "projectModel"
                visible: proj.p !== null
                divider: true
                title: "Model"
                detail: "What " + (proj.p ? Theme.harnessLabel(proj.p.harness) : "the agent") + " starts with here."
                    + (proj.p && proj.p.remote ? (projModel.hostModels && projModel.hostModels.error ? " Couldn't read its models: " + projModel.hostModels.error : "") : "")
                icon: "sparkling"
                property int revision: 0
                readonly property string key: proj.p ? "project_model_" + proj.p.id + "_" + proj.p.harness : ""
                readonly property string current: revision >= 0 && key !== "" ? Workspace.setting(key) : ""
                // Another machine's project: its agent's models there,
                // asked when the menu opens (null until it answers).
                property var hostModels: null
                Connections {
                    target: Workspace
                    function onHostModelsReady(id, harness, json) {
                        if (proj.p && id === proj.p.id && harness === proj.p.harness) projModel.hostModels = JSON.parse(json)
                    }
                }
                readonly property var choices: {
                    if (proj.p && proj.p.remote) {
                        const list = hostModels && hostModels.models ? hostModels.models : []
                        return list.filter(m => m.local).concat(list.filter(m => !m.local))
                    }
                    const p = proj.p ? (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === proj.p.harness) : null
                    if (!p) return []
                    const all = (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 }))
                    return all.filter(m => m.local).concat(all.filter(m => !m.local))
                }
                function choose(id) { Workspace.setSetting(key, id); revision++ }
                FlatButton {
                    id: projModelButton
                    readonly property var m: projModel.choices.find(x => x.id === projModel.current)
                    text: projModel.current === "" ? "Default model" : Theme.modelLabel(m ? m.name : projModel.current)
                    font.pixelSize: Theme.px(12)
                    onClicked: {
                        if (proj.p.remote) { projModel.hostModels = null; Workspace.requestHostModels(proj.p.id, proj.p.harness) }
                        else Workspace.ensureProviders()
                        projModelMenu.popup(projModelButton, 0, projModelButton.height + 4)
                    }
                    AppMenu {
                        id: projModelMenu
                        AppMenuItem { text: "Default model"; marked: projModel.current === ""; onTriggered: projModel.choose("") }
                        AppMenuItem {
                            visible: proj.p !== null && proj.p.remote && projModel.hostModels === null
                            enabled: false
                            text: "Reading its models on " + (proj.p ? proj.p.host : "") + "…"
                        }
                        AppMenuSeparator { visible: projModel.choices.length > 0 }
                        Repeater {
                            model: projModel.choices
                            AppMenuItem {
                                required property var modelData
                                text: Theme.modelLabel(modelData.name)
                                hint: modelData.local ? "Local" : Theme.providerOf(modelData.id)
                                marked: projModel.current === modelData.id
                                onTriggered: projModel.choose(modelData.id)
                            }
                        }
                    }
                }
            }
        }
        Rule { }

        SectionHeader { title: "Worktree setup"; subtitle: "Commands run in each new worktree of this project, one per line (e.g. npm install)." }
        TextArea {
            id: setupArea
            objectName: "projectSetup"
            Layout.fillWidth: true
            Layout.topMargin: 8
            Layout.preferredHeight: Math.max(70, implicitHeight)
            text: proj.p ? proj.p.setup : ""
            color: Theme.foreground
            font.family: Theme.mono
            font.pixelSize: Theme.px(12)
            wrapMode: TextEdit.Wrap
            placeholderText: "npm install"
            placeholderTextColor: Theme.muted
            background: Rectangle { radius: 6; color: Theme.background; border.color: setupArea.activeFocus ? Theme.primary : Theme.border }
        }
        RowLayout {
            Layout.topMargin: 6
            visible: proj.p !== null && setupArea.text !== proj.p.setup
            FlatButton { text: "Save"; variant: "primary"; font.pixelSize: Theme.px(12); onClicked: Workspace.setProjectSetup(proj.p.id, setupArea.text) }
            FlatButton { text: "Cancel"; outlined: false; font.pixelSize: Theme.px(12); onClicked: setupArea.text = proj.p.setup }
        }
        Rule { visible: proj.p !== null }
        // The wiki and the shared plan.
        SectionHeader {
            visible: proj.p !== null
            title: "Wiki, lookup and plans"
            subtitle: "Tools that help this project's agents find their way, plan and keep track."
            more: "Every agent gets them, through Harlo's tools (Settings → Teamwork). Sessions started afterwards get them."
        }
        Group {
            id: featureGroup
            visible: proj.p !== null
            // (Re-read when another project is picked, or a switch moves.)
            readonly property int pid: proj.p ? proj.p.id : -1
            property int rev: 0
            function on(name, byDefault) {
                const v = rev >= 0 && pid >= 0 ? Workspace.setting(name + "_" + pid) : "0"
                return v === "" ? !!byDefault : v === "1"
            }
            function set(name, on) { Workspace.setSetting(name + "_" + pid, on ? "1" : "0"); rev++ }
            SettingRow {
                title: "Wiki"
                detail: "A wiki of this project in .okf/, written by your agent from the code. Agents are pointed to it and read the pages they need. On unless you switch it off."
                more: "Create and update it from the Wiki panel (right edge); each run spends the tokens of the model that writes it (Settings → Wiki). Its pages are Open Knowledge Format, each claim footnoted with the code it comes from."
                icon: "book-open"
                AppSwitch {
                    objectName: "projectWiki"
                    checked: featureGroup.on("wiki", true)
                    onToggled: featureGroup.set("wiki", checked)
                }
            }
            SettingRow {
                divider: true
                visible: featureGroup.on("wiki", true)
                title: "Keep it current as you work"
                detail: "When this project's sessions go quiet after changing code, harlo checks the wiki and updates it in the background if the code moved past it."
                more: "The check needs no model; each update is a session of the wiki's writer (Settings → Wiki) and spends its tokens. Without this, the Wiki panel shows what's out of date and offers Update when work is merged. Off unless you switch it on."
                icon: "refresh"
                AppSwitch {
                    objectName: "projectWikiKeep"
                    checked: featureGroup.on("wiki_keep", false)
                    onToggled: featureGroup.set("wiki_keep", checked)
                }
            }
            SettingRow {
                divider: true
                visible: featureGroup.on("wiki", true)
                title: "Wiki map"
                detail: "Sessions get each wiki page and the code its sections explain in their instructions, so they find code without searching."
                more: "The default is in Settings → Wiki; here this project can differ from it either way."
                icon: "compass-3"
                ComboField {
                    objectName: "projectWikiMap"
                    width: 190
                    // ("" follows Settings → Wiki, read again when a switch moves.)
                    readonly property bool byDefault: featureGroup.rev >= 0 && Workspace.setting("wiki_map") !== "0"
                    readonly property var values: ["", "1", "0"]
                    model: [byDefault ? "As in Settings (on)" : "As in Settings (off)", "On", "Off"]
                    function at() { return featureGroup.rev >= 0 && featureGroup.pid >= 0 ? Math.max(0, values.indexOf(Workspace.setting("wiki_map_" + featureGroup.pid))) : 0 }
                    currentIndex: at()
                    onActivated: i => {
                        Workspace.setSetting("wiki_map_" + featureGroup.pid, values[i])
                        featureGroup.rev++
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
            SettingRow {
                divider: true
                title: "Code lookup"
                detail: "Agents can look up a function or type by name: where it's declared, its signature, what it calls and what calls it, without reading files."
                more: "From harlo's code index of this project (its own cache; nothing is written in the project). Harlo has it as a tool of its own, other agents through Harlo's tools. Off unless you switch it on: it's one more tool in every request."
                icon: "node-tree"
                AppSwitch {
                    objectName: "projectLookup"
                    checked: featureGroup.on("lookup", false)
                    onToggled: featureGroup.set("lookup", checked)
                }
            }
            SettingRow {
                id: planRow
                divider: true
                title: "Shared plan"
                detail: "For a bigger task the agent writes a plan of steps and works through it."
                more: "You add, change, reorder or skip steps at any time (above the message box), and it reads them before each step. On by default in a git repository."
                icon: "list-check-2"
                // (Unset: on in a git repository.)
                readonly property bool on: featureGroup.rev >= 0 && featureGroup.pid >= 0 && Workspace.planOn(featureGroup.pid)
                AppSwitch {
                    objectName: "projectPlan"
                    checked: planRow.on
                    onToggled: featureGroup.set("plan", checked)
                }
            }
            SettingRow {
                divider: true
                visible: planRow.on
                title: "Commit each finished step"
                detail: "When a step is done, Harlo commits its changes with the step as the message."
                more: "Only if they're provably the step's: not when a file it touched already had changes, the branch moved, or another session worked there meanwhile (then it says why and leaves them to you). A finished step can be reviewed by another agent. Off unless you turn it on (or the shared plan) here."
                icon: "git-commit"
                AppSwitch {
                    objectName: "projectPlanCommits"
                    checked: featureGroup.rev >= 0 && featureGroup.pid >= 0 && Workspace.planCommitsOn(featureGroup.pid)
                    onToggled: featureGroup.set("plan_commits", checked)
                }
            }
        }
        // Who the agents' helpers are (a plan's run, a split).
        SectionHeader {
            visible: proj.p !== null && Workspace.orchestrationOn
            Layout.topMargin: 22
            title: "Team"
            subtitle: "Who does the steps and who reviews them when a plan runs with helpers."
            more: "Also who works when an agent splits a task. You can plan with one model, have another do the steps and a third check them, and still choose others for one run, or one step."
        }
        Group {
            id: team
            visible: proj.p !== null && Workspace.orchestrationOn
            readonly property int pid: proj.p ? proj.p.id : -1
            property int rev: 0
            // { agent, model, thinking } ({}: the same as the session).
            readonly property var value: {
                if (rev < 0 || pid < 0) return ({})
                try { return JSON.parse(Workspace.setting("team_helpers_" + pid) || "{}") } catch (e) { return ({}) }
            }
            readonly property var providers: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).filter(p => p.canChat && p.enabled && p.state !== "missing") : []
            readonly property var provider: providers.find(p => p.id === value.agent) || null
            readonly property var levels: ["", "low", "medium", "high"]
            function save(v) { Workspace.setSetting("team_helpers_" + pid, v.agent ? JSON.stringify(v) : ""); rev++ }
            function change(k, x) { const v = Object.assign({}, value); v[k] = x; save(v) }
            // Ready-made teams: one agent for the steps, or one doing
            // them and another reviewing them.
            readonly property var reviewerProvider: value.reviewer ? providers.find(p => p.id === value.reviewer.agent) || null : null
            onVisibleChanged: if (visible) Workspace.ensureProviders()
            SettingRow {
                title: "Helpers"
                detail: team.provider ? "Helpers are " + team.provider.label + " sessions, with the model and thinking below."
                                      : "The same as the session that starts them: its agent, model and thinking."
                icon: "node-tree"
                ComboField {
                    objectName: "teamAgent"
                    width: 190
                    model: ["Same as the session"].concat(team.providers.map(p => p.label))
                    function at() { return team.provider ? team.providers.indexOf(team.provider) + 1 : 0 }
                    currentIndex: at()
                    onModelChanged: currentIndex = Qt.binding(() => at())
                    onActivated: i => {
                        team.save(i === 0 ? {} : { agent: team.providers[i - 1].id, model: "", thinking: "" })
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
            SettingRow {
                visible: team.provider !== null
                divider: true
                title: "Their model"
                detail: team.provider && team.provider.state === "checking" ? "Finding " + team.provider.label + "'s models…" : ""
                icon: "sparkling"
                ComboField {
                    objectName: "teamModel"
                    width: 190
                    readonly property var ids: team.provider ? team.provider.modelIds || [] : []
                    model: ["Default model"].concat(team.provider ? (team.provider.models || []).map(m => Theme.modelLabel(m)) : [])
                    function at() { return !team.value.model ? 0 : ids.indexOf(team.value.model) + 1 }
                    currentIndex: at()
                    onModelChanged: currentIndex = Qt.binding(() => at())
                    onActivated: i => {
                        team.change("model", i > 0 ? ids[i - 1] || "" : "")
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
            SettingRow {
                visible: team.provider !== null
                divider: true
                title: "Their thinking"
                detail: "How hard they think: more is slower and costs more."
                icon: "speed"
                ComboField {
                    objectName: "teamThinking"
                    width: 190
                    model: ["Default", "Low", "Medium", "High"]
                    function at() { return Math.max(0, team.levels.indexOf(team.value.thinking || "")) }
                    currentIndex: at()
                    onActivated: i => {
                        team.change("thinking", team.levels[i])
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
            SettingRow {
                visible: team.provider !== null
                divider: true
                title: "Reviews by"
                detail: team.reviewerProvider ? "Each finished step is reviewed by a " + team.reviewerProvider.label + " session."
                                              : "Each finished step is reviewed by a session like its helper."
                icon: "eye"
                ComboField {
                    objectName: "teamReviewer"
                    width: 190
                    model: ["The helpers' agent"].concat(team.providers.map(p => p.label))
                    function at() { return team.reviewerProvider ? team.providers.indexOf(team.reviewerProvider) + 1 : 0 }
                    currentIndex: at()
                    onModelChanged: currentIndex = Qt.binding(() => at())
                    onActivated: i => {
                        team.change("reviewer", i === 0 ? undefined : { agent: team.providers[i - 1].id, model: "" })
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
            SettingRow {
                visible: team.provider !== null && team.reviewerProvider !== null
                divider: true
                title: "The reviewer's model"
                icon: "sparkling"
                ComboField {
                    objectName: "teamReviewerModel"
                    width: 190
                    readonly property var ids: team.reviewerProvider ? team.reviewerProvider.modelIds || [] : []
                    model: ["Default model"].concat(team.reviewerProvider ? (team.reviewerProvider.models || []).map(m => Theme.modelLabel(m)) : [])
                    function at() { return !team.value.reviewer || !team.value.reviewer.model ? 0 : ids.indexOf(team.value.reviewer.model) + 1 }
                    currentIndex: at()
                    onModelChanged: currentIndex = Qt.binding(() => at())
                    onActivated: i => {
                        team.change("reviewer", { agent: team.value.reviewer.agent, model: i > 0 ? ids[i - 1] || "" : "" })
                        currentIndex = Qt.binding(() => at())
                    }
                }
            }
        }
        Rule { }

        RowLayout {
            spacing: 10
            FlatButton {
                objectName: "projectRemove"
                text: "Remove from Harlo"
                variant: "danger"
                accent: Theme.error
                font.pixelSize: Theme.px(12)
                onClicked: removeProject.ask(proj.p.id, proj.p.name)
            }
            FormNote { text: "Its folder stays; you choose what happens to its sessions." }
            RemoveProjectDialog { id: removeProject; onRemoved: proj.chosen = -1 }
        }
    }
}
