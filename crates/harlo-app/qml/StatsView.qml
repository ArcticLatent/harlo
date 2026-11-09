import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Usage across all Harlo and Claude sessions: totals, daily activity, token mix,
// efficiency, models and tool calls. Filter by period and project.
Rectangle {
    id: root
    color: Theme.background

    UsageStats { id: stats }
    readonly property var r: stats.json ? JSON.parse(stats.json) : null
    property int days: 30
    property int projectId: -1
    property string modelMetric: "tokens"
    // Shown above the filters, in the same scroll (Settings → Usage puts
    // its title and limits there).
    property Component header: null
    // Where the column starts and how wide it gets (-1: centred).
    property real columnX: -1
    property real columnWidth: 980

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

    // The four kinds of token, here and in each model's bar. Cache writes
    // are a paler cache read: the two are one cache.
    readonly property color cInput: Theme.info
    readonly property color cOutput: Theme.success
    readonly property color cCacheRead: Theme.isLight ? "#b8901a" : "#c9a227"
    readonly property color cCacheWrite: Qt.rgba(cCacheRead.r, cCacheRead.g, cCacheRead.b, 0.45)
    readonly property var tokenKinds: [
        { key: "input", label: "Input", color: cInput },
        { key: "output", label: "Output (with reasoning)", color: cOutput },
        { key: "cacheRead", label: "Cache reads", color: cCacheRead },
        { key: "cacheWrite", label: "Cache writes", color: cCacheWrite },
    ]
    // "2026-11-09" as "Nov 9" (with the year when the period spans years).
    function day(iso, withYear) {
        const p = String(iso || "").split("-").map(Number)
        if (p.length < 3) return ""
        return Qt.formatDate(new Date(p[0], p[1] - 1, p[2]), withYear ? "MMM d, yyyy" : "MMM d")
    }
    function plural(n, one, many) { return n + " " + (n === 1 ? one : many) }
    function modelName(m) { return m === "unknown" || m === "" ? "Model not recorded" : Theme.modelLabel(m) }

    component SectionTitle: Label {
        color: Theme.foreground
        font.weight: Font.DemiBold
        font.pixelSize: Theme.px(13)
        Layout.topMargin: 14
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
    // A legend's colour.
    component Swatch: Rectangle {
        implicitWidth: 8
        implicitHeight: 8
        radius: 4
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
            objectName: "statsBody"
            readonly property int cols: width > 720 ? 3 : 2
            x: root.columnX >= 0 ? root.columnX : Math.max(16, Math.round((flick.width - width) / 2))
            y: root.header ? 26 : 20
            width: root.columnX >= 0 ? Math.min(flick.width - root.columnX - 30, root.columnWidth) : Math.min(flick.width - 32, root.columnWidth)
            spacing: 10

            Loader { active: !!root.header; visible: active; sourceComponent: root.header; Layout.fillWidth: true }

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
                        model: [[7, "7 days"], [30, "30 days"], [90, "90 days"], [0, "All time"]]
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
                        width: Math.min(200, body.width)
                        readonly property var projects: [{ id: -1, name: "All projects" }].concat(
                            Workspace.projectList().map(l => { const p = l.split("\t"); return { id: Number(p[0]), name: p[1] } }))
                        model: projects.map(p => p.name)
                        onActivated: i => root.projectId = projects[i].id
                    }
                }
                BusyIndicator { running: stats.loading; visible: stats.loading; implicitWidth: 18; implicitHeight: 18; Layout.alignment: Qt.AlignTop }
                // (Narrow: the time goes to the button's tooltip, so the
                // periods stay on one line.)
                Label { visible: body.width >= 560; text: stats.updated ? "Updated " + stats.updated : ""; color: Theme.muted; font.pixelSize: Theme.px(12); Layout.alignment: Qt.AlignTop; Layout.topMargin: 7 }
                Button {
                    id: refreshButton
                    objectName: "statsRefresh"
                    flat: true
                    padding: 0
                    implicitWidth: 28
                    implicitHeight: 28
                    Layout.alignment: Qt.AlignTop
                    Accessible.name: "Refresh"
                    onClicked: root.refresh()
                    contentItem: Item { Icon { anchors.centerIn: parent; name: "refresh"; color: Theme.textMuted; implicitWidth: 15; implicitHeight: 15 } }
                    background: Rectangle {
                        radius: 6
                        color: refreshButton.down ? Theme.pressed : refreshButton.hovered ? Theme.hover : "transparent"
                        border.color: refreshButton.visualFocus ? Theme.accent : "transparent"
                    }
                    ToolTip.visible: hovered || visualFocus
                    ToolTip.delay: 400
                    ToolTip.text: stats.updated ? "Refresh (updated " + stats.updated + ")" : "Refresh"
                }
            }

            Label {
                visible: !root.r
                text: "Reading sessions…"
                color: Theme.muted
                Layout.topMargin: 20
            }

            // (Nothing yet: one line, not a page of zeros.)
            Label {
                objectName: "statsEmpty"
                visible: !!root.r && !(root.r.sessions > 0)
                text: root.days > 0 ? "No sessions in the last " + root.days + " days" + (root.projectId >= 0 ? " in this project." : ".") : "No sessions yet."
                color: Theme.textMuted
                font.pixelSize: Theme.px(13)
                Layout.topMargin: 14
            }
            ColumnLayout {
                id: content
                objectName: "statsInner"
                visible: !!root.r && root.r.sessions > 0
                Layout.fillWidth: true
                spacing: 10
                readonly property var r: root.r || ({ tokens: {}, efficiency: {}, daily: [], models: [], tools: [] })
                readonly property bool hasCost: (r.cost || 0) > 0
                readonly property string period: (root.days > 0 ? "in the last " + root.days + " days" : "so far") + (root.projectId >= 0 ? " in this project" : "")

                // The lead: what it cost (or, without prices, how many
                // tokens), over how much work; then the other figures.
                ColumnLayout {
                    objectName: "statsLead"
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 8
                    // The figure, and beside it (below when it doesn't fit)
                    // what it covers.
                    Item {
                        id: lead
                        Layout.fillWidth: true
                        readonly property bool beside: leadValue.implicitWidth + 10 + leadText.implicitWidth <= width
                        implicitHeight: beside ? leadValue.implicitHeight : leadValue.implicitHeight + 2 + leadText.implicitHeight
                        Label {
                            id: leadValue
                            objectName: "statsLeadValue"
                            text: content.hasCost ? root.money(content.r.cost) : root.compact(content.r.tokens.total) + " tokens"
                            color: Theme.text
                            font.pixelSize: Theme.px(28)
                            font.weight: Font.DemiBold
                            font.letterSpacing: -0.3
                        }
                        Label {
                            id: leadText
                            objectName: "statsLeadText"
                            x: lead.beside ? leadValue.implicitWidth + 10 : 0
                            // On the figure's baseline.
                            y: lead.beside ? leadValue.baselineOffset - baselineOffset : leadValue.implicitHeight + 2
                            width: lead.beside ? implicitWidth : lead.width
                            wrapMode: Text.Wrap
                            text: "over " + root.plural(content.r.sessions || 0, "session", "sessions") + " and " + root.plural(content.r.prompts || 0, "prompt", "prompts") + " " + content.period
                            color: Theme.textMuted
                            font.pixelSize: Theme.px(14)
                        }
                    }
                    // The other figures, wrapping between them.
                    Label {
                        objectName: "statsFigures"
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.StyledText
                        lineHeight: 1.25
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(13)
                        readonly property var figures: [
                            content.hasCost ? [root.compact(content.r.tokens.total), "tokens"] : null,
                            [root.compact(content.r.steps), content.r.steps === 1 ? "model step" : "model steps"],
                            [String(content.r.activeDays || 0), content.r.activeDays === 1 ? "day active" : "days active"],
                            (content.r.longestStreak || 0) > 1 ? [String(content.r.longestStreak), "days in a row at most"] : null,
                            (content.r.subagents || 0) > 0 ? [String(content.r.subagents), content.r.subagents === 1 ? "helper session" : "helper sessions"] : null,
                        ].filter(f => f)
                        text: figures.map(f => "<b><font color='" + Theme.rgb(Theme.text) + "'>" + f[0] + "</font></b>&nbsp;" + f[1].replace(/ /g, "&nbsp;")).join("&nbsp;&nbsp;&nbsp;&nbsp; ")
                    }
                }

                // Daily activity
                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle { text: "Daily activity" }
                    Item { Layout.fillWidth: true }
                    Label { text: "Model steps per day"; color: Theme.textMuted; font.pixelSize: Theme.px(12); Layout.topMargin: 14 }
                }
                Rectangle {
                    id: chart
                    objectName: "statsChart"
                    Layout.fillWidth: true
                    implicitHeight: 190
                    radius: 8
                    color: Theme.elevated
                    readonly property var daily: content.r.daily || []
                    readonly property real peak: Math.max(1, ...daily.map(d => d.steps))
                    readonly property bool years: daily.length > 0 && daily[0].date.slice(0, 4) !== daily[daily.length - 1].date.slice(0, 4)
                    // The busiest day's line, with its count.
                    Rectangle {
                        x: bars.x
                        y: bars.y
                        width: bars.width
                        height: 1
                        color: Theme.hairline
                    }
                    Label {
                        objectName: "statsPeak"
                        x: bars.x
                        y: bars.y - height - 2
                        text: root.plural(chart.peak, "step", "steps")
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(10.5)
                    }
                    Rectangle {
                        x: bars.x
                        y: bars.y + bars.height
                        width: bars.width
                        height: 1
                        color: Theme.hairline
                    }
                    Row {
                        id: bars
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 14
                        anchors.bottomMargin: 34
                        height: parent.height - 66
                        spacing: chart.daily.length > 60 ? 1 : 3
                        Repeater {
                            model: chart.daily
                            Item {
                                required property var modelData
                                width: Math.max(1, (bars.width - bars.spacing * (chart.daily.length - 1)) / Math.max(1, chart.daily.length))
                                height: bars.height
                                Rectangle {
                                    visible: modelData.steps > 0
                                    anchors.bottom: parent.bottom
                                    width: parent.width
                                    height: Math.max(2, parent.height * modelData.steps / chart.peak)
                                    radius: Math.min(2, width / 2)
                                    color: barHover.hovered ? Qt.lighter(Theme.info, 1.25) : Theme.info
                                }
                                HoverHandler { id: barHover }
                                ToolTip.visible: barHover.hovered
                                ToolTip.text: root.day(modelData.date, true) + ": " + (modelData.steps > 0 ? root.plural(modelData.steps, "model step", "model steps") : "nothing")
                                ToolTip.delay: 100
                            }
                        }
                    }
                    // First, middle and last day.
                    Repeater {
                        model: chart.daily.length > 2 ? [0, Math.floor((chart.daily.length - 1) / 2), chart.daily.length - 1] : chart.daily.map((d, i) => i)
                        Label {
                            required property int modelData
                            required property int index
                            readonly property real slot: bars.width / Math.max(1, chart.daily.length)
                            x: Math.max(bars.x, Math.min(bars.x + bars.width - width, bars.x + slot * (modelData + 0.5) - width / 2))
                            y: bars.y + bars.height + 8
                            text: root.day(chart.daily[modelData].date, chart.years)
                            color: Theme.textMuted
                            font.pixelSize: Theme.px(11)
                        }
                    }
                }

                // Token composition
                SectionTitle { text: "Token composition" }
                Rectangle {
                    objectName: "statsComposition"
                    Layout.fillWidth: true
                    implicitHeight: compCol.implicitHeight + 28
                    radius: 8
                    color: Theme.elevated
                    readonly property var t: content.r.tokens
                    ColumnLayout {
                        id: compCol
                        x: 14
                        y: 14
                        width: parent.width - 28
                        spacing: 12
                        Row {
                            id: compBar
                            Layout.fillWidth: true
                            height: 8
                            spacing: 0
                            Repeater {
                                model: root.tokenKinds
                                Rectangle {
                                    required property var modelData
                                    width: compBar.width * (compCol.parent.t[modelData.key] || 0) / Math.max(1, compCol.parent.t.total || 0)
                                    height: 8
                                    color: modelData.color
                                }
                            }
                        }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: body.width > 900 ? 4 : body.width > 400 ? 2 : 1
                            columnSpacing: 20
                            rowSpacing: 6
                            Repeater {
                                model: root.tokenKinds
                                RowLayout {
                                    required property var modelData
                                    readonly property real v: compCol.parent.t[modelData.key] || 0
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 1
                                    Layout.minimumWidth: 0
                                    spacing: 7
                                    Swatch { color: modelData.color }
                                    Label { text: modelData.label; color: Theme.textMuted; font.pixelSize: Theme.px(12); Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight }
                                    Label {
                                        text: root.compact(v) + " · " + (100 * v / Math.max(1, compCol.parent.t.total || 0)).toFixed(1) + "%"
                                        color: Theme.text
                                        font.pixelSize: Theme.px(12)
                                    }
                                }
                            }
                        }
                    }
                }

                // Efficiency: one card of figures, not a card each.
                SectionTitle { text: "Efficiency" }
                Rectangle {
                    objectName: "statsEfficiency"
                    Layout.fillWidth: true
                    implicitHeight: effGrid.implicitHeight + 24
                    radius: 8
                    color: Theme.elevated
                    readonly property var e: content.r.efficiency
                    GridLayout {
                        id: effGrid
                        x: 14
                        y: 12
                        width: parent.width - 28
                        columns: body.width > 720 ? 3 : body.width > 440 ? 2 : 1
                        columnSpacing: 28
                        rowSpacing: 8
                        Repeater {
                            model: [
                                ["Cache hit rate", root.pct(effGrid.parent.e.cacheHit), "Of the input the models read, the share that came from the cache"],
                                ["Cost per session", root.money(effGrid.parent.e.costPerSession), ""],
                                ["Cost per million tokens", root.money(effGrid.parent.e.costPerMillion), ""],
                                ["Tokens per session", root.compact(effGrid.parent.e.tokensPerSession), ""],
                                ["Model steps per session", Number(effGrid.parent.e.stepsPerSession || 0).toFixed(1), ""],
                                ["Reasoning in output", root.pct(effGrid.parent.e.reasoningShare), "Of the output tokens, the share spent reasoning before answering"],
                            ].filter(row => content.hasCost || row[0].indexOf("Cost") !== 0)
                            RowLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.minimumWidth: 0
                                spacing: 8
                                Label {
                                    text: modelData[0]
                                    color: Theme.textMuted
                                    font.pixelSize: Theme.px(12.5)
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    elide: Text.ElideRight
                                    HoverHandler { id: effHover }
                                    ToolTip.visible: effHover.hovered && modelData[2] !== ""
                                    ToolTip.delay: 400
                                    ToolTip.text: modelData[2]
                                }
                                Label { text: modelData[1]; color: Theme.text; font.pixelSize: Theme.px(13); font.weight: Font.DemiBold }
                            }
                        }
                    }
                }

                // Models
                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle { text: "Models" }
                    Item { Layout.fillWidth: true }
                    Repeater {
                        model: [["tokens", "Tokens"], ["cost", "Cost"], ["steps", "Steps"]]
                        Pill {
                            required property var modelData
                            Layout.topMargin: 14
                            text: modelData[1]
                            selected: root.modelMetric === modelData[0]
                            onClicked: root.modelMetric = modelData[0]
                        }
                    }
                }
                Label {
                    visible: root.modelMetric === "tokens"
                    text: "Coloured as in Token composition."
                    color: Theme.textFaint
                    font.pixelSize: Theme.px(11.5)
                    Layout.topMargin: -4
                }
                Repeater {
                    id: modelRows
                    // (Models with none of what's measured are left out.)
                    readonly property var rows: (content.r.models || []).filter(m => (m[root.modelMetric] || 0) > 0).sort((a, b) => b[root.modelMetric] - a[root.modelMetric])
                    readonly property real peak: Math.max(1e-9, ...rows.map(m => m[root.modelMetric]))
                    model: rows
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 5
                        // The name, its effort and provider, then the numbers at
                        // the right; the names give way on narrow windows.
                        Item {
                            id: modelHead
                            Layout.fillWidth: true
                            implicitHeight: modelNum.implicitHeight
                            readonly property real room: width - modelNum.implicitWidth - 12
                            Label {
                                id: modelName
                                width: Math.min(implicitWidth, modelHead.room)
                                text: root.modelName(modelData.model)
                                color: Theme.foreground
                                font.pixelSize: Theme.px(13)
                                elide: Text.ElideRight
                            }
                            Label {
                                id: modelThinking
                                x: modelName.width + 8
                                anchors.baseline: modelName.baseline
                                width: visible ? Math.max(0, Math.min(implicitWidth, modelHead.room - x)) : 0
                                visible: modelData.thinking !== "" && modelHead.room - x > 24
                                text: modelData.thinking
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(12)
                                elide: Text.ElideRight
                            }
                            Label {
                                x: modelThinking.visible ? modelThinking.x + modelThinking.width + 8 : modelName.width + 8
                                anchors.baseline: modelName.baseline
                                width: Math.max(0, Math.min(implicitWidth, modelHead.room - x))
                                visible: modelData.provider !== "" && modelHead.room - x > 24
                                text: modelData.provider
                                color: Theme.textFaint
                                font.pixelSize: Theme.px(12)
                                elide: Text.ElideRight
                            }
                            Label {
                                id: modelNum
                                anchors.right: parent.right
                                anchors.baseline: modelName.baseline
                                text: root.modelMetric === "steps"
                                    ? root.plural(modelData.steps, "step", "steps")
                                    : root.modelMetric === "cost" ? root.money(modelData.cost) + " · " + root.compact(modelData.tokens)
                                    : root.compact(modelData.tokens) + (modelData.cost > 0 ? " · " + root.money(modelData.cost) : "")
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(12)
                            }
                        }
                        Rectangle {
                            id: track
                            Layout.fillWidth: true
                            implicitHeight: 6
                            radius: 3
                            color: Theme.subtle
                            clip: true
                            readonly property real frac: modelData[root.modelMetric] / modelRows.peak
                            Item {
                                width: track.width * track.frac
                                height: parent.height
                                // Cost and steps: one colour. Tokens: the four kinds.
                                Rectangle {
                                    visible: root.modelMetric !== "tokens"
                                    anchors.fill: parent
                                    radius: 3
                                    color: root.modelMetric === "cost" ? Theme.primary : Theme.info
                                }
                                Row {
                                    id: kindsRow
                                    visible: root.modelMetric === "tokens"
                                    anchors.fill: parent
                                    Repeater {
                                        model: root.tokenKinds
                                        Rectangle {
                                            required property var modelData
                                            readonly property var m: track.parent.modelData
                                            width: kindsRow.width * (m[modelData.key] || 0) / Math.max(1, m.tokens)
                                            height: kindsRow.height
                                            color: modelData.color
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Tool calls
                SectionTitle { text: "Tool calls" }
                Label {
                    visible: (content.r.tools || []).length === 0
                    text: "No tool calls in this period."
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                FontMetrics { id: toolFont; font.family: Theme.mono; font.pixelSize: Theme.px(12) }
                Repeater {
                    id: toolRows
                    readonly property var rows: (content.r.tools || []).slice(0, 15)
                    readonly property real peak: Math.max(1, ...rows.map(t => t.count))
                    // The names' column: as wide as the longest, up to a third.
                    readonly property real nameWidth: Math.min(body.width * 0.34, Math.max(40, ...rows.map(t => toolFont.advanceWidth(t.name))) + 12)
                    model: rows
                    RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 10
                        Label { text: modelData.name; color: Theme.foreground; font.family: Theme.mono; font.pixelSize: Theme.px(12); Layout.preferredWidth: toolRows.nameWidth; Layout.minimumWidth: 0; Layout.fillWidth: true; Layout.maximumWidth: toolRows.nameWidth; elide: Text.ElideRight }
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1000
                            implicitHeight: 6
                            radius: 3
                            color: Theme.subtle
                            Rectangle { width: parent.width * modelData.count / toolRows.peak; height: parent.height; radius: 3; color: Theme.primary }
                        }
                        Label { text: root.compact(modelData.count); color: Theme.textMuted; font.pixelSize: Theme.px(12); Layout.preferredWidth: 32; horizontalAlignment: Text.AlignRight }
                    }
                }
            }
        }
    }
}
