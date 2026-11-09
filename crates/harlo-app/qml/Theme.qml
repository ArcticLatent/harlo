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
    // A code well's line-number column: a step off the well, toward the text.
    readonly property color codeGutter: blend(codeBg, foreground, isLight ? 0.05 : 0.075)
    // A rule inside a code well (under a command): plainer than the gutter.
    readonly property color codeRule: blend(codeBg, foreground, isLight ? 0.12 : 0.14)
    readonly property color selection: c.interactive.selection
    readonly property color error: c.status.error
    readonly property color warning: c.status.warning
    readonly property color success: c.status.success
    readonly property color info: c.status.info
    // Buttons (ControlSurface, outlined): a fill a step off
    // the page and a border that stands out from it, in the theme's own
    // colours.
    function blend(a, b, t) { return Qt.rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1) }
    // WCAG contrast between two colours, and the colour moved from `from`
    // toward `toward` just far enough to reach `target` against both the
    // canvas and the chrome, so small text stays readable in every theme.
    function relLuminance(c) {
        const f = v => v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }
    function contrastOf(a, b) {
        const x = relLuminance(a), y = relLuminance(b)
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05)
    }
    function reach(from, toward, target) {
        const worst = c => Math.min(contrastOf(c, background), contrastOf(c, sidebar))
        if (worst(from) >= target) return from
        let lo = 0, hi = 1
        for (let i = 0; i < 16; i++) {
            const mid = (lo + hi) / 2
            if (worst(blend(from, toward, mid)) >= target) hi = mid; else lo = mid
        }
        return blend(from, toward, hi)
    }
    // The floors scale down with Settings → Contrast below 100%.
    readonly property real faintFloor: 4.5 * Math.min(1, Math.max(0.67, contrast))
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
    readonly property color textMuted: reach(blend(foreground, muted, Math.min(1, contrast <= 1 ? 1 : 1 / contrast + 0.15)), foreground, faintFloor * 1.35)
    readonly property color textFaint: reach(blend(muted, background, Math.max(0, 0.32 / contrast)), foreground, faintFloor)
    /// Section labels: small, uppercase, spaced.
    readonly property color label: reach(blend(muted, background, Math.max(0, 0.12 / contrast)), foreground, faintFloor)
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
    // Comfortable: 760, about 100 characters of a reply's text, with room
    // for code beside it (the reading width alone, ~550, left code and
    // tool output cramped in a large window); the session's text, code,
    // helpers and composer all share it. Wide and Full widen them
    // together. (Unscaled: `px` scales it with the text.)
    readonly property real chatMaxWidth: look.chatWidth === "full" ? 100000 : look.chatWidth === "wide" ? 1180 : 760
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
        const base = ({ claude: "#E5895A", demo: "#A7AAB2" })[id] || "#A7AAB2"
        const col = Qt.color(base)
        return { fill: Qt.rgba(col.r, col.g, col.b, isLight ? 0.16 : 0.2), ink: isLight ? blend(col, Qt.rgba(0, 0, 0, 1), 0.25) : blend(col, Qt.rgba(1, 1, 1, 1), 0.15) }
    }
    function agentInitials(id) {
        return ({ claude: "CL", demo: "DE" })[id] || (id || "?").slice(0, 2).toUpperCase()
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
    // A tool's name as people read it: an MCP server's tools
    // ("mcp__harlo__spec_list") as "harlo spec list".
    /// A local model's name, the same however its server names it: the
    /// model, then its size (quantization). llama.cpp's file name
    /// "Qwen3-4B-UD-Q4_K_XL" and Ollama's "hf.co/unsloth/Qwen3-4B-GGUF:UD-Q4_K_XL"
    /// are both "Qwen3-4B · UD-Q4_K_XL".
    // What a session used: "12.3k tokens", ", $0.04" when it cost
    // something ("" for nothing). `u`: { tokens, cost }.
    function usedLabel(u) {
        if (!u) return ""
        const t = Number(u.tokens || 0), c = Number(u.cost || 0)
        const tokens = t >= 1000000 ? (t / 1000000).toFixed(1) + "M tokens" : t >= 1000 ? (t / 1000).toFixed(1) + "k tokens" : t > 0 ? t + " tokens" : ""
        const cost = c > 0 ? "$" + (c < 0.01 ? "<0.01" : c.toFixed(2)) : ""
        return [tokens, cost].filter(x => x !== "").join(" · ")
    }
    // Sums `usedLabel`'s inputs.
    function usedTotal(list) {
        return list.reduce((t, u) => u ? { tokens: t.tokens + Number(u.tokens || 0), cost: t.cost + Number(u.cost || 0) } : t, { tokens: 0, cost: 0 })
    }
    function modelLabel(id) {
        let name = id
        let quant = ""
        // (Wherever it is: an agent may put its server's name in front.)
        const hf = name.indexOf("hf.co/")
        if (hf >= 0) {
            const parts = name.slice(hf + 6).split(":")
            name = parts[0].split("/").pop().replace(/-GGUF$/i, "")
            quant = parts[1] || ""
        } else {
            const m = name.match(/^(.+?)[-_.:](IST-DAS-[\d.]+bpw|(?:UD-)?(?:I?Q\d[\w]*|TQ\d[\w]*|MXFP\d[\w]*|NVFP\d[\w]*|BF16|F16|F32))$/i)
            if (m) { name = m[1].split("/").pop(); quant = m[2] }
        }
        return quant !== "" ? name + " · " + quant : name
    }
    /// A helper (a subagent or a helper session), in words: who or what
    /// kind it is ("Claude Code", "Explore"), its model as the picker
    /// names it, and its status as the sidebar says it.
    function helperKind(k) {
        // (One form for all: a name, in sentence case: "Explore", "Web fetch".)
        const l = String(k || "task").toLowerCase()
        const named = ({ harlo: "Harlo", claude: "Claude Code", pi: "pi", scout: "Explore", task: "Task", general: "Task",
                         "general-purpose": "Task", reviewer: "Review" })[l]
        if (named) return named
        const words = String(k).replace(/[-_]+/g, " ").trim()
        return words.charAt(0).toUpperCase() + words.slice(1)
    }
    function helperModel(agent, model) {
        if (!model) return ""
        const p = Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(x => x.id === agent) : null
        const i = p ? (p.modelIds || []).indexOf(model) : -1
        if (i >= 0 && p.models[i]) return modelLabel(p.models[i])
        // (Not listed: a Claude id reads as its picker names it, "claude-sonnet-5" → "Sonnet 5".)
        const id = String(model).split("/").pop()
        const c = /^claude-([a-z]+)-(\d+)(?:-(\d+))?(\[1m\])?$/.exec(id)
        return c ? c[1].charAt(0).toUpperCase() + c[1].slice(1) + " " + c[2] + (c[3] ? "." + c[3] : "") + (c[4] ? " 1M" : "") : modelLabel(id)
    }
    function helperStatus(st) {
        return ({ running: "working", pending: "starting", completed: "finished", aborted: "stopped", waiting: "needs you", asking: "needs you" })[st] || st || ""
    }
    /// Who serves a model, from its id: "provider/model".
    function providerOf(id) {
        return (id || "").split(/[\/:]/)[0]
    }
    /// The model server behind a session's model ("provider/model"), from
    /// Settings → Model servers: `{ name,
    /// engine ("" for one harlo didn't set up), address (host:port), place
    /// ("here", "network", "online") }`, or null for a provider's own models.
    function serverOf(harness, modelId) {
        if (!modelId || Accounts.revision < 0) return null
        const name = modelId.split("/")[0]
        // (A llama.cpp on its usual port.)
        if (name === "llama.cpp")
            return { name: name, engine: name === "ollama" ? "Ollama" : "llama.cpp", address: "127.0.0.1:" + (name === "ollama" ? 11434 : 8080), place: "here" }
        const sv = JSON.parse(Accounts.servers(harness)).find(s => s.name === name)
        if (!sv) return null
        return { name: sv.name, engine: sv.engine === "Your command" ? "" : (sv.engine || ""), address: sv.baseUrl.replace(/^https?:\/\//, "").replace(/\/v1\/?$/, ""), place: sv.place }
    }
    /// The engine a model on one of your servers runs on ("" for any other).
    function modelEngine(harness, modelId) {
        const sv = serverOf(harness, modelId)
        return sv ? sv.engine : ""
    }
    function toolLabel(name) {
        // harlo's own (Claude's "mcp__harlo__x", others' "harlo_x", Harlo's
        // own "x"): what it does.
        const h = /^(?:mcp__harlo__|harlo_)?(.+)$/.exec(name || "")
        if (h && harloTools[h[1]]) return harloTools[h[1]]
        const m = /^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/.exec(name || "")
        return m ? m[1] + " " + m[2].replace(/_/g, " ") : name
    }
    readonly property var harloTools: ({
        propose_helpers: "harlo: offer a split",
        start_session: "harlo: start a session",
        send_message: "harlo: message a session",
        read_session: "harlo: read a session",
        list_sessions: "harlo: list sessions",
        list_agents: "harlo: list agents",
        stop_session: "harlo: stop a session",
        request_approval: "harlo: ask you to approve",
        report_done: "harlo: report back",
        ask_parent: "harlo: ask the session that started it",
        bring_over: "harlo: bring over a helper's changes",
    })
    // What a harlo tool's call is busy with before its arguments are in: the
    // agent writing them (a briefing can take a minute).
    function toolWriting(name) {
        const h = /^(?:mcp__harlo__|harlo_)?(.+)$/.exec(name || "")
        return h ? ({ propose_helpers: "writing each helper's briefing…", start_session: "writing its briefing…", send_message: "writing the message…",
                      request_approval: "writing the question…", report_done: "writing the report…", ask_parent: "writing the question…" })[h[1]] || "" : ""
    }
    function harnessLabel(id) {
        return ({ harlo: "Harlo", claude: "Claude", demo: "Demo" })[id] || id
    }
    // Text on a primary-coloured fill (buttons, check marks).
    readonly property color textOnPrimary: luminance(primary) > 0.55 ? "#1a1a1a" : (isLight ? "#ffffff" : "#1a1310")
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
            + "p,li{line-height:115%;}"
            // A reply's paragraphs, lists and headings are one text (to
            // select across): 8px between them, as between rows.
            + "p,ul,ol,blockquote,table{margin-top:8px;margin-bottom:8px;}"
            // A list inside a list: no gap above or below it.
            + "li ul,li ol{margin-top:0px;margin-bottom:0px;}"
            // Lists step in 24px a level, not Qt's 40 (a numbered one 28:
            // room for "10.").
            + "ul{margin-left:-16px;}ol{margin-left:-12px;}"
            // (Headings: see `proseHtml`.)
            // Tables: no grid (Qt draws a cell's borders all or none); a
            // hairline under the header row (see `proseHtml`), the names
            // left as the cells.
            + "table{border-style:none;}"
            + "td,th{border-style:none;padding:6px 12px 6px 0px;}"
            + "th{color:" + rgb(text) + ";text-align:left;font-weight:600;}"
            // A rule (see `proseHtml`): a faint hairline.
            + "table.rule{border-style:none;}td.rule{border-style:none;padding:0px;}"
            // A code block inside a list or a quote (see `proseHtml`): a
            // shaded box, its lines wrapped to the column.
            + "table.code{border-style:none;}td.code{border-style:none;}"
            // A quote's rule and its text (see `proseHtml`): no table lines.
            + "table.quote{border-style:none;}td.quote{border-style:none;padding:0px;}"
            + "td.code code{color:" + rgb(foreground) + ";}"
            + "a.copy{color:" + rgb(muted) + ";text-decoration:none;}"
            + css("a", m.link) + css("code", m.inlineCode) + css("blockquote", m.blockquote)
            + css(".s1", s.comment) + css(".s2", s.keyword) + css(".s3", s.string) + css(".s4", s.number)
            + css(".s5", s.function) + css(".s6", s.variable) + css(".s7", s.type) + css(".s8", s.operator)
            + css(".s9", h.diffAdded) + css(".s10", h.diffRemoved) + css(".s11", h.diffModified)
            + "</style>"
    }

    /// A reply's prose HTML for its text box: code blocks inside lists and
    /// quotes (Qt draws them as one unwrapped line, past the column) in a
    /// shaded box that wraps, with a Copy link (`harlo-copy:<n>`, the n-th
    /// block: see `proseCode`; `copied`: the one that says "Copied", -1
    /// none). (One paragraph, its lines joined with breaks: Qt shades a
    /// table cell behind each paragraph alone, so a <pre>'s lines, or a
    /// paragraph above it, leave gaps.)
    function proseHtml(html, copied, atTop) {
        // Headings step down in size, each with more room above than below
        // so it belongs to the text under it; the fourth level is a little
        // larger than the text, never only bold. (Paragraphs styled as
        // headings: Qt sizes <h1>–<h6> itself, the fourth smaller than the
        // text, whatever a style says.) One that opens a reply has no room
        // above it (the reply's name is there).
        const heads = { 1: [21, 24, 8], 2: [19, 22, 6], 3: [17, 18, 4], 4: [16, 16, 2], 5: [14, 14, 2], 6: [14, 14, 2] }
        html = html.replace(/<h([1-6])(?:\s[^>]*)?>([\s\S]*?)<\/h\1>/g, (all, level, inner, offset) => {
            const [size, above, below] = heads[level]
            const top = atTop && offset === 0 ? 0 : above
            return '<p style="font-size:' + px(size) + 'px;font-weight:600;margin-top:' + top + 'px;margin-bottom:' + below + 'px;color:'
                + rgb(level >= 5 ? textMuted : text) + ';">' + inner + "</p>"
        })
        // A rule: a hairline the width of the text (Qt draws <hr> in the
        // text colour, the loudest line in a reply). A one-cell table
        // with a 1px font is one pixel high.
        html = html.replace(/<hr\s*\/?>/g, '<table class="rule" width="100%" cellspacing="0" cellpadding="0" style="margin-top:14px;margin-bottom:14px;"><tr>'
            + '<td class="rule" bgcolor="' + rgb(hairline) + '" style="font-size:1px;line-height:100%;">&nbsp;</td></tr></table>')
        // A table's header: a hairline row under it, across its columns.
        // (No frame: Qt keeps room for one, even unstyled.)
        html = html.replace(/<table border="1"/g, '<table border="0"').replace(/(<thead>\s*<tr>([\s\S]*?)<\/tr>\s*<\/thead>\s*<tbody>)/g, (all, head, cells) =>
            all + '<tr><td class="rule" colspan="' + Math.max(1, (cells.match(/<th[\s>]/g) || []).length) + '" bgcolor="' + rgb(hairline)
                + '" style="font-size:1px;line-height:100%;">&nbsp;</td></tr>')
        // A quote: a rule down its left (Qt draws no side borders, so a
        // two-cell table: a thin shaded cell, then the quote), innermost
        // first.
        while (html.indexOf("<blockquote>") >= 0) {
            const before = html
            html = html.replace(/<blockquote>((?:(?!<blockquote>)[\s\S])*?)<\/blockquote>/g, (_, inner) =>
                '<table class="quote" width="100%" cellspacing="0" cellpadding="0" style="margin-top:8px;margin-bottom:8px;"><tr>'
                + '<td class="quote" width="3" bgcolor="' + rgb(textFaint) + '"></td><td class="quote" width="12"></td>'
                + '<td class="quote" style="color:' + rgb(Qt.color(c.markdown.blockquote || muted)) + ';">' + inner + "</td></tr></table>")
            if (html === before) break
        }
        if (html.indexOf("<pre>") < 0)
            return html
        let n = 0
        return html.replace(/<pre><code[^>]*>([\s\S]*?)\n?<\/code><\/pre>/g, (_, code) => {
            const link = '<a class="copy" href="harlo-copy:' + n + '"><span style="font-family:\'' + uiFont + '\';font-size:small;">' + (n++ === copied ? "Copied" : "Copy") + "</span></a>"
            // A shaded table inside a plain one: Qt shades a table's whole frame
            // without gaps, but ignores the list or quote indent for a table
            // placed straight in one; inside a cell it sits where the text does.
            return '<table class="code" width="100%" cellspacing="0" cellpadding="0"><tr><td class="code" style="padding:0px;">'
                + '<table class="code" width="100%" bgcolor="' + rgb(codeBg) + '" cellspacing="0" cellpadding="10"><tr><td class="code">'
                + '<p align="right" style="margin:0px;line-height:100%;">' + link + "</p>"
                + '<p style="margin:0px;white-space:pre-wrap;font-family:\'' + mono + '\';line-height:125%;">'
                + "<code>" + code.replace(/\n/g, "<br/>") + "</code></p></td></tr></table></td></tr></table>"
        })
    }
    /// The n-th code block of a reply's prose HTML, as plain text.
    function proseCode(html, n) {
        const blocks = html.match(/<pre><code[^>]*>[\s\S]*?<\/code><\/pre>/g) || []
        if (n < 0 || n >= blocks.length)
            return ""
        return blocks[n].replace(/<[^>]+>/g, "").replace(/\n$/, "")
            .replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&")
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
