import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The specs of the active session's repository (SPEC.md files: what each
// part is for, how it behaves, and why), as their tree: each under its
// parent, with its type and status, and what's wrong in their links; or as
// a graph, drawn, with their links. Clicking one (or a problem) opens it in
// a tab.
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

    property var st: ({ root: "", project: -1, enabled: false, specs: [], problems: [], mermaid: "" })
    /// Tree, or the drawn graph.
    property bool graphView: false
    readonly property bool showGraph: graphView && (st.mermaid || "") !== ""
    // The graph's picture, drawn in the panel's colours.
    readonly property var graphPic: (st.mermaid || "") !== "" && Diagrams.revision >= 0
        ? JSON.parse(Diagrams.diagram(st.mermaid, Theme.isLight, JSON.stringify({ background: String(Theme.canvas), surface: String(Theme.raised), text: String(Theme.foreground),
                                                                                    muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
        : null
    /// A problem in plain words ("src/SPEC.md depends on sync, which
    /// isn't a spec"), its names in the code font.
    function problemHtml(t) {
        const plain = String(t).replace(/^(\S+): (depends-on|references|implements) (`[^`]+`) isn't a spec/,
                                        (m, file, how, name) => file + " " + how.replace("-", " ") + " " + name + ", which isn't a spec")
        const esc = plain.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        return esc.replace(/`([^`]+)`/g, (m, x) => "<font face=\"" + Theme.mono + "\" color=\"" + Theme.foreground + "\">" + x + "</font>")
    }
    /// What a spec's status means.
    function statusMeaning(st) {
        return st === "stale" ? "Stale: it may no longer match the code."
             : st === "draft" ? "Draft: not settled yet."
             : st === "deprecated" ? "Deprecated: no longer in force."
             : st === "proposed" ? "Proposed: not built yet."
             : st
    }
    /// The spec a problem is about ("src/SPEC.md: …"), or "".
    function problemFile(t) {
        const m = /^(\S*SPEC\.md):/.exec(String(t))
        return m ? m[1] : ""
    }
    readonly property string fixPrompt: "Fix the problems in this project's specs' links (spec_validate lists them): " + (st.problems || []).join("; ") + ". Point each link at the spec it means, or take it out if there's none."
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

    PictureViewer { id: graphViewer }
    property alias graphViewer: graphViewer

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
                    text: root.st.enabled ? "Agents look up the spec that governs their work and keep it true, in sessions started from now on." : "When on, agents look up the spec that governs their work and keep it true."
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
                ItemDelegate {
                    id: problem
                    objectName: "specsProblem"
                    required property string modelData
                    readonly property string file: root.problemFile(modelData)
                    Layout.fillWidth: true
                    Layout.leftMargin: -6
                    Layout.rightMargin: -6
                    padding: 6
                    topPadding: 3
                    bottomPadding: 3
                    enabled: file !== ""
                    hoverEnabled: true
                    background: Rectangle { radius: 6; color: problem.hovered ? Theme.rowHover : "transparent" }
                    contentItem: Label {
                        text: root.problemHtml(problem.modelData)
                        textFormat: Text.StyledText
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(11.5)
                        wrapMode: Text.Wrap
                    }
                    onClicked: root.openRequested(root.st.root + "/" + file)
                    ToolTip.visible: hovered && file !== ""
                    ToolTip.text: "Open " + file
                    ToolTip.delay: 600
                }
            }
            Label {
                Layout.fillWidth: true
                visible: (root.st.problems || []).length > 4
                text: "and " + ((root.st.problems || []).length - 4) + " more"
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
            }
            FlatButton {
                objectName: "specsFix"
                Layout.topMargin: 3
                visible: root.canSend
                text: "Ask the agent to fix them"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                onClicked: root.sendRequested(root.fixPrompt)
            }
        }

        // Tree or graph.
        SegmentedControl {
            Layout.leftMargin: 18
            Layout.topMargin: 12
            visible: (root.st.mermaid || "") !== "" && root.st.specs.length > 1
            labels: ["Tree", "Graph"]
            current: root.graphView ? 1 : 0
            namePrefix: "specsView"
            onPicked: i => root.graphView = i === 1
        }

        // The graph, drawn: fitted to the panel, full size a click away.
        Flickable {
            id: graphBox
            objectName: "specsGraph"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 10
            visible: root.showGraph
            clip: true
            contentHeight: graphCol.implicitHeight + 16
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ColumnLayout {
                id: graphCol
                x: 18
                width: graphBox.width - 32
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    visible: !!root.graphPic && root.graphPic.state !== "ready"
                    text: !root.graphPic ? "" : root.graphPic.state === "drawing" ? "Drawing the graph…" : (root.graphPic.error || "")
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    wrapMode: Text.Wrap
                }
                Image {
                    id: graphImage
                    objectName: "specsGraphPicture"
                    readonly property bool ready: !!root.graphPic && root.graphPic.state === "ready"
                    readonly property real fit: ready ? Math.min(1, graphCol.width / Math.max(1, root.graphPic.width)) : 1
                    visible: ready
                    Layout.alignment: Qt.AlignHCenter
                    Layout.preferredWidth: ready ? Math.round(root.graphPic.width * fit) : 0
                    Layout.preferredHeight: ready ? Math.round(root.graphPic.height * fit) : 0
                    source: ready ? root.graphPic.url : ""
                    sourceSize.width: Layout.preferredWidth * 2
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    mipmap: true
                    asynchronous: true
                    cache: false
                    Accessible.role: Accessible.Graphic
                    Accessible.name: "The specs' graph"
                    HoverHandler { id: graphHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: graphViewer.show("Specs", [{ label: "Graph", url: root.graphPic.url, width: root.graphPic.width, height: root.graphPic.height }], 0, "Graph",
                                                   () => Diagrams.openInBrowser(root.st.mermaid, Theme.isLight), 2)
                    }
                    ToolTip.visible: graphHover.hovered
                    ToolTip.text: "Open full size"
                    ToolTip.delay: 600
                }
                Label {
                    Layout.fillWidth: true
                    visible: graphImage.visible
                    text: "A line joins a spec to the ones under it; a dashed arrow is a link (depends on, references, implements). The tree opens each one."
                    color: Theme.textFaint
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                }
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 10
            visible: root.st.specs.length > 0 && !root.showGraph
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
                            HoverHandler { id: statusHover }
                            ToolTip.visible: statusHover.hovered
                            ToolTip.text: root.statusMeaning(text)
                            ToolTip.delay: 500
                        }
                        Label {
                            objectName: "specType"
                            visible: text !== ""
                            text: row.modelData.type || ""
                            color: Theme.textFaint
                            font.pixelSize: Theme.px(11)
                            HoverHandler { id: typeHover }
                            ToolTip.visible: typeHover.hovered
                            ToolTip.text: "What kind of spec it is (its type), as it says itself."
                            ToolTip.delay: 500
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
        Item { Layout.fillHeight: true; visible: !list.visible && !graphBox.visible }
    }
}
