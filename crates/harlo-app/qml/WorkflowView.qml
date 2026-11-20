pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// The New workflow page (the sidebar's New workflow): pick a project, then
// the same form as a project's Run a workflow… dialog. A run works in a
// worktree of a project, so the home folder isn't offered.
Rectangle {
    id: root
    color: Theme.background

    property int projectId: -1
    property var projects: []
    readonly property var project: projects.find(p => p.id === projectId) || null
    // Test hooks.
    property alias form: form

    /// A run started: the page is done with.
    signal startedWorkflow()
    signal editWorkflowsRequested()
    signal addProjectRequested()

    /// Opens the page for a project (-1, or one that isn't there: the
    /// first project), keeping what was typed for the same project.
    function openFor(id) {
        projects = JSON.parse(Workspace.projectsJson())
        if (!projects.some(p => p.id === id)) id = projects.length > 0 ? projects[0].id : -1
        pick(id)
    }
    function pick(id) {
        const p = projects.find(x => x.id === id)
        projectId = p ? id : -1
        if (p) {
            form.load(p.id, p.name, p.harness)
            form.taskArea.forceActiveFocus()
        }
    }

    // (A tab kept from last time: filled in when first shown.)
    onVisibleChanged: if (visible && project === null) openFor(-1)

    Connections {
        target: Workspace
        function onProjectsRevisionChanged() {
            if (!root.visible) return
            root.projects = JSON.parse(Workspace.projectsJson())
            if (root.project === null) root.openFor(-1)
        }
        // Another machine: its projects.
        function onInstanceChanged() { root.openFor(-1) }
    }

    AppMenu {
        id: projectMenu
        Repeater {
            model: root.projects
            AppMenuItem {
                required property var modelData
                text: modelData.name
                marked: root.projectId === modelData.id
                onTriggered: root.pick(modelData.id)
            }
        }
        AppMenuSeparator { visible: root.projects.length > 0 }
        AppMenuItem { text: "Add project…"; onTriggered: root.addProjectRequested() }
    }

    Flickable {
        anchors.fill: parent
        contentHeight: Math.max(height, page.implicitHeight + 80)
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: page
            objectName: "workflowPage"
            width: Math.min(parent.width - 48, Theme.px(640))
            x: (parent.width - width) / 2
            y: Math.max(40, (parent.height - implicitHeight) / 2 - 40)
            spacing: 14

            Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.StyledText
                text: root.project ? "Run a workflow in <b>" + root.project.name.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</b>" : "Run a workflow"
                color: Theme.foreground
                font.pixelSize: Theme.px(26)
                wrapMode: Text.Wrap
            }

            // The project (a run needs one).
            Button {
                id: projectChip
                objectName: "workflowProject"
                visible: root.projects.length > 0
                Layout.alignment: Qt.AlignHCenter
                flat: true
                implicitHeight: 30
                leftPadding: 10
                rightPadding: 8
                font.pixelSize: Theme.px(13)
                text: root.project ? root.project.name : ""
                onClicked: projectMenu.popup(this, 0, height + 4)
                contentItem: Row {
                    spacing: 7
                    ProjectIcon { anchors.verticalCenter: parent.verticalCenter; icon: root.project ? root.project.icon : ""; color: root.project ? root.project.color : ""; fallback: Theme.info; implicitWidth: 15; implicitHeight: 15 }
                    Label { anchors.verticalCenter: parent.verticalCenter; text: projectChip.text; color: Theme.foreground; font: projectChip.font }
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "arrow-down-s"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                }
                background: ControlSurface {
                    control: projectChip
                    Rectangle { anchors.fill: parent; radius: 3; color: "transparent"; border.color: Theme.primary; visible: projectChip.visualFocus }
                }
            }

            // No project yet: what to do first.
            ColumnLayout {
                objectName: "workflowNoProject"
                visible: root.projects.length === 0
                Layout.fillWidth: true
                spacing: 10
                PlainLabel {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("A workflow works in its own copy of a project. Add a project first.")
                    color: Theme.textMuted
                    wrapMode: Text.Wrap
                }
                FlatButton {
                    Layout.alignment: Qt.AlignHCenter
                    text: qsTr("Add project…")
                    onClicked: root.addProjectRequested()
                }
            }

            Rectangle {
                visible: root.project !== null
                Layout.fillWidth: true
                implicitHeight: form.implicitHeight + 32
                radius: 4
                color: Theme.elevated
                border.color: Theme.border
                WorkflowForm {
                    id: form
                    x: 16
                    y: 16
                    width: parent.width - 32
                    onEditRequested: root.editWorkflowsRequested()
                    onStarted: root.startedWorkflow()
                }
            }

            RowLayout {
                visible: root.project !== null
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                DialogButton {
                    objectName: "workflowPageStart"
                    text: qsTr("Start")
                    accent: true
                    enabled: form.canStart
                    onClicked: form.start()
                }
            }
        }
    }
}
