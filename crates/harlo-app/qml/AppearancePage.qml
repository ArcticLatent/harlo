pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Appearance: theme, glass, fonts, sizes, motion, the prompt rail.
ColumnLayout {
    id: appearance
    property bool compact
    signal promptRailToggled(bool on)
    signal rowRequested(string page, string text)
    spacing: 0
    readonly property var families: Workspace.themeFamilies().map(l => {
        const p = l.split("\t")
        return { name: p[0], dark: p[1].split(","), light: p[2].split(",") }
    })
    // The current family's two variants (the scheme cards draw them).
    readonly property var current: families.find(f => f.name === Workspace.themeFamily) || families[0]
    SectionHeader { title: "Color scheme"; subtitle: "System follows your desktop's light or dark setting." }
    GridLayout {
        Layout.fillWidth: true
        Layout.topMargin: 10
        columns: 3
        columnSpacing: 12
        rowSpacing: 12
        Repeater {
            model: [["system", "System"], ["light", "Light"], ["dark", "Dark"]]
            Rectangle {
                id: schemeCard
                required property var modelData
                objectName: "scheme-" + modelData[0]
                readonly property bool chosen: Workspace.themeMode === modelData[0]
                Layout.fillWidth: true
                implicitHeight: 136
                radius: 4
                color: schemeHover.hovered ? Theme.rowHover : Theme.raised
                Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
                border.color: chosen ? Theme.accent : Theme.hairline
                border.width: chosen ? 2 : 1
                HoverHandler { id: schemeHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: Workspace.chooseThemeMode(schemeCard.modelData[0]) }
                Item {
                    x: 8
                    y: 8
                    width: parent.width - 16
                    height: 96
                    MiniWindow {
                        anchors.fill: parent
                        colors: schemeCard.modelData[0] === "dark" ? appearance.current.dark : appearance.current.light
                    }
                    // System: light on the left, dark on the right.
                    Item {
                        visible: schemeCard.modelData[0] === "system"
                        x: parent.width / 2
                        width: parent.width / 2
                        height: parent.height
                        clip: true
                        MiniWindow { x: -parent.x; width: parent.parent.width; height: parent.height; colors: appearance.current.dark }
                    }
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 110
                    text: schemeCard.modelData[1]
                    color: schemeCard.chosen ? Theme.text : Theme.textMuted
                    font.pixelSize: Theme.px(13)
                    font.weight: schemeCard.chosen ? Font.DemiBold : Font.Normal
                }
            }
        }
    }
    Rule { }
    SectionHeader { title: "Themes"; subtitle: appearance.families.length + " themes, each in light and dark. Shown in " + (Theme.isLight ? "light" : "dark") + ", as chosen above." }
    ThemeList {
        Layout.topMargin: 14
        families: appearance.families
    }
    Rule { }
    SectionHeader { title: "Interface" }
    Group {
        SettingRow {
            title: "Contrast"
            detail: "How far apart the colours of surfaces, borders and text are, across the interface."
            icon: "contrast"
            RowLayout {
                spacing: 10
                FlatButton {
                    visible: (Theme.look.contrast || 100) !== 100
                    text: "Reset"
                    outlined: false
                    font.pixelSize: Theme.px(12)
                    onClicked: Workspace.chooseLook("contrast", "null")
                }
                ValueChip { text: Math.round(Theme.contrast * 100) + "%" }
                AppSlider {
                    objectName: "contrastSlider"
                    implicitWidth: appearance.compact ? 110 : 160
                    from: 50
                    to: 200
                    stepSize: 5
                    value: Theme.contrast * 100
                    onMoved: Workspace.chooseLook("contrast", String(value))
                }
            }
        }
        SettingRow {
            id: glassRow
            divider: true
            title: "Glass"
            // (Without acceleration it can't be drawn: off and
            // disabled, the choice kept for when it can.)
            readonly property bool noGpu: GraphicsInfo.api === GraphicsInfo.Software
            detail: noGpu ? "Needs graphics acceleration; Harlo is drawing in software, so menus and dialogs stay solid."
                : "Menus, dialogs and the composer show a soft blur of what's behind them."
            icon: "window"
            RowLayout {
                spacing: 8
                FlatButton {
                    objectName: "glassRenderer"
                    visible: glassRow.noGpu
                    text: "Renderer…"
                    font.pixelSize: Theme.px(12)
                    onClicked: appearance.rowRequested("general", "Renderer")
                }
                AppSwitch {
                    objectName: "glassSwitch"
                    enabled: !glassRow.noGpu
                    checked: Workspace.glass && !glassRow.noGpu
                    onToggled: { Workspace.chooseGlass(checked, Workspace.glassOpacity); checked = Qt.binding(() => Workspace.glass && !glassRow.noGpu) }
                }
            }
        }
        SettingRow {
            divider: true
            opacity: glassRow.noGpu ? 0.6 : 1
            title: "Glass opacity"
            detail: "Higher values make menus and dialogs more solid."
            icon: "eye"
            RowLayout {
                spacing: 10
                FlatButton {
                    visible: Math.abs(Workspace.glassOpacity - 0.8) > 0.001
                    text: "Reset"
                    outlined: false
                    font.pixelSize: Theme.px(12)
                    onClicked: Workspace.chooseGlass(Workspace.glass, 0.8)
                }
                ValueChip { text: Math.round(Workspace.glassOpacity * 100) + "%" }
                AppSlider {
                    objectName: "glassOpacity"
                    implicitWidth: appearance.compact ? 110 : 160
                    enabled: Workspace.glass && !glassRow.noGpu
                    from: 0.4
                    to: 1.0
                    stepSize: 0.05
                    value: Workspace.glassOpacity
                    onMoved: Workspace.chooseGlass(true, value)
                }
            }
        }
        SettingRow {
            divider: true
            title: "Diff colours"
            detail: "For lines added and removed, and change counts."
            icon: "git-commit"
            RowLayout {
                spacing: 6
                Repeater {
                    model: [["red-green", "Red & green", "#3dd68c", "#f26d6d"], ["blue-orange", "Blue & orange", "#5b9bf0", "#f2a65a"]]
                    ChoicePill {
                        required property var modelData
                        objectName: "diff-" + modelData[0]
                        text: modelData[1]
                        selected: (Theme.look.diff || "red-green") === modelData[0]
                        onClicked: Workspace.chooseLook("diff", modelData[0] === "red-green" ? "null" : JSON.stringify(modelData[0]))
                    }
                }
            }
        }
        SettingRow {
            divider: true
            title: "Chat width"
            detail: "How wide messages and the composer grow in a large window."
            icon: "layout-column"
            RowLayout {
                spacing: 6
                Repeater {
                    model: [["comfortable", "Comfortable"], ["wide", "Wide"], ["full", "Full width"]]
                    ChoicePill {
                        required property var modelData
                        objectName: "chatWidth-" + modelData[0]
                        text: modelData[1]
                        selected: (Theme.look.chatWidth || "comfortable") === modelData[0]
                        onClicked: Workspace.chooseLook("chatWidth", modelData[0] === "comfortable" ? "null" : JSON.stringify(modelData[0]))
                    }
                }
            }
        }
    }
    Rule { }
    SectionHeader { title: "Motion" }
    Group {
        SettingRow {
            title: "Animations"
            detail: "How fast dialogs and menus open and close (0: at once)."
            icon: "speed"
            RowLayout {
                spacing: 10
                ValueChip { text: Theme.motion + " ms" }
                AppSlider {
                    objectName: "motionSlider"
                    implicitWidth: appearance.compact ? 110 : 160
                    from: 0
                    to: 400
                    stepSize: 20
                    value: Theme.motion
                    onMoved: Workspace.chooseLook("motion", value === 160 ? "null" : String(value))
                }
            }
        }
    }
    Rule { }
    SectionHeader { title: "Typography" }
    Group {
        Repeater {
            model: [
                { kind: "ui", title: "Interface font", detail: "Everything outside code and the terminal.", icon: "file-text", fixed: false },
                { kind: "mono", title: "Code font", detail: "Code blocks, diffs, file previews, the editor and the terminal.", icon: "code-box", fixed: true },
            ]
            ColumnLayout {
                id: fontPick
                required property var modelData
                required property int index
                readonly property bool ui: modelData.kind === "ui"
                // The defaults: harlo's own fonts (Geist, JetBrains Mono).
                readonly property string system: ui ? Theme.defaultUiFont : Theme.defaultMonoFont
                readonly property string chosen: ui ? Workspace.uiFont : Workspace.monoFont
                readonly property var installed: Workspace.fontFamilies(modelData.fixed).filter(f => f !== system)
                // A chosen font is always listed, even if it isn't detected as monospace.
                readonly property var families: ["Default (" + system + ")"].concat(installed,
                    chosen !== "" && installed.indexOf(chosen) < 0 ? [chosen] : [])
                // Sizes in px: the interface's base size (text scale 1 = 14 px),
                // and the code's (13 px at scale 1).
                readonly property var sizes: ui ? [12, 13, 14, 15, 16, 17, 18, 20] : [10, 11, 12, 13, 14, 15, 16, 18]
                readonly property int size: ui ? Math.round(14 * Workspace.textScale) : Math.round(13 * Workspace.textScale * Theme.monoScale)
                Layout.fillWidth: true
                spacing: 10
                Rectangle { visible: fontPick.index > 0; Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
                // The title, then the font and its size (under it when narrow).
                GridLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    Layout.rightMargin: 16
                    Layout.topMargin: 10
                    columns: appearance.compact ? 2 : 3
                    columnSpacing: 10
                    rowSpacing: 8
                    // (As every row: its icon, its title and a line under it.)
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.columnSpan: appearance.compact ? 2 : 1
                        spacing: 12
                        Rectangle {
                            implicitWidth: 32
                            implicitHeight: 32
                            radius: 3
                            color: Theme.controlFill
                            Icon { anchors.centerIn: parent; name: fontPick.modelData.icon; color: Theme.textMuted; implicitWidth: 16; implicitHeight: 16 }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Label { text: fontPick.modelData.title; color: Theme.text; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium }
                            Label { Layout.fillWidth: true; text: fontPick.modelData.detail; color: Theme.textMuted; font.pixelSize: Theme.px(12.5); wrapMode: Text.Wrap }
                        }
                    }
                    ComboField {
                        objectName: "fontPick-" + fontPick.modelData.title
                        Layout.fillWidth: appearance.compact
                        Layout.preferredWidth: appearance.compact ? -1 : 210
                        fontPreview: true
                        defaultFont: fontPick.system
                        model: fontPick.families
                        currentIndex: fontPick.chosen === "" ? 0 : Math.max(0, fontPick.families.indexOf(fontPick.chosen))
                        onActivated: i => Workspace.chooseFont(fontPick.modelData.kind, i === 0 ? "" : fontPick.families[i])
                    }
                    ComboField {
                        objectName: "fontSize-" + fontPick.modelData.kind
                        Layout.preferredWidth: 90
                        model: fontPick.sizes.map(v => v + " px")
                        currentIndex: Math.max(0, fontPick.sizes.indexOf(fontPick.sizes.reduce((b, v) => Math.abs(v - fontPick.size) < Math.abs(b - fontPick.size) ? v : b)))
                        onActivated: i => {
                            const v = fontPick.sizes[i]
                            // Each size on its own: the code's stays when the interface's changes.
                            const code = fontPick.ui ? 13 * Workspace.textScale * Theme.monoScale : v
                            if (fontPick.ui) Workspace.chooseTextScale(v / 14)
                            const m = code / (13 * Workspace.textScale)
                            Workspace.chooseLook("monoScale", Math.abs(m - 1) < 0.01 ? "null" : String(m))
                        }
                    }
                }
                // A preview in the font, at its size.
                Rectangle {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    Layout.rightMargin: 16
                    Layout.bottomMargin: 14
                    implicitHeight: fontPick.ui ? uiSample.implicitHeight + 24 : codeSample.implicitHeight + 20
                    radius: 4
                    color: fontPick.ui ? Theme.canvas : Theme.codeBg
                    border.color: Theme.hairline
                    Label {
                        id: uiSample
                        visible: fontPick.ui
                        x: 14
                        y: 12
                        width: parent.width - 28
                        wrapMode: Text.Wrap
                        textFormat: Text.StyledText
                        color: Theme.text
                        font.family: Theme.uiFont
                        font.pixelSize: Theme.px(15)
                        text: "Use <b>Plan</b> to fix the flaky test in <font color=\"" + Theme.rgb(Theme.accent) + "\">surface.test.ts</font> and line the header up with the sidebar before shipping."
                    }
                    Column {
                        id: codeSample
                        visible: !fontPick.ui
                        x: 0
                        y: 10
                        width: parent.width
                        spacing: 0
                        Repeater {
                            model: [
                                { n: "1", k: " ", t: "export function formatUser(user: User) {" },
                                { n: "2", k: "-", t: "  return user.name.toUpperCase();" },
                                { n: "2", k: "+", t: "  return `${user.name} <${user.email}>`; // 0O 1lI" },
                                { n: "3", k: " ", t: "}" },
                                { n: "", k: " ", t: "" },
                                { n: "$", k: "t", t: "✓ 85 passed   ⚠ 2 warnings   ✗ 0 failed" },
                            ]
                            Rectangle {
                                required property var modelData
                                width: codeSample.width
                                height: codeLine.implicitHeight + 3
                                color: modelData.k === "+" ? Qt.rgba(Theme.diffAdd.r, Theme.diffAdd.g, Theme.diffAdd.b, 0.16)
                                     : modelData.k === "-" ? Qt.rgba(Theme.diffRemove.r, Theme.diffRemove.g, Theme.diffRemove.b, 0.16) : "transparent"
                                Rectangle { visible: modelData.k === "+" || modelData.k === "-"; width: 3; height: parent.height; color: modelData.k === "+" ? Theme.diffAdd : Theme.diffRemove }
                                Label {
                                    x: 14
                                    width: 22
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.n
                                    color: Theme.textFaint
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.mpx(13)
                                }
                                Label {
                                    id: codeLine
                                    x: 44
                                    width: parent.width - 54
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.t
                                    elide: Text.ElideRight
                                    color: modelData.k === "t" ? Theme.success : Theme.text
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.mpx(13)
                                }
                            }
                        }
                    }
                }
            }
        }
        SettingRow {
            divider: true
            title: "Word wrap"
            detail: "Long lines wrap in code blocks."
            icon: "text-wrap"
            AppSwitch { objectName: "wrapSwitch"; checked: Theme.wrapCode; onToggled: Workspace.chooseLook("wrap", checked ? "true" : "null") }
        }
    }
    Rule { }
    SectionHeader { title: "Chat" }
    Group {
        SettingRow {
            title: "Prompt navigator"
            detail: "Ticks at the chat's right edge, one per message you sent. Hover to see them, click to jump. From the keyboard: Jump to a prompt, Previous / Next prompt (Settings → Shortcuts)."
            icon: "chat-thread"
            AppSwitch {
                checked: Workspace.setting("prompt_rail") !== "0"
                onToggled: { Workspace.setSetting("prompt_rail", checked ? "1" : "0"); appearance.promptRailToggled(checked) }
            }
        }
    }
}
