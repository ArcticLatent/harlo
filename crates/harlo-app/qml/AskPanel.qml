import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// omp's `ask` tool: every question of one call, answered together.
// Options are radio (single) or check (multi) rows with descriptions; there
// is always a free-text answer. `request` is the raw extension_ui_request.
Rectangle {
    id: root
    property var request: ({})
    signal submitted(var result)
    signal skipped()

    readonly property var questions: request.questions || []
    // Per question: { selected: [labels], custom: "" }.
    property var answers: []
    property string previewText: ""
    property real deadline: 0         // ms since epoch; 0 = no timeout
    property real now: Date.now()

    // Reads `request` directly: in onRequestChanged the `questions` binding
    // may not have updated yet.
    function reset() {
        answers = (request.questions || []).map(q => ({
            selected: !q.multi && q.recommended !== undefined && q.options[q.recommended] ? [q.options[q.recommended].label] : [],
            custom: ""
        }))
        deadline = request.timeout ? Date.now() + request.timeout : 0
        previewText = ""
    }
    onRequestChanged: reset()
    Component.onCompleted: reset()

    function toggle(qi, label) {
        const q = questions[qi]
        const s = answers.slice()
        const cur = Object.assign({}, s[qi])
        if (q.multi) {
            cur.selected = cur.selected.includes(label) ? cur.selected.filter(l => l !== label) : cur.selected.concat([label])
        } else {
            cur.selected = [label]
            cur.custom = ""       // one or the other for single choice
        }
        s[qi] = cur
        answers = s
    }
    function setCustom(qi, text) {
        const s = answers.slice()
        const cur = Object.assign({}, s[qi])
        cur.custom = text
        if (!questions[qi].multi && text.trim() !== "")
            cur.selected = []
        s[qi] = cur
        answers = s
    }
    function answered(qi) {
        const s = answers[qi]
        return !!s && (questions[qi].multi || s.selected.length > 0 || s.custom.trim() !== "")
    }
    readonly property bool complete: questions.length > 0 && questions.every((q, i) => answered(i))
    function submit() {
        if (!complete)
            return
        root.submitted(questions.map((q, i) => {
            const a = { id: q.id, selectedOptions: answers[i].selected }
            if (answers[i].custom.trim() !== "")
                a.customInput = answers[i].custom.trim()
            return a
        }))
    }

    Timer { interval: 1000; repeat: true; running: root.visible && root.deadline > 0; onTriggered: root.now = Date.now() }
    readonly property int secondsLeft: deadline > 0 ? Math.max(0, Math.ceil((deadline - now) / 1000)) : -1

    implicitHeight: col.implicitHeight + 24
    radius: 10
    color: Theme.elevated
    border.color: Theme.primary
    Keys.onPressed: event => {
        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
            root.submit()
            event.accepted = true
        }
    }

    ColumnLayout {
        id: col
        x: 14
        y: 12
        width: parent.width - 28
        spacing: 10

        // Header
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            Rectangle {
                implicitWidth: 26
                implicitHeight: 26
                radius: 7
                color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.15)
                Icon { anchors.centerIn: parent; name: "question"; color: Theme.primary; implicitWidth: 16; implicitHeight: 16 }
            }
            Label {
                Layout.fillWidth: true
                text: root.questions.length === 1 ? "The agent has a question" : "The agent has " + root.questions.length + " questions"
                color: Theme.foreground
                font.pixelSize: Theme.px(14)
                font.bold: true
            }
            Label {
                visible: root.secondsLeft >= 0
                text: "Picks the recommended answers in " + (root.secondsLeft >= 60 ? Math.floor(root.secondsLeft / 60) + "m " + (root.secondsLeft % 60) + "s" : root.secondsLeft + "s")
                color: root.secondsLeft <= 15 ? Theme.warning : Theme.muted
                font.pixelSize: Theme.px(11)
            }
        }

        // Questions (scroll when there are many)
        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(questionsCol.implicitHeight, root.maxQuestionsHeight)
            contentHeight: questionsCol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ColumnLayout {
                id: questionsCol
                width: flick.width - (flick.contentHeight > flick.height ? 12 : 0)
                spacing: 14
                Repeater {
                    model: root.questions
                    ColumnLayout {
                        id: qBlock
                        required property var modelData
                        required property int index
                        readonly property var q: modelData
                        readonly property var answer: root.answers[index] || ({ selected: [], custom: "" })
                        Layout.fillWidth: true
                        spacing: 6
                        RowLayout {
                            spacing: 8
                            Rectangle {
                                visible: !!qBlock.q.header
                                implicitWidth: headerLabel.implicitWidth + 12
                                implicitHeight: 18
                                radius: 4
                                color: Theme.subtle
                                border.color: Theme.border
                                Label { id: headerLabel; anchors.centerIn: parent; text: (qBlock.q.header || "").toUpperCase(); color: Theme.muted; font.pixelSize: Theme.px(10); font.bold: true; font.letterSpacing: 0.6 }
                            }
                            Label {
                                visible: root.questions.length > 1
                                text: (qBlock.index + 1) + " / " + root.questions.length
                                color: Theme.muted
                                font.pixelSize: Theme.px(11)
                            }
                            Label {
                                visible: !!qBlock.q.multi
                                text: "choose any"
                                color: Theme.muted
                                font.pixelSize: Theme.px(11)
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: qBlock.q.question
                            color: Theme.foreground
                            font.pixelSize: Theme.px(14)
                            wrapMode: Text.Wrap
                        }
                        Repeater {
                            model: qBlock.q.options || []
                            Rectangle {
                                id: optRow
                                required property var modelData
                                required property int index
                                readonly property bool chosen: qBlock.answer.selected.includes(modelData.label)
                                readonly property bool recommended: qBlock.q.recommended !== undefined && qBlock.q.recommended === index
                                Layout.fillWidth: true
                                implicitHeight: optCol.implicitHeight + 14
                                radius: 7
                                color: chosen ? Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.10) : optHover.hovered ? Theme.hover : "transparent"
                                border.color: chosen ? Theme.primary : Theme.border
                                HoverHandler {
                                    id: optHover
                                    cursorShape: Qt.PointingHandCursor
                                    onHoveredChanged: if (hovered) root.previewText = optRow.modelData.preview || ""
                                }
                                TapHandler { onTapped: { root.toggle(qBlock.index, optRow.modelData.label); root.previewText = optRow.modelData.preview || "" } }
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.margins: 7
                                    anchors.leftMargin: 10
                                    spacing: 10
                                    // Radio (single) or check box (multi).
                                    Rectangle {
                                        Layout.alignment: Qt.AlignTop
                                        Layout.topMargin: 1
                                        implicitWidth: 16
                                        implicitHeight: 16
                                        radius: qBlock.q.multi ? 4 : 8
                                        color: qBlock.q.multi && optRow.chosen ? Theme.primary : "transparent"
                                        border.color: optRow.chosen ? Theme.primary : Theme.muted
                                        border.width: !qBlock.q.multi && optRow.chosen ? 5 : 1.5
                                        Icon { anchors.centerIn: parent; visible: !!qBlock.q.multi && optRow.chosen; name: "check"; color: Theme.onPrimary; implicitWidth: 13; implicitHeight: 13 }
                                    }
                                    ColumnLayout {
                                        id: optCol
                                        Layout.fillWidth: true
                                        spacing: 2
                                        RowLayout {
                                            Layout.fillWidth: true
                                            spacing: 8
                                            Label { text: optRow.modelData.label; color: Theme.foreground; font.pixelSize: Theme.px(13); Layout.maximumWidth: optCol.width - 100; elide: Text.ElideRight }
                                            Rectangle {
                                                visible: optRow.recommended
                                                implicitWidth: recLabel.implicitWidth + 10
                                                implicitHeight: 16
                                                radius: 8
                                                color: Qt.rgba(Theme.success.r, Theme.success.g, Theme.success.b, 0.15)
                                                Label { id: recLabel; anchors.centerIn: parent; text: "Recommended"; color: Theme.success; font.pixelSize: Theme.px(10); font.bold: true }
                                            }
                                            Item { Layout.fillWidth: true }
                                        }
                                        Label {
                                            Layout.fillWidth: true
                                            visible: !!optRow.modelData.description
                                            text: optRow.modelData.description || ""
                                            color: Theme.muted
                                            font.pixelSize: Theme.px(12)
                                            wrapMode: Text.Wrap
                                        }
                                    }
                                }
                            }
                        }
                        // Free text is always possible.
                        AppTextField {
                            Layout.fillWidth: true
                            placeholderText: qBlock.q.multi ? "Something else (optional)…" : "Or type your own answer…"
                            text: qBlock.answer.custom
                            onTextEdited: root.setCustom(qBlock.index, text)
                            onAccepted: root.submit()
                        }
                    }
                }
            }
        }

        // Preview of the hovered/chosen option, when it has one.
        Rectangle {
            Layout.fillWidth: true
            visible: root.previewText !== ""
            implicitHeight: Math.min(previewLabel.implicitHeight + 16, 160)
            radius: 6
            color: Theme.codeBg
            border.color: Theme.border
            clip: true
            Label {
                id: previewLabel
                x: 10
                y: 8
                width: parent.width - 20
                text: root.previewText
                color: Theme.foreground
                font.family: Theme.mono
                font.pixelSize: Theme.px(12)
                wrapMode: Text.Wrap
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: root.complete ? "Ctrl+Enter to send" : "Answer every question to send"
                color: Theme.muted
                font.pixelSize: Theme.px(11)
            }
            DialogButton { text: "Skip"; ToolTip.visible: hovered; ToolTip.text: "Close without answering"; onClicked: root.skipped() }
            DialogButton { text: "Send answers"; accent: true; enabled: root.complete; onClicked: root.submit() }
        }
    }
    property real maxQuestionsHeight: 360
}
