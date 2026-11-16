import QtQuick
import QtQuick.Controls.Basic
import harlo

// A description read at a glance: its first sentence, and "More" for
// the rest (how, exceptions, where it applies). `more` gives the rest
// outright; otherwise a long text splits after its first sentence.
Label {
    id: moreText
    // (Its first sentence, and the rest.)
    function splitLead(t) {
        t = String(t || "")
        if (t.length <= 110) return [t, ""]
        for (let i = 20; i < t.length - 2; i++) {
            const c = t[i]
            if ((c === "." || c === "?" || c === "!" || c === ")") && t[i + 1] === " " && (/[A-Z(]/.test(t[i + 2]) || t.startsWith("harlo", i + 2))) {
                if (c === ")" && t[i - 1] !== ".") continue
                if (/(e\.g|i\.e|etc|vs)\.$/.test(t.slice(Math.max(0, i - 4), i + 1))) continue
                return [t.slice(0, i + 1), t.slice(i + 2)]
            }
        }
        return [t, ""]
    }
    function escapeHtml(t) { return String(t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\n/g, "<br>") }
    property string full
    property string more: ""
    property bool open: false
    // (A short rest is shown whole: a link for a few words costs more
    // than the words.)
    readonly property var split: more !== "" ? [full, more] : moreText.splitLead(full)
    readonly property var parts: split[1] !== "" && split[1].length < 70 ? [split[0] + " " + split[1], ""] : split
    // (The words as shown, for tests and copying.)
    readonly property string plain: open && parts[1] !== "" ? parts[0] + " " + parts[1] : parts[0]
    // More reads as a quiet word (Text, Medium), underlined on hover.
    textFormat: Text.RichText
    text: moreText.escapeHtml(plain) + (parts[1] === "" ? "" : "&nbsp;&nbsp;<a href=\"more\" style=\"color:" + Theme.text + "; text-decoration:" + (hoveredLink !== "" ? "underline" : "none") + "; font-weight:500\">" + (open ? "Less" : "More") + "</a>")
    wrapMode: Text.Wrap
    onLinkActivated: open = !open
    HoverHandler { cursorShape: moreText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
