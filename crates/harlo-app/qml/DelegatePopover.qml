import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The composer's Delegate menu: hand the task off, get a second opinion,
// or form a committee, by picking agents rather than typing a skill's
// name. harlo writes the instruction (Workspace.delegatePrompt) and sends
// it to this session's agent, which writes the briefing: only it knows
// the conversation.
Popup {
    id: root
    property ChatSession session
    /// The composer's text: the note, unless one's typed here.
    property string draft: ""
    signal draftUsed()
    signal sent()

    property string action: "advisor"
    readonly property var actions: ({
        handoff: { title: "Hand off", icon: "send-plane", button: "Hand off",
                   about: "A new session takes over the task, with a full briefing from this one." },
        advisor: { title: "Second opinion", icon: "chat-thread", button: "Ask",
                   about: "Another agent looks at the problem and advises; it doesn't edit anything. Its answer comes back here." },
        committee: { title: "Committee", icon: "scales-3", button: "Start committee",
                     about: "Two agents (best from different families) each find the root cause, then argue until they agree on a plan. It doesn't edit anything." },
    })
    readonly property var current: actions[action]

    // The agents sessions can start with (and their models, once checked).
    readonly property var providers: {
        Workspace.providersRevision
        return JSON.parse(Workspace.providersJson()).filter(p => p.canChat && p.enabled && p.state !== "missing")
    }
    readonly property var agentIds: providers.map(p => p.id)
    function provider(id) { return providers.find(p => p.id === id) || null }
    // The first agent that isn't `but` (a fresh view), else `but`.
    function other(but) { return agentIds.find(id => id !== but) || but }

    readonly property var orch: { Workspace.providersRevision; return visible ? JSON.parse(Workspace.orchestrationJson()) : null }
    // pi and omp take the tools from their MCP config.
    readonly property var bridge: {
        const a = orch ? orch.agents.find(x => x.id === session.harness) : null
        return a && a.bridge === false ? a : null
    }
    property string bridgeNote: ""

    function show(which) {
        action = which
        const mine = agentIds.indexOf(session.harness) >= 0 ? session.harness : (agentIds[0] || "")
        memberA.pick(which === "advisor" ? other(mine) : mine)
        memberB.pick(other(memberA.agent))
        note.text = draft
        worktree.checked = false
        bridgeNote = ""
        Workspace.ensureProviders()
        open()
    }

    function start() {
        const members = [memberA.chosen()]
        if (action === "committee") members.push(memberB.chosen())
        const text = Workspace.delegatePrompt(JSON.stringify({ action: action, members: members, note: note.text, worktree: worktree.checked }))
        if (text === "") return
        session.send(text, session.status === "busy" ? "followUp" : "normal")
        if (draft !== "" && note.text.indexOf(draft.trim()) >= 0) draftUsed()
        close()
        sent()
    }

    width: Math.min(480, parent ? parent.width - 16 : 480)
    padding: 14
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    background: GlassSurface { radius: 10 }
    onOpened: note.forceActiveFocus()

    // An agent and its model.
    component MemberRow: RowLayout {
        id: row
        property string label: ""
        property string agent: ""
        property string model: ""          // "" its default
        readonly property var info: root.provider(agent)
        readonly property var modelIds: info ? info.modelIds : []
        function pick(id) { agent = id; model = ""; sync() }
        // The boxes show `agent` and `model` (their lists change as the
        // providers are checked, which resets a ComboBox).
        function sync() {
            agentBox.currentIndex = Math.max(0, root.agentIds.indexOf(agent))
            modelBox.currentIndex = model === "" ? 0 : modelIds.indexOf(model) + 1
        }
        function chosen() { return { agent: agent, model: model } }
        Layout.fillWidth: true
        spacing: 8
        Label { visible: row.label !== ""; text: row.label; color: Theme.muted; font.pixelSize: Theme.px(12) }
        ComboField {
            id: agentBox
            objectName: "delegateAgent" + row.label.replace(/\s/g, "")
            // (The two share the room equally.)
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            model: root.providers.map(p => p.label)
            marks: root.agentIds
            onActivated: i => row.pick(root.agentIds[i])
            onModelChanged: row.sync()
        }
        ComboField {
            id: modelBox
            objectName: "delegateModel" + row.label.replace(/\s/g, "")
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            model: [row.info && row.info.state === "checking" ? "Default model (finding models…)" : "Default model"].concat(row.info ? row.info.models : [])
            onActivated: i => row.model = i > 0 ? row.modelIds[i - 1] || "" : ""
            onModelChanged: row.sync()
        }
    }

    contentItem: ColumnLayout {
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: root.current.icon; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label { text: root.current.title; color: Theme.foreground; font.pixelSize: Theme.px(14); font.bold: true }
        }
        Label {
            Layout.fillWidth: true
            text: root.current.about
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        MemberRow { id: memberA; label: root.action === "committee" ? "Member 1" : root.action === "handoff" ? "To" : "Ask" }
        MemberRow { id: memberB; label: "Member 2"; visible: root.action === "committee" }
        Label {
            visible: root.action === "committee" && memberA.agent === memberB.agent && root.agentIds.length > 1
            text: "Two different agents give a more independent view."
            color: Theme.warning
            font.pixelSize: Theme.px(11)
        }
        TextArea {
            id: note
            objectName: "delegateNote"
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(Math.max(implicitHeight, 60), 140)
            placeholderText: root.action === "handoff" ? "Anything to add for the new agent (optional)"
                           : root.action === "advisor" ? "The question (optional: the current approach)"
                           : "The problem (optional: what we're stuck on)"
            placeholderTextColor: Theme.muted
            color: Theme.foreground
            wrapMode: TextEdit.Wrap
            font.pixelSize: Theme.px(12.5)
            background: Rectangle { radius: 6; color: Theme.background; border.color: note.activeFocus ? Theme.primary : Theme.border }
            Keys.onPressed: event => {
                if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
                    root.start()
                    event.accepted = true
                }
            }
        }
        AppCheckBox {
            id: worktree
            objectName: "delegateWorktree"
            visible: root.action === "handoff"
            text: "In a new git worktree (its own branch, apart from this checkout)"
        }
        // pi / omp without harlo's tools.
        RowLayout {
            visible: root.bridge !== null
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: root.bridgeNote !== "" ? root.bridgeNote : (root.bridge ? root.bridge.name : "") + " doesn't have harlo's tools yet."
                color: Theme.warning
                font.pixelSize: Theme.px(12)
                wrapMode: Text.Wrap
            }
            FlatButton {
                objectName: "delegateAddBridge"
                text: "Add them"
                font.pixelSize: Theme.px(12)
                onClicked: {
                    const name = root.bridge.name
                    const err = Workspace.setOrchestrationBridge(root.bridge.id, true)
                    root.bridgeNote = err !== "" ? err : "Added: new " + name + " sessions have them."
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: "Ctrl+Enter"
                color: Theme.muted
                font.pixelSize: Theme.px(11)
            }
            DialogButton { text: "Cancel"; onClicked: root.close() }
            DialogButton {
                objectName: "delegateStart"
                text: root.current.button
                accent: true
                enabled: memberA.agent !== "" && (root.action !== "committee" || memberB.agent !== "")
                onClicked: root.start()
            }
        }
    }
}
