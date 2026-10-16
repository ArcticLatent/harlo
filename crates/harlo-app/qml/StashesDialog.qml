import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Git stashes of the Changes panel's repository: stash what is uncommitted
// (untracked files too), and apply, pop or drop saved stashes.
AppDialog {
    id: dlg
    required property GitPanel git
    title: "Stashes"
    standardButtons: Dialog.Close
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)

    property var entries: []
    // (Read off the UI thread; `entries` follows git.stashList.)
    function reload() { git.loadStashes() }
    Connections { target: dlg.git; function onStashListChanged() { try { dlg.entries = dlg.git.stashList !== "" ? JSON.parse(dlg.git.stashList) : [] } catch (e) { dlg.entries = [] } } }
    onAboutToShow: { reload(); stashMessage.text = "" }
    // Results arrive asynchronously; reload when the git job finishes.
    Connections { target: dlg.git; function onBusyChanged() { if (!dlg.git.busy && dlg.visible) dlg.reload() } }

    readonly property int changed: git.stagedCount + git.unstagedCount
    function ago(ts) {
        if (!ts) return ""
        const s = Date.now() / 1000 - ts
        return s < 3600 ? Math.max(1, Math.round(s / 60)) + " min ago" : s < 86400 ? Math.round(s / 3600) + " h ago"
             : s < 86400 * 60 ? Math.round(s / 86400) + " days ago" : new Date(ts * 1000).toLocaleDateString()
    }

    AppDialog {
        id: confirmDrop
        property int index: -1
        title: "Drop stash@{" + index + "}?"
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label { width: 340; wrapMode: Text.Wrap; color: Theme.muted; text: "Its changes are deleted. This cannot be undone from harlo." }
        onDiscarded: { dlg.git.stashDrop(index); close() }
    }

    ColumnLayout {
        width: parent.width
        spacing: 10
        Label {
            Layout.fillWidth: true
            text: "Save uncommitted changes aside and bring them back later. Untracked files are included."
            color: Theme.muted
            wrapMode: Text.Wrap
            font.pixelSize: Theme.px(12)
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            AppTextField {
                id: stashMessage
                objectName: "stashMessage"
                Layout.fillWidth: true
                enabled: dlg.changed > 0
                placeholderText: dlg.changed > 0 ? "Message (optional)" : "No uncommitted changes"
                onAccepted: if (dlg.changed > 0) dlg.git.stashPush(text)
            }
            DialogButton {
                text: dlg.changed === 0 ? "Stash" : dlg.changed === 1 ? "Stash 1 file" : "Stash " + dlg.changed + " files"
                accent: true
                enabled: dlg.changed > 0 && !dlg.git.busy
                onClicked: { dlg.git.stashPush(stashMessage.text); stashMessage.text = "" }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: dlg.git.message !== ""
            text: dlg.git.message
            color: dlg.git.error ? Theme.error : Theme.muted
            wrapMode: Text.Wrap
            font.pixelSize: Theme.px(12)
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
        Label {
            visible: dlg.entries.length === 0
            text: "No stashes yet."
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 320)
            visible: dlg.entries.length > 0
            clip: true
            spacing: 6
            model: dlg.entries
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Rectangle {
                id: stash
                required property var modelData
                width: ListView.view.width
                implicitHeight: stashRow.implicitHeight + 16
                radius: 8
                color: Theme.background
                border.color: Theme.border
                RowLayout {
                    id: stashRow
                    x: 10
                    y: 8
                    width: parent.width - 20
                    spacing: 8
                    Icon { name: "archive-stack"; color: Theme.muted; implicitWidth: 16; implicitHeight: 16 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            text: stash.modelData.message || "(no message)"
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13)
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            text: ["stash@{" + stash.modelData.index + "}", stash.modelData.branch ? "on " + stash.modelData.branch : "", dlg.ago(stash.modelData.date)].filter(x => x).join(" · ")
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                            elide: Text.ElideRight
                        }
                    }
                    FlatButton {
                        text: "Apply"
                        font.pixelSize: Theme.px(12)
                        enabled: !dlg.git.busy
                        onClicked: dlg.git.stashApply(stash.modelData.index, false)
                        ToolTip.visible: hovered
                        ToolTip.text: "Restore these changes and keep the stash"
                    }
                    FlatButton {
                        text: "Pop"
                        font.pixelSize: Theme.px(12)
                        enabled: !dlg.git.busy
                        onClicked: dlg.git.stashApply(stash.modelData.index, true)
                        ToolTip.visible: hovered
                        ToolTip.text: "Restore these changes and drop the stash"
                    }
                    FlatButton {
                        text: "Drop"
                        font.pixelSize: Theme.px(12)
                        accent: Theme.error
                        enabled: !dlg.git.busy
                        onClicked: { confirmDrop.index = stash.modelData.index; confirmDrop.open() }
                    }
                }
            }
        }
    }
}
