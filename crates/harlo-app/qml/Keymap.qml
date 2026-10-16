pragma Singleton
import QtQuick
import harlo

// Keyboard shortcuts: each action's default and the user's overrides
// (Settings → Shortcuts, saved as JSON in the "shortcuts" setting). The
// registry in Main.qml, the cheat sheet and hints elsewhere read keys here.
QtObject {
    id: keymap

    readonly property var defaults: ({
        palette: "Ctrl+K", goToSession: "Ctrl+P", searchAll: "Ctrl+Shift+F", actions: "Ctrl+Shift+P",
        nextTab: "Ctrl+Tab", nextTabAlt: "Ctrl+PgDown", prevTab: "Ctrl+Shift+Tab", prevTabAlt: "Ctrl+PgUp",
        jumpPrompt: "Ctrl+J", prevPrompt: "Alt+Up", nextPrompt: "Alt+Down",
        newSession: "Ctrl+N", closeTab: "Ctrl+W",
        toggleSidebar: "Ctrl+B", settings: "Ctrl+,", shortcuts: "Ctrl+/",
        contextPanel: "Ctrl+Shift+I", activityPanel: "Ctrl+Shift+A", changesPanel: "Ctrl+Shift+G", filesPanel: "Ctrl+Shift+E", notesPanel: "Ctrl+Shift+N",
        terminal: "Ctrl+`", newTerminalTab: "Ctrl+Shift+T", closeTerminalTab: "Ctrl+Shift+W",
        miniChat: "Ctrl+Shift+M", expandComposer: "Ctrl+Shift+X", dictate: "Ctrl+Shift+D",
    })
    /// Keys the app handles itself; they can't be assigned.
    readonly property var reserved: {
        const r = { "Ctrl+Return": "Steer (composer)", "Ctrl+Enter": "Steer (composer)", "Ctrl+S": "Save file",
                    "Ctrl+Shift+C": "Copy (terminal)", "Ctrl+Shift+V": "Paste (terminal)", "Ctrl+Alt+L": "Layout readout" }
        for (let i = 1; i <= 9; i++) r["Ctrl+" + i] = "Go to tab " + i
        return r
    }

    property var overrides: ({})
    /// True while Settings records a key: shortcuts stand down.
    property bool recording: false

    /// The action's key ("" when unassigned).
    function seq(id) { return overrides[id] !== undefined ? overrides[id] : (defaults[id] || "") }
    /// " (Ctrl+N)" for tooltips, or "" when unassigned.
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
