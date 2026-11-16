import QtQuick
import harlo

// Notices in a corner, one card per entry (a ToastCard as `card`): kept by
// key as the entries change, so each comes in once (sliding in from the
// side) and one that's gone fades out before it leaves the column.
Column {
    id: stack
    spacing: 8
    width: 380
    /// The entries shown, newest last.
    property var entries: []
    /// An entry's key (a string).
    property var keyOf: e => String(e.key)
    /// The card made for each entry (its `json`, `key` and `leaving` come
    /// from here; it calls `remove` once it has faded out).
    property alias card: cards.delegate

    add: Transition {
        NumberAnimation { property: "x"; from: 48; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion }
    }
    move: Transition {
        NumberAnimation { property: "y"; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    }

    ListModel { id: shown }
    function sync() {
        const keys = stack.entries.map(e => stack.keyOf(e))
        for (let i = 0; i < shown.count; i++)
            if (keys.indexOf(shown.get(i).key) < 0) shown.setProperty(i, "leaving", true)
        for (const e of stack.entries) {
            const key = stack.keyOf(e)
            let i = 0
            while (i < shown.count && shown.get(i).key !== key) i++
            if (i < shown.count) shown.set(i, { json: JSON.stringify(e), leaving: false })
            else shown.append({ key: key, json: JSON.stringify(e), leaving: false })
        }
    }
    function remove(key: string) {
        for (let i = 0; i < shown.count; i++)
            if (shown.get(i).key === key && shown.get(i).leaving) { shown.remove(i); return }
    }
    onEntriesChanged: sync()
    Component.onCompleted: sync()

    Repeater {
        id: cards
        model: shown
    }
}
