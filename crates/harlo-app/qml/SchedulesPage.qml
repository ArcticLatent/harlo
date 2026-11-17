pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Schedules: prompts that run by themselves, at set times.
ColumnLayout {
    id: schedules
    spacing: 0
    readonly property var list: Workspace.schedulesRevision >= 0 ? JSON.parse(Workspace.schedulesJson()) : []
    readonly property var projects: Workspace.projectList().map(l => { const p = l.split("\t"); return { id: Number(p[0]), name: p[1] } })
    // "in 3 h", "in 12 min", "tomorrow 09:00"…
    function when(ts) {
        if (!ts) return ""
        const d = new Date(ts * 1000), now = new Date(), mins = Math.round((d - now) / 60000)
        const clock = d.toLocaleTimeString(Qt.locale(), "HH:mm")
        const day = d.toDateString() === now.toDateString() ? "today" : d.toDateString() === new Date(now.getTime() + 86400000).toDateString() ? "tomorrow" : d.toLocaleDateString(Qt.locale(), "ddd d MMM")
        const rel = mins < 1 ? "now" : mins < 60 ? "in " + mins + " min" : mins < 48 * 60 ? "in " + Math.round(mins / 60) + " h" : ""
        return day + " " + clock + (rel !== "" ? " (" + rel + ")" : "")
    }
    function ago(ts) {
        if (!ts) return "never"
        const d = new Date(ts * 1000), now = new Date()
        const day = d.toDateString() === now.toDateString() ? "today" : d.toLocaleDateString(Qt.locale(), "ddd d MMM")
        return day + " " + d.toLocaleTimeString(Qt.locale(), "HH:mm")
    }

    SectionHeader {
        title: "Scheduled prompts"
        subtitle: "Send a prompt on a schedule, e.g. every weekday morning."
        more: "Each run starts a new session in the project; you get the usual notification when it finishes. Schedules run while Harlo is running (Settings → General → Start Harlo when you log in)."
        AddButton { objectName: "newSchedule"; text: "New schedule"; onClicked: editor.openFor(null) }
    }
    Group {
        visible: schedules.list.length === 0
        Layout.topMargin: 12
        EmptyNote { text: "No schedules yet." }
    }
    Repeater {
        model: schedules.list
        Rectangle {
            id: scheduleCard
            required property var modelData
            readonly property var sch: modelData
            Layout.fillWidth: true
            Layout.topMargin: 10
            implicitHeight: cardCol.implicitHeight + 24
            radius: 10
            color: Theme.elevated
            border.color: Theme.border
            opacity: sch.enabled ? 1 : 0.7
            ColumnLayout {
                id: cardCol
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 4
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Icon { name: "history"; color: scheduleCard.sch.enabled ? Theme.primary : Theme.muted; implicitWidth: 16; implicitHeight: 16 }
                    Label { Layout.fillWidth: true; text: scheduleCard.sch.title; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(14); elide: Text.ElideRight }
                    AppSwitch {
                        objectName: "scheduleSwitch"
                        checked: scheduleCard.sch.enabled
                        onToggled: Workspace.setScheduleEnabled(scheduleCard.sch.id, checked)
                    }
                }
                Label {
                    Layout.fillWidth: true
                    text: [scheduleCard.sch.describe, scheduleCard.sch.projectName, scheduleCard.sch.harness || "default harness", scheduleCard.sch.model].filter(x => x).join(" · ")
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    text: scheduleCard.sch.prompt
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12)
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 4
                    spacing: 6
                    Label {
                        Layout.fillWidth: true
                        text: (scheduleCard.sch.enabled ? (scheduleCard.sch.nextRun ? "Next: " + schedules.when(scheduleCard.sch.nextRun) : "No more runs") : "Off") + "  ·  Last: " + schedules.ago(scheduleCard.sch.lastRun)
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                        elide: Text.ElideRight
                    }
                    FlatButton { text: "Run now"; font.pixelSize: Theme.px(12); onClicked: Workspace.runScheduleNow(scheduleCard.sch.id) }
                    FlatButton { text: "Edit"; font.pixelSize: Theme.px(12); onClicked: editor.openFor(scheduleCard.sch) }
                    FlatButton { text: "Delete"; font.pixelSize: Theme.px(12); accent: Theme.error; onClicked: { confirmDelete.sch = scheduleCard.sch; confirmDelete.open() } }
                }
            }
        }
    }

    AppDialog {
        id: confirmDelete
        property var sch: ({})
        title: "Delete “" + (sch.title || "") + "”?"
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label { width: 360; wrapMode: Text.Wrap; color: Theme.muted; text: "It stops running. Sessions it started stay." }
        onDiscarded: { Workspace.deleteSchedule(sch.id); close() }
    }

    // ---- the editor ------------------------------------------------
    AppDialog {
        id: editor
        property var sch: null
        property string kind: "days"
        property int days: 0x1f
        property string harness: ""
        property string error: ""
        title: sch ? "Edit schedule" : "New schedule"
        implicitWidth: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 40)
        function pad(n) { return (n < 10 ? "0" : "") + n }
        function openFor(s) {
            sch = s
            error = ""
            titleField.text = s ? s.title : ""
            promptField.text = s ? s.prompt : ""
            projectBox.currentIndex = s ? Math.max(0, schedules.projects.findIndex(p => p.id === s.projectId) + 1) : 0
            harness = s ? s.harness : ""
            modelField.text = s ? s.model : ""
            const r = s ? s.repeat : { kind: "days", days: 0x1f, hour: 9, minute: 0 }
            kind = r.kind
            days = r.kind === "days" ? r.days : 0x1f
            timeField.text = r.kind === "days" ? pad(r.hour) + ":" + pad(r.minute) : "09:00"
            everyField.text = r.kind === "every" ? String(r.minutes % 60 === 0 ? r.minutes / 60 : r.minutes) : "1"
            everyHours.checked = r.kind === "every" ? r.minutes % 60 === 0 : true
            const t = new Date(Date.now() + 86400000)
            onceField.text = r.kind === "once" ? new Date(r.at * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm")
                                               : t.toLocaleDateString(Qt.locale(), "yyyy-MM-dd") + " 09:00"
            catchUp.checked = s ? s.catchUp : true
            open()
        }
        function save() {
            const p = projectBox.currentIndex > 0 ? schedules.projects[projectBox.currentIndex - 1].id : -1
            const repeat = kind === "days" ? { kind: "days", days: days, time: timeField.text }
                         : kind === "every" ? { kind: "every", minutes: Number(everyField.text) * (everyHours.checked ? 60 : 1) }
                         : { kind: "once", at: onceField.text }
            error = Workspace.saveSchedule(JSON.stringify({
                id: sch ? sch.id : 0, title: titleField.text, prompt: promptField.text, projectId: p,
                harness: harness, model: modelField.text, repeat: repeat, enabled: sch ? sch.enabled : true, catchUp: catchUp.checked
            }))
            if (error === "") close()
        }
        footer: Item {
            implicitHeight: editorButtons.implicitHeight + 26
            implicitWidth: editorButtons.implicitWidth + 40
            Row {
                id: editorButtons
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.top: parent.top
                anchors.topMargin: 8
                spacing: 8
                DialogButton { text: "Cancel"; onClicked: editor.close() }
                DialogButton { text: "Save"; accent: true; onClicked: editor.save() }
            }
        }
        ColumnLayout {
            width: parent.width
            spacing: 8
            AppTextField { id: titleField; objectName: "scheduleTitle"; Layout.fillWidth: true; placeholderText: "Name (optional: the prompt's first line)" }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 90
                TextArea {
                    id: promptField
                    objectName: "schedulePrompt"
                    wrapMode: TextEdit.Wrap
                    color: Theme.foreground
                    placeholderText: "The prompt to send, e.g. \"Summarize yesterday's commits and anything that looks unfinished.\""
                    placeholderTextColor: Theme.muted
                    selectionColor: Theme.selection
                    font.pixelSize: Theme.px(13)
                    background: Rectangle { radius: 6; color: Theme.background; border.color: promptField.activeFocus ? Theme.primary : Theme.border }
                    onTextChanged: editor.error = ""
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Label { text: "In"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                ComboField { id: projectBox; Layout.fillWidth: true; model: ["Home folder"].concat(schedules.projects.map(p => p.name)) }
                Repeater {
                    model: [["", "Default"], ["harlo", "Harlo"], ["claude", "Claude"]]
                    ChoicePill { required property var modelData; text: modelData[1]; selected: editor.harness === modelData[0]; onClicked: editor.harness = modelData[0] }
                }
            }
            AppTextField { id: modelField; Layout.fillWidth: true; placeholderText: "Model (optional, provider/model)"; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
            RowLayout {
                spacing: 6
                Label { text: "When"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.rightMargin: 4 }
                Repeater {
                    model: [["days", "On days"], ["every", "Every"], ["once", "Once"]]
                    ChoicePill { required property var modelData; text: modelData[1]; selected: editor.kind === modelData[0]; onClicked: editor.kind = modelData[0] }
                }
            }
            RowLayout {
                visible: editor.kind === "days"
                spacing: 4
                Repeater {
                    model: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                    ChoicePill {
                        required property string modelData
                        required property int index
                        text: modelData
                        selected: (editor.days & (1 << index)) !== 0
                        onClicked: editor.days = editor.days ^ (1 << index)
                    }
                }
                Label { text: "at"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.leftMargin: 6 }
                AppTextField { id: timeField; objectName: "scheduleTime"; Layout.preferredWidth: 70; placeholderText: "09:00" }
            }
            RowLayout {
                visible: editor.kind === "every"
                spacing: 6
                AppTextField { id: everyField; objectName: "scheduleEvery"; Layout.preferredWidth: 60; validator: IntValidator { bottom: 1; top: 9999 } }
                ChoicePill { id: everyHours; checkable: true; text: "hours"; selected: checked; onClicked: checked = true }
                ChoicePill { text: "minutes"; selected: !everyHours.checked; onClicked: everyHours.checked = false }
            }
            RowLayout {
                visible: editor.kind === "once"
                spacing: 6
                AppTextField { id: onceField; objectName: "scheduleOnce"; Layout.preferredWidth: 160; placeholderText: "2026-10-13 09:00" }
                Label { text: "(date and time)"; color: Theme.muted; font.pixelSize: Theme.px(11) }
            }
            AppCheckBox { id: catchUp; text: "If Harlo was closed at the time, run it when Harlo starts" }
            Label {
                Layout.fillWidth: true
                visible: editor.error !== ""
                text: editor.error
                color: Theme.error
                font.pixelSize: Theme.px(12)
                wrapMode: Text.Wrap
            }
        }
    }
}
