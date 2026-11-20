pragma Singleton
import QtQuick
import harlo

// Keyboard shortcuts: each action's default and the user's overrides
// (Settings → Shortcuts, saved as JSON in the "shortcuts" setting). The
// registry in Main.qml, the cheat sheet and hints elsewhere read keys here.
QtObject {
    id: keymap

    readonly property var defaults: ({
        palette: "Ctrl+K", goToSession: "Ctrl+P", searchAll: "Ctrl+Shift+F", findInSession: "Ctrl+F", actions: "Ctrl+Shift+P",
        nextTab: "Ctrl+Tab", nextTabAlt: "Ctrl+PgDown", prevTab: "Ctrl+Shift+Tab", prevTabAlt: "Ctrl+PgUp",
        jumpPrompt: "Ctrl+J", prevPrompt: "Alt+Up", nextPrompt: "Alt+Down", focusRuns: "Ctrl+Shift+Up",
        newSession: "Ctrl+N", closeTab: "Ctrl+W",
        toggleSidebar: "Ctrl+B", settings: "Ctrl+,", shortcuts: "Ctrl+/", quit: "Ctrl+Q",
        contextPanel: "Ctrl+Shift+I", activityPanel: "Ctrl+Shift+A", changesPanel: "Ctrl+Shift+G", filesPanel: "Ctrl+Shift+E", notesPanel: "Ctrl+Shift+N", wikiPanel: "Ctrl+Shift+K",
        terminal: "Ctrl+`", newTerminalTab: "Ctrl+Shift+T", closeTerminalTab: "Ctrl+Shift+W",
        miniChat: "Ctrl+Shift+M", expandComposer: "Ctrl+Shift+X", dictate: "Ctrl+Shift+D",
    })
    /// Keys the app handles itself; they can't be assigned.
    readonly property var reserved: {
        const r = { "Ctrl+Return": "Steer (composer)", "Ctrl+Enter": "Steer (composer)", "Ctrl+S": "Save file", "Ctrl+H": "Replace (file)", "Ctrl+G": "Go to line (file)",
                    "Ctrl+Shift+C": "Copy (terminal)", "Ctrl+Shift+V": "Paste (terminal)", "Ctrl+Alt+L": "Layout readout" }
        for (let i = 1; i <= 9; i++) r["Ctrl+" + i] = "Go to tab " + i
        return r
    }

    /// The order of the shortcut groups, in Settings and the shortcuts sheet.
    readonly property var groupOrder: ["Navigation", "Sessions and tabs", "Window", "Panels", "Composer", "Runs in a session", "File editor", "Image viewer", "Sidebar"]

    property var overrides: ({})
    /// True while Settings records a key: shortcuts stand down.
    property bool recording: false

    /// The action's key ("" when unassigned).
    function seq(id) { return overrides[id] !== undefined ? overrides[id] : (defaults[id] || "") }
    /// " (Ctrl+N)" for tooltips, or "" when unassigned.
    // A pi key id ("ctrl+shift+x", "alt+enter", "f5") as a key sequence
    // ("Ctrl+Shift+X"); "" for one Qt has no name for.
    function fromPiKey(id) {
        const names = { escape: "Esc", esc: "Esc", enter: "Return", return: "Return", tab: "Tab", space: "Space", backspace: "Backspace",
                        delete: "Del", insert: "Ins", up: "Up", down: "Down", left: "Left", right: "Right", home: "Home", end: "End",
                        pageup: "PgUp", pagedown: "PgDown" }
        const mods = { ctrl: "Ctrl", control: "Ctrl", shift: "Shift", alt: "Alt", meta: "Meta", super: "Meta" }
        const parts = String(id).split("+")
        const key = parts.pop().toLowerCase()
        const out = []
        for (const p of parts) {
            const m = mods[p.toLowerCase()]
            if (!m) return ""
            out.push(m)
        }
        const name = names[key] || (/^f([1-9]|1[0-2])$/.test(key) ? key.toUpperCase() : key.length === 1 ? key.toUpperCase() : "")
        return name === "" ? "" : out.concat([name]).join("+")
    }
    function hint(id) { const s = seq(id); return s !== "" ? " (" + s + ")" : "" }
    function isCustom(id) { return overrides[id] !== undefined && overrides[id] !== defaults[id] }
    /// The action already on `key`, if any (other than `except`).
    function owner(key, except) {
        for (const id in defaults)
            if (id !== except && seq(id) === key) return id
        return ""
    }

    function set(id, key) {
        const o = Object.assign({}, overrides)
        if (key === defaults[id]) delete o[id]; else o[id] = key
        save(o)
    }
    function reset(id) { const o = Object.assign({}, overrides); delete o[id]; save(o) }
    function resetAll() { save({}) }
    function save(o) {
        overrides = o
        Workspace.setSetting("shortcuts", Object.keys(o).length ? JSON.stringify(o) : "")
    }
    Component.onCompleted: {
        try { overrides = JSON.parse(Workspace.setting("shortcuts") || "{}") } catch (e) { overrides = {} }
    }
}
