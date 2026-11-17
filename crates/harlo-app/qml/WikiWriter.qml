pragma ComponentBehavior: Bound
import QtQuick
import harlo

// Who writes the wiki for one kind of run (Settings → Wiki): agent,
// model and thinking, kept in `wiki_writer` under `op`.
Group {
    id: writer
    property string op
    property string objectPrefix
    // (Shared with the page's other writer: a save here refreshes both.)
    property int rev: 0
    signal saved()
    function stored() {
        try { return JSON.parse(Workspace.setting("wiki_writer") || "{}") || ({}) } catch (e) { return ({}) }
    }
    readonly property var all: rev >= 0 ? stored() : ({})
    // { agent, model, thinking } ({}: the project's own).
    readonly property var value: all[op] || ({})
    readonly property var providers: Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).filter(p => p.canChat && p.enabled && p.state !== "missing") : []
    readonly property var provider: providers.find(p => p.id === value.agent) || null
    readonly property var levels: ["", "low", "medium", "high"]
    // Model servers: the agent's, and which are up.
    readonly property var servers: Accounts.revision >= 0 && provider ? JSON.parse(Accounts.servers(provider.id)) : []
    readonly property var serverStates: Accounts.serverStates !== "" ? JSON.parse(Accounts.serverStates) : ({})
    function serverOf(id) { return servers.find(sv => id.split("/")[0] === sv.name) || null }
    function up(id) { const sv = serverOf(id); const st = sv ? serverStates[sv.baseUrl] : null; return !st || st.state === "running" || st.state === "starting" }
    // Local models whose server is up first, then online ones, then
    // local ones whose server is stopped.
    readonly property var models: {
        if (!provider) return []
        const all = (provider.modelIds || []).map((id, i) => ({ id: id, name: Theme.modelLabel((provider.models || [])[i] || id), local: (provider.localIds || []).indexOf(id) >= 0 }))
        const rank = m => !m.local ? 1 : up(m.id) ? 0 : 2
        return all.sort((a, b) => rank(a) - rank(b)).map(m => Object.assign(m, { label: m.name + (!m.local ? "" : up(m.id) ? " · local" : " · local, server stopped") }))
    }
    function save(v) {
        // (Merged into what's stored now, not what this one read.)
        const next = stored()
        if (v.agent) next[op] = v; else delete next[op]
        Workspace.setSetting("wiki_writer", Object.keys(next).length ? JSON.stringify(next) : "")
        saved()
        Workspace.wikiWriterChanged()
    }
    function change(k, x) { const v = Object.assign({}, value); v[k] = x; save(v) }
    Component.onCompleted: { Workspace.ensureProviders(); Accounts.loadServers() }
    onServersChanged: { const urls = [...new Set(servers.filter(sv => sv.place !== "online").map(sv => sv.baseUrl))]; if (urls.length) Accounts.checkServers(JSON.stringify(urls)) }
    SettingRow {
        title: "Agent"
        detail: writer.provider ? "A " + writer.provider.label + " session writes it, with the model and thinking below." : "Each project's own agent and model write it."
        icon: "robot-2"
        ComboField {
            objectName: writer.objectPrefix + "Agent"
            width: 230
            model: ["Same as the project"].concat(writer.providers.map(p => p.label))
            function at() { return writer.provider ? writer.providers.indexOf(writer.provider) + 1 : 0 }
            currentIndex: at()
            onModelChanged: currentIndex = Qt.binding(() => at())
            onActivated: i => {
                writer.save(i === 0 ? {} : { agent: writer.providers[i - 1].id, model: "", thinking: "" })
                currentIndex = Qt.binding(() => at())
            }
        }
    }
    SettingRow {
        visible: writer.provider !== null
        divider: true
        title: "Model"
        detail: writer.provider && writer.provider.state === "checking" ? "Finding " + writer.provider.label + "'s models…"
              : writer.models.some(m => m.local) ? "Local models, from Local models, come first: they spend no API money."
              : "Add a server in Local models to write it with a local model, spending no API money."
        icon: "sparkling"
        ComboField {
            objectName: writer.objectPrefix + "Model"
            width: 230
            readonly property var ids: writer.models.map(m => m.id)
            model: ["Default model"].concat(writer.models.map(m => m.label))
            function at() { return !writer.value.model ? 0 : ids.indexOf(writer.value.model) + 1 }
            currentIndex: at()
            onModelChanged: currentIndex = Qt.binding(() => at())
            onActivated: i => {
                writer.change("model", i > 0 ? ids[i - 1] || "" : "")
                currentIndex = Qt.binding(() => at())
            }
        }
    }
    SettingRow {
        visible: writer.provider !== null
        divider: true
        title: "Thinking"
        detail: "How hard it thinks: more is slower and costs more."
        icon: "speed"
        ComboField {
            objectName: writer.objectPrefix + "Thinking"
            width: 230
            model: ["Default", "Low", "Medium", "High"]
            function at() { return Math.max(0, writer.levels.indexOf(writer.value.thinking || "")) }
            currentIndex: at()
            onActivated: i => {
                writer.change("thinking", writer.levels[i])
                currentIndex = Qt.binding(() => at())
            }
        }
    }
}
