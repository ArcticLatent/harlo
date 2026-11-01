import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// pi's packages (Settings → Providers → Pi, and a project's settings):
// what's installed, with what each adds, and installing, updating and
// removing them with pi's own commands.
ColumnLayout {
    id: root
    /// The project ("" for yours).
    property string folder: ""
    /// Its own packages (`pi install -l`), not yours.
    property bool project: false
    readonly property var st: Accounts.revision >= 0 ? JSON.parse(Accounts.packages(folder)) : ({ state: "loading", list: [] })
    readonly property var list: (st.list || []).filter(p => p.project === root.project)
    readonly property var job: st.job || null
    readonly property bool busy: job !== null && job.state === "running"
    property string confirming: ""
    spacing: 6
    Component.onCompleted: Accounts.loadPackages(folder)
    onFolderChanged: Accounts.loadPackages(folder)

    function addsText(adds) {
        const words = { extensions: "extensions", skills: "skills", prompts: "prompt templates", themes: "themes" }
        return adds.map(a => words[a] || a).join(", ")
    }

    Repeater {
        model: root.list
        RowLayout {
            id: pkg
            required property var modelData
            readonly property bool working: root.busy && root.job.source === modelData.source
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "code-box"; color: Theme.muted; implicitWidth: 13; implicitHeight: 13; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label {
                    Layout.fillWidth: true
                    text: pkg.modelData.source + (pkg.modelData.version !== "" ? "  " + pkg.modelData.version : "")
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12.5)
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    visible: text !== ""
                    text: [pkg.modelData.description, pkg.modelData.adds.length > 0 ? "Adds " + root.addsText(pkg.modelData.adds) : ""].filter(t => t !== "").join(" · ")
                    color: Theme.muted
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
            }
            Label { visible: pkg.working; text: root.job && root.job.action === "remove" ? "Removing…" : "Updating…"; color: Theme.muted; font.pixelSize: Theme.px(12) }
            FlatButton {
                visible: !pkg.working
                enabled: !root.busy
                text: "Update"
                padding: 3
                font.pixelSize: Theme.px(11)
                onClicked: Accounts.changePackage(root.folder, "update", pkg.modelData.source, root.project)
            }
            FlatButton {
                objectName: "piPackageRemove-" + pkg.modelData.source
                visible: !pkg.working
                enabled: !root.busy
                text: root.confirming === pkg.modelData.source ? "Remove it?" : "Remove"
                accent: root.confirming === pkg.modelData.source ? Theme.error : Theme.foreground
                padding: 3
                font.pixelSize: Theme.px(11)
                onClicked: {
                    if (root.confirming !== pkg.modelData.source) { root.confirming = pkg.modelData.source; return }
                    root.confirming = ""
                    Accounts.changePackage(root.folder, "remove", pkg.modelData.source, root.project)
                }
            }
        }
    }
    Label {
        visible: root.st.state === "ready" && root.list.length === 0 && (root.st.error || "") === ""
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.project ? "This project has no pi packages of its own." : "No packages installed."
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: (root.st.error || "") !== "" || (root.job !== null && root.job.state === "failed")
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.job !== null && root.job.state === "failed" ? "Couldn't " + root.job.action + " " + (root.job.source === "all" ? "the packages" : root.job.source) + ": " + root.job.message : (root.st.error || "")
        color: Theme.error
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: root.job !== null && root.job.state === "done"
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.job ? root.job.message : ""
        color: Theme.success
        font.pixelSize: Theme.px(12)
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        AppTextField {
            id: source
            objectName: "piPackageSource" + (root.project ? "Project" : "")
            Layout.fillWidth: true
            enabled: !root.busy
            placeholderText: "npm:package-name, git:github.com/user/repo, a URL or a folder"
            font.family: Theme.mono
            font.pixelSize: Theme.px(12)
            onAccepted: install.clicked()
        }
        FlatButton {
            id: install
            objectName: "piPackageInstall" + (root.project ? "Project" : "")
            text: root.busy && root.job.action === "install" ? "Installing…" : "Install"
            enabled: !root.busy && source.text.trim() !== ""
            accent: Theme.primary
            font.pixelSize: Theme.px(12)
            onClicked: { Accounts.changePackage(root.folder, "install", source.text, root.project); source.text = "" }
        }
        FlatButton {
            visible: !root.project && root.list.length > 1
            enabled: !root.busy
            text: root.busy && root.job.source === "all" ? "Updating…" : "Update all"
            font.pixelSize: Theme.px(12)
            onClicked: Accounts.changePackage(root.folder, "update", "all", root.project)
        }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: (root.project
            ? "Installed for this project only (its .pi/settings.json), for everyone who works on it with pi. "
            : "Extensions, skills, prompt templates and themes for pi, from npm, git or a folder. A package runs with your permissions: install only what you trust. ")
            + "New pi sessions pick changes up; open ones keep what they started with."
        color: Theme.muted
        font.pixelSize: Theme.px(11.5)
    }
}
