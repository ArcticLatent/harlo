import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// One subagent (Claude's, Harlo's): status, name, task, the tool
// it is running, and View / Steer / Cancel (where its provider can).
// Used by the subagent dock and under task tool rows.
Rectangle {
    id: agentRow
    property ChatSession session
    property var a: ({})
    signal viewRequested(var agent, bool steer)
    // Finished is the usual end: a faint dot, no colour. Colour is for
    // what's news (running, failed, waiting for you).
    function statusColor(s) {
        return s === "running" || s === "pending" ? Theme.primary : s === "failed" ? Theme.error
             : s === "asking" || s === "waiting" ? Theme.warning : Theme.textFaint
    }
    // What it's doing, in words ("Running cargo test").
    function toolLine(a) {
        const p = a.progress || {}
        if (a.status !== "running" || !p.currentTool) return ""
        const args = String(p.currentToolIntent || (p.currentToolArgs ? (p.currentToolArgs.command || p.currentToolArgs.path || p.currentToolArgs.pattern || p.currentToolArgs.query || "") : "")).replace(/\s+/g, " ").slice(0, 80)
        if (p.currentToolIntent) return args
        const t = String(p.currentTool).toLowerCase()
        const verb = /^(bash|shell|exec|run|command)/.test(t) ? "Running" : /^read/.test(t) ? "Reading" : /^(edit|multiedit|patch|apply)/.test(t) ? "Editing"
                   : /^write/.test(t) ? "Writing" : /^(grep|find|glob|search|ls)/.test(t) ? "Searching" : /^(fetch|web)/.test(t) ? "Looking up" : ""
        return verb !== "" ? verb + (args ? " " + args : "") : (String(p.currentTool).charAt(0).toUpperCase() + String(p.currentTool).slice(1)) + (args ? ": " + args : "")
    }
    // Who it is, as the app names it elsewhere ("Claude Code · Sonnet 5").
    function agentName(id) { return ({ harlo: "Harlo", claude: "Claude Code", pi: "pi" })[id] || id || "" }
    function modelName(agent, model) {
        const p = Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(x => x.id === agent) : null
        const i = p ? (p.modelIds || []).indexOf(model) : -1
        if (i >= 0 && p.models[i]) return Theme.modelLabel(p.models[i])
        // (Not listed: a Claude id reads as its picker names it, "claude-sonnet-5" → "Sonnet 5".)
        const id = String(model).split("/").pop()
        const c = /^claude-([a-z]+)-(\d+)(?:-(\d+))?(\[1m\])?$/.exec(id)
        return c ? c[1].charAt(0).toUpperCase() + c[1].slice(1) + " " + c[2] + (c[3] ? "." + c[3] : "") + (c[4] ? " 1M" : "") : Theme.modelLabel(id)
    }
    readonly property bool live: a.status === "running" || a.status === "pending"
    // A harlo helper waiting for you: what it asks (a tool's approval is
    // answered here).
    readonly property var asks: a.asks || null
    readonly property bool approvable: !!asks && asks.approval && asks.options.indexOf("Approve") >= 0
    implicitHeight: rowCol.implicitHeight + 12
    radius: 7
    color: rowHover.hovered ? Theme.hover : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
    HoverHandler { id: rowHover }
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 4
        spacing: 10
        // Status: pulsing while running.
        Rectangle {
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: 6
            implicitWidth: 8
            implicitHeight: 8
            radius: 4
            color: agentRow.statusColor(agentRow.a.status)
            SequentialAnimation on opacity {
                running: agentRow.live
                loops: Animation.Infinite
                NumberAnimation { from: 1; to: 0.3; duration: 700 }
                NumberAnimation { from: 0.3; to: 1; duration: 700 }
                onRunningChanged: if (!running) parent.opacity = 1
            }
        }
        ColumnLayout {
            id: rowCol
            Layout.fillWidth: true
            // (What the buttons leave: they stay in reach.)
            Layout.preferredWidth: 0
            spacing: 2
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Label {
                    Layout.maximumWidth: agentRow.width * 0.45
                    text: agentRow.a.name || agentRow.a.id
                    color: Theme.foreground
                    font.pixelSize: Theme.px(13)
                    font.bold: true
                    elide: Text.ElideRight
                }
                // Who it is (a harlo session: its model too), in plain text.
                Label {
                    id: agentLabel
                    objectName: "subagentWho"
                    text: agentRow.agentName(agentRow.a.agent || "task") + (agentRow.a.model ? " · " + agentRow.modelName(agentRow.a.agent, agentRow.a.model) : "")
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(11.5)
                }
                Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    // (Takes what's left, never more: a long task is cut.)
                    Layout.preferredWidth: 0
                    // (Named after its description: the task instead, on one line.)
                    text: String((agentRow.a.name && agentRow.a.name === agentRow.a.description ? "" : agentRow.a.description) || agentRow.a.task || "").replace(/\s+/g, " ").trim().slice(0, 300)
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                }
                // What it used so far.
                Label {
                    objectName: "subagentUsed"
                    visible: text !== "" && agentRow.width >= 420
                    text: Theme.usedLabel(agentRow.a.used)
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                Label {
                    objectName: "subagentStatus"
                    // (Finished: the faint dot says it. A harlo session
                    // waiting: on its parent, or on you.)
                    visible: !agentRow.live && agentRow.a.status !== "completed"
                    text: agentRow.a.status === "waiting" ? qsTr("needs you") : agentRow.a.status
                    color: agentRow.a.status === "stopped" ? Theme.textMuted : agentRow.statusColor(agentRow.a.status)
                    font.pixelSize: Theme.px(11.5)
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                visible: text !== ""
                maximumLineCount: 1
                text: {
                    if (agentRow.asks) return "Waiting for you: " + String(agentRow.asks.text).replace(/\s+/g, " ")
                    return agentRow.toolLine(agentRow.a)
                }
                objectName: "subagentDoing"
                color: agentRow.asks ? Theme.warning : Theme.textMuted
                font.pixelSize: Theme.px(11.5)
                elide: Text.ElideRight
            }
        }
        FlatButton {
            // Narrow and running: Steer opens the same view, and Cancel
            // must stay in reach.
            // (A harlo session an agent started: opens as its tab.)
            objectName: "subagentView"
            visible: (!!agentRow.a.sessionFile || !!agentRow.a.openKey) && !(agentRow.live && agentRow.width < 380)
            text: "View"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.a.openKey ? Workspace.open(agentRow.a.openKey) : agentRow.viewRequested(agentRow.a, false)
        }
        FlatButton {
            objectName: "subagentDeny"
            visible: agentRow.approvable
            text: "Deny"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: Workspace.answerHelper(agentRow.a.openKey, agentRow.asks.id, "Deny")
        }
        FlatButton {
            objectName: "subagentApprove"
            visible: agentRow.approvable
            text: "Approve"
            variant: "primary"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: Workspace.answerHelper(agentRow.a.openKey, agentRow.asks.id, "Approve")
        }
        FlatButton {
            visible: agentRow.live && agentRow.a.canSteer !== false
            text: "Steer"
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.viewRequested(agentRow.a, true)
        }
        FlatButton {
            objectName: "subagentCancel"
            visible: agentRow.live && agentRow.a.canCancel !== false
            text: "Cancel"
            accent: Theme.error
            font.pixelSize: Theme.px(11)
            padding: 4
            onClicked: agentRow.a.openKey ? Workspace.stopSession(agentRow.a.openKey) : agentRow.session.cancelSubagent(agentRow.a.id)
        }
    }
}
