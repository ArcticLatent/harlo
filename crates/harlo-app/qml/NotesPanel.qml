import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Notes for the active session's project (sessions outside any project
// share one pad): free text, saved as you type, in harlo's database. Lines
// like "- [ ] task" are todos, also listed with checkboxes. "Insert into
// message" hands the notes to the agent through the composer.
Rectangle {
    id: root
    /// The project (-1: none) and its name.
    property int projectId: -1
    property string projectName: ""
    /// The active session can take a message.
    property bool canInsert: false
    signal closeRequested()
    signal insertRequested(string text)
    color: Theme.canvas
    clip: true

    // ---- loading and saving ----------------------------------------------
    property int loadedFor: -2
    property bool loading: false
    function load() {
        if (loadedFor === projectId) return
        save()
        loading = true
        editor.text = Workspace.projectNotes(projectId)
        loading = false
        loadedFor = projectId
    }
    function save() {
        saveTimer.stop()
        if (loadedFor !== -2) Workspace.setProjectNotes(loadedFor, editor.text)
    }
    onProjectIdChanged: if (visible) load()
    onVisibleChanged: visible ? load() : save()
    Component.onCompleted: if (visible) load()
    Component.onDestruction: save()
    Timer { id: saveTimer; interval: 600; onTriggered: root.save() }

    // ---- todos: "- [ ] text" / "- [x] text" lines ------------------------
    readonly property var todos: {
        const out = []
        const lines = editor.text.split("\n")
        for (let i = 0; i < lines.length; i++) {
            const m = lines[i].match(/^\s*[-*] \[( |x|X)\] (.*)$/)
            if (m) out.push({ line: i, done: m[1] !== " ", text: m[2] })
        }
        return out
    }
    readonly property int openTodos: todos.filter(t => !t.done).length
    function setDone(line, done) {
        const lines = editor.text.split("\n")
        lines[line] = lines[line].replace(/\[( |x|X)\]/, done ? "[x]" : "[ ]")
        editor.text = lines.join("\n")
    }
    function addTodo(text) {
        if (text.trim() === "") return
        const t = editor.text
        editor.text = t + (t === "" || t.endsWith("\n") ? "" : "\n") + "- [ ] " + text.trim()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Notes"
            subtitle: root.projectId >= 0 ? root.projectName : "No project"
            onCloseRequested: root.closeRequested()
        }

        // The notes
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 14
            TextArea {
                id: editor
                objectName: "notesEditor"
                wrapMode: TextEdit.Wrap
                color: Theme.text
                padding: 12
                placeholderText: "Notes for " + (root.projectId >= 0 ? root.projectName : "sessions outside a project") + ": ideas, decisions, things to remember.\n\nLines like \"- [ ] task\" are todos."
                placeholderTextColor: Theme.textFaint
                selectionColor: Theme.selection
                font.pixelSize: Theme.px(13.5)
                background: Rectangle { radius: 12; color: Theme.raised; border.color: editor.activeFocus ? Theme.accent : Theme.hairline }
                onTextChanged: if (!root.loading) saveTimer.restart()
            }
        }

        // Todos
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 8
            visible: root.todos.length > 0
            spacing: 2
            Label {
                text: "Todos · " + (root.openTodos === 0 ? "all done" : root.openTodos + " open")
                font.capitalization: Font.AllUppercase
                color: Theme.label
                font.pixelSize: Theme.px(11)
                font.weight: Font.Medium
                font.letterSpacing: Theme.px(11) * 0.07
                bottomPadding: 4
            }
            ListView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 160)
                clip: true
                model: root.todos
                boundsBehavior: Flickable.StopAtBounds
                delegate: AppCheckBox {
                    required property var modelData
                    width: ListView.view.width
                    text: modelData.text
                    checked: modelData.done
                    onToggled: root.setDone(modelData.line, checked)
                }
            }
        }

        // Add a todo; hand the notes to the agent.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 6
            AppTextField {
                id: todoField
                objectName: "todoField"
                Layout.fillWidth: true
                placeholderText: "Add a todo (Enter)"
                onAccepted: { root.addTodo(text); text = "" }
            }
            FlatButton {
                text: "Insert into message"
                font.pixelSize: Theme.px(12)
                enabled: root.canInsert && editor.text.trim() !== ""
                onClicked: root.insertRequested("Project notes:\n" + editor.text.trim())
                ToolTip.visible: hovered
                ToolTip.text: root.canInsert ? "Add these notes to the message you're writing" : "Open a session first"
            }
        }
    }
}
