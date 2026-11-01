import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Removes a project from harlo (the sidebar's menu, Settings → Projects):
// its folder stays, and its sessions move to Other sessions unless they're
// deleted with it.
AppDialog {
    id: dlg
    objectName: "removeProjectDialog"
    /// Removed: what asked can move on.
    signal removed()

    property int projectId: -1
    property string projectName: ""
    /// What its sessions are: `{ count, trash, opencode, listed, archived,
    /// running, worktree }`.
    property var sessions: ({})
    readonly property int count: sessions.count || 0
    readonly property bool deleting: deleteSessions.checked && count > 0

    function ask(id, name) {
        projectId = id
        projectName = name
        sessions = JSON.parse(Workspace.projectSessions(id))
        deleteSessions.checked = false
        open()
    }
    function plural(n, one, many) { return n + " " + (n === 1 ? one : many) }

    title: "Remove project?"
    footer: AppButtonBox {
        DialogButton { text: "Cancel"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        DialogButton {
            objectName: "removeProjectConfirm"
            text: dlg.deleting ? "Remove and delete" : "Remove"
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            accent: false
            destructive: true
        }
    }
    onAccepted: {
        if (deleting) Workspace.removeProjectAndSessions(projectId)
        else Workspace.removeProject(projectId)
        removed()
    }

    ColumnLayout {
        spacing: 10
        width: 380
        Label {
            Layout.fillWidth: true
            text: dlg.projectName + "\n\nIts folder and files stay; harlo stops listing it." + (dlg.count > 0 && !dlg.deleting ? " Its sessions move to Other sessions." : "")
            wrapMode: Text.Wrap
            color: Theme.muted
        }
        AppCheckBox {
            id: deleteSessions
            objectName: "removeProjectSessions"
            visible: dlg.count > 0
            Layout.fillWidth: true
            text: "Also delete " + (dlg.count === 1 ? "its session" : "its " + dlg.count + " sessions")
        }
        // What deleting does, by what can be got back.
        Label {
            visible: dlg.deleting
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            text: [
                dlg.sessions.trash ? dlg.plural(dlg.sessions.trash, "goes", "go") + " to the trash, so you can still restore " + (dlg.sessions.trash === 1 ? "it" : "them") + " from there." : "",
                dlg.sessions.opencode ? dlg.plural(dlg.sessions.opencode, "OpenCode session is", "OpenCode sessions are") + " deleted with subagents' sessions. This can't be undone." : "",
                dlg.sessions.listed ? dlg.plural(dlg.sessions.listed, "Hermes session stays", "Hermes sessions stay") + " in Hermes' own copy; harlo stops listing " + (dlg.sessions.listed === 1 ? "it" : "them") + "." : "",
                dlg.sessions.archived ? "Including " + dlg.sessions.archived + " archived." : "",
                dlg.sessions.running ? dlg.plural(dlg.sessions.running, "is", "are") + " running, and will be stopped." : "",
                dlg.sessions.worktree ? "Worktree folders stay on disk." : "",
            ].filter(x => x !== "").join("\n")
        }
    }
}
