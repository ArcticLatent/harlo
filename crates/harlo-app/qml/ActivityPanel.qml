import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// What the active session did: every command it ran and every file it
// read, changed or wrote (other tools too), newest first, with how each
// went. From its messages (the context report's `actions`), so saved
// sessions have it too. A file opens on click; a command copies.
Rectangle {
    id: root
    required property ChatSession session
    signal closeRequested()
    color: Theme.canvas
    clip: true

    readonly property var log: {
        if (!session.contextJson) return null
        const r = JSON.parse(session.contextJson)
        return r.actions || { actions: [], commands: 0, failed: 0, changed: [], read: [] }
    }
    // all | command | change | read | other
    property string filter: "all"
    readonly property var shown: {
        if (!log) return []
        const of = { command: ["command"], change: ["edit", "write"], read: ["read", "search"], other: ["web", "agent", "other"] }
        return log.actions.filter(a => filter === "all" || of[filter].indexOf(a.kind) >= 0).slice().reverse()
    }
    function when(ms) { return ms ? new Date(ms).toLocaleTimeString(Qt.locale(), "h:mm AP") : "" }
    function icon(kind) {
        return ({ command: "terminal-box", edit: "code-box", write: "add-circle", read: "eye", search: "search", web: "global", agent: "robot-2" })[kind] || "sparkling"
    }
    /// The file's full path (the agent may give it relative to the folder).
    function fullPath(p) { return p.startsWith("/") || p.startsWith("ssh://") ? p : session.cwd + "/" + p }
    /// The log as Markdown (Copy).
    function markdown() {
        const lines = ["# " + (session.title || "Session") + ": what the agent did", ""]
        for (const a of log.actions) {
            const result = a.error ? "failed" + (a.result && a.result !== "failed" ? " (" + a.result + ")" : "") : a.result
            lines.push("- " + root.when(a.ts) + " **" + a.tool + "** `" + a.target.replace(/`/g, "'") + "`" + (result ? " — " + result : ""))
        }
        return lines.join("\n")
    }

    function reload() { if (visible && session.key !== "") session.loadContext() }
    onVisibleChanged: reload()
    Connections {
        target: root.session
        function onKeyChanged() { root.reload() }
        function onStatusChanged() { if (root.session.status !== "busy") root.reload() }
    }
    // While a run goes, keep up.
    Timer { interval: 2000; repeat: true; running: root.visible && root.session.status === "busy"; onTriggered: root.reload() }
    // Copies through a hidden TextEdit (QML has no clipboard API).
    TextEdit { id: clip; visible: false; function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" } }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Activity"
            subtitle: root.session.title || ""
            onCloseRequested: root.closeRequested()
            PanelButton {
                objectName: "activityCopy"
                iconName: "survey"
                tip: "Copy the log as Markdown"
                enabled: !!root.log && root.log.actions.length > 0
                onClicked: { clip.copyText(root.markdown()); copiedTip.show("Copied", 1200) }
                ToolTip { id: copiedTip }
            }
            PanelButton { iconName: "refresh"; tip: "Refresh"; onClicked: root.reload() }
        }

        Label {
            Layout.margins: 18
            visible: !root.log || root.log.actions.length === 0
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: root.session.key === "" ? "No session selected." : !root.log ? "Loading…" : "Nothing yet: commands the agent runs and files it reads or changes show here."
            color: Theme.muted
        }

        // In all: commands (failed), files changed, files read.
        Flow {
            objectName: "activitySummary"
            Layout.fillWidth: true
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.topMargin: 12
            visible: !!root.log && root.log.actions.length > 0
            spacing: 6
            Repeater {
                model: root.log ? [
                    { id: "all", label: root.log.actions.length + " actions" },
                    { id: "command", label: root.log.commands + (root.log.commands === 1 ? " command" : " commands") + (root.log.failed > 0 ? " · " + root.log.failed + " failed" : "") },
                    { id: "change", label: root.log.changed.length + (root.log.changed.length === 1 ? " file changed" : " files changed") },
                    { id: "read", label: root.log.read.length + " read" },
                    { id: "other", label: "Other" },
                ] : []
                Button {
                    id: filterChip
                    required property var modelData
                    objectName: "activityFilter-" + modelData.id
                    flat: true
                    implicitHeight: 28
                    leftPadding: 10
                    rightPadding: 10
                    onClicked: root.filter = modelData.id
                    contentItem: Label {
                        text: filterChip.modelData.label
                        color: root.filter === filterChip.modelData.id ? Theme.text : Theme.textMuted
                        font.pixelSize: Theme.px(12)
                    }
                    background: ControlSurface { control: filterChip; variant: root.filter === filterChip.modelData.id ? "selected" : "outline"; radius: 14 }
                }
            }
        }

        ListView {
            id: list
            objectName: "activityList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 8
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            model: root.shown
            delegate: ItemDelegate {
                id: row
                required property var modelData
                required property int index
                readonly property bool isFile: ["edit", "write", "read"].indexOf(modelData.kind) >= 0 && modelData.target !== "" && modelData.target.indexOf(", ") < 0
                objectName: "activity-" + index
                width: ListView.view.width
                leftPadding: 16
                rightPadding: 16
                topPadding: 7
                bottomPadding: 7
                onClicked: {
                    if (row.isFile) Workspace.openFileTab(root.fullPath(modelData.target))
                    else if (modelData.kind === "command") { clip.copyText(modelData.target); copied.show("Command copied", 1200) }
                }
                ToolTip { id: copied }
                ToolTip.visible: hovered && !copied.visible
                ToolTip.delay: 600
                ToolTip.text: modelData.tool + ": " + modelData.target + (row.isFile ? "\nClick to open" : modelData.kind === "command" ? "\nClick to copy" : "")
                background: Rectangle {
                    color: row.hovered ? Theme.rowHover : "transparent"
                    Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                    Rectangle { visible: row.index > 0; width: parent.width - 32; x: 16; height: 1; color: Theme.hairline }
                }
                contentItem: RowLayout {
                    spacing: 9
                    Icon {
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: 1
                        name: root.icon(row.modelData.kind)
                        color: row.modelData.error ? Theme.error : Theme.textMuted
                        implicitWidth: 15
                        implicitHeight: 15
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            text: row.modelData.target !== "" ? row.modelData.target : row.modelData.tool
                            color: Theme.text
                            font.family: row.modelData.target !== "" ? Theme.mono : Theme.uiFont
                            font.pixelSize: Theme.px(12)
                            elide: row.isFile ? Text.ElideMiddle : Text.ElideRight
                            maximumLineCount: 2
                            wrapMode: row.isFile ? Text.NoWrap : Text.WrapAnywhere
                        }
                        Label {
                            Layout.fillWidth: true
                            text: [row.modelData.tool, row.modelData.pending ? "running…" : row.modelData.error ? "failed" + (row.modelData.result && row.modelData.result !== "failed" ? " · " + row.modelData.result : "") : row.modelData.result]
                                  .filter(s => s).join(" · ")
                            color: row.modelData.error ? Theme.error : Theme.textFaint
                            font.pixelSize: Theme.px(11.5)
                            elide: Text.ElideRight
                        }
                    }
                    Label {
                        Layout.alignment: Qt.AlignTop
                        text: root.when(row.modelData.ts)
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                    }
                }
            }
        }
    }
}
