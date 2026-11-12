import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Merged work on a project's default branch changes code its wiki
// describes: offer the update in the corner (it spends the agent's
// tokens, so it waits for you). Not now holds until the branch moves again.
Column {
    id: root
    spacing: 8
    width: 380
    readonly property var offers: Workspace.wikiOffersJson !== "" ? JSON.parse(Workspace.wikiOffersJson) : []
    // A notice slides in from the side, and fades out as it goes.
    add: Transition {
        NumberAnimation { property: "x"; from: 48; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion }
    }
    move: Transition {
        NumberAnimation { property: "y"; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    }

    // The notices shown, by project: kept as they change, so one that's
    // gone (taken, or Not now) fades out first.
    ListModel { id: shown }
    function sync() {
        const keys = root.offers.map(o => String(o.project))
        for (let i = 0; i < shown.count; i++)
            if (keys.indexOf(shown.get(i).key) < 0) shown.setProperty(i, "leaving", true)
        for (const o of root.offers) {
            let i = 0
            while (i < shown.count && shown.get(i).key !== String(o.project)) i++
            if (i < shown.count) shown.set(i, { json: JSON.stringify(o), leaving: false })
            else shown.append({ key: String(o.project), json: JSON.stringify(o), leaving: false })
        }
    }
    function remove(key) {
        for (let i = 0; i < shown.count; i++)
            if (shown.get(i).key === key && shown.get(i).leaving) { shown.remove(i); return }
    }
    onOffersChanged: sync()
    Component.onCompleted: sync()

    /// "src/a.rs", "src/a.rs and src/b.rs", "src/a.rs and 3 more files".
    function filesText(files) {
        const name = f => f.split("/").pop()
        if (files.length === 1) return name(files[0])
        if (files.length === 2) return name(files[0]) + " and " + name(files[1])
        return name(files[0]) + " and " + (files.length - 1) + " more files"
    }

    Repeater {
        model: shown
        Rectangle {
            id: toast
            required property string json
            required property string key
            required property bool leaving
            readonly property var o: JSON.parse(json)
            objectName: leaving ? "" : "wikiOffer-" + o.project
            opacity: leaving ? 0 : 1
            Behavior on opacity {
                enabled: Theme.paneMotion > 0
                NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic; onRunningChanged: if (!running && toast.leaving) root.remove(toast.key) }
            }
            onLeavingChanged: if (leaving && Theme.paneMotion === 0) Qt.callLater(root.remove, key)
            transform: Translate { x: toast.leaving ? 32 : 0; Behavior on x { NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic } } }
            width: root.width
            implicitHeight: body.implicitHeight + 24
            radius: 10
            color: Theme.elevated
            border.color: Theme.border
            // A soft shadow under the card.
            Rectangle { z: -1; anchors.fill: parent; anchors.topMargin: 3; anchors.leftMargin: -1; anchors.rightMargin: -1; anchors.bottomMargin: -4; radius: 12; color: "#000000"; opacity: 0.18 }

            RowLayout {
                id: body
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 12
                Icon { Layout.alignment: Qt.AlignTop; Layout.topMargin: 2; name: "book-open"; color: Theme.primary; implicitWidth: 18; implicitHeight: 18 }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "Merged work changes what " + (toast.o.name !== "" ? toast.o.name + "'s" : "the") + " wiki describes"
                        color: Theme.foreground
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.DemiBold
                    }
                    Label {
                        objectName: "wikiOfferText"
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: (toast.o.commits === 1 ? "A commit" : toast.o.commits + " commits") + " on " + toast.o.branch
                            + " since its last update change " + root.filesText(toast.o.files)
                            + ". An update redoes only the pages about them, in a session you can watch."
                        color: Theme.muted
                        font.pixelSize: Theme.px(12)
                    }
                    RowLayout {
                        spacing: 8
                        FlatButton {
                            objectName: "wikiOfferUpdate"
                            text: "Update the wiki"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            onClicked: {
                                const why = Workspace.takeWikiOffer(toast.o.project)
                                if (why !== "") Workspace.showNotice(why, true)
                            }
                        }
                        FlatButton {
                            objectName: "wikiOfferLater"
                            text: "Not now"
                            outlined: false
                            font.pixelSize: Theme.px(12)
                            onClicked: Workspace.declineWikiOffer(toast.o.project)
                        }
                    }
                }
            }
        }
    }
}
