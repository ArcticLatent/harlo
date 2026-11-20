pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// pi's extensions on this computer or a host: harlo's built-in one (here),
// those installed (on or off, removed), the ones harlo suggests, and
// installing one by its source.
ColumnLayout {
    id: ext
    /// "" this computer, else a host's name.
    property string host: ""
    /// Claude Code's sign-in here (the built-in Claude subscription
    /// extension uses it): "yes", "no", or "" not known.
    property string claudeSignedIn: ""
    /// Who it's signed in as ("me@example.com"), when it is.
    property string claudeAccount: ""
    signal claudeSignInRequested()
    signal claudeSignOutRequested()
    readonly property var st: PiExtensions.revision >= 0 ? JSON.parse(PiExtensions.status(host)) : ({})
    readonly property var packages: st.packages || []
    readonly property var jobs: st.jobs || ({})
    readonly property var bridge: PiExtensions.revision >= 0 ? JSON.parse(PiExtensions.bridgeStatus()) : ({})
    readonly property var suggested: (st.recommended || []).filter(r => !packages.some(p => p.source === r.source))
    property string installError: ""
    spacing: 6
    Component.onCompleted: if (visible) PiExtensions.refresh(host)
    onVisibleChanged: if (visible) PiExtensions.refresh(host)

    function jobOf(source) { return jobs[source] || null }

    Label { text: "Extensions"; color: Theme.foreground; font.pixelSize: Theme.px(12.5); font.weight: Font.Medium }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: "pi loads these in every session it starts" + (ext.host === "" ? "" : " on " + ext.host) + ". They run with your permissions, so install only ones you trust. Changes apply to pi sessions started afterwards."
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }

    // Built in (this computer): the Claude subscription.
    RowLayout {
        objectName: "piBridge"
        visible: ext.host === "" && ext.bridge.state !== "missing"
        Layout.fillWidth: true
        spacing: 8
        AppSwitch {
            objectName: "piBridgeSwitch"
            Layout.alignment: Qt.AlignTop
            checked: ext.bridge.on === true
            enabled: ext.bridge.inPi !== true
            onToggled: PiExtensions.setBridgeEnabled(checked)
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: "Claude subscription  · built in"
                color: ext.bridge.on ? Theme.foreground : Theme.muted
                font.pixelSize: Theme.px(12.5)
            }
            Label {
                objectName: "piBridgeDetail"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: ext.bridge.inPi ? "pi has its own copy installed (below), which it loads instead."
                    : ext.bridge.state === "error" ? ext.bridge.error
                    : ext.bridge.state === "preparing" && ext.bridge.step === "claude" ? "Getting it ready: downloading Claude Code (about 150 MB, once), since this computer has none of its own…"
                    : ext.bridge.state === "preparing" ? "Getting it ready: downloading Anthropic's Agent SDK, once…"
                    : "Claude's models with your Claude plan, through Claude Code's sign-in. pi still runs every tool."
                        + (ext.claudeSignedIn === "no" ? " Claude Code isn't signed in yet."
                           : ext.claudeSignedIn === "yes" && ext.claudeAccount !== "" ? " Signed in as " + ext.claudeAccount + "." : "")
                color: ext.bridge.state === "error" ? Theme.error : Theme.muted
                font.pixelSize: Theme.px(11.5)
            }
            RowLayout {
                spacing: 6
                visible: ext.bridge.state === "error" || (ext.claudeSignedIn !== "" && ext.bridge.on && !ext.bridge.inPi)
                Layout.topMargin: 3
                FlatButton {
                    visible: ext.bridge.state === "error"
                    text: "Try again"
                    padding: 3
                    font.pixelSize: Theme.px(11)
                    onClicked: PiExtensions.prepareBridge()
                }
                FlatButton {
                    objectName: "piBridgeSignIn"
                    visible: ext.claudeSignedIn === "no"
                    text: "Sign in to Claude…"
                    padding: 3
                    font.pixelSize: Theme.px(11)
                    onClicked: ext.claudeSignInRequested()
                }
                FlatButton {
                    objectName: "piBridgeSignOut"
                    visible: ext.claudeSignedIn === "yes"
                    text: "Sign out of Claude"
                    padding: 3
                    font.pixelSize: Theme.px(11)
                    onClicked: ext.claudeSignOutRequested()
                }
            }
        }
    }

    // Installed.
    Repeater {
        model: ext.packages
        RowLayout {
            id: pkg
            required property var modelData
            readonly property var job: ext.jobOf(modelData.source)
            objectName: "piExt-" + modelData.name
            Layout.fillWidth: true
            spacing: 8
            AppSwitch {
                objectName: "piExtSwitch-" + pkg.modelData.name
                checked: pkg.modelData.enabled
                enabled: !(pkg.job && pkg.job.state === "running")
                onToggled: PiExtensions.setEnabled(ext.host, pkg.modelData.source, checked)
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: pkg.modelData.name + (pkg.modelData.version !== "" ? "  " + pkg.modelData.version : "")
                    color: pkg.modelData.enabled ? Theme.foreground : Theme.muted
                    font.pixelSize: Theme.px(12.5)
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    visible: text !== ""
                    text: pkg.job && pkg.job.state !== "done" ? pkg.job.message : pkg.modelData.description
                    color: pkg.job && pkg.job.state === "failed" ? Theme.error : Theme.muted
                    font.pixelSize: Theme.px(11.5)
                }
            }
            FlatButton {
                objectName: "piExtRemove-" + pkg.modelData.name
                text: "Remove"
                padding: 3
                font.pixelSize: Theme.px(11)
                enabled: !(pkg.job && pkg.job.state === "running")
                onClicked: PiExtensions.remove(ext.host, pkg.modelData.source)
            }
        }
    }
    Label {
        visible: ext.st.state === "ready" && ext.packages.length === 0
        text: "None installed yet."
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: ext.st.state === "error"
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: ext.st.error || ""
        color: Theme.error
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: ext.st.state === "loading" && ext.packages.length === 0
        text: "Checking…"
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }

    // Suggested, not installed yet.
    Label {
        visible: ext.suggested.length > 0
        Layout.topMargin: 6
        text: "Suggested"
        color: Theme.foreground
        font.pixelSize: Theme.px(12)
        font.weight: Font.Medium
    }
    Repeater {
        model: ext.suggested
        RowLayout {
            id: rec
            required property var modelData
            readonly property var job: ext.jobOf(modelData.source)
            objectName: "piExtSuggested-" + modelData.source
            Layout.fillWidth: true
            spacing: 8
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label { text: rec.modelData.title; color: Theme.foreground; font.pixelSize: Theme.px(12.5) }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: rec.job ? rec.job.message : rec.modelData.detail
                    color: rec.job && rec.job.state === "failed" ? Theme.error : Theme.muted
                    font.pixelSize: Theme.px(11.5)
                }
            }
            FlatButton {
                objectName: "piExtInstall-" + rec.modelData.source
                text: rec.job && rec.job.state === "running" ? "Installing…" : "Install"
                enabled: !(rec.job && rec.job.state === "running")
                accent: Theme.primary
                padding: 3
                font.pixelSize: Theme.px(11)
                onClicked: ext.installError = PiExtensions.install(ext.host, rec.modelData.source)
            }
        }
    }

    // Any other, by its source.
    RowLayout {
        Layout.fillWidth: true
        Layout.topMargin: 6
        spacing: 8
        AppTextField {
            id: sourceField
            objectName: "piExtSource"
            Layout.fillWidth: true
            placeholderText: "npm:name, git:github.com/owner/repo or a folder"
            font.family: Theme.mono
            font.pixelSize: Theme.px(12)
            onAccepted: installButton.clicked()
        }
        FlatButton {
            id: installButton
            objectName: "piExtInstallSource"
            text: "Install"
            enabled: sourceField.text.trim() !== ""
            font.pixelSize: Theme.px(12)
            onClicked: {
                ext.installError = PiExtensions.install(ext.host, sourceField.text)
                if (ext.installError === "") sourceField.text = ""
            }
        }
    }
    Label {
        visible: ext.installError !== ""
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: ext.installError
        color: Theme.error
        font.pixelSize: Theme.px(12)
    }
    // Typed in: how its install goes, while it runs or when it failed
    // (done, the list shows it).
    Repeater {
        model: Object.keys(ext.jobs).filter(s => ext.jobs[s].state !== "done" && !ext.packages.some(p => p.source === s) && !(ext.st.recommended || []).some(r => r.source === s))
        Label {
            required property string modelData
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: modelData + ": " + ext.jobs[modelData].message
            color: ext.jobs[modelData].state === "failed" ? Theme.error : Theme.muted
            font.pixelSize: Theme.px(12)
        }
    }
}
