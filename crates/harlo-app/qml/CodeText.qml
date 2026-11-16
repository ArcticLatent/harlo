pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Code: one selectable text, its line numbers in a column beside it
// (each where its line is, wrapped or not), some lines tinted (added,
// removed, a grep match), long lines scrolled sideways (the wheel
// sideways, or Shift and the wheel). `lines`: [{ n, h, mark }] (mark:
// add, remove, match, context, file); or `html`, a whole <pre>,
// numbered 1…N.
Item {
    id: ct
    property var lines: null
    property string html: ""
    // Numbers in their own column; and an edit's +/− (`signs`), after
    // its numbers when it has them.
    property bool numbers: true
    property bool signs: false
    // A piece of a long block: its first line's number, and the digits
    // its numbers take at least (the pieces' columns line up).
    property int firstNumber: 1
    property int minDigits: 1
    // A line with a path (`p`) opens its file at that line: its number
    // is a link, and so is a file's name.
    signal openAt(string path, int line)
    property real fontSize: Theme.mpx(12)
    property bool wrap: Theme.wrapCode
    property real topPad: 0
    property real bottomPad: 0
    // (Numbers: 8 px either side in their column, then 6 px to the code.)
    // (Whole pixels, so the code beside it starts on one.)
    readonly property real gutterWidth: numbers ? Math.ceil(probe.implicitWidth) + 22 : signs ? 22 : 10
    // Unwrapped, some lines go past the right edge.
    readonly property bool cut: !wrap && codeFlick.contentWidth > codeFlick.width + 1
    readonly property int count: lines ? lines.length : Math.max(1, starts.length - (edit.getText(Math.max(0, edit.length - 1), edit.length) === "\n" ? 1 : 0))
    readonly property string selectedText: edit.selectedText
    implicitHeight: edit.implicitHeight + topPad + bottomPad
    function number(i) {
        if (!lines) return String(ct.firstNumber + i)
        const l = lines[i]
        if (!l) return ""
        if (!signs) return l.n
        const sign = l.mark === "add" ? "+" : l.mark === "remove" ? "−" : ""
        // (A removed line has no number: its sign under the others'.)
        return numbers ? (l.n || "") + "\u00a0" + (sign || "\u00a0") : sign
    }
    // Wrapped lines: each its own block, hanging from its indentation.
    FontMetrics { id: monoMetrics; font.family: Theme.mono; font.pixelSize: ct.fontSize }
    function htmlLines(h) {
        const inner = h.replace(/^<pre[^>]*>/, "").replace(/<\/pre>\s*$/, "")
        const out = inner.split("\n")
        if (out.length > 1 && out[out.length - 1] === "") out.pop()
        return out
    }
    function hanging(lines) {
        const ch = monoMetrics.advanceWidth(" ")
        return lines.map(h => {
            const plain = h.replace(/<[^>]*>/g, "")
            const lead = /^[ \t]*/.exec(plain)[0].replace(/\t/g, "    ").length
            const hang = Math.round((lead + 2) * ch)
            // (Each its own <pre>: a <p> lets the font join "=>" into an arrow.)
            return "<pre style=\"margin:0; margin-left:" + hang + "px; text-indent:-" + hang + "px; white-space:pre-wrap;\">" + (h === "" ? " " : h) + "</pre>"
        }).join("")
    }
    Label {
        id: probe
        visible: false
        text: ct.lines ? ct.lines.reduce((w, l) => String(l.n).length > w.length ? String(l.n) : w, "0") + (ct.signs ? "\u00a0+" : "") : String(ct.firstNumber + ct.count - 1).padStart(ct.minDigits, "0")
        font.family: Theme.mono
        font.pixelSize: ct.fontSize
    }
    // Where each line starts in the text, and (once laid out) its top.
    readonly property var starts: {
        const t = edit.getText(0, edit.length)
        const s = [0]
        for (let i = 0; i < t.length; i++) {
            const c = t.charCodeAt(i)
            if (c === 10 || c === 0x2028 || c === 0x2029) s.push(i + 1)
        }
        return s
    }
    property int laidOut: 0
    // (Clamped: for a moment after the text changes, the starts can
    // still be the old text's, and Qt warns of a position past its end.)
    function lineTop(i) { return laidOut >= 0 && i < starts.length ? edit.positionToRectangle(Math.min(starts[i], edit.length)).y : edit.contentHeight }
    function lineHeight(i) { return laidOut >= 0 ? (i + 1 < starts.length ? lineTop(i + 1) : edit.contentHeight) - lineTop(i) : 0 }
    // (A wrapped line's first row: where its number goes.)
    function rowHeight(i) { return laidOut >= 0 && i < starts.length ? edit.positionToRectangle(Math.min(starts[i], edit.length)).height : 0 }
    // Tints: an added or removed line, a grep match among its context.
    Repeater {
        model: ct.lines ? ct.lines.length : 0
        Rectangle {
            required property int index
            readonly property string mark: ct.lines[index].mark || ""
            visible: mark === "add" || mark === "remove" || mark === "match"
            y: ct.topPad + ct.lineTop(index)
            width: ct.width
            height: ct.lineHeight(index)
            color: mark === "add" ? Qt.rgba(Theme.diffAdd.r, Theme.diffAdd.g, Theme.diffAdd.b, 0.10)
                 : mark === "remove" ? Qt.rgba(Theme.diffRemove.r, Theme.diffRemove.g, Theme.diffRemove.b, 0.10)
                 : Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.07)
        }
    }
    Repeater {
        model: ct.numbers || ct.signs ? ct.count : 0
        // (Plain text: a number that opens its file, a listing's, gets
        // its pointer, tooltip and link role from the loader in it.)
        Text {
            id: numberText
            required property int index
            readonly property string mark: ct.lines && ct.lines[index] ? ct.lines[index].mark || "" : ""
            // (Its line's number, without an edit's sign.)
            readonly property string num: ct.lines && ct.lines[index] ? String(ct.lines[index].n || "") : String(index + 1)
            readonly property bool opens: !!(ct.lines && ct.lines[index] && ct.lines[index].p) && /^\d+$/.test(num)
            readonly property bool hot: opens && numberLink.item !== null && numberLink.item.hovered
            objectName: opens ? "codeNumber" : ""
            x: 0
            y: ct.topPad + ct.lineTop(index)
            width: ct.gutterWidth - 14
            height: ct.rowHeight(index)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignRight
            text: ct.number(index)
            textFormat: Text.PlainText
            color: hot ? Theme.primary : mark === "add" ? Theme.diffAdd : mark === "remove" ? Theme.diffRemove : mark === "match" ? Theme.textMuted : Theme.textFaint
            font.family: Theme.mono
            font.pixelSize: ct.fontSize
            font.underline: hot
            Loader {
                id: numberLink
                anchors.fill: parent
                active: numberText.opens
                sourceComponent: Item {
                    id: numberTarget
                    readonly property bool hovered: numberHover.hovered
                    // (Read safely: the lines can change before the
                    // loader lets this go.)
                    readonly property string linePath: ct.lines && ct.lines[numberText.index] ? ct.lines[numberText.index].p || "" : ""
                    HoverHandler { id: numberHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: if (numberTarget.linePath !== "") ct.openAt(numberTarget.linePath, Number(numberText.num)) }
                    Accessible.role: Accessible.Link
                    Accessible.name: "Open " + linePath.split("/").pop() + " at line " + numberText.num
                    ToolTip.visible: numberHover.hovered
                    ToolTip.text: "Open at line " + numberText.num
                    ToolTip.delay: 600
                }
            }
        }
    }
    // The text; the wheel sideways moves it (up and down goes on to the
    // list around it).
    MouseArea {
        x: ct.gutterWidth
        y: ct.topPad
        width: ct.width - x
        height: edit.implicitHeight
        acceptedButtons: Qt.NoButton
        onWheel: wheel => {
            const along = wheel.pixelDelta.x !== 0 ? wheel.pixelDelta.x : wheel.angleDelta.x !== 0 ? wheel.angleDelta.x / 2.5
                        : (wheel.modifiers & Qt.ShiftModifier) ? (wheel.pixelDelta.y !== 0 ? wheel.pixelDelta.y : wheel.angleDelta.y / 2.5) : 0
            if (along === 0 || codeFlick.contentWidth <= codeFlick.width) { wheel.accepted = false; return }
            codeFlick.contentX = Math.max(0, Math.min(codeFlick.contentWidth - codeFlick.width, codeFlick.contentX - along))
        }
        Flickable {
            id: codeFlick
            objectName: "codeScroll"
            anchors.fill: parent
            contentWidth: ct.wrap ? width : edit.implicitWidth + 12
            contentHeight: height
            clip: true
            // (A drag selects; the wheel or the bar scrolls.)
            interactive: false
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.horizontal: ScrollBar { policy: codeFlick.contentWidth > codeFlick.width ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            TextEdit {
                id: edit
                objectName: "codeText"
                width: ct.wrap ? codeFlick.width - 10 : implicitWidth
                textFormat: TextEdit.RichText
                // (Unwrapped, one <pre>. Wrapped, a block per line, its
                // rows after the first under its indentation and 2 more,
                // so the rest of a line never reads as a line of its own.)
                text: Theme.htmlStyle + (!ct.wrap ? (ct.lines ? "<pre style=\"margin:0;\">" + ct.lines.map(l => l.h === "" ? " " : l.h).join("\n") + "</pre>" : ct.html)
                                                 : ct.hanging(ct.lines ? ct.lines.map(l => l.h) : ct.htmlLines(ct.html)))
                color: Theme.foreground
                selectionColor: Theme.selection
                selectedTextColor: Theme.foreground
                readOnly: true
                selectByMouse: true
                // (At a space where there is one; a long word breaks anywhere.)
                wrapMode: ct.wrap ? TextEdit.Wrap : TextEdit.NoWrap
                font.family: Theme.mono
                font.pixelSize: ct.fontSize
                // (A file's name in it: a link to the file.)
                onLinkActivated: link => {
                    const m = /^harlo-open:(\d+):(.*)$/.exec(link)
                    if (m) ct.openAt(decodeURIComponent(m[2]), Number(m[1]))
                }
                HoverHandler { cursorShape: edit.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.IBeamCursor }
                // (Lines move as the text lays out again: their numbers with
                // them. The box can be gone by then, its row scrolled away.)
                onContentHeightChanged: Qt.callLater(() => { if (ct) ct.laidOut++ })
                onWidthChanged: Qt.callLater(() => { if (ct) ct.laidOut++ })
                onTextChanged: Qt.callLater(() => { if (ct) ct.laidOut++ })
            }
        }
        // More to the side: that edge fades out.
        Rectangle {
            visible: codeFlick.contentX > 1
            width: 18
            height: parent.height
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Theme.codeBg }
                GradientStop { position: 1; color: Qt.rgba(Theme.codeBg.r, Theme.codeBg.g, Theme.codeBg.b, 0) }
            }
        }
        Rectangle {
            objectName: "codeMoreRight"
            visible: codeFlick.contentX + codeFlick.width < codeFlick.contentWidth - 1
            x: parent.width - width
            width: 28
            height: parent.height
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.rgba(Theme.codeBg.r, Theme.codeBg.g, Theme.codeBg.b, 0) }
                GradientStop { position: 1; color: Theme.codeBg }
            }
        }
    }
}
