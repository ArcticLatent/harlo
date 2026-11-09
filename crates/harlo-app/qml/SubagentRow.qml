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
        anchors.topMargin: 6
        anchors.bottomMargin: 6
        spacing: 10
        // Status: pulsing while running.
        Rectangle {
            objectName: "subagentDot"
            Layout.alignment: Qt.AlignTop
            // (Centred on the name's line: the dot and the text share a top.)
            Layout.topMargin: Math.max(0, Math.round((nameLabel.implicitHeight - 8) / 2))
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
            Layout.alignment: Qt.AlignTop
            // (What the buttons leave: they stay in reach.)
            Layout.preferredWidth: 0
            Layout.minimumWidth: 0
            spacing: 2
            // The name line, placed by hand: its widths come from the
            // line's own width, and nothing here feeds back into it (a
            // layout did, and rearranged itself in a loop).
            Item {
                id: nameLine
                Layout.fillWidth: true
                implicitWidth: 0
                implicitHeight: nameLabel.implicitHeight
                // The status and what it used, at the right.
                readonly property real tailRoom: tail.width > 0 ? tail.width + 6 : 0
                Label {
                    id: nameLabel
                    // (The name tells helpers apart: it keeps its whole
                    // width, the kind and the task give way first; cut only
                    // when the line has no more room.)
                    width: Math.max(0, Math.min(Math.ceil(implicitWidth), Math.floor(nameLine.width - nameLine.tailRoom)))
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
                    // (Shown while the name fits beside it; narrow, shortened.)
                    readonly property real room: nameLine.width - nameLine.tailRoom - nameLabel.implicitWidth - 6
                    visible: room >= Math.min(implicitWidth, Theme.px(40))
                    x: nameLabel.width + 6
                    anchors.baseline: nameLabel.baseline
                    width: visible ? Math.min(implicitWidth, room) : 0
                    text: Theme.helperKind(agentRow.a.agent) + (agentRow.a.model ? " · " + Theme.helperModel(agentRow.a.agent, agentRow.a.model) : "")
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(11.5)
                    elide: Text.ElideRight
                }
                Label {
                    // (Takes what's left, never more: a long task is cut.)
                    x: agentLabel.visible ? agentLabel.x + agentLabel.width + 6 : nameLabel.width + 6
                    anchors.baseline: nameLabel.baseline
                    width: Math.max(0, nameLine.width - nameLine.tailRoom - x)
                    visible: width > 20
                    // (Named after its description: the task instead, on one
                    // line; nothing when that only repeats the name.)
                    readonly property string task: String((agentRow.a.name && agentRow.a.name === agentRow.a.description ? "" : agentRow.a.description) || agentRow.a.task || "").replace(/\s+/g, " ").trim().slice(0, 300)
                    function same(x, y) { return x.toLowerCase().replace(/[\s.…:]+$/, "") === y.toLowerCase().replace(/[\s.…:]+$/, "") }
                    text: task !== "" && agentRow.a.name && (same(task, String(agentRow.a.name).trim()) || String(agentRow.a.name).toLowerCase().startsWith(task.toLowerCase())) ? "" : task
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                }
                Row {
                    id: tail
                    anchors.right: parent.right
                    anchors.verticalCenter: nameLabel.verticalCenter
                    spacing: 6
                    // What it used so far.
                    Label {
                        id: usedLabel
                        objectName: "subagentUsed"
                        visible: text !== "" && agentRow.width >= 420
                        text: Theme.usedLabel(agentRow.a.used)
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                    }
                    Label {
                        id: statusLabel
                        objectName: "subagentStatus"
                        // (Finished: the faint dot says it. A harlo session
                        // waiting: on its parent, or on you.)
                        visible: !agentRow.live && agentRow.a.status !== "completed"
                        text: Theme.helperStatus(agentRow.a.status)
                        color: agentRow.a.status === "stopped" ? Theme.textMuted : agentRow.statusColor(agentRow.a.status)
                        font.pixelSize: Theme.px(11.5)
                    }
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
            id: viewButton
            objectName: "subagentView"
            visible: (!!agentRow.a.sessionFile || !!agentRow.a.openKey) && !(agentRow.live && agentRow.width < 380)
            // (A finished helper's View shows on its row's hover or the
            // keyboard's focus: a column of the same button is noise.
            // It keeps its place, so nothing moves.)
            opacity: agentRow.live || agentRow.asks || rowHover.hovered || activeFocus ? 1 : 0
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
