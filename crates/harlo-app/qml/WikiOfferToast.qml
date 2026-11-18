pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Merged work on a project's default branch left its wiki owing the code
// (new declarations no page mentions, cited code that changed): offer the
// update in the corner (it spends the agent's tokens, so it waits for
// you). Not now holds until the branch moves again.
ToastStack {
    id: root
    entries: Workspace.wikiOffersJson !== "" ? JSON.parse(Workspace.wikiOffersJson) : []
    keyOf: o => String(o.project)

    /// "post", "post and close", "post and 3 more".
    function namesText(names) {
        const short = n => String(n).split(/::|\./).pop()
        if (names.length === 1) return short(names[0])
        if (names.length === 2) return short(names[0]) + " and " + short(names[1])
        return short(names[0]) + " and " + (names.length - 1) + " more"
    }

    card: ToastCard {
        id: toast
        stack: root
        namePrefix: "wikiOffer-"
        iconName: "book-open"
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: "Merged work isn't in " + (toast.entry.name !== "" ? toast.entry.name + "'s" : "the") + " wiki yet"
            color: Theme.foreground
            font.pixelSize: Theme.px(13)
            font.weight: Font.DemiBold
        }
        Label {
            objectName: "wikiOfferText"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: (toast.entry.commits === 1 ? "A commit" : toast.entry.commits + " commits") + " on " + toast.entry.branch
                + ((toast.entry.new || []).length > 0 ? " added " + root.namesText(toast.entry.new) + ", which no page mentions" : " changed code the wiki cites")
                + ". An update deals with " + (toast.entry.owed === 1 ? "that" : "the " + toast.entry.owed + " things it owes") + " in a session you can watch."
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
