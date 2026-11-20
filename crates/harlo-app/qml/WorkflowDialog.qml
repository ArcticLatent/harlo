import QtQuick
import QtQuick.Controls.Basic
import harlo

// Starts a workflow in a project (a project's menu → Run a workflow…):
// WorkflowForm, in a dialog.
AppDialog {
    id: dialog
    signal editRequested()
    // A run started (from the dialog's Start).
    signal started()
    // Test hooks.
    property alias form: form

    function show(id, name, harness) {
        form.load(id, name, harness)
        open()
        form.taskArea.forceActiveFocus()
    }

    objectName: "workflowDialog"
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 520
    title: form.title
    footer: AppButtonBox {
        DialogButton {
            objectName: "workflowStart"
            text: qsTr("Start")
            accent: true
            // (No test command: the checks that would run it are left out.)
            enabled: form.canStart
            onClicked: form.start()
        }
        DialogButton { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
    }

    WorkflowForm {
        id: form
        width: parent.width
        onEditRequested: { dialog.close(); dialog.editRequested() }
        onStarted: { dialog.close(); dialog.started() }
    }
}
