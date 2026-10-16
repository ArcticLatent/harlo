import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Dialogs for creating, configuring and finishing worktrees.
Item {
    id: root
    // Test hooks.
    property alias newDialog: newDialog
    property alias setupDialog: setupDialog
    property alias finishDialog: finishDialog
    property alias baseBox: baseBox

    function openNew(projectId, projectName, harness) {
        newDialog.projectId = projectId
        newDialog.title = "New worktree session in " + projectName
        branchField.text = Workspace.suggestBranch()
        // (Read off the UI thread: a remote project's over SSH.)
        baseBox.model = []
        Workspace.requestBranches(projectId)
        harnessBox.providers = JSON.parse(Workspace.chatProvidersJson())
        harnessBox.currentIndex = Math.max(0, harnessBox.providers.findIndex(p => p.id === harness))
        newDialog.open()
        branchField.selectAll()
        branchField.forceActiveFocus()
    }

    Connections {
        target: Workspace
        function onBranchesReady(id, list) {
            if (id !== newDialog.projectId) return
            baseBox.model = list
            baseBox.currentIndex = 0
        }
    }

    function openSetup(projectId, projectName) {
        setupDialog.projectId = projectId
        setupDialog.title = "Worktree setup for " + projectName
        setupArea.text = Workspace.projectSetup(projectId)
        setupDialog.open()
    }

    function openFinish(key) {
        const json = Workspace.worktreeInfo(key)
        if (json === "")
            return
        finishDialog.key = key
        finishDialog.info = JSON.parse(json)
        deleteBranch.checked = finishDialog.info.ahead === 0
        forceBox.checked = false
        finishDialog.open()
    }

    AppDialog {
        id: newDialog
        property int projectId: -1
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 440
        footer: AppButtonBox {
            DialogButton { text: "Create"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole; enabled: branchField.text.trim() !== "" && baseBox.count > 0 }
            DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onAccepted: Workspace.newWorktreeSession(projectId, branchField.text.trim(), baseBox.currentText, harnessBox.providers[harnessBox.currentIndex] ? harnessBox.providers[harnessBox.currentIndex].id : "")
        GridLayout {
            width: parent.width
            columns: 2
            columnSpacing: 10
            rowSpacing: 8
            Label { text: "Branch" }
            AppTextField { id: branchField; Layout.fillWidth: true; onAccepted: if (text.trim() !== "") newDialog.accept() }
            Label { text: "From" }
            ComboField { id: baseBox; Layout.fillWidth: true }
            Label { text: "Provider" }
            ComboField { id: harnessBox; property var providers: []; Layout.fillWidth: true; model: providers.map(p => p.label); marks: providers.map(p => p.id) }
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                text: baseBox.count === 0
                    ? "This project is not a git repository."
                    : "An existing branch is checked out; a new name branches off “" + baseBox.currentText + "”. The worktree lives in harlo's data folder, so your checkout is untouched."
            }
        }
    }

    AppDialog {
        id: setupDialog
        property int projectId: -1
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 480
        standardButtons: Dialog.Save | Dialog.Cancel
        onAccepted: Workspace.setProjectSetup(projectId, setupArea.text)
        ColumnLayout {
            width: parent.width
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                text: "Commands run in every new worktree of this project, one per line (for example: npm install, cargo fetch). Lines starting with # are skipped."
            }
            TextArea {
                id: setupArea
                Layout.fillWidth: true
                Layout.preferredHeight: 140
                font.family: Theme.mono
                wrapMode: TextEdit.NoWrap
                color: Theme.foreground
                selectionColor: Theme.selection
                background: Rectangle { radius: 6; color: Theme.background; border.color: setupArea.activeFocus ? Theme.primary : Theme.border }
            }
        }
    }

    AppDialog {
        id: finishDialog
        property string key: ""
        property var info: ({})
        title: "Worktree " + (info.branch || "")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 460
        footer: AppButtonBox {
            DialogButton {
                text: "Merge into " + (finishDialog.info.mainBranch || "main")
                enabled: finishDialog.info.exists === true && finishDialog.info.ahead > 0 && !!finishDialog.info.mainBranch
                onClicked: { Workspace.mergeWorktree(finishDialog.key); finishDialog.close() }
            }
            DialogButton {
                text: finishDialog.info.exists ? "Remove worktree" : "Forget"
                enabled: !(finishDialog.info.dirty > 0 && !forceBox.checked)
                onClicked: { Workspace.removeWorktree(finishDialog.key, deleteBranch.checked, forceBox.checked); finishDialog.close() }
            }
            DialogButton { text: "Close"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        ColumnLayout {
            width: parent.width
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: finishDialog.info.path || ""
                color: Theme.muted
                font.family: Theme.mono
                font.pixelSize: Theme.px(11)
                elide: Text.ElideMiddle
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: !finishDialog.info.exists
                    ? "The worktree folder no longer exists."
                    : (finishDialog.info.ahead || 0) + " commit(s) not in " + (finishDialog.info.base || "the base branch")
                      + (finishDialog.info.behind > 0 ? ", " + finishDialog.info.behind + " behind it" : "")
                      + ". " + (finishDialog.info.dirty > 0 ? finishDialog.info.dirty + " file(s) with uncommitted changes." : "No uncommitted changes.")
            }
            AppCheckBox { id: deleteBranch; text: "Also delete branch " + (finishDialog.info.branch || ""); visible: finishDialog.info.exists === true }
            AppCheckBox {
                id: forceBox
                visible: finishDialog.info.dirty > 0 || (deleteBranch.checked && finishDialog.info.ahead > 0)
                text: "Discard uncommitted changes and unmerged commits"
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.muted
                font.pixelSize: Theme.px(11)
                text: "Merging runs in the project's main checkout, which must be clean. Conflicts abort the merge."
            }
        }
    }
}
