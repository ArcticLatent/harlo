pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A tool call: one line (what it is, what it touched, what it came to)
// and, when there's something to see, one well under it: a write's or
// an edit's lines, a command's last lines.
ColumnLayout {
    id: card
    objectName: "toolCard"
    /// The transcript it's in (its helpers, its list's state).
    /// (An Item: a TranscriptView makes ToolCards, so naming its type here would be a cycle.)
    required property Item transcript
    property string rowKey
    property string title
    property string subtitle
    property string text
    property string rowState
    property string callId
    property string result
    property string diff
    property real took
    property string label
    property string command
    /// Its pictures (files, a line each): returned, or ones it names.
    property string images
    // Waiting for your approval (the row's own).
    property bool asking: false
    // Its run ("" for a call on its own).
    property string group
    // Reads after it in its run: only a run's last 3 reads show their
    // lines at once (the rest open on a click).
    property int readsAfter: 0
    readonly property bool isRead: /^(read|view|read_?file)$/i.test(title)
    property var askLines: []
    readonly property bool open: card.transcript.listView.expanded[rowKey] === true
    readonly property var agents: callId !== "" ? card.transcript.subagents.filter(a => a.parentToolCallId === callId) : []
    // A write: every line added, numbered; an edit: lines out and in,
    // numbered where its new lines are in the file now (a local one).
    readonly property bool preview: diff !== "" && !asking
    readonly property bool newFile: preview && result.indexOf("−") < 0
    readonly property int previewLines: 6
    readonly property int diffCount: diff === "" ? 0 : diff.split("\n").length
    // (A write's own count: its diff stops at 400 lines.)
    readonly property int lineCount: newFile && /^\+\d+$/.test(result) ? Number(result.slice(1)) : diffCount
    // The well's code: a write's lines numbered, an edit's out and in, or
    // a listing (grep's matches tinted among their context, each file's
    // lines under its name, a blank line between files).
    readonly property bool hasContext: !!code && code.lines.some(l => l.c)
    readonly property var codeLines: {
        if (preview) {
            const path = subtitle.replace(/…$/, "")
            // (Again when a host's file has come: its numbers with it.)
            void card.transcript.session.filesRead
            return JSON.parse(card.transcript.session.diffLines(path, diff, open ? 400 : previewLines, newFile ? "" : absolute(path)))
                .map((l, i) => ({ n: newFile ? String(i + 1) : l.n, h: l.h, mark: newFile ? "" : l.s === "+" ? "add" : l.s === "-" ? "remove" : "", p: newFile || l.n !== "" ? absolute(path) : "" }))
        }
        if (!code) return []
        const out = []
        let file = code.path
        for (const l of code.lines) {
            if (l.f !== "") {
                file = l.f
                if (out.length) out.push({ n: "", h: "", mark: "gap" })
                // (Its name: a link to the file, at its first line here.)
                const first = code.lines.find((x, i) => i > code.lines.indexOf(l) && x.n !== "")
                out.push({ n: "", mark: "file", h: "<a href=\"harlo-open:" + (first ? first.n : 1) + ":" + encodeURIComponent(absolute(l.f)) + "\" style=\"color:"
                           + Theme.rgb(Theme.textMuted) + "; text-decoration:none;\">" + card.transcript.local(l.f).replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</a><span style=\"color:" + Theme.rgb(Theme.textFaint) + ";\"> ↗</span>" })
            } else out.push({ n: l.n, h: l.h, mark: l.c ? "context" : hasContext ? "match" : "", p: file !== "" ? absolute(file) : "" })
        }
        return out
    }
    // Output that is a file's lines (a printed file, grep -n, a read):
    // numbered and highlighted like a file, its first lines (where it
    // starts matters).
    readonly property var code: {
        if (preview || asking || text === "" || rowState === "running") return null
        const j = card.transcript.session.codeView(title, command, subtitle.replace(/…$/, ""), text, open ? 400 : previewLines)
        return j === "" ? null : JSON.parse(j)
    }
    readonly property bool shell: command !== "" && !asking
    readonly property string shownCommand: shell ? card.transcript.local(command) : ""
    // The command in the well when its line doesn't say it all (cut
    // short there too, when narrow: measured as if no Copy showed, so
    // hovering doesn't move it).
    readonly property bool commandCut: shell && label === "" && whatLabel.implicitWidth > whatLabel.width + (callCopy.visible ? callCopy.width + head.spacing : 0) + 0.5
    readonly property bool commandInWell: shell && (label !== "" || shownCommand.indexOf("\n") >= 0 || shownCommand.length > 90 || commandCut)
    readonly property var outLines: text === "" ? [] : text.split("\n").filter(l => l.trim() !== "")
    readonly property int tailLines: 3
    readonly property var tail: outInWell && shell && !open && !code ? outLines.slice(-tailLines) : []
    readonly property var tailGutters: card.transcript.numbered(tail)
    // (One line of output that's what the line already says, "48 passed": not again.)
    readonly property bool echoed: shell && outLines.length === 1 && result !== "" && outLines[0].trim() === result.trim()
    readonly property bool outInWell: shell ? outLines.length > 0 && rowState !== "running" && !echoed : open && text !== "" && !preview
    readonly property bool codeShown: !!code && (!isRead || readsAfter < 3 || open)
    // Kept to its end (a log, a command's last lines): what's left out
    // came before, so it's said above them.
    readonly property bool fromEnd: code ? code.earlier === true : shell && !preview
    // Before its first line shown, when its lines are numbered from 1
    // on (more than the hidden ones when its start wasn't kept).
    readonly property int before: {
        const n = code && code.lines.length ? Number(code.lines[0].n) : NaN
        return n > 1 && /^\d+$/.test(code.lines[0].n) ? n - 1 : 0
    }
    // Opened or folded; kept to its end, its last line stays where it
    // is on screen (what grows or shrinks is above it).
    function toggleOpen() {
        const y = fromEnd ? wellCode.mapToItem(null, 0, wellCode.height).y : NaN
        card.transcript.listView.toggle(rowKey)
        if (fromEnd) card.transcript.holdPlace(wellCode, y)
    }
    function absolute(p) { return card.transcript.fileAt(p, command) }
    // A workflow's report (harlo's report_done): said in words, its
    // fields read from the call's line ("outcome=… summary=… attempt=2").
    readonly property bool report: /^(?:mcp__harlo__|harlo_)?report_done$/.test(title)
    readonly property var reportArgs: {
        if (!report) return null
        const field = k => { const m = new RegExp("(?:^|\\s)" + k + "=(.*?)(?=\\s{2}\\w+=|$)").exec(subtitle); return m ? m[1].trim() : "" }
        return { attempt: field("attempt"), summary: field("summary"), outcome: field("outcome") }
    }
    // A report for an attempt that's over (a later one is under way): not
    // a failure, just not used.
    readonly property bool superseded: report && /is over/.test(result + " " + text)
    readonly property bool failed: !superseded && (rowState === "error" || /^(exit|failed)|failed$/.test(result))
    // A listing's whole length: the call's own count ("1200 lines")
    // when its output was cut, else what's kept.
    readonly property int fullCount: {
        if (!code) return 0
        const m = /^(\d+) lines?$/.exec(result)
        return m ? Math.max(Number(m[1]), code.total) : code.total
    }
    readonly property int hidden: preview ? lineCount - previewLines : code ? fullCount - previewLines : shell ? outLines.length - tailLines : 0
    // Opened, what still isn't shown (-1: more, how many unknown), and
    // where its file goes on ({p, n}), for "Open at line n".
    readonly property int beyond: !open ? 0
                                : code ? (code.earlier ? 0 : Math.max(0, fullCount - code.lines.length) || (code.more ? -1 : 0))
                                : preview ? Math.max(0, lineCount - 400) : 0
    readonly property var beyondAt: {
        if (beyond === 0 || (code && code.path === "")) return null
        const nums = codeLines.filter(l => /^\d+$/.test(l.n) && l.p)
        const first = nums[0], last = nums[nums.length - 1]
        // (Only a file's lines in a row: search results skip about.)
        if (!last || Number(last.n) - Number(first.n) !== nums.length - 1) return null
        return { p: last.p, n: Number(last.n) + 1 }
    }
    // Its last lines cut short to the well (each one line): opened, all of it.
    property int cutLines: 0
    readonly property bool canOpen: (text !== "" && !shell && !preview) || hidden > 0 || cutLines > 0 || (commandInWell && shownCommand.split("\n").length > 2)
    // (harlo's tools about a session: its title, not its key.)
    readonly property bool aboutSession: /^(mcp__harlo__)?(bring_over|send_message|read_session|stop_session)$/.test(title) && /^[\w-]+:\S+$/.test(subtitle)
    readonly property string sessionTitle: aboutSession ? Workspace.sessionTitle(subtitle) : ""
    readonly property string target: reportArgs && reportArgs.summary !== "" ? reportArgs.summary : label !== "" ? label : sessionTitle !== "" ? sessionTitle : subtitle !== "" || rowState !== "pending" ? card.transcript.local(subtitle) : Theme.toolWriting(title)
    // Code type only for code: a command, a path, a pattern, a link. A
    // title, a question or a task's description is prose.
    // (A wiki step's words, "Quickstart" or "1 page planned", aren't
    // code; a wiki read's page path is.)
    readonly property bool codeLike: label === "" && sessionTitle === "" && !/openwiki_(?!read$)/.test(title) && (shell || command !== "" || /^(bash|shell|exec|run_?command|read|edit|multi_?edit|write|apply_?patch|patch|create|ls|find|glob|grep|lsp|web_?fetch|fetch|notebook)/i.test(title) || !/\s/.test(target))
    readonly property bool isPath: label === "" && !shell && /^[^\s]+$/.test(target) && /[\/.]/.test(target)
    readonly property real headHeight: headBox.height
    spacing: 4
    HoverHandler { id: cardHover }
    Item {
        id: headBox
        objectName: "toolHead"
        Layout.fillWidth: true
        implicitHeight: head.implicitHeight + 6
        readonly property string stepGroup: card.group
        // The keyboard: a call in a run is reached from the run's line
        // (Up and Down); one on its own takes Tab. Enter or Space opens
        // it, Ctrl+C copies its listing.
        // (Kept while it has the focus: Qt won't take it from the focused
        // item, and warns; a call joining a run drops it once focus moves.)
        activeFocusOnTab: activeFocus || (card.group === "" && (card.canOpen || !!card.code || card.preview))
        Accessible.role: Accessible.Button
        Accessible.name: card.transcript.toolName(card.title) + ", " + card.target + (card.result !== "" ? ", " + card.result.replace(/<[^>]+>/g, "") : "") + (card.canOpen ? (card.open ? ", open" : ", folded") : "")
        Keys.onPressed: e => {
            card.transcript.clickedStep = null
            if (card.canOpen && (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space)) { card.toggleOpen(); e.accepted = true }
            else if (e.key === Qt.Key_C && (e.modifiers & Qt.ControlModifier) && (!!card.code || card.preview)) { callCopy.clicked(); e.accepted = true }
            else if (e.key === Qt.Key_Escape && card.group === "") { card.transcript.leaveRuns(""); e.accepted = true }
            else e.accepted = (card.group !== "" && card.transcript.stepKey(headBox, e.key)) || card.transcript.typedOn(e)
        }
        // (Hover and focus share a left edge: the step's name.)
        Rectangle {
            x: toolNameLabel.x - 6; width: parent.width - x + 6; height: parent.height; radius: 6
            color: headHover.hovered && card.canOpen ? Theme.hover : "transparent"
        }
        // (Focused: ringed from its name, the run's line left clear.)
        Rectangle {
            visible: card.transcript.keyFocus(headBox)
            x: toolNameLabel.x - 6; width: parent.width - x + 6; height: parent.height; radius: 6
            color: "transparent"; border.width: 1; border.color: Theme.accent
        }
        RowLayout {
            id: head
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            spacing: 8
            RunNode {
                objectName: "toolMark"
                step: card.asking ? "asking" : card.superseded ? "done" : card.rowState === "error" || (card.failed && card.rowState === "done") ? "error" : card.rowState === "done" ? "done" : "running"
            }
            // (A long name, a plugin's tool say, cut in the middle: the
            // whole of it on hover.)
            PlainLabel {
                id: toolNameLabel
                objectName: "toolName"
                Layout.maximumWidth: Math.max(80, Math.round(head.width * 0.45))
                text: !card.reportArgs ? card.transcript.toolName(card.title)
                    // (Narrow: "Attempt N", so the summary keeps its room.)
                    : card.reportArgs.attempt !== "" ? (head.width < 460 ? qsTr("Attempt %1") : qsTr("Reported attempt %1")).arg(card.reportArgs.attempt) : qsTr("Reported back")
                elide: Text.ElideMiddle
                color: Theme.text
                font.weight: Font.Medium
                font.pixelSize: Theme.px(13)
                HoverHandler { id: toolNameHover }
                ToolTip.visible: toolNameLabel.truncated && toolNameHover.hovered
                ToolTip.text: toolNameLabel.text
                ToolTip.delay: 500
            }
            Label {
                id: whatLabel
                objectName: "toolWhat"
                Layout.fillWidth: true
                // (Plain, for tests and copying: the words shown.)
                readonly property string plain: card.target
                textFormat: card.isPath ? Text.StyledText : Text.PlainText
                text: card.isPath ? card.transcript.pathHtml(card.target) : card.target
                color: Theme.textMuted
                elide: Text.ElideRight
                font.family: card.codeLike ? Theme.mono : Theme.uiFont
                font.pixelSize: card.codeLike ? Theme.mpx(12) : Theme.px(13)
            }
            // Its lines cut at the right: wrap them (always shown: it's
            // what says they go on).
            WrapButton { list: card.transcript.listView; key: card.rowKey; code: wellCode }
            Label {
                objectName: "toolResult"
                visible: text !== ""
                textFormat: Text.StyledText
                text: {
                    const faint = s => s === "" ? "" : "<font color='" + Theme.rgb(Theme.textFaint) + "'>" + s + "</font>"
                    let r = card.newFile ? (card.lineCount === 1 ? "1 line" : card.lineCount + " lines")
                          : card.result.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/^\+(\d+) −(\d+)$/, "<font color='" + Theme.rgb(Theme.diffAdd) + "'>+$1</font> <font color='" + Theme.rgb(Theme.diffRemove) + "'>−$2</font>")
                    // (Narrow: the short form, so the summary keeps its room.)
                    if (card.superseded) r = faint(head.width < 460 ? qsTr("not used") : qsTr("not used: a later attempt is under way"))
                    else if (card.failed && r !== "") r = "<font color='" + Theme.rgb(Theme.error) + "'>" + r + "</font>"
                    return [r, faint(card.transcript.seconds(card.took))].filter(x => x !== "").join("&nbsp;&nbsp;")
                }
                color: Theme.textMuted
                font.pixelSize: Theme.px(12)
                HoverHandler { id: resultHover; enabled: card.superseded }
                ToolTip.visible: card.superseded && resultHover.hovered
                ToolTip.text: qsTr("Not used: a later attempt is under way")
                ToolTip.delay: 400
            }
        }
        // (The line's colour under them, fading in over the words.)
        Rectangle {
            id: hoverBg
            // (From the first one shown: a faded diff before Copy covers nothing.)
            readonly property real skip: diffButton.visible && diffButton.opacity === 0 && callCopy.visible ? diffButton.width + hoverActions.spacing : 0
            visible: hoverActions.lit && hoverActions.width > hoverActions.leftPadding
            x: hoverActions.x + skip
            width: hoverActions.width - skip
            height: parent.height
            readonly property color line: headHover.hovered && card.canOpen ? Qt.tint(Theme.canvas, Theme.hover) : Theme.canvas
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.rgba(hoverBg.line.r, hoverBg.line.g, hoverBg.line.b, 0) }
                GradientStop { position: Math.min(0.5, hoverActions.leftPadding / Math.max(1, hoverBg.width)); color: hoverBg.line }
                GradientStop { position: 1; color: hoverBg.line }
            }
        }
        // What hovering offers (diff, Copy, Show lines): over the right
        // end of the call's words, on the line's own colour, never in the
        // line's layout, so nothing in it moves (or gives up room) on hover.
        Row {
            id: hoverActions
            readonly property bool lit: diffButton.opacity > 0 || callCopy.opacity > 0 || showLinesLabel.opacity > 0
            anchors.verticalCenter: parent.verticalCenter
            x: head.x + whatLabel.x + whatLabel.width - width
            spacing: 6
            leftPadding: 16
            FlatButton {
                id: diffButton
                anchors.verticalCenter: parent.verticalCenter
                // An edit or a write: its file's diff, in the Changes panel.
                visible: /^(edit|multi_?edit|write|apply_?patch|patch|create)/i.test(card.title) && card.subtitle !== "" && card.rowState !== "error"
                opacity: headHover.hovered || activeFocus ? 1 : 0
                text: "diff"
                font.pixelSize: Theme.px(11)
                padding: 2
                outlined: false
                accent: Theme.info
                onClicked: card.transcript.openDiff(card.subtitle.replace(/…$/, ""))
            }
            // Copy: all of the listing (not just the lines shown), a
            // write's file, an edit's new lines; on hover (or the keyboard).
            CodeCopy {
                id: callCopy
                anchors.verticalCenter: parent.verticalCenter
                // (A folded read offers its lines instead; Ctrl+C still copies.)
                visible: card.codeShown || card.preview
                opacity: cardHover.hovered || copied || activeFocus || card.transcript.keyFocus(headBox) ? 1 : 0
                // (From the call's whole output: the row may keep only part.)
                copyFrom: () => card.code ? card.transcript.session.codeText(card.title, card.command, card.subtitle.replace(/…$/, ""), card.transcript.session.callOutput(card.rowKey))
                                          : card.diff.split("\n").filter(l => !l.startsWith("-")).map(l => l.slice(1)).join("\n")
            }
            // An older read, folded to its line: what a click does.
            Label {
                id: showLinesLabel
                objectName: "showLines"
                anchors.verticalCenter: parent.verticalCenter
                visible: card.isRead && !!card.code && !card.codeShown
                opacity: headHover.hovered || card.transcript.keyFocus(headBox) ? 1 : 0
                text: "Show lines"
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
            }
        }
        HoverHandler { id: headHover; cursorShape: card.canOpen ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { enabled: card.canOpen; onTapped: { card.transcript.clickFocus(headBox); card.toggleOpen() } }
    }
    // The subagents this call started (the task tool), as a tree.
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 22
        visible: card.agents.length > 0
        spacing: 6
        Rectangle { Layout.fillHeight: true; Layout.leftMargin: 4; implicitWidth: 2; radius: 1; color: Theme.border }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            // (By place: the list is read again a few times a second
            // while they run, and its rows stay as they are, updated.)
            Repeater {
                model: card.agents.length
                SubagentRow {
                    required property int index
                    Layout.fillWidth: true
                    a: card.agents[index] || ({})
                    session: card.transcript.session
                    onViewRequested: (agent, steer) => card.transcript.subagentRequested(agent, steer)
                }
            }
        }
    }
    // The approval: its details (reason, command…) and the answer.
    // A command that can't be undone says so, and its confirm names
    // what it does rather than looking like good news.
    readonly property bool destructive: askLines.some(l => l.startsWith("Reason: this can't be undone"))
    Label {
        Layout.fillWidth: true
        Layout.leftMargin: 22
        visible: text !== ""
        text: card.askLines.filter(l => l.startsWith("Reason:")).map(l => {
            const r = l.slice(7).trim()
            return r.charAt(0).toUpperCase() + r.slice(1)
        }).join("\n")
        // (The card is already the warning; only "can't be undone" is news.)
        color: card.destructive ? Theme.error : Theme.textMuted
        font.pixelSize: Theme.px(12)
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.leftMargin: 22
        // (Not the command again when the line above shows it.)
        readonly property string details: card.askLines.filter(l => !l.startsWith("Reason:") && l !== "Outside the sandbox"
            && l.replace(/^Command:\s*/, "").trim() !== card.transcript.local(card.subtitle).replace(/…$/, "").trim()).join("\n").trim()
        visible: card.asking && details !== ""
        implicitHeight: askDetails.implicitHeight + 12
        radius: 4
        color: Theme.codeBg
        Label {
            id: askDetails
            x: 8
            y: 6
            width: parent.width - 16
            text: parent.details
            color: Theme.foreground
            font.family: Theme.mono
            font.pixelSize: Theme.mpx(12)
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 14
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
    }
    // The question, and its answers beside it; under it when they'd
    // leave it too little room.
    GridLayout {
        id: approvalRow
        Layout.fillWidth: true
        Layout.leftMargin: 22
        Layout.topMargin: 2
        visible: card.asking
        columns: approvalQ.implicitWidth + approvalButtons.implicitWidth + columnSpacing > width ? 1 : 2
        columnSpacing: 6
        rowSpacing: 6
        Label {
            id: approvalQ
            objectName: "approvalQuestion"
            Layout.fillWidth: true
            text: card.transcript.approvalQuestion(card.title, card.askLines)
            color: Theme.text
            font.pixelSize: Theme.px(13)
            // (The one thing to read before answering: wrapped, never cut.)
            wrapMode: Text.Wrap
        }
        RowLayout {
        id: approvalButtons
        Layout.alignment: Qt.AlignRight
        spacing: 6
        FlatButton { text: "Deny"; font.pixelSize: Theme.px(12.5); onClicked: card.transcript.session.answerValue("Deny") }
        FlatButton {
            objectName: "alwaysAllowInline"
            visible: card.transcript.session.requestOptions.indexOf("Always allow") >= 0
            text: approvalRow.width < 420 ? "Always" : "Always allow"
            font.pixelSize: Theme.px(12.5)
            ToolTip.visible: hovered
            ToolTip.delay: 400
            ToolTip.text: { try { return "Remember: " + (JSON.parse(card.transcript.session.requestJson).alwaysHint || "this") } catch (e) { return "" } }
            onClicked: card.transcript.session.answerValue("Always allow")
        }
        FlatButton {
            objectName: "approveInline"
            text: card.destructive ? "Run anyway" : "Approve"
            variant: card.destructive ? "danger" : "primary"
            accent: card.destructive ? Theme.error : Theme.foreground
            font.pixelSize: Theme.px(12.5)
            onClicked: card.transcript.session.answerValue("Approve")
        }
        }
    }
    // The well: a write's or an edit's lines; a command (when its line
    // doesn't say it) and its last lines; any call's output, opened.
    Rectangle {
        id: toolWellBox
        objectName: card.preview ? "toolPreview" : "toolWell"
        Layout.fillWidth: true
        Layout.leftMargin: 22
        visible: card.preview || card.commandInWell || card.outInWell || card.codeShown
        // (As tall as it is: a run's box is what scrolls.)
        implicitHeight: wellCol.implicitHeight + 12
        radius: 6
        color: Theme.codeBg
        // Line numbers on a column of their own, from the code's first
        // line (under a command, if any) to the well's bottom.
        readonly property bool gutter: (card.preview && card.codeLines.some(l => l.n !== "")) || (card.codeShown && card.code.lines.some(l => l.n !== ""))
        readonly property bool hasCode: card.preview || card.codeShown
        Rectangle {
            visible: parent.gutter
            // (Under the command's rule: the column is the code's, not the command's.)
            y: card.commandInWell ? 6 + cmdLabel.height + 7 : 0
            width: wellCode.gutterWidth - 6
            height: parent.height - y
            color: Theme.codeGutter
            topLeftRadius: y === 0 ? 6 : 0
            bottomLeftRadius: 6
        }
        Item {
            id: wellFlick
            y: 6
            width: parent.width
            height: parent.height - 12
            clip: true
            Column {
                id: wellCol
                width: wellFlick.width
                spacing: 0
                Label {
                    id: cmdLabel
                    objectName: "toolCommand"
                    visible: card.commandInWell
                    x: 10
                    width: wellCol.width - 20
                    text: "<font color='" + Theme.rgb(Theme.textFaint) + "'>$</font> " + card.shownCommand.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>")
                    textFormat: Text.StyledText
                    color: Theme.foreground
                    // (At a space; a word too long for the line anywhere.)
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    maximumLineCount: card.open ? 400 : 2
                    elide: Text.ElideRight
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
                // The command is the well's header: a rule across the well
                // between it and what it printed.
                Item {
                    objectName: "toolCommandRule"
                    visible: card.commandInWell && (card.outInWell || card.codeShown)
                    width: wellCol.width
                    height: 13
                    Rectangle { y: 6; width: parent.width; height: 1; color: Theme.codeRule }
                }
                // Kept to its end, folded: its last lines, what came
                // before them a click away.
                Label {
                    objectName: "toolEarlierLines"
                    visible: card.fromEnd && card.hidden > 0 && !card.open
                    x: toolWellBox.gutter ? wellCode.gutterWidth : 10
                    bottomPadding: 3
                    // (Of all before it, when only some were kept.)
                    text: "↑ " + card.hidden + (card.before > card.hidden ? " of " + card.before : "") + (Math.max(card.hidden, card.before) === 1 ? " earlier line" : " earlier lines")
                    color: earlierHover.hovered ? Theme.foreground : Theme.muted
                    font.pixelSize: Theme.px(12)
                    Accessible.role: Accessible.Button
                    Accessible.name: "Show " + card.hidden + (card.hidden === 1 ? " earlier line" : " earlier lines")
                    HoverHandler { id: earlierHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: card.toggleOpen() }
                }
                // Kept to its end: what came before isn't here.
                Label {
                    objectName: "toolEarlier"
                    visible: toolWellBox.hasCode && !!card.code && card.code.earlier === true && (card.open || card.hidden <= 0)
                    x: toolWellBox.gutter ? wellCode.gutterWidth : 10
                    topPadding: 4
                    text: "Earlier lines not shown"
                    color: Theme.textFaint
                    font.pixelSize: Theme.px(12)
                }
                CodeText {
                    id: wellCode
                    objectName: card.preview ? "previewCode" : "codeView"
                    visible: toolWellBox.hasCode
                    width: wellCol.width
                    lines: visible ? card.codeLines : []
                    wrap: Theme.wrapCode || card.transcript.listView.wraps[card.rowKey] === true
                    numbers: toolWellBox.gutter
                    signs: card.preview && !card.newFile
                    onOpenAt: (p, line) => card.transcript.openFileAt(p, line)
                }
                // A command's last lines (each one shortened to the well), or
                // all of a call's output, opened.
                Repeater {
                    model: card.tail
                    Label {
                        required property string modelData
                        required property int index
                        objectName: "toolTail"
                        x: 10
                        width: wellCol.width - 20
                        readonly property var row: card.tailGutters ? card.tailGutters[index] : null
                        text: row ? card.transcript.numberedHtml(row, true) : modelData
                        color: Theme.textMuted
                        elide: Text.ElideRight
                        onTruncatedChanged: card.cutLines += truncated ? 1 : -1
                        Component.onDestruction: if (truncated) card.cutLines--
                        textFormat: row ? Text.StyledText : Text.PlainText
                        font.family: Theme.mono
                        font.pixelSize: Theme.mpx(12)
                    }
                }
                TextEdit {
                    visible: card.outInWell && (card.open || !card.shell) && !card.code
                    x: 10
                    width: wellCol.width - 20
                    // (Line numbers at the start of its lines as a gutter.)
                    readonly property var gutters: visible ? card.transcript.numbered(card.text.split("\n")) : null
                    textFormat: gutters ? TextEdit.RichText : TextEdit.PlainText
                    text: !visible ? "" : gutters ? "<p style='margin:0;white-space:pre-wrap;'>" + gutters.map(r => card.transcript.numberedHtml(r, false)).join("<br>") + "</p>" : card.text
                    color: Theme.muted
                    readOnly: true
                    selectByMouse: true
                    // (At a space; a path too long for the line anywhere.)
                    wrapMode: TextEdit.WrapAtWordBoundaryOrAnywhere
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
                // What's left out, a click away; opened, what's past the
                // lines shown, and where the file goes on (a second line
                // when narrow).
                Flow {
                    x: toolWellBox.gutter ? wellCode.gutterWidth : 10
                    width: wellCol.width - x - 10
                    visible: (card.hidden > 0 && (card.open || !card.fromEnd)) || card.beyond !== 0
                    spacing: 2
                    topPadding: 3
                    Label {
                        objectName: "toolMoreLines"
                        visible: card.hidden > 0 && (card.open || !card.fromEnd)
                        rightPadding: 12
                        text: card.open ? "↑ Fewer lines" : "↓ " + card.hidden + (card.hidden === 1 ? " more line" : " more lines")
                        color: moreHover.hovered ? Theme.foreground : Theme.muted
                        font.pixelSize: Theme.px(12)
                        HoverHandler { id: moreHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: card.toggleOpen() }
                    }
                    Label {
                        objectName: "toolBeyond"
                        visible: card.beyond !== 0
                        rightPadding: 12
                        text: (card.beyond > 0 ? card.beyond + (card.beyond === 1 ? " more line" : " more lines") : "More lines")
                              + (card.beyondAt ? "" : " not shown here")
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(12)
                    }
                        Label {
                            objectName: "toolBeyondOpen"
                            visible: !!card.beyondAt
                            text: card.beyondAt ? "Open at line " + card.beyondAt.n : ""
                            color: beyondHover.hovered ? Theme.primary : Theme.muted
                            font.pixelSize: Theme.px(12)
                            font.underline: beyondHover.hovered
                            Accessible.role: Accessible.Link
                            Accessible.name: text
                            HoverHandler { id: beyondHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: card.transcript.openFileAt(card.beyondAt.p, card.beyondAt.n) }
                        }
                }
            }
        }
    }
    PictureStrip {
        Layout.fillWidth: true
        // (Under the step's name, as its output: the run's line left clear.)
        Layout.leftMargin: 22
        Layout.topMargin: 4
        Layout.bottomMargin: 2
        side: 96
        labelOf: p => card.transcript.pictureLabel(p)
        paths: card.images !== "" ? card.images.split("\n") : []
        onOpenRequested: path => card.transcript.showPicture(path)
    }
}
