pragma Singleton
import QtQuick
import harlo

// Colours of the chosen theme (Settings → Appearance), one of the theme
// files. Workspace hands over the JSON with colours
// already in Qt's #AARRGGBB order. Screens use the roles below (canvas,
// chrome, raised, hairline, text levels, accent…), which every theme fills,
// scaled by Settings → Appearance → Contrast.
QtObject {
    id: theme

    // "dark" / "light": the mode setting, or the desktop's scheme for "system".
    readonly property string variant: Workspace.themeMode === "light" ? "light"
        : Workspace.themeMode === "system" ? (Qt.styleHints.colorScheme === Qt.ColorScheme.Light ? "light" : "dark")
        : "dark"
    readonly property bool isLight: variant === "light"
    readonly property var data: JSON.parse(Workspace.themeJson(Workspace.themeFamily, variant))
    readonly property var c: data.colors

    // The rest of Settings → Appearance (contrast, diff colours, chat width,
    // panel speed, code text size, word wrap).
    readonly property var look: { try { return JSON.parse(Workspace.look || "{}") } catch (e) { return ({}) } }
    // Your avatar: one of the bundled penguins (chosen from the left rail).
    readonly property var avatars: ["classic", "beanie", "glasses", "headphones", "jacket", "scarf"]
    readonly property string avatar: avatars.indexOf(look.avatar) >= 0 ? look.avatar : "classic"
    function avatarUrl(name) { return "qrc:/qt/qml/harlo/icons/avatars/penguin-" + name + ".png" }
    /// 0.5–2: how far apart the theme's surfaces, borders and text levels are.
    readonly property real contrast: Math.max(0.5, Math.min(2, (look.contrast || 100) / 100))

    readonly property color background: c.surface.background
    readonly property color sidebar: c.surface.muted
    readonly property color foreground: c.surface.foreground
    readonly property color muted: c.surface.mutedForeground
    readonly property color elevated: c.surface.elevated
    readonly property color subtle: c.surface.subtle || c.surface.elevated
    // Opaque (blended over the background) so it looks the same on any surface.
    readonly property color border: blend(background, Qt.tint(c.surface.background, c.chat.divider), contrast)
    readonly property color hover: c.interactive.hover
    readonly property color pressed: c.interactive.active
    readonly property color selected: c.chat.userMessageBackground
    readonly property color primary: c.primary.base
    readonly property color primaryHover: c.primary.hover
    readonly property color userBubble: c.chat.userMessageBackground
    readonly property color codeBg: (c.syntax.base.background) || c.markdown.inlineCodeBackground
    readonly property color selection: c.interactive.selection
    readonly property color error: c.status.error
    readonly property color warning: c.status.warning
    readonly property color success: c.status.success
    readonly property color info: c.status.info
    // Buttons (ControlSurface, outlined): a fill a step off
    // the page and a border that stands out from it, in the theme's own
    // colours.
    function blend(a, b, t) { return Qt.rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1) }
    readonly property color controlFill: blend(background, foreground, (isLight ? 0.045 : 0.07) * contrast)
    readonly property color controlFillHover: blend(background, foreground, (isLight ? 0.08 : 0.11) * contrast)
    readonly property color controlFillDown: blend(background, foreground, (isLight ? 0.11 : 0.05) * contrast)
    readonly property color controlBorder: blend(background, foreground, (isLight ? 0.1 : 0.09) * contrast)
    readonly property color controlBorderHover: blend(background, foreground, (isLight ? 0.18 : 0.16) * contrast)

    // ---- roles: what screens are built from -------------------------------
    /// The main area (chat, settings pages, panels).
    readonly property color canvas: background
    /// The sidebar and the window around the panels: a step darker (lighter
    /// in a light theme) than the canvas.
    readonly property color chrome: sidebar
    /// Cards on the canvas: a tool run, code, the composer.
    readonly property color raised: blend(background, foreground, (isLight ? 0.035 : 0.035) * contrast)
    /// Fields on the chrome (search).
    readonly property color well: blend(sidebar, foreground, (isLight ? 0.05 : 0.045) * contrast)
    /// Lines between messages and sections: barely there.
    readonly property color hairline: blend(background, foreground, (isLight ? 0.08 : 0.075) * contrast)
    /// Text: primary, secondary (names, metadata), faint (times, hints).
    readonly property color text: foreground
    readonly property color textMuted: blend(foreground, muted, Math.min(1, contrast <= 1 ? 1 : 1 / contrast + 0.15))
    readonly property color textFaint: blend(muted, background, Math.max(0, 0.32 / contrast))
    /// Section labels: small, uppercase, spaced.
    readonly property color label: blend(muted, background, Math.max(0, 0.12 / contrast))
    /// Sidebar rows: hovered, and the open one.
    readonly property color rowHover: blend(sidebar, foreground, (isLight ? 0.04 : 0.04) * contrast)
    readonly property color rowSelected: blend(sidebar, foreground, (isLight ? 0.075 : 0.085) * contrast)
    readonly property color accent: primary
    /// The accent as a quiet fill (badges, soft highlights).
    readonly property color accentSoft: Qt.rgba(primary.r, primary.g, primary.b, isLight ? 0.12 : 0.17)
    /// Diffs and change counts (Settings → Appearance → Diff colours).
    readonly property bool diffBlueOrange: look.diff === "blue-orange"
    readonly property color diffAdd: diffBlueOrange ? (isLight ? "#1f6fd6" : "#5b9bf0") : ((c.syntax.highlights || {}).diffAdded || success)
    readonly property color diffRemove: diffBlueOrange ? (isLight ? "#c7761c" : "#f2a65a") : ((c.syntax.highlights || {}).diffRemoved || error)
    /// How wide messages and the composer grow (Settings → Appearance → Chat width).
    readonly property real chatMaxWidth: look.chatWidth === "full" ? 100000 : look.chatWidth === "wide" ? 1180 : 860
    /// Panels open and close in this long (ms; 0: at once).
    readonly property int motion: look.motion === undefined ? 160 : look.motion
    /// Hover and selection colours ease in this long (ms).
    readonly property int hoverMotion: Math.round(motion * 0.7)
    /// A side pane slides open or shut in this long (ms).
    readonly property int paneMotion: Math.round(motion * 1.4)
    /// Long code lines wrap (code blocks, diffs, file previews).
    readonly property bool wrapCode: look.wrap === true
    /// An agent's avatar colours: a soft fill and its letters, from the theme.
    function agentColor(id) {
        const base = ({ claude: "#E5895A", omp: "#9B8AFB", pi: "#4FC1AE", opencode: "#5B9BF0", demo: "#A7AAB2" })[id] || "#A7AAB2"
        const col = Qt.color(base)
        return { fill: Qt.rgba(col.r, col.g, col.b, isLight ? 0.16 : 0.2), ink: isLight ? blend(col, Qt.rgba(0, 0, 0, 1), 0.25) : blend(col, Qt.rgba(1, 1, 1, 1), 0.15) }
    }
    function agentInitials(id) {
        return ({ claude: "CL", omp: "OM", pi: "PI", opencode: "OC", demo: "DE" })[id] || (id || "?").slice(0, 2).toUpperCase()
    }
    // Project colours (Settings → Projects), from the theme, so they suit
    // every theme. Stored by key.
    readonly property var projectColors: [
        { key: "keyword", color: c.syntax.base.keyword },
        { key: "string", color: c.syntax.base.string },
        { key: "number", color: c.syntax.base.number },
        { key: "type", color: c.syntax.base.type },
        { key: "function", color: c.syntax.base["function"] },
        { key: "comment", color: c.syntax.base.comment },
        { key: "error", color: c.status.error },
        { key: "primary", color: c.primary.base },
        { key: "success", color: c.status.success },
    ]
    /// A project colour's value; `fallback` for none (or one this theme lacks).
    function projectColor(key, fallback) {
        const found = projectColors.find(p => p.key === key)
        return found && found.color ? found.color : fallback
    }
    /// Whether a project on `host` can start `id` sessions: an agent its last
    /// check found there (any, until it's connected and checked).
    function offeredOnHost(host, id) {
        const h = Remotes.revision >= 0 ? JSON.parse(Remotes.hostsJson()).find(x => x.name === host) : null
        return !h || h.state !== "connected" || (h[id] || "") !== ""
    }
    /// A provider's name for people (its mark is Icon "harness-<id>").
    function harnessLabel(id) {
        return ({ claude: "Claude", opencode: "OpenCode", pi: "Pi", omp: "Oh My Pi", hermes: "Hermes", demo: "Demo" })[id] || id
    }
    // Text on a primary-coloured fill (buttons, check marks).
    readonly property color onPrimary: luminance(primary) > 0.55 ? "#1a1a1a" : (isLight ? "#ffffff" : "#1a1310")
    // Fonts: Geist (bundled) for the interface, JetBrains Mono (bundled) for
    // code, unless others are chosen in Settings → Appearance.
    readonly property string defaultUiFont: "Geist"
    readonly property string defaultMonoFont: "JetBrains Mono"
    readonly property string systemUiFont: Workspace.systemFont(false)
    readonly property string uiFont: Workspace.uiFont !== "" ? Workspace.uiFont : defaultUiFont
    readonly property string mono: Workspace.monoFont !== "" ? Workspace.monoFont : defaultMonoFont

    // Glass (Settings → Appearance → Glass): menus and
    // dialogs blur what's behind them. `backdrop` is the window's content
    // (Main sets it); `glassStack` the popup items of open glass dialogs,
    // bottom first, so one opened over another blurs that one; `scrims`
    // the open dialog scrims (only the bottom one blurs).
    property Item backdrop: null
    property var glassStack: []
    property var scrims: []
    readonly property bool glass: Workspace.glass && backdrop !== null
    function glassPush(list, it) { return list.indexOf(it) < 0 ? list.concat([it]) : list }
    function glassPop(list, it) { return list.filter(x => x !== it) }

    // Text sizes scale with Settings → Appearance → Text size; code (`mpx`)
    // with the code font's size too.
    function px(n) { return Math.round(n * Workspace.textScale) }
    readonly property real monoScale: Math.max(0.7, Math.min(1.6, look.monoScale || 1))
    function mpx(n) { return Math.round(n * Workspace.textScale * monoScale) }

    function luminance(col) { return 0.2126 * col.r + 0.7152 * col.g + 0.0722 * col.b }
    // Qt rich text wants #RRGGBB (alpha dropped).
    function rgb(col) { return "#" + [col.r, col.g, col.b].map(v => Math.round(v * 255).toString(16).padStart(2, "0")).join("") }

    // Prepended to every rich-text block: markdown colours and the token
    // classes of highlighted code (see TOKEN_CLASSES in markdown.rs).
    readonly property string htmlStyle: {
        const s = c.syntax.base, h = c.syntax.highlights || {}, m = c.markdown
        const css = (sel, col) => col ? sel + "{color:" + rgb(Qt.color(col)) + ";}" : ""
        return "<style>"
            // Room between the lines of prose (not code).
            + "p,li{line-height:150%;}"
            // Tables: hairlines, as the rest of the interface.
            + "table{border-color:" + rgb(hairline) + ";border-style:solid;}"
            + "td,th{border-color:" + rgb(hairline) + ";border-style:solid;padding:6px 10px;}"
            + "th{color:" + rgb(text) + ";}"
            + css("a", m.link) + css("code", m.inlineCode) + css("blockquote", m.blockquote)
            + css(".s1", s.comment) + css(".s2", s.keyword) + css(".s3", s.string) + css(".s4", s.number)
            + css(".s5", s.function) + css(".s6", s.variable) + css(".s7", s.type) + css(".s8", s.operator)
            + css(".s9", h.diffAdded) + css(".s10", h.diffRemoved) + css(".s11", h.diffModified)
            + "</style>"
    }

    function statusColor(status) {
        switch (status) {
        case "busy": case "starting": return primary
        case "input": return warning
        case "unread": return info
        case "live": case "idle": return success
        case "exited": return error
        default: return muted
        }
    }

    // "3m", "2h", "5d" from unix seconds; `now` (ms) is passed in so
    // bindings refresh when the caller's clock ticks.
    function ago(seconds, now) {
        if (!seconds)
            return ""
        const s = Math.max(0, now / 1000 - seconds)
        if (s < 60) return "now"
        if (s < 3600) return Math.floor(s / 60) + "m"
        if (s < 86400) return Math.floor(s / 3600) + "h"
        if (s < 86400 * 30) return Math.floor(s / 86400) + "d"
        return Math.floor(s / (86400 * 30)) + "mo"
    }
}
