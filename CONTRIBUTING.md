# Contributing to harlo

Thanks for wanting to help. Bug reports, ideas, fixes, translations of the
website, packaging for more distributions and whole new platforms are all
welcome.

- **Found a bug or have an idea?** [Open an issue](https://github.com/ArcticLatent/harlo/issues/new/choose).
  The forms ask for what helps most.
- **Want to fix or build something?** For anything bigger than a small
  fix, open an issue first (or comment on one) and say what you plan, so we
  can agree on the approach before you spend the time.
- **Security problems:** please report privately, see [SECURITY.md](SECURITY.md).

## Wanted: macOS and Windows

harlo is Linux-only today, but most of it isn't tied to Linux: the Rust core
and the Qt 6 interface run on macOS and Windows, and so do the agents
(Claude Code, OpenCode, pi, Oh My Pi). We'd love to have harlo there, and we
are looking for **contributors and maintainers** for a macOS app and a
Windows app. If you'd like to take one on, open an issue titled
"macOS port" or "Windows port" and introduce yourself; we'll help you find
your way around and review the work.

What's Linux-specific today, and would need a macOS / Windows counterpart:

| Area | Where | Today on Linux |
|---|---|---|
| Starting and stopping agents | `crates/harlo-harness/src/process.rs`, `claude_run.rs`, `opencode.rs` | Unix process groups and signals, `PR_SET_PDEATHSIG` |
| Notifications | `crates/harlo-app/src/notify.rs` | `notify-send` |
| One running harlo | `crates/harlo-app/src/instance.rs` | a local socket in the runtime folder |
| Start at login | `crates/harlo-app/src/workspace.rs` (`set_autostart`) | an XDG autostart entry |
| Drop-down window | `crates/harlo-app/qml/DropDownLayer.qml` | Wayland layer-shell |
| Phone access through the firewall | `crates/harlo-core/src/firewall.rs` | ufw / firewalld with `pkexec` |
| Self-update | `crates/harlo-core/src/updates.rs`, `crates/harlo-app/src/updates.rs` | apt / dnf / zypper / pacman with `pkexec`, or the AppImage |
| Where files go | throughout | XDG folders (`~/.local/share/harlo`, `~/.config/harlo`) |
| Packages and releases | `packaging/linux/`, `.github/workflows/release.yml` | .deb, .rpm, Arch, AppImage |

The iPhone app needs no changes: it talks to harlo over the network,
whatever system harlo runs on. An Android app is welcome too; `ios/HarloKit`
(the iPhone app's client library) shows the protocol.

## Building

You need Rust 1.85 or newer, Qt 6.9 or newer (`qt6-base`,
`qt6-declarative`, `qt6-multimedia`), cmake and clang (they build the bundled
whisper.cpp).

```sh
cargo run -p harlo-app --release     # the app
cargo test --workspace               # the tests
cargo clippy --workspace --all-targets
```

Without any agent installed, **Demo** at the bottom of the sidebar starts a
pretend agent that streams canned replies, so you can work on the interface
without spending tokens. To keep your real sessions and settings out of a
test run, point `HOME` (or `XDG_DATA_HOME` and `XDG_CONFIG_HOME`) at a
scratch folder.

The iPhone app is in `ios/` (SwiftUI, generated with XcodeGen from
`ios/Harlo/project.yml`); `ios/HarloKit` is a Swift package that also builds
and tests on Linux (`swift test`).

## How the code is organised

| Crate | Role |
|---|---|
| `crates/harlo-harness` | Runs the agents and turns what they say into one stream of events |
| `crates/harlo-core` | Sessions, transcripts, markdown, git, worktrees, search, remote machines, the phone protocol, state. No Qt |
| `crates/harlo-app` | The desktop app: cxx-qt bridges (`src/`) and the QML interface (`qml/`) |
| `crates/harlo-relay` | The relay phones use from anywhere |
| `crates/harlo-apns` | Apple's push service, for the relay |

Logic that doesn't need Qt belongs in `harlo-core` or `harlo-harness`, with
tests there. `harlo-app` connects it to QML.

## Pull requests

- Keep a pull request to one change; small ones are reviewed fastest.
- Write code like the code around it: its naming, its comment style, its
  level of detail. Comments say what something does and why, in plain words.
- Add or update tests for what you change, and say in the pull request how
  you checked it (and which agents you tried, for anything about sessions).
- For anything visible, add a screenshot or a short video, and check it in
  a dark and a light theme.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` should
  pass without new warnings.

By contributing, you agree that your contribution is licensed under the
project's [MIT license](LICENSE).

## Being kind

Be respectful and assume good intent, in issues, reviews and everywhere else
around the project. Harassment or personal attacks aren't tolerated;
maintainers may remove such comments and block those who make them. To
report a problem with someone's behaviour, email hello@harlo.run.
