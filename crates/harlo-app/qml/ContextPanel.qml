import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Token usage and message stats of the active session, from its messages
// (and the harness's own figures while it runs).
Rectangle {
    id: root
    required property ChatSession session
    signal closeRequested()
    color: Theme.canvas
    clip: true

    readonly property var report: session.contextJson ? JSON.parse(session.contextJson) : null
    function fmt(n) { return n === undefined || n === null ? "–" : Number(n).toLocaleString(Qt.locale("en_US"), "f", 0) }
    function when(ms) { return ms ? new Date(ms).toLocaleString(Qt.locale(), "M/d, h:mm AP") : "" }

    // The messages list shows this many, then "Show N more" (as the sidebar's
    // sessions): a long session would otherwise be a very long list.
    readonly property int messagesPage: 20
    property int messagesShown: messagesPage

    function reload() { if (visible && session.key !== "") session.loadContext() }
    onVisibleChanged: reload()
    Connections {
        target: root.session
        function onKeyChanged() { root.messagesShown = root.messagesPage; root.reload() }
        function onStatusChanged() { if (root.session.status !== "busy") root.reload() }
    }
    // While a run streams, refresh now and then.
    Timer { interval: 3000; repeat: true; running: root.visible && root.session.status === "busy"; onTriggered: root.reload() }

    // A card's label: small, in sentence case, quieter than its value.
    component Caption: Label {
        color: Theme.label
        font.pixelSize: Theme.px(12)
        font.weight: Font.Medium
    }
    // A section's label: as a card's, with room above.
    component SectionLabel: Caption {
        Layout.topMargin: 10
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Context"
            subtitle: root.session.title || ""
            onCloseRequested: root.closeRequested()
            PanelButton { iconName: "refresh"; tip: "Refresh"; onClicked: root.reload() }
        }

        Label {
            Layout.margins: 18
            visible: !root.report
            text: root.session.key === "" ? "No session selected." : "Loading…"
            color: Theme.muted
        }

        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !!root.report
            contentHeight: body.implicitHeight + 20
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }

            ColumnLayout {
                id: body
                x: 18
                y: 14
                width: parent.width - 36
                spacing: 10
                readonly property var r: root.report || ({})

                // The model: its name, what serves it (an engine on this
                // computer or the network, else its provider), when it started.
                Rectangle {
                    id: modelCard
                    visible: !!body.r.model
                    readonly property string fullId: root.session.modelId || (body.r.provider ? body.r.provider + "/" + (body.r.model || "") : (body.r.model || ""))
                    readonly property var server: Theme.serverOf(root.session.harness, modelCard.fullId)
                    readonly property string servedBy: !server ? (body.r.provider || "")
                        : [server.engine || server.name,
                           server.place === "here" ? "on this computer (" + server.address + ")" : server.place === "network" ? "on your network (" + server.address + ")" : server.address].filter(s => s).join(" · ")
                    Layout.fillWidth: true
                    implicitHeight: modelCol.implicitHeight + 28
                    radius: 12
                    color: Theme.raised
                    border.color: Theme.hairline
                    ColumnLayout {
                        id: modelCol
                        x: 14
                        y: 14
                        width: parent.width - 28
                        spacing: 4
                        Caption { text: "Model" }
                        Label {
                            Layout.fillWidth: true
                            Layout.topMargin: 2
                            // (As the composer's picker names it: "Sonnet 5".)
                            text: Theme.helperModel(root.session.harness, body.r.model || "") || (body.r.model || "")
                            color: Theme.text
                            font.pixelSize: Theme.px(15)
                            font.weight: Font.Medium
                            elide: Text.ElideRight
                        }
                        // What serves it, with its logo: the engine's on your own
                        // server, else the provider's.
                        RowLayout {
                            Layout.fillWidth: true
                            visible: modelCard.servedBy !== ""
                            spacing: 6
                            ProviderLogo {
                                provider: modelCard.server ? (modelCard.server.engine || modelCard.server.name).toLowerCase() : (body.r.provider || "")
                                color: Theme.textMuted
                                implicitWidth: 14
                                implicitHeight: 14
                            }
                            Label {
                                Layout.fillWidth: true
                                text: modelCard.servedBy
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(12.5)
                                elide: Text.ElideRight
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: text !== ""
                            text: body.r.started ? "Started " + root.when(body.r.started) : ""
                            color: Theme.textFaint
                            font.pixelSize: Theme.px(12)
                        }
                    }
                }

                // Context window
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: ctxCol.implicitHeight + 28
                    radius: 12
                    color: Theme.raised
                    border.color: Theme.hairline
                    ColumnLayout {
                        id: ctxCol
                        x: 14
                        y: 14
                        width: parent.width - 28
                        spacing: 10
                        RowLayout {
                            Layout.fillWidth: true
                            Caption { text: "Context window"; Layout.fillWidth: true }
                            Label {
                                text: root.fmt(body.r.contextTokens) + (body.r.contextWindow > 0 ? " / " + root.fmt(body.r.contextWindow) : " tokens")
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(12.5)
                            }
                        }
                        Rectangle {
                            Layout.fillWidth: true
                            visible: body.r.contextWindow > 0
                            implicitHeight: 6
                            radius: 3
                            color: Theme.controlFill
                            Rectangle {
                                readonly property real f: body.r.contextWindow > 0 ? Math.min(1, body.r.contextTokens / body.r.contextWindow) : 0
                                width: parent.width * f
                                height: parent.height
                                radius: 3
                                color: f > 0.85 ? Theme.error : f > 0.6 ? Theme.warning : Theme.primary
                            }
                        }
                        Label {
                            text: body.r.contextWindow > 0
                                ? (100 * body.r.contextTokens / body.r.contextWindow).toFixed(1) + "% used"
                                : "Window size is known once the agent runs."
                            color: body.r.contextWindow > 0 ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.px(body.r.contextWindow > 0 ? 14 : 12.5)
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                        }
                    }
                }

                // What the session holds, in one line that adds up: yours,
                // the replies and the tools' results make the total.
                Label {
                    objectName: "contextCounts"
                    Layout.fillWidth: true
                    Layout.leftMargin: 2
                    Layout.topMargin: 2
                    wrapMode: Text.Wrap
                    textFormat: Text.StyledText
                    readonly property int results: body.r.toolResults || 0
                    text: "<b>" + root.fmt(body.r.messages) + "</b> " + (body.r.messages === 1 ? "message" : "messages") + ": "
                        + root.fmt(body.r.user) + " yours, " + root.fmt(body.r.assistant) + (body.r.assistant === 1 ? " reply" : " replies")
                        + (results > 0 ? ", " + root.fmt(results) + (results === 1 ? " tool result" : " tool results") : "")
                        + " · " + "$" + Number(body.r.cost || 0).toFixed(2)
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(13)
                }

                // Last assistant message
                Rectangle {
                    Layout.fillWidth: true
                    visible: !!body.r.last
                    implicitHeight: lastCol.implicitHeight + 28
                    radius: 12
                    color: Theme.raised
                    border.color: Theme.hairline
                    ColumnLayout {
                        id: lastCol
                        x: 14
                        y: 14
                        width: parent.width - 28
                        spacing: 10
                        Caption { text: "Last reply" }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: 3
                            rowSpacing: 8
                            Repeater {
                                model: body.r.last ? [
                                    ["Input", root.fmt(body.r.last.input)],
                                    ["Output", root.fmt(body.r.last.output)],
                                    ["Reasoning", root.fmt(body.r.last.reasoning)],
                                    ["Cache read", root.fmt(body.r.last.cacheRead)],
                                    ["Cache write", root.fmt(body.r.last.cacheWrite)],
                                    ["Cache hit", body.r.last.cacheHit === null ? "–" : body.r.last.cacheHit.toFixed(1) + "%"],
                                ] : []
                                Column {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Label { text: modelData[0]; color: Theme.textMuted; font.pixelSize: Theme.px(11.5) }
                                    Label { text: modelData[1]; color: Theme.text; font.pixelSize: Theme.px(14); font.weight: Font.Medium }
                                }
                            }
                        }
                    }
                }

                // Where the text went (by size)
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    readonly property var b: body.r.breakdown || ({ user: 0, assistant: 0, tools: 0, other: 0 })
                    readonly property var parts: [
                        { label: "User", pct: b.user, color: Theme.success },
                        { label: "Assistant", pct: b.assistant, color: "#b48ead" },
                        { label: "Tool calls", pct: b.tools, color: Theme.primary },
                        { label: "Other", pct: b.other, color: Theme.muted },
                    ]
                    SectionLabel { text: "Where the text went" }
                    // One bar, rounded, in parts.
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: 6
                        radius: 3
                        color: Theme.controlFill
                        clip: true
                        Row {
                            id: bar
                            anchors.fill: parent
                            Repeater {
                                model: parent.parent.parent.parts
                                Rectangle { required property var modelData; width: bar.width * modelData.pct / 100; height: 6; color: modelData.color }
                            }
                        }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 14
                        Repeater {
                            model: parent.parent.parts
                            Row {
                                required property var modelData
                                spacing: 6
                                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 7; height: 7; radius: 3.5; color: modelData.color }
                                Label { text: modelData.label + " " + Math.round(modelData.pct) + "%"; color: Theme.textMuted; font.pixelSize: Theme.px(12) }
                            }
                        }
                    }
                }

                SectionLabel { text: "Messages" }
                // A plain list: a hairline between rows.
                Repeater {
                    // The newest 300 are plenty here.
                    model: (body.r.rows || []).slice(0, Math.min(300, root.messagesShown))
                    Item {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        implicitHeight: 34
                        Rectangle { visible: index > 0; width: parent.width; height: 1; color: Theme.hairline }
                        RowLayout {
                            anchors.fill: parent
                            spacing: 8
                            Label {
                                Layout.fillWidth: true
                                textFormat: Text.StyledText
                                text: modelData.kind === "user" ? "<b>You:</b> " + modelData.label.replace(/&/g, "&amp;").replace(/</g, "&lt;") : modelData.label
                                color: modelData.kind === "idle" ? Theme.textMuted : modelData.error ? Theme.error : Theme.text
                                elide: Text.ElideRight
                                font.pixelSize: Theme.px(13)
                            }
                            Label {
                                visible: modelData.input !== undefined && modelData.input !== null
                                text: root.fmt(modelData.input) + " / " + root.fmt(modelData.output)
                                color: Theme.textFaint
                                font.pixelSize: Theme.px(12)
                            }
                            Label { text: root.when(modelData.ts); color: Theme.textFaint; font.pixelSize: Theme.px(12) }
                        }
                    }
                }
                ItemDelegate {
                    id: moreRow
                    objectName: "contextShowMore"
                    readonly property int remaining: Math.min(300, (body.r.rows || []).length) - root.messagesShown
                    visible: remaining > 0
                    Layout.fillWidth: true
                    implicitHeight: 34
                    leftPadding: 10
                    leftInset: 0
                    rightInset: 0
                    contentItem: Label {
                        text: "Show " + Math.min(50, moreRow.remaining) + " more"
                        color: moreRow.hovered ? Theme.text : Theme.textMuted
                        font.pixelSize: Theme.px(13)
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle {
                        radius: 8
                        color: moreRow.hovered ? Theme.rowHover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                        Rectangle { width: parent.width; height: 1; color: Theme.hairline }
                    }
                    onClicked: root.messagesShown += 50
                }
            }
        }
    }
}
