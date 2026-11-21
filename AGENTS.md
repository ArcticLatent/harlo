# harlo (iced)

A native desktop app, in Rust and iced 0.14, for coding agents run by OpenCode. Harlo attaches to
OpenCode 2's shared background server over HTTP/SSE. OpenCode owns agents, sessions, tools,
permissions and models. Harlo owns the window: following sessions, approving, reviewing diffs,
plus notes, git, an editor and a terminal.

- `crates/opencode-client` — the OpenCode v2 client. It holds the generated API types, which never leave this crate.
- `crates/harlo-core` — the domain model and reducer, git, diffs and SQLite. No iced.
- `crates/harlo` — the iced app (Elm architecture). Depends on `harlo-core` only.

## Language skills

Load the skill before touching the code, and follow the guidelines it loads. This applies to every change, however small, and again after a context compaction.

- **Rust** (`.rs` files, `Cargo.toml`): **`ms-rust`** (Microsoft's Pragmatic Rust Guidelines), always.
  - Load only the guideline files that apply to the task.
  - Follow their rules: canonical doc sections and a short summary sentence; `#[expect(..., reason = "...")]` instead of `#[allow]`; a `// SAFETY:` comment on every `unsafe` block; errors instead of panics for recoverable failures.
  - Mark a file that fully complies with the comment the skill specifies.
- **iced UI** (anything using `iced`, its widgets, `Task`s or `Subscription`s): **`iced`**, together with `ms-rust`.
  - Follow its architecture, widget, styling, task and anti-pattern references.
  - iced is pinned at 0.14. When the skill and the 0.14 source disagree, the source wins.

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
- **Cap memory for heavy runs.** `/tmp` is RAM-backed. Wrap long or parallel runs in `systemd-run --user --scope -q -p MemoryMax=12G …` and clean up leftovers.

## Done means

`cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` for the crates you touched. UI changes also need their `iced_test` checks.
