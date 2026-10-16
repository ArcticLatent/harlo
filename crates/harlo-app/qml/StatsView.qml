import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Usage across all pi/omp sessions: totals, daily activity, token mix,
// efficiency, models and tool calls. Filter by period and project.
Rectangle {
    id: root
    color: Theme.background

    UsageStats { id: stats }
    readonly property var r: stats.json ? JSON.parse(stats.json) : null
    property int days: 30
    property int projectId: -1
    property string modelMetric: "tokens"

    // Test hook.
    function scrollToEnd() { flick.contentY = Math.max(0, flick.contentHeight - flick.height) }
    function refresh() {
        stats.refresh(days, projectId < 0 ? [] : Workspace.projectRoots(projectId))
    }
    onVisibleChanged: if (visible) refresh()
    onDaysChanged: refresh()
    onProjectIdChanged: refresh()
    Component.onCompleted: if (visible) refresh()
    Timer { interval: 60000; repeat: true; running: root.visible; onTriggered: root.refresh() }

    // 228M, 5.4M, 627.1K
    function compact(n) {
        n = Number(n || 0)
        const units = [[1e9, "B"], [1e6, "M"], [1e3, "K"]]
        for (const [v, u] of units)
            if (n >= v) {
                const x = n / v
                return (x >= 100 ? x.toFixed(0) : x.toFixed(1).replace(/\.0$/, "")) + u
            }
        return String(Math.round(n))
    }
    function money(n) { return "$" + Number(n || 0).toFixed(2) }
    function pct(n) { return Number(n || 0).toFixed(1) + "%" }

    readonly property color cInput: Theme.info
    readonly property color cOutput: Theme.success
    readonly property color cCacheRead: "#c9a227"
    readonly property color cCacheWrite: Theme.primary

    component Card: Rectangle {
        property string label
        property string value
        property string detail: ""
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        Layout.fillHeight: true
        implicitHeight: detail !== "" ? 78 : 64
        radius: 8
        color: Theme.elevated
        Column {
            anchors.verticalCenter: parent.verticalCenter
            x: 14
            width: parent.width - 28
            spacing: 4
            Label { text: parent.parent.label; color: Theme.muted; font.pixelSize: Theme.px(12) }
            Label { text: parent.parent.value; color: Theme.foreground; font.pixelSize: Theme.px(20); font.bold: true }
            Label { visible: parent.parent.detail !== ""; text: parent.parent.detail; color: Theme.muted; font.pixelSize: Theme.px(11); elide: Text.ElideRight; width: parent.width }
        }
    }
    component SectionTitle: Label {
        color: Theme.foreground
        font.bold: true
        font.pixelSize: Theme.px(13)
        Layout.topMargin: 10
    }
    component Pill: Button {
        id: pill
        property bool selected: false
        flat: true
        padding: 6
        leftPadding: 10
        rightPadding: 10
        font.pixelSize: Theme.px(12)
        contentItem: Label { text: pill.text; color: pill.selected ? Theme.foreground : Theme.muted; font: pill.font; horizontalAlignment: Text.AlignHCenter }
        background: ControlSurface { control: pill; variant: pill.selected ? "selected" : "outline"; radius: 7 }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: body.implicitHeight + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        ColumnLayout {
            id: body
            readonly property int cols: width > 720 ? 3 : 2
            x: Math.max(16, (flick.width - width) / 2)
            y: 20
            width: Math.min(flick.width - 32, 980)
            spacing: 10

            // Filters
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                // Period and project; wraps on narrow windows.
                Flow {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 6
                    Repeater {
                        model: [[7, "7 days"], [30, "30 days"], [90, "90 days"], [0, "all time"]]
                        Pill {
                            required property var modelData
                            text: modelData[1]
                            selected: root.days === modelData[0]
                            onClicked: root.days = modelData[0]
                        }
                    }
                    Item { width: 4; height: 1 }
                    ComboField {
                        id: projectBox
                        width: 200
                        readonly property var projects: [{ id: -1, name: "All projects" }].concat(
                            Workspace.projectList().map(l => { const p = l.split("\t"); return { id: Number(p[0]), name: p[1] } }))
                        model: projects.map(p => p.name)
                        onActivated: i => root.projectId = projects[i].id
                    }
                }
                BusyIndicator { running: stats.loading; visible: stats.loading; implicitWidth: 18; implicitHeight: 18; Layout.alignment: Qt.AlignTop }
                Label { text: stats.updated ? "Updated " + stats.updated : ""; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.alignment: Qt.AlignTop; Layout.topMargin: 7 }
                FlatButton { text: "↻"; outlined: false; Layout.alignment: Qt.AlignTop; onClicked: root.refresh() }
            }

            Label {
                visible: !root.r
                text: "Reading sessions…"
                color: Theme.muted
                Layout.topMargin: 20
            }

            ColumnLayout {
                visible: !!root.r
                Layout.fillWidth: true
                spacing: 10
                readonly property var r: root.r || ({ tokens: {}, efficiency: {}, daily: [], models: [], tools: [] })

                GridLayout {
                    Layout.fillWidth: true
                    columns: body.cols
                    columnSpacing: 10
                    rowSpacing: 10
                    Card { label: "Sessions"; value: root.compact(parent.parent.r.sessions); detail: "Subagents: " + (parent.parent.r.subagents || 0) }
                    Card { label: "Prompts"; value: root.compact(parent.parent.r.prompts); detail: root.compact(parent.parent.r.steps) + " model steps" }
                    Card {
                        label: "Tokens"
                        value: root.compact(parent.parent.r.tokens.total)
                        detail: "In " + root.compact(parent.parent.r.tokens.input) + " · Out " + root.compact(parent.parent.r.tokens.output) + " · Cache " + root.compact(parent.parent.r.tokens.cacheRead)
                    }
                    Card { label: "Cost"; value: root.money(parent.parent.r.cost) }
                    Card { label: "Active days"; value: String(parent.parent.r.activeDays || 0) }
                    Card { label: "Longest streak (days)"; value: String(parent.parent.r.longestStreak || 0) }
                }

                // Daily activity
                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle { text: "Daily activity" }
                    Item { Layout.fillWidth: true }
                    Label { text: "Model steps per day"; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.topMargin: 10 }
                }
                Rectangle {
                    id: chart
                    Layout.fillWidth: true
                    implicitHeight: 190
                    radius: 8
                    color: Theme.elevated
                    readonly property var daily: parent.r.daily || []
                    readonly property real peak: Math.max(1, ...daily.map(d => d.steps))
                    Row {
                        id: bars
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 14
                        anchors.bottomMargin: 36
                        height: parent.height - 50
                        spacing: chart.daily.length > 60 ? 1 : 3
                        Repeater {
                            model: chart.daily
                            Item {
                                required property var modelData
                                width: Math.max(1, (bars.width - bars.spacing * (chart.daily.length - 1)) / Math.max(1, chart.daily.length))
                                height: bars.height
                                Rectangle {
                                    anchors.bottom: parent.bottom
                                    width: parent.width
                                    height: modelData.steps > 0 ? Math.max(2, parent.height * modelData.steps / chart.peak) : 1
                                    radius: Math.min(2, width / 2)
                                    color: modelData.steps > 0 ? (barHover.hovered ? Qt.lighter(Theme.info, 1.25) : Theme.info) : Theme.border
                                }
                                HoverHandler { id: barHover }
                                ToolTip.visible: barHover.hovered
                                ToolTip.text: modelData.date + ": " + modelData.steps + " steps"
                                ToolTip.delay: 100
                            }
                        }
                    }
                    Label {
                        anchors.left: parent.left
                        anchors.bottom: parent.bottom
                        anchors.margins: 14
                        anchors.bottomMargin: 12
                        text: chart.daily.length ? chart.daily[0].date : ""
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                    }
                    Label {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 14
                        anchors.bottomMargin: 12
                        text: chart.daily.length ? chart.daily[chart.daily.length - 1].date : ""
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                    }
                }

                // Token composition
                SectionTitle { text: "Token composition" }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: compCol.implicitHeight + 24
                    radius: 8
                    color: Theme.elevated
                    readonly property var t: parent.r.tokens
                    readonly property var parts: [
                        { label: "Input", value: t.input || 0, color: root.cInput },
                        { label: "Output + reasoning", value: t.output || 0, color: root.cOutput },
                        { label: "Cache reads", value: t.cacheRead || 0, color: root.cCacheRead },
                        { label: "Cache writes", value: t.cacheWrite || 0, color: root.cCacheWrite },
                    ]
                    ColumnLayout {
                        id: compCol
                        x: 12
                        y: 12
                        width: parent.width - 24
                        spacing: 10
                        Row {
                            id: compBar
                            Layout.fillWidth: true
                            height: 8
                            Repeater {
                                model: compCol.parent.parts
                                Rectangle {
                                    required property var modelData
                                    width: compBar.width * modelData.value / Math.max(1, compCol.parent.t.total || 0)
                                    height: 8
                                    color: modelData.color
                                }
                            }
                        }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: body.width > 720 ? 4 : 2
                            columnSpacing: 16
                            Repeater {
                                model: compCol.parent.parts
                                RowLayout {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    Label { text: "●"; color: modelData.color; font.pixelSize: Theme.px(10) }
                                    Label { text: modelData.label; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.fillWidth: true; elide: Text.ElideRight }
                                    Label {
                                        text: root.compact(modelData.value) + " · " + (100 * modelData.value / Math.max(1, compCol.parent.t.total || 0)).toFixed(1) + "%"
                                        color: Theme.foreground
                                        font.pixelSize: Theme.px(12)
                                    }
                                }
                            }
                        }
                    }
                }

                // Efficiency
                SectionTitle { text: "Efficiency" }
                GridLayout {
                    Layout.fillWidth: true
                    columns: body.cols
                    columnSpacing: 10
                    rowSpacing: 10
                    readonly property var e: parent.r.efficiency
                    Card { label: "Cache hit rate"; value: root.pct(parent.e.cacheHit) }
                    Card { label: "Cost / session"; value: root.money(parent.e.costPerSession) }
                    Card { label: "Cost per 1M tokens"; value: root.money(parent.e.costPerMillion) }
                    Card { label: "Tokens / session"; value: root.compact(parent.e.tokensPerSession) }
                    Card { label: "Steps / session"; value: Number(parent.e.stepsPerSession || 0).toFixed(1) }
                    Card { label: "Reasoning share"; value: root.pct(parent.e.reasoningShare) }
                }

                // Models
                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle { text: "Models" }
                    Item { Layout.fillWidth: true }
                    Repeater {
                        model: ["tokens", "cost", "steps"]
                        Pill {
                            required property string modelData
                            Layout.topMargin: 10
                            text: modelData
                            selected: root.modelMetric === modelData
                            onClicked: root.modelMetric = modelData
                        }
                    }
                }
                Repeater {
                    id: modelRows
                    readonly property var rows: (parent.r.models || []).slice().sort((a, b) => b[root.modelMetric] - a[root.modelMetric])
                    readonly property real peak: Math.max(1e-9, ...rows.map(m => m[root.modelMetric]))
                    model: rows
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 4
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            // Names give way on narrow windows; the numbers stay.
                            Label { text: modelData.model; color: Theme.foreground; font.pixelSize: Theme.px(13); elide: Text.ElideRight; Layout.minimumWidth: 0 }
                            Label { visible: modelData.thinking !== ""; text: modelData.thinking; color: Theme.muted; font.pixelSize: Theme.px(12); elide: Text.ElideRight; Layout.minimumWidth: 0 }
                            Label { text: modelData.provider; color: Theme.muted; font.pixelSize: Theme.px(12); opacity: 0.8; elide: Text.ElideRight; Layout.minimumWidth: 0 }
                            Item { Layout.fillWidth: true; Layout.minimumWidth: 0 }
                            Label {
                                text: root.modelMetric === "steps"
                                    ? modelData.steps + " steps · " + root.compact(modelData.tokens)
                                    : root.compact(modelData.tokens) + " · " + root.money(modelData.cost)
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                            }
                        }
                        Rectangle {
                            id: track
                            Layout.fillWidth: true
                            implicitHeight: 6
                            radius: 3
                            color: Theme.subtle
                            readonly property real frac: modelData[root.modelMetric] / modelRows.peak
                            Rectangle {
                                width: track.width * track.frac
                                height: parent.height
                                radius: 3
                                color: root.modelMetric === "cost" ? Theme.primary : root.modelMetric === "steps" ? Theme.info : root.cCacheRead
                                // Input share of the tokens, like the composition bar.
                                Rectangle {
                                    visible: root.modelMetric === "tokens"
                                    width: parent.width * modelData.input / Math.max(1, modelData.tokens)
                                    height: parent.height
                                    radius: 3
                                    color: root.cInput
                                }
                            }
                        }
                    }
                }

                // Tool calls
                SectionTitle { text: "Tool calls" }
                Label {
                    visible: (parent.r.tools || []).length === 0
                    text: "No tool calls in this period."
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
                Repeater {
                    id: toolRows
                    readonly property var rows: (parent.r.tools || []).slice(0, 15)
                    readonly property real peak: Math.max(1, ...rows.map(t => t.count))
                    model: rows
                    RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 10
                        Label { text: modelData.name; color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(12); Layout.preferredWidth: 240; Layout.minimumWidth: 0; elide: Text.ElideRight }
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 6
                            radius: 3
                            color: Theme.subtle
                            Rectangle { width: parent.width * modelData.count / toolRows.peak; height: parent.height; radius: 3; color: Theme.primary }
                        }
                        Label { text: root.compact(modelData.count); color: Theme.muted; font.pixelSize: Theme.px(12); Layout.preferredWidth: 50; horizontalAlignment: Text.AlignRight }
                    }
                }
            }
        }
    }
}
