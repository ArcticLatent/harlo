---
name: harlo-plugin
description: Write or change a Harlo plugin - its plugin.json, plugin.js, skills, commands and agents. Use when asked to make, extend or debug a plugin for Harlo.
---

# Writing a Harlo plugin

A plugin is a git repository with a `plugin.json` at its root. Everything else is optional:

- `skills/<name>/SKILL.md`, `commands/<name>.md`, `agents/<name>.md`: read as Harlo's own config folders are.
- `plugin.js`: code that gives agents tools, runs commands, and runs when a turn ends.

## Rules that decide whether it loads

- `name`: lower-case letters, digits, `-`, `_`, `.`; 64 characters at most.
- Every tool, command, event, setting and permission the code uses must be named in `plugin.json`. Code that registers something `plugin.json` doesn't name is never called; a name in `plugin.json` with no code fails when called.
- Tool and setting names: letters, digits and `_`.
- `permissions` has two keys, `network` (hosts) and `project` (of `read`, `grep`, `glob`, `edit`, `write`, `shell`). Any other key makes the plugin unusable.
- `events` holds only `turn.ended`.
- `plugin.js` is plain JavaScript in one file, 2 MiB at most. Bundle TypeScript and libraries into it. There is no `require`, `import`, `process`, `fetch` or timer: only `harlo` and `console.log`.

## Writing the code

- Each call runs `plugin.js` from the top in a fresh runtime. Keep the top level to `harlo.tool`, `harlo.command` and `harlo.on` calls; keep anything between calls in `harlo.storage`.
- A tool takes one object and returns JSON a model will read. Return what was asked, short. Throw an `Error` with a sentence the model can act on.
- A command returns the prompt to send, as text. Returning nothing is an error.
- Ask for the least: a tool that only reads names `"project": ["read"]`; a host is named exactly (`api.example.com`), not `*.example.com`, unless it must be.
- Every `harlo.*` call that reaches out returns a promise: `await` it.
- Secrets come from `harlo.settings.get`, declared with `"secret": true`. Never write one into storage, a result or a log.

## Checking it

1. Commit the change; Harlo fetches commits, not the working folder.
2. Add the repository's folder in Settings → Extensions → Plugins, or press Update on its page.
3. Read what the page says it asks for, and allow it.
4. Ask an agent to call the tool, or run the command. An error names the plugin and carries its last ten `console.log` lines.

The full reference is `docs/plugins.md` in Harlo's repository; `harlo.d.ts` beside `plugin.js` has the types.
