# iced_term, vendored

Copied from https://github.com/Harzu/iced_term at commit
`d7c8187bd426d19a2c0d1abe615332c2c8fffab0`: `src/`, `LICENSE`, `README.md`,
`rustfmt.toml` and `clippy.toml`. The commit is past the 0.8.0 release and
holds the fix for a UI deadlock under heavy output (upstream PR #69), which no
release has yet. MIT License (`LICENSE`).

## Changes

In the code each carries a `Harlo:` comment.

- `Cargo.toml`: rewritten as a workspace member on the workspace's `iced`,
  `tokio` (only `sync`), `open` and `anyhow`. One clippy lint that upstream's
  view tests trip is allowed.
- Remote terminals: `Terminal::remote` makes a terminal for a program that
  runs elsewhere (an OpenCode PTY). Its output comes in as
  `BackendCommand::Feed`, parsed on the caller's thread; input and size changes
  leave as `RemoteInput` on a channel. Local terminals work as before.
  `tests/remote.rs` covers it.
- No panics: a link that can't be opened does nothing, a bad palette colour
  draws black, and the event stream ends when the terminal's events stop or
  the app stops listening (it used to panic, or spin after the shell exited).
