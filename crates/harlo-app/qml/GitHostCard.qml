import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A git hosting service (Settings → Git, the setup guide): who git is signed
// in as there, and signing in (its tool's one-time code, or an access token)
// and out. `chosen`: the default service (picked with "Use by default").
Rectangle {
    id: card
    property string service
    property bool chosen: false
    signal pick()
    objectName: "gitHost-" + service

    readonly property var st: Setup.revision >= 0 ? JSON.parse(Setup.gitStatus(service)) : ({})
    readonly property var flow: Setup.gitFlow !== "" ? JSON.parse(Setup.gitFlow) : null
    readonly property bool mine: flow !== null && flow.service === service
    readonly property bool running: mine && flow.state === "running"
    readonly property bool signedIn: (st.account || "") !== ""
    readonly property bool forgejo: service === "forgejo"
    readonly property bool cliMissing: !forgejo && st.state === "ready" && st.cliFound === false
    // (Forgejo signs in with a token; the others can too.)
    property bool tokenOpen: forgejo
    readonly property string toolName: service === "github" ? "the GitHub CLI (gh)" : "glab, GitLab's CLI"
    readonly property string installUrl: service === "github" ? "https://cli.github.com" : "https://gitlab.com/gitlab-org/cli#installation"
    readonly property string tokenUrl: service === "github" ? "https://github.com/settings/tokens/new?scopes=repo,read:org,workflow&description=harlo"
        : service === "gitlab" ? "https://" + (st.host || "gitlab.com") + "/-/user_settings/personal_access_tokens?name=harlo&scopes=api,write_repository"
        : "https://" + (st.host || "codeberg.org") + "/user/settings/applications"

    implicitHeight: col.implicitHeight + 28
    radius: 12
    color: Theme.raised
    border.color: chosen ? Theme.accent : Theme.hairline
    border.width: chosen ? 2 : 1

    // Copies through a hidden TextEdit (QML has no clipboard API).
    TextEdit {
        id: clip
        visible: false
        function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" }
    }

    ColumnLayout {
        id: col
        x: 14
        y: 14
        width: parent.width - 28
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Rectangle {
                implicitWidth: 34
                implicitHeight: 34
                radius: 9
                color: Theme.controlFill
                Icon { anchors.centerIn: parent; name: "git-repository"; color: card.chosen ? Theme.accent : Theme.textMuted; implicitWidth: 17; implicitHeight: 17 }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label { text: card.st.label || card.service; color: Theme.text; font.pixelSize: Theme.px(14); font.weight: Font.DemiBold }
                Label {
                    objectName: "gitHostStatus-" + card.service
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.px(12.5)
                    color: card.signedIn ? Theme.success : card.cliMissing ? Theme.warning : Theme.textMuted
                    text: card.st.state === "checking" ? "Checking…"
                        : card.signedIn ? "Signed in to " + card.st.host + " as " + card.st.account
                        : card.cliMissing ? "Signing in needs " + card.toolName + "."
                        : "Not signed in to " + (card.st.host || "")
                }
            }
            Label {
                visible: card.chosen
                text: "Default"
                color: Theme.accent
                font.pixelSize: Theme.px(12)
                font.weight: Font.Medium
            }
            FlatButton {
                objectName: "gitHostUse-" + card.service
                visible: !card.chosen
                text: "Use by default"
                font.pixelSize: Theme.px(12)
                onClicked: card.pick()
            }
        }

        // GitLab's and Forgejo's server (self-hosted ones too).
        RowLayout {
            visible: card.service !== "github" && !card.signedIn
            Layout.fillWidth: true
            spacing: 8
            Label { text: "Server"; color: Theme.textMuted; font.pixelSize: Theme.px(12.5) }
            AppTextField {
                objectName: "gitHostServer-" + card.service
                Layout.fillWidth: true
                text: Workspace.setting(card.service + "_host")
                placeholderText: card.forgejo ? "codeberg.org" : "gitlab.com"
                font.family: Theme.mono
                font.pixelSize: Theme.px(12.5)
                onEditingFinished: { Workspace.setSetting(card.service + "_host", text.trim()); Setup.refreshGit() }
            }
        }
        Label {
            visible: card.forgejo && !card.signedIn
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.textFaint
            font.pixelSize: Theme.px(11.5)
            text: "Your own server too: its web address with the port, e.g. git.company.lan:3000, or http://forgejo.lan:3000 when it has no TLS. A company certificate authority installed on this computer is trusted."
        }

        // Signed in: signing out.
        RowLayout {
            visible: card.signedIn && !card.running
            spacing: 8
            FlatButton {
                objectName: "gitHostSignOut-" + card.service
                text: "Sign out"
                font.pixelSize: Theme.px(12)
                onClicked: Setup.gitSignOut(card.service)
            }
            Label {
                visible: (card.st.error || "") !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: card.st.error || ""
                color: Theme.error
                font.pixelSize: Theme.px(12)
            }
        }

        // Not signed in: its tool's sign-in, or a token.
        RowLayout {
            visible: !card.signedIn && !card.running && card.st.state === "ready"
            spacing: 8
            FlatButton {
                objectName: "gitHostSignIn-" + card.service
                visible: !card.forgejo && !card.cliMissing
                text: "Sign in with the browser"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                onClicked: Setup.gitSignIn(card.service)
            }
            FlatButton {
                visible: card.cliMissing
                text: "How to install it"
                font.pixelSize: Theme.px(12)
                onClicked: Qt.openUrlExternally(card.installUrl)
            }
            FlatButton {
                visible: !card.forgejo && !card.cliMissing
                text: card.tokenOpen ? "Hide the token" : "Use a token"
                outlined: false
                font.pixelSize: Theme.px(12)
                onClicked: card.tokenOpen = !card.tokenOpen
            }
            Item { Layout.fillWidth: true }
            Button {
                visible: card.tokenOpen && !card.cliMissing
                flat: true
                padding: 2
                contentItem: Label { text: "Make a token…"; color: Theme.accent; font.pixelSize: Theme.px(12); font.underline: true }
                background: null
                onClicked: Qt.openUrlExternally(card.tokenUrl)
            }
        }
        RowLayout {
            visible: card.tokenOpen && !card.signedIn && !card.running && !card.cliMissing
            Layout.fillWidth: true
            spacing: 8
            AppTextField {
                id: tokenField
                objectName: "gitHostToken-" + card.service
                Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: card.forgejo ? "Access token (read:user and repository: write)" : "Access token"
                onAccepted: tokenButton.clicked()
            }
            FlatButton {
                id: tokenButton
                text: "Sign in"
                accent: Theme.primary
                enabled: tokenField.text.trim() !== ""
                font.pixelSize: Theme.px(12)
                onClicked: { Setup.gitSignInWithToken(card.service, tokenField.text); tokenField.text = "" }
            }
        }

        // The sign-in going on: its one-time code and page, or how it went.
        ColumnLayout {
            visible: card.mine
            Layout.fillWidth: true
            spacing: 8
            Label {
                visible: card.running && (card.flow.code || "") === ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: card.forgejo || card.tokenOpen ? "Checking the token…" : "Starting " + card.toolName + "…"
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
            }
            ColumnLayout {
                visible: card.running && (card.flow.code || "") !== ""
                Layout.fillWidth: true
                spacing: 6
                Label { text: "Enter this code on " + (card.st.host || "the page") + ", then approve git there:"; color: Theme.text; font.pixelSize: Theme.px(12.5); Layout.fillWidth: true; wrapMode: Text.Wrap }
                RowLayout {
                    spacing: 10
                    Label {
                        objectName: "gitHostCode-" + card.service
                        text: card.flow ? card.flow.code || "" : ""
                        color: Theme.text
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(22)
                        font.bold: true
                    }
                    FlatButton { text: "Copy"; font.pixelSize: Theme.px(12); onClicked: clip.copyText(card.flow.code) }
                    FlatButton {
                        visible: card.flow !== null && (card.flow.url || "") !== ""
                        text: "Copy and open the page"
                        accent: Theme.primary
                        font.pixelSize: Theme.px(12)
                        onClicked: { clip.copyText(card.flow.code); Qt.openUrlExternally(card.flow.url) }
                    }
                }
            }
            RowLayout {
                visible: card.running
                spacing: 8
                BusyIndicator { running: card.running; implicitWidth: 16; implicitHeight: 16 }
                Label { text: "Waiting for " + (card.st.host || "it") + "…"; color: Theme.textMuted; font.pixelSize: Theme.px(12) }
                FlatButton { text: "Cancel"; outlined: false; font.pixelSize: Theme.px(12); onClicked: Setup.gitCancel() }
            }
            Label {
                visible: card.mine && !card.running && (card.flow.message || "") !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: card.flow ? card.flow.message || "" : ""
                color: card.flow && card.flow.state === "done" ? Theme.success : Theme.error
                font.pixelSize: Theme.px(12.5)
            }
        }
    }
}
