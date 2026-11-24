# Contributing to Harlo

Thanks for wanting to help. Harlo is made by one person, me, so every
report, idea and pull request makes a real difference. Bug reports, ideas,
fixes, translations of the website, packaging for more distributions and
whole new platforms are all welcome.

- **Found a bug or have an idea?** [Open an issue](https://github.com/ArcticLatent/harlo/issues/new/choose).
  The forms ask for what helps most.
- **Want to fix or build something?** For anything bigger than a small
  fix, open an issue first (or comment on one) and say what you plan, so I
  can tell you early whether it fits, before you spend the time.
- **Security problems:** please report privately, see [SECURITY.md](SECURITY.md).

## Wanted: macOS, Windows and Android

Harlo is Linux-only today, but most of it isn't tied to Linux: Rust, Slint
and the agent engine run on macOS and Windows. I'd love to have Harlo there,
and I'm looking for **contributors and maintainers** for a macOS app and a
Windows app. If you'd like to take one on, open an issue titled "macOS port"
or "Windows port" and introduce yourself; I'll help you find your way around
and review the work.

What's Linux-specific today, and would need a macOS / Windows counterpart:

| Area | Where | Today on Linux |
|---|---|---|
| Running commands for agents | `crates/harlo-engine/src/tools/shell.rs`, `crates/harlo-server/src/pty.rs` | Unix process groups and signals, a pseudo-terminal |
| The background server | `crates/harlo-server/src/service.rs`, `crates/harlo-core/src/server.rs` | a detached process, registered in `$XDG_STATE_HOME/harlo/service.json` |
| One running Harlo | `crates/harlo/src/instance.rs` | a local socket in the runtime folder |
| Tray icon | `crates/harlo/src/tray.rs` | StatusNotifierItem (`ksni`) |
| Start at login | `crates/harlo-core/src/autostart.rs` | an XDG autostart entry |
| Self-update | `crates/harlo-core/src/updates.rs`, `crates/harlo/src/app/updates.rs` | apt / dnf / zypper / pacman with `sudo` or `pkexec`, or the AppImage |
| Where files go | throughout | XDG folders (`~/.config`, `~/.local/share`, `~/.local/state`) |
| Packages and releases | `packaging/linux/`, `.github/workflows/release.yml` | .deb, .rpm, Arch, AppImage |

The iPhone app needs no changes: it talks to Harlo's server over the
network, whatever system that runs on.

### Android

I'm also looking for **contributors and maintainers for an Android app**,
the counterpart of the iPhone app: follow sessions live, answer questions
and approvals from notifications, start new sessions. Open an issue titled
"Android app" to get started. What it needs:

- **The protocol.** `ios/HarloKit` is the iPhone app's client library and
  the reference: pairing by QR code, NaCl box encryption with per-device
  keys, and the JSON messages (`Pairing.swift`, `Crypto.swift`,
  `Client.swift`, `Messages.swift`). The server's side is
  `crates/harlo-server/src/phone.rs` and `crates/harlo-core/src/phone.rs`.
- **Reaching Harlo** directly on the same network, or through the relay
  (`crates/harlo-relay`) from anywhere.
- **Notifications.** The relay sends the iPhone's through Apple's push
  service (`crates/harlo-apns`); Android would need Firebase Cloud Messaging
  in the relay the same way, with the notification sealed for the phone.

## Building

You need Rust 1.92 or newer, cmake and clang (they build the bundled
whisper.cpp), and the headers for ALSA, fontconfig and FreeType
(`libasound2-dev libfontconfig1-dev libfreetype6-dev` on Debian and Ubuntu,
`alsa-lib-devel fontconfig-devel freetype-devel` on Fedora).

```sh
cargo build -p harlo -p harlo-server    # the app, and the server it starts
target/debug/harlo
cargo test --workspace                  # the tests
cargo clippy --workspace --all-targets
```

To work on the interface without spending tokens or touching your own
sessions, start a private server with a scripted model and point a window
at it:

```sh
eval "$(tools/sandbox.sh)"              # prints URL, PASSWORD, DIR, PROJECT…
HARLO_SERVER_PASSWORD=$PASSWORD target/debug/harlo --server "$URL"
tools/sandbox.sh stop "$DIR"
```

The sandbox keeps everything in a scratch folder: its server never sees your
config, sessions or desktop keyring. `tools/gui-drive.py` does the same in an
invisible X display and drives the window (clicks, keys, screenshots), and
`tools/verify/verify.sh` runs the whole app's checks that way.

The iPhone app is in `ios/` (SwiftUI, generated with XcodeGen from
`ios/Harlo/project.yml`); `ios/HarloKit` is a Swift package that also builds
and tests on Linux (`swift test`).

## How the code is organised

| Crate | Role |
|---|---|
| `crates/harlo` | The desktop app: the Slint interface (`ui/`), the window's state (`src/app/`) and what it shows (`src/present/`) |
| `crates/harlo-server` | The background server: sessions, approvals, git, the terminal, phone access, over HTTP and server-sent events |
| `crates/harlo-engine` | The agent loop, the providers and the tools |
| `crates/harlo-client` | The client for the server's API |
| `crates/harlo-core` | Themes, projects, sessions, git, local models, dictation, phone pairing and updates, with no UI code |
| `crates/harlo-relay` | The relay phones use from anywhere |
| `crates/harlo-apns` | Apple's push service, for the relay |

Logic that doesn't need the window belongs in `harlo-core`,
`harlo-engine` or `harlo-server`, with tests there. The app's state changes
by messages (`crates/harlo/src/app.rs`), and its tests drive the real Slint
window on Slint's testing backend (`crates/harlo/tests/`).

## Pull requests

- Keep a pull request to one change; small ones are reviewed fastest.
- Write code like the code around it: its naming, its comment style, its
  level of detail. Comments say what something does and why, in plain words.
- Add or update tests for what you change, and say in the pull request how
  you checked it.
- For anything visible, add a screenshot or a short video, and check it in
  a dark and a light theme.
- `cargo fmt --check`, `cargo test --workspace` and
  `cargo clippy --workspace --all-targets -- -D warnings` should pass.

By contributing, you agree that your contribution is licensed under the
project's [MIT license](LICENSE).

## Being kind

Be respectful and assume good intent, in issues, reviews and everywhere else
around the project. Harassment or personal attacks aren't tolerated;
I may remove such comments and block those who make them. To
report a problem with someone's behaviour, email hello@harlo.run.
