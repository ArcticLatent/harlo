import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Opening a pull request (a merge request on GitLab) from the branch: its
// title and description (from its commits), the branch it goes into, as a
// draft or not. Created with gh / glab (pushing the branch's new commits
// first), or on the site's own page (Forgejo, or without the tool).
AppDialog {
    id: dlg
    objectName: "pullRequestDialog"
    required property GitPanel git
    readonly property var req: git.requestJson !== "" ? JSON.parse(git.requestJson) : ({ state: "none" })
    readonly property bool creating: req.state === "creating"
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
    title: "New " + (req.word || "pull request") + " · " + (req.repo || "")

    function show() {
        titleField.text = req.title || ""
        bodyField.text = req.body || ""
        baseField.text = req.base || "main"
        draft.checked = false
        open()
        titleField.forceActiveFocus()
    }
    // Created: it closes (the panel's button opens it now).
    onReqChanged: if (visible && req.state === "ready" && (req.url || "") !== "") close()

    ColumnLayout {
        width: parent.width
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: (dlg.req.head || "") + " → " + (dlg.req.base || "") + " on " + (dlg.req.host || "")
                  + (dlg.git.ahead > 0 ? " · its " + dlg.git.ahead + (dlg.git.ahead === 1 ? " new commit is" : " new commits are") + " pushed first" : "")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }
        Label { text: "Title"; color: Theme.text; font.pixelSize: Theme.px(13); Layout.topMargin: 4 }
        AppTextField {
            id: titleField
            objectName: "requestTitle"
            Layout.fillWidth: true
            enabled: !dlg.creating
        }
        Label { text: "Description"; color: Theme.text; font.pixelSize: Theme.px(13); Layout.topMargin: 4 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 140
            TextArea {
                id: bodyField
                objectName: "requestBody"
                enabled: !dlg.creating
                wrapMode: TextEdit.Wrap
                color: Theme.text
                placeholderText: "What it changes and why (optional)"
                placeholderTextColor: Theme.textFaint
                font.pixelSize: Theme.px(13)
                background: Rectangle { radius: 8; color: Theme.background; border.color: bodyField.activeFocus ? Theme.accent : Theme.border }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 4
            spacing: 8
            Label { text: "Into"; color: Theme.text; font.pixelSize: Theme.px(13) }
            AppTextField {
                id: baseField
                objectName: "requestBase"
                Layout.preferredWidth: 180
                enabled: !dlg.creating
                font.family: Theme.mono
                font.pixelSize: Theme.px(12.5)
            }
            Item { Layout.fillWidth: true }
            AppCheckBox { id: draft; objectName: "requestDraft"; text: "Draft"; enabled: !dlg.creating }
        }
        Label {
            visible: (dlg.req.error || "") !== ""
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlg.req.error || ""
            color: Theme.error
            font.pixelSize: Theme.px(12.5)
        }
        Label {
            visible: !dlg.req.canCreate
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlg.req.service === "forgejo" ? "To open it from here, sign in to Forgejo in Settings → Git (a token from the server). Or open it on the site: the page fills in the branches."
                : "Creating it from harlo needs " + (dlg.req.service === "gitlab" ? "glab" : "the GitHub CLI (gh)") + " (Settings → Git). Its site can still open it."
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }
    }

    footer: Item {
        implicitHeight: buttons.implicitHeight + 26
        RowLayout {
            id: buttons
            x: 20
            y: 8
            width: parent.width - 40
            spacing: 8
            DialogButton {
                objectName: "requestWeb"
                text: "Open on " + (dlg.req.label || "the site")
                onClicked: { Qt.openUrlExternally(dlg.req.web); dlg.close() }
            }
            Item { Layout.fillWidth: true }
            BusyIndicator { visible: dlg.creating; running: visible; implicitWidth: 18; implicitHeight: 18 }
            DialogButton { text: "Cancel"; onClicked: dlg.close() }
            DialogButton {
                objectName: "requestCreate"
                visible: dlg.req.canCreate === true
                text: dlg.creating ? "Creating…" : "Create"
                accent: true
                enabled: !dlg.creating && titleField.text.trim() !== "" && baseField.text.trim() !== ""
                onClicked: dlg.git.createRequest(titleField.text, bodyField.text, baseField.text.trim(), draft.checked)
            }
        }
    }
}
