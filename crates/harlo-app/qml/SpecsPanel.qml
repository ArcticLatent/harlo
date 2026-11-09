import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The specs of the active session's repository (SPEC.md files: what each
// part is for, how it behaves, and why), as their tree: each under its
// parent, with its type and status, and what's wrong in their links.
// Clicking one opens it in a tab.
// With specs on for the project, its agents read and keep them (harlo's
// spec tools); the switch is here and in the project's settings.
Rectangle {
    id: root
    /// The active session's key.
    property string sessionKey: ""
    /// The active session can take a message.
    property bool canSend: false
    /// The new-session page is shown (its message box takes the request).
    property bool drafting: false
    signal closeRequested()
    signal openRequested(string path)
    /// A message for the agent (writing the first specs).
    signal sendRequested(string text)
    /// The same, starting the session on the new-session page.
    signal draftRequested(string text)
    color: Theme.canvas
    clip: true

    property var st: ({ root: "", project: -1, enabled: false, specs: [], problems: [] })
    function load() { st = JSON.parse(Workspace.specsJson(sessionKey)) }
    onSessionKeyChanged: if (visible) load()
    onVisibleChanged: if (visible) load()
    Component.onCompleted: if (visible) load()
    // (Another machine's: read there, then shown.)
    Connections {
        target: Workspace
        function onSpecsReady() { if (root.visible) root.load() }
    }

    readonly property string writePrompt: "Write this project's specs: SPEC.md files that hold its intent between sessions. Start with the root SPEC.md (the product: its goal, who it's for, its main parts and the decisions behind them), then one SPEC.md in the folder of each main part (what it's for, how it behaves, its boundaries, and why it's built that way). Read the code to learn them, keep each short and about intent rather than a copy of the code, link a part to the parts it relies on (depends-on in its frontmatter), and tell me what you couldn't tell from the code."

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Specs"
            subtitle: root.st.root.replace(/\/+$/, "").split("/").pop()
            onCloseRequested: root.closeRequested()
            PanelButton { objectName: "specsRefresh"; iconName: "refresh"; tip: "Refresh"; onClicked: root.load() }
        }

        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            visible: root.st.project < 0
            wrapMode: Text.Wrap
            text: root.sessionKey === "" ? "No session selected." : "Specs are for sessions in a project."
            color: Theme.muted
        }

        // The project's switch.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.topMargin: 12
            visible: root.st.project >= 0
            spacing: 10
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label {
                    Layout.fillWidth: true
                    text: "Spec tools"
                    color: Theme.text
                    font.pixelSize: Theme.px(13)
                    font.weight: Font.Medium
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: root.st.enabled ? "Agents look up the spec that governs their work and keep it true (new sessions)." : "When on, agents look up the spec that governs their work and keep it true."
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
            }
            AppSwitch {
                Layout.alignment: Qt.AlignTop
                objectName: "specsSwitch"
                checked: root.st.enabled
                onToggled: { Workspace.setSetting("specs_" + root.st.project, checked ? "1" : "0"); root.load() }
            }
        }

        // What doesn't hold in their links (spec_validate's).
        ColumnLayout {
            objectName: "specsProblems"
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.topMargin: 12
            visible: (root.st.problems || []).length > 0
            spacing: 3
            Label {
                Layout.fillWidth: true
                text: (root.st.problems || []).length === 1 ? "1 problem in the specs' links" : (root.st.problems || []).length + " problems in the specs' links"
                color: Theme.warning
                font.pixelSize: Theme.px(12)
                font.weight: Font.Medium
            }
            Repeater {
                model: (root.st.problems || []).slice(0, 4)
                Label {
                    required property string modelData
                    Layout.fillWidth: true
                    text: modelData
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                }
            }
            Label {
                Layout.fillWidth: true
                visible: (root.st.problems || []).length > 4
                text: "and " + ((root.st.problems || []).length - 4) + " more"
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 10
            visible: root.st.specs.length > 0
            clip: true
            model: root.st.specs
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: ItemDelegate {
                id: row
                required property var modelData
                readonly property string full: root.st.root + "/" + modelData.path
                readonly property bool open: Workspace.activeKey === "file:" + full
                width: ListView.view.width
                leftInset: 8
                rightInset: 8
                // (Under its parent.)
                leftPadding: 18 + Math.min(row.modelData.depth || 0, 6) * 14
                rightPadding: 14
                topPadding: 7
                bottomPadding: 7
                hoverEnabled: true
                background: Rectangle { radius: 8; color: row.open ? Theme.rowSelected : row.hovered ? Theme.rowHover : "transparent" }
                contentItem: ColumnLayout {
                    spacing: 2
                    RowLayout {
                        spacing: 6
                        Icon { name: "file-text"; color: Theme.textMuted; implicitWidth: 13; implicitHeight: 13 }
                        Label { Layout.fillWidth: true; text: row.modelData.title; color: Theme.text; font.pixelSize: Theme.px(13); font.weight: Font.Medium; elide: Text.ElideRight }
                        // Its status, when it isn't simply in force.
                        Label {
                            objectName: "specStatus"
                            visible: text !== "" && text !== "active"
                            text: row.modelData.status || ""
                            color: text === "stale" ? Theme.warning : Theme.textFaint
                            font.pixelSize: Theme.px(11)
                            font.strikeout: text === "deprecated"
                        }
                        Label {
                            objectName: "specType"
                            visible: text !== ""
                            text: row.modelData.type || ""
                            color: Theme.textFaint
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        text: row.modelData.path
                        color: Theme.textFaint
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(11)
                        elide: Text.ElideMiddle
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: row.modelData.summary
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                }
                onClicked: root.openRequested(full)
            }
        }

        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            visible: root.st.project >= 0 && !!root.st.loading
            wrapMode: Text.Wrap
            text: "Reading the specs on " + root.st.host + "…"
            color: Theme.muted
            font.pixelSize: Theme.px(12.5)
        }

        // None yet.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 18
            visible: root.st.project >= 0 && root.st.specs.length === 0 && !root.st.loading
            spacing: 10
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "No specs yet. A spec is a SPEC.md in a folder saying what that part is for, how it behaves and why; the root one does that for the whole product. They keep the project's intent between sessions."
                color: Theme.muted
                font.pixelSize: Theme.px(12.5)
            }
            FlatButton {
                objectName: "specsWrite"
                visible: root.st.enabled
                enabled: root.canSend || root.drafting
                text: "Ask the agent to write them"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                onClicked: root.drafting ? root.draftRequested(root.writePrompt) : root.sendRequested(root.writePrompt)
            }
            Label {
                Layout.fillWidth: true
                visible: root.st.enabled && !root.drafting && !root.canSend
                wrapMode: Text.Wrap
                text: "Start or open a session in this project to ask its agent."
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
            }
        }
        Item { Layout.fillHeight: true; visible: !list.visible }
    }
}
