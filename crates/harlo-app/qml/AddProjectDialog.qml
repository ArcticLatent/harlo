pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import harlo

// Adds a project (the sidebar's + Project, the new-session page's Add
// project…): its folder, the agent its sessions start with and that
// agent's model. Adding saves both as the project's own and opens the
// new-session page for it. New folder… makes the folder (a git repository)
// instead of picking one.
AppDialog {
    id: dlg
    objectName: "addProjectDialog"
    /// Choose… pressed: the sidebar opens its folder picker (this computer's,
    /// or the host's) and calls setFolder.
    signal chooseFolder()
    /// Added: the project's id.
    signal added(int projectId)
    // New folder…: its name and where it goes (this computer only: a
    // host's folder picker makes folders itself).
    property bool creating: false
    property string newParent: ""
    property string problem: ""
    // Test hooks.
    property alias newFolderName: newFolderName

    /// The folder: a path, or a host's (from its listing).
    property string folder: ""
    property string harness: ""
    property string model: ""
    readonly property bool remote: Workspace.instance !== ""
    readonly property var agents: (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.chatProvidersJson()) : [])
        .filter(p => !remote || Theme.offeredOnHost(Workspace.instance, p.id))
    // The agent's models: this computer's and the local network's first.
    readonly property var modelChoices: {
        const p = (Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()) : []).find(x => x.id === harness)
        if (!p) return []
        const all = (p.modelIds || []).map((id, i) => ({ id: id, name: (p.models || [])[i] || id, local: (p.localIds || []).indexOf(id) >= 0 }))
        return all.filter(m => m.local).concat(all.filter(m => !m.local))
    }

    function setFolder(path) {
        folder = path.startsWith("file://") ? decodeURIComponent(path.substring(7)).replace(/\/+$/, "") : path
    }
    function add() {
        if (creating) {
            const made = JSON.parse(Workspace.createProjectFolder(newParent, newFolderName.text))
            if (made.error) {
                problem = made.error
                return
            }
            folder = made.path
        }
        const before = JSON.parse(Workspace.projectsJson()).map(p => p.id)
        Workspace.addProject(folder)
        const now = JSON.parse(Workspace.projectsJson())
        const p = now.find(x => !before.includes(x.id)) || now.find(x => x.path === folder || x.host !== "" && folder.endsWith(x.path))
        if (!p) return
        Workspace.setProjectHarness(p.id, harness)
        Workspace.setSetting("project_model_" + p.id + "_" + harness, remote ? "" : model)
        close()
        added(p.id)
    }

    title: "Add a project"
    width: Math.min(520, (Overlay.overlay ? Overlay.overlay.width : 560) - 40)
    onAboutToShow: {
        folder = ""
        creating = false
        problem = ""
        newFolderName.text = ""
        harness = Workspace.defaultAgent()
        if (!agents.some(a => a.id === harness) && agents.length > 0) harness = agents[0].id
        model = ""
        modelFilter.text = ""
        // (The agents' models are read when first asked for.)
        Workspace.ensureProviders()
    }
    onHarnessChanged: model = ""

    footer: AppButtonBox {
        DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        DialogButton {
            objectName: "addProjectAdd"
            text: dlg.creating ? "Create and add" : "Add project"
            enabled: (dlg.creating ? newFolderName.text.trim() !== "" && dlg.newParent !== "" : dlg.folder !== "") && dlg.harness !== ""
            DialogButtonBox.buttonRole: DialogButtonBox.ApplyRole
            onClicked: dlg.add()
        }
    }

    ColumnLayout {
        width: parent.width
        spacing: 6

        FormLabel { text: dlg.creating ? "New folder" : "Folder" }
        RowLayout {
            visible: !dlg.creating
            Layout.fillWidth: true
            spacing: 8
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 32
                radius: 6
                color: Theme.background
                border.color: Theme.border
                Label {
                    anchors.fill: parent
                    leftPadding: 10
                    rightPadding: 10
                    verticalAlignment: Text.AlignVCenter
                    text: dlg.folder !== "" ? (dlg.remote ? Workspace.instance + ": " + (dlg.folder.split("/").pop() || "/") : dlg.folder) : dlg.remote ? "A folder on " + Workspace.instance : "A folder on this computer"
                    color: dlg.folder !== "" ? Theme.foreground : Theme.muted
                    font.family: dlg.folder !== "" ? Theme.mono : Theme.uiFont
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideMiddle
                }
                TapHandler { onTapped: dlg.chooseFolder() }
            }
            FlatButton {
                objectName: "addProjectChoose"
                text: dlg.folder === "" ? "Choose…" : "Change…"
                accent: dlg.folder === "" ? Theme.primary : Theme.foreground
                font.pixelSize: Theme.px(12)
                onClicked: dlg.chooseFolder()
            }
        }
        FlatButton {
            objectName: "addProjectNewFolder"
            visible: !dlg.creating && !dlg.remote
            text: "Or start a new folder…"
            outlined: false
            padding: 2
            font.pixelSize: Theme.px(12)
            onClicked: {
                dlg.newParent = Workspace.newProjectParent()
                dlg.creating = true
                dlg.problem = ""
                newFolderName.forceActiveFocus()
            }
        }
        // New folder: its name, and where.
        AppTextField {
            id: newFolderName
            objectName: "addProjectNewName"
            visible: dlg.creating
            Layout.fillWidth: true
            placeholderText: "Its name, e.g. my-app"
            font.pixelSize: Theme.px(12.5)
            onTextEdited: dlg.problem = ""
            onAccepted: if (text.trim() !== "" && dlg.harness !== "") dlg.add()
        }
        RowLayout {
            visible: dlg.creating
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: "In " + dlg.newParent
                color: Theme.muted
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                elide: Text.ElideMiddle
            }
            FlatButton { objectName: "addProjectNewWhere"; text: "Change…"; font.pixelSize: Theme.px(12); onClicked: parentDialog.open() }
            FlatButton { text: "Use an existing folder"; font.pixelSize: Theme.px(12); onClicked: { dlg.creating = false; dlg.problem = "" } }
        }
        FormNote {
            visible: dlg.creating
            text: "Harlo makes the folder and sets up git in it (a workflow needs a repository)."
        }
        PlainLabel {
            objectName: "addProjectProblem"
            visible: dlg.problem !== ""
            Layout.fillWidth: true
            text: dlg.problem
            color: Theme.error
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        FolderDialog {
            id: parentDialog
            title: "Where the new folder goes"
            currentFolder: "file://" + dlg.newParent
            onAccepted: dlg.newParent = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, "")).replace(/\/+$/, "") || "/"
        }

        FormLabel { text: "Agent"; Layout.topMargin: 10 }
        Flow {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: dlg.agents
                Button {
                    id: ab
                    required property var modelData
                    readonly property bool on: dlg.harness === modelData.id
                    objectName: "addProjectAgent-" + modelData.id
                    flat: true
                    implicitHeight: 30
                    leftPadding: 10
                    rightPadding: 10
                    onClicked: dlg.harness = modelData.id
                    contentItem: Row {
                        spacing: 6
                        ProviderMark { anchors.verticalCenter: parent.verticalCenter; provider: ab.modelData.id; tip: ""; implicitWidth: 13; implicitHeight: 13; color: ab.on ? Theme.foreground : Theme.muted }
                        Label { anchors.verticalCenter: parent.verticalCenter; text: ab.modelData.label; color: ab.on ? Theme.foreground : Theme.muted; font.pixelSize: Theme.px(12.5) }
                    }
                    background: ControlSurface { control: ab; variant: ab.on ? "selected" : "outline"; radius: 7 }
                }
            }
        }
        FormNote { visible: dlg.agents.length === 0; text: "No agent is on: turn one on in Settings → Agents." }

        // The model (not for a host's project: its agent's own).
        ColumnLayout {
            visible: !dlg.remote && dlg.harness !== ""
            Layout.fillWidth: true
            Layout.topMargin: 10
            spacing: 6
            FormLabel { text: "Model" }
            AppTextField {
                id: modelFilter
                objectName: "addProjectModelFilter"
                Layout.fillWidth: true
                placeholderText: "Filter models"
                font.pixelSize: Theme.px(12)
                Keys.onDownPressed: models.incrementCurrentIndex()
                Keys.onUpPressed: models.decrementCurrentIndex()
                onAccepted: { const m = models.model[models.currentIndex]; if (m) dlg.model = m.id }
            }
            ListView {
                id: models
                objectName: "addProjectModels"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 220)
                clip: true
                highlightMoveDuration: 0
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                model: [{ id: "", name: "Default model" }].concat(dlg.modelChoices)
                    .filter(m => m.id === "" ? modelFilter.text === "" : (m.id + " " + m.name).toLowerCase().includes(modelFilter.text.toLowerCase()))
                delegate: ItemDelegate {
                    id: mRow
                    required property var modelData
                    required property int index
                    objectName: "addProjectModel-" + modelData.id
                    // The first of each group: its name above it (when there
                    // are local models; "Default model" has none).
                    readonly property real heading: dlg.modelChoices.length > 0 && dlg.modelChoices[0].local && modelData.id !== ""
                        && (index === 0 || ListView.view.model[index - 1].id === "" || !!ListView.view.model[index - 1].local !== !!modelData.local) ? 30 : 0
                    width: ListView.view.width
                    height: 30 + heading
                    topInset: heading
                    topPadding: heading + bottomPadding
                    highlighted: dlg.model === modelData.id
                    background: Rectangle { radius: 6; color: mRow.highlighted ? Theme.rowSelected : mRow.hovered ? Theme.hover : "transparent" }
                    Label {
                        visible: mRow.heading > 0
                        x: 8
                        y: mRow.heading - height - 3
                        text: mRow.modelData.local ? "LOCAL" : "ONLINE"
                        color: Theme.label
                        font.pixelSize: Theme.px(11)
                        font.weight: Font.Medium
                        font.letterSpacing: Theme.px(11) * 0.07
                    }
                    contentItem: RowLayout {
                        Label {
                            Layout.fillWidth: true
                            text: Theme.modelLabel(mRow.modelData.name) + (mRow.highlighted ? "  ✓" : "")
                            color: mRow.modelData.id === "" ? Theme.muted : Theme.foreground
                            font.pixelSize: Theme.px(12.5)
                            elide: Text.ElideRight
                        }
                        // (A local model: its engine.)
                        Label {
                            readonly property string engine: mRow.modelData.local ? Theme.modelEngine(dlg.harness, mRow.modelData.id) : ""
                            visible: engine !== "" || (mRow.modelData.id !== "" && mRow.modelData.id !== mRow.modelData.name)
                            text: engine || Theme.providerOf(mRow.modelData.id)
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    onClicked: dlg.model = modelData.id
                }
            }
            FormNote {
                visible: dlg.modelChoices.length === 0
                text: Workspace.providersJson().indexOf('"checking"') >= 0 ? "Reading " + Theme.harnessLabel(dlg.harness) + "'s models…"
                    : Theme.harnessLabel(dlg.harness) + " starts with its own default model; a login in Settings → Agents or an API key in Settings → Models gives it more."
            }
        }
        FormNote {
            Layout.topMargin: 6
            text: "New sessions in this project start with these. You can pick others for a session, or change them in Settings → Projects."
        }
    }

}
