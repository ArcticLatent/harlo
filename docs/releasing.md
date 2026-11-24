# Releasing

One command releases Harlo: the .deb, .rpm and Arch packages, the AppImage,
the signed update manifest and the GitHub release with its notes.

## Making a release

1. Add a section at the top of [CHANGELOG.md](../CHANGELOG.md):

   ```markdown
   ## 2.0.1

   - Fixed …
   ```

   The version comes from that heading; the section's text becomes the
   release's notes (how to install is in the README). Settings → General
   shows the start of every section newer than the installed version (read
   from CHANGELOG.md at the release's tag), with a link to the releases
   page for the rest, so keep the first items the ones that matter.

2. Run, at GitHub's `main`:

   ```bash
   packaging/release.sh --dry-run   # optional: shows the version and notes
   packaging/release.sh
   ```

   It checks that the checkout is at GitHub's `main` and only CHANGELOG.md
   has uncommitted changes, sets the version in `Cargo.toml` and
   `Cargo.lock`, commits "Release 2.0.1", tags `v2.0.1`, pushes both, then
   follows the release workflow and prints the release's address.

Needs `git`, `cargo` and the GitHub CLI (`gh auth login`).

The iPhone app is released separately (TestFlight and the App Store); its
version doesn't follow this one.

## What the workflow does

`.github/workflows/release.yml` runs on a `v*` tag:

| Job | Runs on | Does |
| --- | --- | --- |
| linux | `ubuntu-22.04` | Builds harlo and harlo-server, the AppImage and the .deb, .rpm and Arch packages (`packaging/linux/bundle.sh`, nfpm) |
| install | Ubuntu 22.04 and 24.04, Debian 12, Fedora 42, openSUSE Tumbleweed, Arch containers | Installs each package with its system's package manager, and the AppImage, then starts the server and the window there (`packaging/linux/check-start.sh`) |
| publish | `ubuntu-22.04` | Checks the tag against `Cargo.toml`, writes and signs `linux-release.json`, writes `SHA256SUMS`, publishes the release |

The publish job signs `linux-release.json` with the update-signing key and
checks it against the public key built into Harlo, so a wrong key stops the
release instead of shipping an update nobody can install. Installed Harlos
read it from `releases/latest/download/linux-release.json` and install only
packages whose SHA-256 matches it. A Harlo built from source never looks.

A branch named `release-check…`, or "Run workflow" on the Actions page,
builds and tests every package without publishing (they're the run's
artifacts).

### After an update

The package manager replaces both programs; Harlo offers Restart, which
starts the new window. The server on the computer keeps running the old
version, with its sessions, until Settings → General's Restart server.
An AppImage's server runs from a copy per version in
`~/.local/share/harlo/server/`, so replacing the AppImage doesn't pull it
from under the running server.

### The update-signing key

| Where | What |
| --- | --- |
| `crates/harlo-update-signing/src/lib.rs` (`PUBLIC_KEY_B64`) | The public half, built into Harlo |
| `HARLO_UPDATE_SIGNING_KEY` repository secret | The private half, used only by the publish job |

Keep a copy of the private key somewhere safe (a password manager). GitHub
can't show a secret again, and without the key no release can update the
Harlos already installed.

To replace it: `cargo run -p harlo-manifest-signer -- keygen`, put the new
public half in `PUBLIC_KEY_B64` and release that Harlo signed with the
**old** key, then change the secret to the new private half for the releases
after it.

## Building packages locally

```bash
packaging/linux/build-in-container.sh   # dist/: the packages, built as the workflow builds them
packaging/linux/test-install.sh dist    # each installed and started on clean systems (Podman)
```

## Files

| Path | What |
| --- | --- |
| `CHANGELOG.md` | Release notes, newest first |
| `packaging/release.sh` | The release command |
| `packaging/changelog.sh` | Reads the version and notes from CHANGELOG.md |
| `packaging/release-manifests.sh` | Writes and signs the update manifest |
| `packaging/harlo.desktop`, `packaging/harlo.svg` | The menu entry and its icon |
| `packaging/linux/` | The bundle, nfpm's package definition, local builds and tests |
| `packaging/aur/` | The AUR recipes: `harlo-bin` (the release's Arch package) and `harlo-git` (the latest commit, from source) |
| `crates/harlo-update-signing/` | What's signed, and the check Harlo makes |
| `crates/harlo-manifest-signer/` | The signing tool (not part of Harlo) |
| `crates/harlo-core/src/updates.rs` | Harlo's update: the manifest, the package for how it was installed, installing it, starting again |
| `crates/harlo/src/app/updates.rs` | Settings → General's Updates and the notification |
