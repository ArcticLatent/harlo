<p align="center">
  <a href="https://harlo.run">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="site/assets/logo.svg">
      <img src="site/assets/logo-light.svg" alt="Harlo" width="300">
    </picture>
  </a>
</p>

<p align="center"><b>Your coding agents, in a truly native app.</b><br>
Agents that keep working in the background on your Linux computer and your other machines,<br>
with the models you choose, your own local ones included, and an iPhone app to follow and answer them.<br>
Written in Rust and drawn with Slint, not Electron.</p>

<p align="center">
  <a href="https://harlo.run/download"><b>Download</b></a> ·
  <a href="https://harlo.run">Website</a> ·
  <a href="#on-your-iphone">iPhone app</a> ·
  <a href="https://harlo.run/support">Support</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

<p align="center">
  <a href="https://github.com/ArcticLatent/harlo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/ArcticLatent/harlo?style=flat-square&color=c6a0f6&label=release"></a>
  <img alt="Rust and Slint" src="https://img.shields.io/badge/Rust%20%2B%20Slint-native-8aadf4?style=flat-square">
  <img alt="Linux" src="https://img.shields.io/badge/Linux-x86--64-8aadf4?style=flat-square&logo=linux&logoColor=white">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-c6a0f6?style=flat-square"></a>
</p>

## Why Harlo

Many agent apps are a web page in an Electron window. Harlo is a desktop app:

- **All Rust.** The window, the agent server and its engine are compiled Rust. Streams, transcripts, diffs, git and search never wait on a browser engine.
- **Drawn by your GPU.** The interface is drawn by [Slint](https://slint.dev) with Skia, with a software renderer to fall back on. There's no HTML, no JavaScript framework and no bundled Chromium.
- **Agents in the background.** Harlo's own server runs your sessions, so they, their schedules and the phone keep going while the window is closed.
- **At home on Linux:** Wayland and X11, your desktop's notifications, a tray icon, start at login, and packages for your distribution.

## What it does

| | |
|---|---|
| **Your models** | Anthropic, OpenAI (an API key, or your ChatGPT plan), OpenRouter, OpenCode Go, Vercel's AI Gateway, Cohere and any OpenAI-compatible server. Settings → Providers puts the popular ones up front, checks keys as you add them, and signs in to OpenRouter and ChatGPT in the browser. |
| **Local models** | Settings → Local models installs Ollama or llama.cpp, runs their servers, and gets models that fit your computer. |
| **Agents that do the work** | They read and edit files, run commands and background jobs, search the web, use MCP servers (OAuth sign-ins included), skills, commands and plugins, run subagents, keep a todo list, and carry notes from one session to the next. |
| **A native chat** | Your prompts and each agent's turn, real lists and tables, code coloured by language, pictures with a viewer, and jumps between earlier prompts. Long conversations are summarized as they grow. |
| **You stay in charge** | Approvals with permission modes, saved approvals, and questions the agent asks you. Undo a turn, one file at a time. Fork, move, archive, export and import sessions; side questions with `/btw`, goals with `/goal`, shell commands with `!`. |
| **Changes and Git** | The Changes panel shows only the lines that changed, takes comments on diff lines and commits (✦ writes the message). Settings → Git signs in to GitHub, GitLab and Forgejo. |
| **Your other machines** | Harlo follows a Harlo server on another computer over SSH: a GPU box, a build server, a NAS. |
| **And** | A terminal, an editor, notes, schedules that send prompts or run commands, dictation recognised on your computer, desktop notifications with a sound each, and light and dark Base16 themes with contrast, fonts, text sizes, chat width and motion of your choosing. |

### On your iPhone

Follow your sessions live and answer questions and approvals, even from a notification. Watch a run in a Live Activity. Look through changes, diffs, files and branches, undo turns, edit and resend messages, and manage the queue, approval modes and pins. The app takes your desktop's theme and avatar. Between the phone and your computer everything is end-to-end encrypted: on your Wi-Fi, or through Harlo's relay, which can't read what it passes on. To pair, open Settings → Phone on your computer and scan the code.

## Install

Download the file for your system from [harlo.run/download](https://harlo.run/download) or the [latest release](https://github.com/ArcticLatent/harlo/releases/latest):

| Your system | File | Install |
|---|---|---|
| Ubuntu 22.04+, Debian 12+, Mint, Pop!_OS | `harlo_<version>-1_amd64.deb` | `sudo apt install ./harlo_*_amd64.deb` |
| Fedora, openSUSE | `harlo-<version>-1.x86_64.rpm` | `sudo dnf install ./harlo-*.rpm` · `sudo zypper install ./harlo-*.rpm` |
| Arch, Manjaro, EndeavourOS, CachyOS | `harlo-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U ./harlo-*.pkg.tar.zst` |
| Any other distribution | `harlo-<version>-x86_64.AppImage` | `chmod +x harlo-*.AppImage`, then run it |

Harlo needs glibc 2.35 or newer, an x86-64 CPU with AVX2, and git. It updates itself: Settings → General → Updates. Each release lists its files' SHA-256 sums in `SHA256SUMS`.

On first start, Harlo's setup guide walks through its server, your models, Git and the theme (`harlo --setup` opens it again).

## Build from source

Needs Rust 1.92 or newer, cmake and clang (for the bundled whisper.cpp), and the headers for ALSA, fontconfig and FreeType (`libasound2-dev libfontconfig1-dev libfreetype6-dev` on Debian and Ubuntu).

```sh
cargo build --release -p harlo -p harlo-server   # harlo starts the harlo-server beside it
target/release/harlo
cargo test --workspace
```

On Arch, `cd packaging/aur/harlo-git && makepkg -si` builds the latest commit. A Harlo built from source doesn't update itself. The release packages are built by `packaging/linux/bundle.sh`; see [docs/releasing.md](docs/releasing.md).

<details>
<summary><b>Using Harlo: a quick tour</b></summary>

- **Sessions.** The sidebar lists your sessions by project. **+ Project** adds a folder; Ctrl+N starts a session. Each session opens in a tab (Ctrl+Tab and Ctrl+Shift+Tab move between them).
- **The composer.** Enter sends, Shift+Enter adds a line, `/` lists commands and skills, `@` mentions files and MCP resources, and Tab picks the next agent. While the agent works, Enter queues a message and Ctrl+Enter steers it. Ctrl+V pastes a picture; Ctrl+Shift+D dictates. Under it: the permissions shield, the agent, the model and its effort.
- **Approvals.** Alt+A allows the action once, Alt+D denies it; a denial can say what to do instead.
- **Panels.** Changes, Files, Context, Todos, Notes and the terminal open from the rail on the right. Ctrl+, opens Settings, where Shortcuts lists every key.
- **The server.** Harlo starts its server in the background on this computer. `harlo-server service start` starts it without the window; `harlo --server <url>` follows one elsewhere, with its password in `HARLO_SERVER_PASSWORD`. Settings → Server adds servers on your other computers over SSH and imports sessions from OpenCode; the menu at the top of the sidebar switches between servers.

</details>

## Configuration

Settings covers most of it. Underneath, the server reads `~/.config/harlo/harlo.jsonc` (or `harlo.json`): the providers and their models, the default `model`, permission rules, `instructions`, `skills`, `commands`, `agents`, `mcp.servers` and `websearch`. A project adds its own `harlo.jsonc`, or one in its `.harlo/` folder, and the nearer file wins. Changes apply as soon as the file is saved.

The first time the server starts, it looks for an earlier OpenCode setup. If there is one, it copies `~/.config/opencode/opencode.json`, `AGENTS.md`, commands, agents and skills into `~/.config/harlo`, once, and leaves the original untouched. Sessions made on OpenCode come over from Settings → Server.

- **Instructions.** A session reads `~/.config/harlo/AGENTS.md` (or `~/.claude/CLAUDE.md`), then each `AGENTS.md` from its folder up to the project's root; in a folder without an `AGENTS.md`, its `CLAUDE.md`. `HARLO_DISABLE_PROJECT_CONFIG=1` skips the project's files; `HARLO_DISABLE_CLAUDE_MD=1` skips every `CLAUDE.md`.
- **Commands.** Markdown files in `~/.config/harlo/commands/` and a project's `.harlo/commands/`. `/name` runs one; its front matter sets the description, agent and model.
- **Agents.** Markdown files in the `agents/` folders beside them: the front matter holds the agent's settings, the body its instructions.

### Plugins

A plugin is a git repository that brings skills, commands, agents, MCP servers and tools of its own. Settings → Extensions → Plugins adds one by its address, or from the list of those people have published. Harlo fetches a plugin only when you add or update it, and what would act for you (programs it starts, its code, what its code may reach) waits for your yes. [docs/plugins.md](docs/plugins.md) has how to write one, with a starter to copy.

### Skills

A skill is a folder with a `SKILL.md`: instructions the agent loads when a task calls for them. Harlo finds them in `.claude/skills/`, `.agents/skills/` and `.harlo/skills/`, in your home folder and in the project's folders, and in `~/.config/harlo/skills/`. `/` lists them, to attach one to a prompt; `disable-model-invocation: true` in a skill's front matter leaves it to you.

## Running your own relay

`harlo-relay` (`crates/harlo-relay`) lets phones reach Harlo from anywhere and sends the iPhone app's notifications. It forwards end-to-end encrypted messages it can't read and stores nothing. Harlo uses Harlo's relay (`relay.harlo.run`) unless `HARLO_RELAY=https://…` names another where Harlo and its server start.

```sh
cargo build --release -p harlo-relay
target/release/harlo-relay --listen 127.0.0.1:8080 --trust-forwarded
# or: docker build -f crates/harlo-relay/Dockerfile -t harlo-relay . && docker run -p 8080:8080 harlo-relay
```

It speaks plain HTTP: put it behind something that adds HTTPS (Caddy, a Cloudflare Tunnel, Tailscale Funnel). Use `--trust-forwarded` only when that proxy is the only way in. It limits connections per address (`--per-ip`) and the number of Harlos (`--max-hosts`). For notifications it needs an APNs key of the app's team (`--apns-key AuthKey_<Key ID>.p8 --apns-key-id <Key ID> --apns-team <Team ID>`); without one it relays everything else.

## Layout

| Crate | Role |
|---|---|
| `crates/harlo` | The desktop app: the Slint interface and the window's state |
| `crates/harlo-server` | Harlo's background server: runs sessions on the engine and serves them over HTTP and server-sent events |
| `crates/harlo-engine` | The agent loop: sends a session's history to a model, runs the tools it calls, and goes on until it answers |
| `crates/harlo-client` | The client for the server's API |
| `crates/harlo-core` | Themes, projects, sessions, git, local models, dictation, phone pairing and updates, with no UI code |
| `crates/harlo-relay`, `crates/harlo-apns` | The relay, and Apple's push service for it |
| `crates/harlo-update-signing`, `crates/harlo-manifest-signer` | The signature on Harlo's update manifest, and the release tool that signs it |
| `packaging/` | The Linux packages, the release scripts and the AUR recipes |
| `ios/` | The iPhone app (SwiftUI), its widgets and HarloKit, its client library |
| `site/` | [harlo.run](https://harlo.run) |

## Contributing

Harlo is open source and made by one person, so contributions are very welcome: bug reports, ideas, fixes, features, and packaging for more distributions.

- **Found a bug or have an idea?** [Open an issue](https://github.com/ArcticLatent/harlo/issues/new/choose).
- **Want to build something?** Read [CONTRIBUTING.md](CONTRIBUTING.md), then comment on an issue or open one to say what you plan.
- **macOS, Windows and Android: maintainers wanted.** Harlo runs on Linux today, but Rust and Slint are cross-platform. [CONTRIBUTING.md](CONTRIBUTING.md#wanted-macos-windows-and-android) lists where to start.
- **Security problems:** please report privately, see [SECURITY.md](SECURITY.md).

## License

MIT, see [LICENSE](LICENSE). Bundled themes, icons, fonts and other work by others keep their own licenses: see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
