# harlo

A native desktop app, in Rust and Slint 1.19, for coding agents run by OpenCode. Harlo attaches to
OpenCode 2's shared background server over HTTP/SSE. OpenCode owns agents, sessions, tools,
permissions and models. Harlo owns the window: following sessions, approving, reviewing diffs,
plus notes, git, an editor and a terminal.

- `crates/opencode-client` — the OpenCode v2 client. It holds the generated API types, which never leave this crate.
- `crates/harlo-core` — the domain model and reducer, git, diffs and SQLite. No UI code.
- `crates/harlo` — the Slint app: the reducer (`App`, `Message`, `update`) in Rust, the UI in
  `ui/*.slint`. Depends on `harlo-core` only.

## Language skills

Load the skill before touching the code, and follow the guidelines it loads. This applies to every change, however small, and again after a context compaction.

- **Rust** (`.rs` files, `Cargo.toml`): **`ms-rust`** (Microsoft's Pragmatic Rust Guidelines), always.
  - Load only the guideline files that apply to the task.
  - Follow their rules: canonical doc sections and a short summary sentence; `#[expect(..., reason = "...")]` instead of `#[allow]`; a `// SAFETY:` comment on every `unsafe` block; errors instead of panics for recoverable failures.
  - Mark a file that fully complies with the comment the skill specifies.
- **Slint UI** (`.slint` files, and Rust that talks to them): **`slint`** (plugin `slint@slint`), together with `ms-rust` for the Rust side.
  - Read the reference file for the area before building in it; check `.slint` diagnostics after each edit.
  - Slint is pinned at 1.19.0. When the skill and the 1.19 docs disagree, the docs win.
  - Slint is used under its Royalty-free 2.0 licence: an exception to the MIT/Apache rule, chosen by the user. `AboutSlint` stays in the app's About.

## Ground rules

- **Don't commit or push unless the user asks.** When they do, add no `Co-Authored-By`, "Generated with" or any other AI attribution. Commit messages are plain prose describing what changed for the user.
- **Local-only files stay local.** `PLAN.md`, `Rust-Iced-OpenCode-Libraries.md` and `mockup/` are excluded through `.git/info/exclude` and must never be added.
- **Never cite another app as a model** in code, comments, docs or UI text. Describe the behaviour itself. Licence notices for code or assets actually copied stay.
- **Libraries first.** Before writing a feature, look for a maintained MIT/Apache library (no GPL) and use it when it fits. Say which one and why, and pin its version.
- **OpenCode: always its latest release.** It is pinned in `crates/opencode-client/OPENCODE_VERSION`. Upgrade with `tools/opencode-bump.sh`, as a deliberate commit. The separate 1.x release line is not "latest".
- **The shared OpenCode server is not ours.** Harlo attaches to it and detaches from it. It never stops, restarts, replaces or reconfigures it. Closing Harlo leaves it running.
- **Tests never touch the user's real setup.**
  - Every test runs its own `opencode serve` with a scratch `HOME`/`XDG_*`, a random password and a scripted model.
  - It uses only the sessions it created.
  - Never the real `~/.config/opencode`, `~/.local/state/opencode` or the shared service.
  - Scratch folders sit outside this repository.
- **Test it yourself.** Run the tests and checks yourself and report results, not commands for the user. The user hand-tests only what automation can't reach, once a feature is finished.
  - For UI work, also drive the real window with `tools/gui-drive.py`, or Slint's MCP server (build with `--features slint/mcp`). It runs Harlo on a private Xvfb display against `tools/oc-sandbox.sh`, with a scratch `HOME`. It can click, type, press keys, scroll, fill the clipboard, resize and take screenshots. Look at the screenshots, and stop it when done.
- **Cap memory for heavy runs.** `/tmp` is RAM-backed. Wrap long or parallel runs in `systemd-run --user --scope -q -p MemoryMax=12G …` and clean up leftovers.

## Done means

`cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` for the crates you touched. UI changes also need their UI tests (on Slint's testing backend) and a look at the real window.
