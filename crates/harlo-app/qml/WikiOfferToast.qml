pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Merged work on a project's default branch changes code its wiki
// describes: offer the update in the corner (it spends the agent's
// tokens, so it waits for you). Not now holds until the branch moves again.
ToastStack {
    id: root
    entries: Workspace.wikiOffersJson !== "" ? JSON.parse(Workspace.wikiOffersJson) : []
    keyOf: o => String(o.project)

    /// "src/a.rs", "src/a.rs and src/b.rs", "src/a.rs and 3 more files".
    function filesText(files) {
        const name = f => f.split("/").pop()
        if (files.length === 1) return name(files[0])
        if (files.length === 2) return name(files[0]) + " and " + name(files[1])
        return name(files[0]) + " and " + (files.length - 1) + " more files"
    }

    card: ToastCard {
        id: toast
        stack: root
        namePrefix: "wikiOffer-"
        iconName: "book-open"
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: "Merged work changes what " + (toast.entry.name !== "" ? toast.entry.name + "'s" : "the") + " wiki describes"
            color: Theme.foreground
            font.pixelSize: Theme.px(13)
            font.weight: Font.DemiBold
        }
        Label {
            objectName: "wikiOfferText"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: (toast.entry.commits === 1 ? "A commit" : toast.entry.commits + " commits") + " on " + toast.entry.branch
                + " since its last update change " + root.filesText(toast.entry.files)
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
                    const why = Workspace.takeWikiOffer(toast.entry.project)
                    if (why !== "") Workspace.showNotice(why, true)
                }
            }
            FlatButton {
                objectName: "wikiOfferLater"
                text: "Not now"
                outlined: false
                font.pixelSize: Theme.px(12)
                onClicked: Workspace.declineWikiOffer(toast.entry.project)
            }
        }
    }
}
