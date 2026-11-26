# Plugins

A plugin is a git repository that adds to what Harlo's agents have: skills, commands, agents, MCP servers, and code of its own. You add one in Settings → Extensions → Plugins by its address. It is added to Harlo's server, so every session, window and phone on that server gets it.

- [Using plugins](#using-plugins)
- [Writing one](#writing-one)
- [plugin.json](#pluginjson)
- [plugin.js](#pluginjs)
- [What it costs](#what-it-costs)
- [Publishing](#publishing)

## Using plugins

**Adding.** Type an `https://` address of a git repository (or a folder on the server's computer) into the Plugin field and press Add. Harlo fetches that one commit and keeps it. It never fetches or updates on its own: a plugin changes only when you press Update on its page.

**What waits for your yes.** A plugin's skills, command templates and agents are text, and are used as soon as it's added. Anything that would act for you waits until you allow it on the plugin's page:

- MCP servers it starts, with the command each runs;
- shell lines in its commands;
- rules that let its agents act without asking;
- its code, and each thing the code may reach: every network host, the session's tools, running when a turn ends.

Nothing of the plugin is used until you allow these. An update that asks for something new waits again; one that doesn't stays in use.

**Yours win.** Your own skills, commands, agents and MCP servers win over a plugin's of the same name.

**Browse.** "Published plugins" lists the GitHub repositories tagged `harlo-plugin`. The list is fetched only when you press Browse. Nobody reviews it: read a plugin before you add it.

**Where things are kept.** The list of plugins is `plugins.json` in `~/.config/harlo`, with the commit of each, so the file can bring the same plugins to another computer. The files are under `~/.local/share/harlo/plugins`. What you allowed, and what you filled in, stay on that computer.

## Writing one

Start from [`docs/plugins/starter`](plugins/starter): copy the folder, `git init`, commit, and add the folder in Settings. It has a tool, a command, a setting, the types file and a skill that teaches an agent to write plugins.

```
my-plugin/
  plugin.json          what it is, what its code offers and may reach
  plugin.js            the code (optional)
  harlo.d.ts           types for plugin.js (optional)
  skills/<name>/SKILL.md
  commands/<name>.md
  agents/<name>.md
```

Harlo fetches commits, so commit before you press Update.

## plugin.json

```json
{
  "name": "release-helper",
  "description": "Writes release notes.",
  "version": "0.4.0",
  "minHarlo": "2.0.0",
  "mcp": { "changelog": { "type": "local", "command": ["changelog-mcp"] } },
  "tools": [
    {
      "name": "next_version",
      "description": "The version after one.",
      "parameters": { "type": "object", "properties": { "current": { "type": "string" } }, "required": ["current"] }
    }
  ],
  "commands": [{ "name": "notes", "description": "Draft release notes." }],
  "events": ["turn.ended"],
  "settings": [{ "key": "token", "label": "GitHub token", "secret": true }],
  "permissions": { "network": ["api.github.com"], "project": ["read"] }
}
```

| Key | What it is |
|---|---|
| `name` | Lower-case letters, digits, `-`, `_`, `.`; 64 at most. Without one, the last part of the address. |
| `description`, `version` | Shown on the plugin's page. |
| `minHarlo` | The oldest Harlo it works with. An older Harlo refuses it and says so. |
| `mcp` | MCP servers by name, written as `mcp.servers` in `harlo.jsonc`. |
| `main` | The code's file; `plugin.js` unless named. It must be inside the plugin. |
| `tools` | Each tool's `name`, `description` and `parameters` (JSON Schema). Agents are told exactly this. |
| `commands` | Each command's `name` and `description`, for commands written in code. |
| `events` | What the code runs for by itself. Only `turn.ended`. |
| `settings` | What a person fills in: `key`, `label`, and `secret` for a token. Eight at most. |
| `permissions.network` | Hosts the code may ask: `api.example.com`, `*.example.com`, `localhost:8080`. |
| `permissions.project` | The session's tools its tools may call: of `read`, `grep`, `glob`, `edit`, `write`, `shell`. |

Harlo refuses a plugin it can't run as written, when it's added or updated, and keeps the version in use: a tool with no code file, code outside the plugin's folder, a name that can't be one, a permission or event this Harlo doesn't know.

## plugin.js

One file of plain JavaScript, 2 MiB at most. TypeScript and libraries are bundled into it by you. It registers a function for each thing `plugin.json` names:

```js
harlo.tool("next_version", async ({ current }, context) => { … });   // returns JSON
harlo.command("notes", async (text, context) => "the prompt to send"); // returns text
harlo.on("turn.ended", async (event, context) => { … });               // returns nothing
```

**How it runs.** Each call runs the file from its top in a runtime of its own, which ends with the call. Nothing is kept between calls but `harlo.storage`. A call may compute for ten seconds and take two minutes with its waiting, and return 256 KiB. There are no files, programs, network, `require` or timers: only `harlo` and `console.log`.

**What it reaches**, each only if `plugin.json` asks and you allowed it:

| | |
|---|---|
| `harlo.fetch(url, {method, headers, body})` | A request to a host in `permissions.network`, over `https` (or `http` to this computer). A redirect to another host is returned, not followed. 30 seconds and 2 MiB at most. Gives `{status, ok, url, headers, body, truncated, text(), json()}`. |
| `harlo.settings.get(key)` | What a person filled in, or `null`. |
| `harlo.storage.get/set/delete(key)` | 256 KiB of JSON of the plugin's own, kept on the server, forgotten when the plugin is removed. |
| `harlo.project.read(…)` and the others | The session's own tool, for those in `permissions.project`. The session's rules decide each call, and a person is asked where they say to, with the change shown. Only a tool an agent's script called has them; a command and an event don't. |
| `harlo.plugin`, `harlo.session` | The plugin's name and version; the session's ID and folder. Also in `context`. |

**How agents reach a tool.** From a script, as `tools.<plugin>.<tool>({…})`, where `<plugin>` is the plugin's name with anything but letters and digits turned into `_`. Scripts run through the `execute` tool, which a session's rules allow, ask about or deny like any other.

**Errors.** What the code throws is the call's error: the plugin's name, the message, and the last ten lines it logged.

**When a turn ends.** `event` has `outcome`, `error`, and `session` (`id`, `directory`, `title`, `parentID`). The code runs beside the server's work with the network, settings and storage, and no project. If it fails, the plugin's page says why until it next runs through.

## What it costs

Skills, commands and agents cost what your own do. A tool written in code adds one line to the description of `execute`, sent with every request: its name, its parameters and its description. Keep descriptions to a sentence. The starter's one tool adds about 50 tokens with its plugin's line (some 210 characters). A plugin with more than ten tools is listed by names only, and a script looks up the one it wants.

## Publishing

Push the repository to GitHub and give it the topic `harlo-plugin`. It then shows under Browse, most starred first. Say in its README what it asks for and why: people read that before they allow it.
