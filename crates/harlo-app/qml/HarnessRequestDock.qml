pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// What the harness asks while the session waits: an approval, a choice
// (with descriptions, a row each), a confirmation, some text; a plan to
// approve as Markdown. An approval whose tool row is on screen is asked
// there alone.
Rectangle {
    id: requestDock
    /// The session asking, and its transcript (an approval on its tool row
    /// is shown there).
    required property ChatSession session
    required property TranscriptView transcript
    /// A file or diff tab covers the chat.
    property bool covered: false
    /// What's asked scrolls past this height.
    property real maxBodyHeight: 140
    // (An approval whose tool row is on screen: asked there alone.)
    visible: requestDock.session.hasRequest && requestDock.session.requestMethod !== "ask" && !requestDock.covered && !inlineShown
    // (Only when its row's end, the card's buttons, is on screen:
    // shown twice beats not at all.)
    readonly property bool inlineShown: inlineApproval && (requestDock.transcript.count, requestDock.transcript.height, requestDock.transcript.rowEndInView(requestDock.session.indexOfKey(requestDock.session.approvalRow)))
    // `select` options may carry descriptions (`optionDetails`).
    readonly property var details: {
        if (!visible || requestDock.session.requestJson === "") return []
        const r = JSON.parse(requestDock.session.requestJson)
        return r.optionDetails || []
    }
    readonly property bool detailed: requestDock.session.requestMethod === "select" && details.some(d => d && d.description)
    // A plan to approve comes as Markdown: drawn as a reply is.
    readonly property bool markdown: visible && requestDock.session.requestJson !== "" && !!JSON.parse(requestDock.session.requestJson).markdown
    readonly property var blocks: markdown ? JSON.parse(Workspace.markdownBlocks(requestDock.session.requestMessage)) : []
    // An approval shown on its tool row: just a reminder here.
    readonly property bool inlineApproval: requestDock.session.requestIsApproval && requestDock.session.approvalRow !== ""
    implicitHeight: reqCol.implicitHeight + 24
    radius: 4
    color: Theme.elevated
    border.color: requestDock.session.requestIsApproval ? Theme.warning : Theme.primary
    ColumnLayout {
        id: reqCol
        x: 14
        y: 12
        width: parent.width - 28
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            visible: requestDock.inlineApproval
            spacing: 6
            PlainLabel {
                Layout.fillWidth: true
                readonly property string tool: requestDock.session.requestTitle.split("\n")[0].replace(/^Allow tool:\s*/, "")
                text: "Approval needed: " + tool + (requestDock.session.requestTitle.split("\n").indexOf("Outside the sandbox") > 0 ? ", outside the sandbox" : "")
                color: Theme.foreground
                font.bold: true
                elide: Text.ElideRight
            }
            FlatButton { text: "Show"; outlined: false; onClicked: requestDock.transcript.revealIndex(requestDock.session.indexOfKey(requestDock.session.approvalRow)) }
            FlatButton { text: "Deny"; onClicked: requestDock.session.answerValue("Deny") }
            // (Always allow: on the tool row, which Show reaches.)
            FlatButton { text: "Approve"; variant: "primary"; onClicked: requestDock.session.answerValue("Approve") }
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: !requestDock.inlineApproval
            text: requestDock.session.requestTitle
            color: Theme.text
            font.pixelSize: Theme.px(13.5)
            font.weight: Font.DemiBold
            wrapMode: Text.Wrap
        }
        // What's asked (a plan to approve can run long): it scrolls
        // past part of the window, so the answers stay in view.
        Flickable {
            id: reqScroll
            objectName: "requestScroll"
            Layout.fillWidth: true
            visible: requestDock.session.requestMessage !== "" && !requestDock.inlineApproval
            readonly property bool scrolls: contentHeight > height + 1
            Layout.preferredHeight: Math.min(contentHeight, requestDock.maxBodyHeight)
            contentWidth: width
            contentHeight: reqBody.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: reqScroll.scrolls ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
            ColumnLayout {
                id: reqBody
                width: reqScroll.width - (reqScroll.scrolls ? 14 : 0)
                spacing: 8
                PlainLabel {
                    Layout.fillWidth: true
                    visible: !requestDock.markdown
                    text: requestDock.session.requestMessage
                    color: Theme.muted
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: requestDock.blocks
                    Item {
                        id: reqBlock
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: modelData.kind === "code" ? reqCode.height : reqProse.implicitHeight
                        TextEdit {
                            id: reqProse
                            objectName: "requestProse"
                            visible: reqBlock.modelData.kind !== "code"
                            width: parent.width
                            textFormat: TextEdit.RichText
                            text: visible ? Theme.htmlStyle + Theme.proseHtml(reqBlock.modelData.html, -1, true) : ""
                            font.family: Theme.uiFont
                            font.pixelSize: Theme.px(13.5)
                            color: Theme.foreground
                            selectionColor: Theme.selection
                            selectedTextColor: Theme.foreground
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                        }
                        Rectangle {
                            id: reqCode
                            visible: reqBlock.modelData.kind === "code"
                            width: parent.width
                            height: visible ? reqCodeText.implicitHeight + 16 : 0
                            radius: 2
                            color: Theme.codeBg
                            TextEdit {
                                id: reqCodeText
                                x: 10
                                y: 8
                                width: parent.width - 20
                                textFormat: TextEdit.RichText
                                text: reqCode.visible ? reqBlock.modelData.html : ""
                                font.family: Theme.mono
                                font.pixelSize: Theme.mpx(12)
                                color: Theme.foreground
                                selectionColor: Theme.selection
                                readOnly: true
                                selectByMouse: true
                                wrapMode: TextEdit.Wrap
                            }
                        }
                    }
                }
            }
        }
        TextArea {
            id: reqInput
            Layout.fillWidth: true
            visible: requestDock.session.requestMethod === "input" || requestDock.session.requestMethod === "editor"
            text: requestDock.session.requestPrefill
            color: Theme.foreground
            wrapMode: TextEdit.Wrap
            background: Rectangle { radius: 2; color: Theme.background; border.color: Theme.border }
        }
        // Options with descriptions: one row each.
        Repeater {
            model: requestDock.detailed ? requestDock.session.requestOptions : []
            Rectangle {
                id: detailRow
                required property string modelData
                required property int index
                Layout.fillWidth: true
                implicitHeight: detailCol.implicitHeight + 14
                radius: 3
                color: detailHover.hovered ? Theme.hover : "transparent"
                border.color: detailHover.hovered ? Theme.primary : Theme.border
                HoverHandler { id: detailHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: requestDock.session.answerValue(detailRow.modelData) }
                ColumnLayout {
                    id: detailCol
                    x: 10
                    y: 7
                    width: parent.width - 20
                    spacing: 2
                    Label { text: detailRow.modelData; color: Theme.foreground; font.pixelSize: Theme.px(13) }
                    PlainLabel {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: (requestDock.details[detailRow.index] || {}).description || ""
                        color: Theme.muted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
        // The answers, on the right: the agent's first choice solid
        // (its suggestion), the others outlined, Dismiss quiet on
        // the left. (Deny is never the solid one.)
        Flow {
            Layout.fillWidth: true
            Layout.topMargin: 2
            visible: !requestDock.inlineApproval
            spacing: 6
            layoutDirection: Qt.RightToLeft
            Repeater {
                model: requestDock.session.requestMethod === "select" && !requestDock.detailed ? requestDock.session.requestOptions : []
                FlatButton {
                    required property string modelData
                    required property int index
                    // (Right to left: the first is rightmost.)
                    readonly property bool first: index === 0
                    text: modelData
                    variant: first && modelData !== "Deny" ? "primary" : "outline"
                    onClicked: requestDock.session.answerValue(modelData)
                }
            }
            FlatButton { visible: requestDock.session.requestMethod === "confirm"; text: "Yes"; variant: "primary"; onClicked: requestDock.session.answerConfirm(true) }
            FlatButton { visible: requestDock.session.requestMethod === "confirm"; text: "No"; onClicked: requestDock.session.answerConfirm(false) }
            FlatButton { visible: reqInput.visible; text: "Submit"; variant: "primary"; onClicked: requestDock.session.answerValue(reqInput.text) }
            FlatButton { text: "Dismiss"; outlined: false; accent: Theme.textMuted; onClicked: requestDock.session.cancelRequest() }
        }
    }
}
