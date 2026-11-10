import QtQuick
import harlo

// Puts a long text into a TextEdit a slice at a time. With the syntax
// highlighter already on the TextEdit, each slice is coloured as it
// arrives. Colouring a whole laid-out file at once held the window still
// (5 s for both sides of an 8000-line diff); sliced it takes about 1 s in
// all, the first screenful at once, and the window answers in between.
Timer {
    id: fill
    property var editor: null
    /// Lines a slice.
    property int slice: 300
    property var lines: []
    property int at: 0
    /// The whole text is in.
    signal done()
    // (0 never fires: a QML Timer runs on the animation clock.)
    interval: 1
    repeat: true

    function put(text) {
        stop()
        const all = text.split("\n")
        if (all.length <= 2 * slice) {
            lines = []
            editor.text = text
            done()
            return
        }
        lines = all
        at = slice
        editor.text = all.slice(0, slice).join("\n")
        start()
    }
    onTriggered: {
        TextOps.append(editor.textDocument, "\n" + lines.slice(at, at + slice).join("\n"))
        at += slice
        if (at >= lines.length) {
            stop()
            lines = []
            done()
        }
    }
}
