---
type: Workflow
title: Releases, signing and distribution
description: Build and test Linux packages, sign update manifests across the release trust boundary, and publish downloads through the static site.
covers:
  - crates/harlo-manifest-signer/
  - crates/harlo-update-signing/
  - packaging/
  - site/
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:54:36Z" }
sources:
  - id: release
    resource: "repo://packaging/release.sh"
  - id: changelog
    resource: "repo://packaging/changelog.sh"
  - id: workflow
    resource: "repo://.github/workflows/release.yml"
  - id: manifests
    resource: "repo://packaging/release-manifests.sh"
  - id: signer
    resource: "repo://crates/harlo-manifest-signer/src/main.rs"
  - id: signer-cargo
    resource: "repo://crates/harlo-manifest-signer/Cargo.toml"
  - id: signing
    resource: "repo://crates/harlo-update-signing/src/lib.rs"
  - id: core
    resource: "repo://crates/harlo-core/src/updates.rs"
  - id: app
    resource: "repo://crates/harlo-app/src/updates.rs"
  - id: bundle
    resource: "repo://packaging/linux/bundle.sh"
  - id: container-build
    resource: "repo://packaging/linux/build-in-container.sh"
  - id: container
    resource: "repo://packaging/linux/Containerfile"
  - id: nfpm
    resource: "repo://packaging/linux/nfpm.yaml"
  - id: install-test
    resource: "repo://packaging/linux/test-install.sh"
  - id: aur-source
    resource: "repo://packaging/aur/harlo/PKGBUILD"
  - id: aur-git
    resource: "repo://packaging/aur/harlo-git/PKGBUILD"
  - id: aur-bin
    resource: "repo://packaging/aur/harlo-bin/PKGBUILD"
  - id: site
    resource: "repo://site/README.md"
  - id: download
    resource: "repo://site/download.js"
  - id: wrangler
    resource: "repo://wrangler.jsonc"
  - resource: "repo://crates/harlo-manifest-signer/"
  - resource: "repo://crates/harlo-update-signing/"
  - resource: "repo://packaging/"
  - resource: "repo://site/"
---
# Releases, signing and distribution

The release path builds Linux artifacts, checks installation on clean distributions, signs the update manifest, and publishes a GitHub Release. The website is a separate static presentation and download-discovery surface; it is not the desktop updater's signature verifier.[^workflow][^site][^download][^core]

## Prepare and publish a release

1. Add the intended release section at the top of `CHANGELOG.md`. `packaging/changelog.sh version` takes the newest version heading, and `notes [version]` extracts its text up to the next second-level heading. `packaging/release.sh --dry-run` performs release preflight without changing version files or creating the release; it still requires authenticated `gh` and fetches the remote.[^changelog][^release]
2. Run `packaging/release.sh` interactively when ready. It requires `git`, `cargo`, and authenticated `gh`, checks that the branch is `main` and matches the fetched remote, rejects an existing version tag, and permits dirty paths only for `CHANGELOG.md`, `Cargo.toml`, and `Cargo.lock`. `REMOTE` overrides remote discovery. After confirmation it updates workspace versions, commits as needed, creates an annotated `v<version>` tag, and atomically pushes main and the tag. A failed push removes the local tag and undoes the newly created commit while keeping changes.[^release]
3. The script watches `release.yml` and prints the published URL/assets. CI also supports `release-check*` branches and manual dispatch for artifact-only testing; the publish job is gated on a `v` tag and successful build/install jobs. Before publication it checks tag/version agreement and extracts changelog notes.[^release][^workflow]
4. The publish job runs `packaging/release-manifests.sh dist "$VERSION" "$GITHUB_REPOSITORY"`, then creates the GitHub Release with `dist/*` and `--verify-tag`. The signing secret is supplied only to the manifest-signing step; the GitHub token is supplied to the publication step.[^workflow]

## Signing trust boundaries

`harlo-manifest-signer` is release tooling, with the executable name `manifest-signer`; the desktop ships verification through `harlo-update-signing`, not the private-key CLI. The CLI reads `HARLO_UPDATE_SIGNING_KEY` as a base64-encoded 32-byte Ed25519 seed. `keygen` prints both key halves, `public-key` derives the public half, `sign <manifest>` replaces its signature, and `verify <manifest> [--public-key <base64>]` verifies against the built-in key unless explicitly overridden. Treat `keygen` output as secret-bearing output, not a harmless diagnostic.[^signer][^signer-cargo]

The manifest script collects four matching package files (`.deb`, `.rpm`, `.pkg.tar.zst`, `.AppImage`), requires each filename to contain the version, calculates SHA-256 hashes, and builds GitHub tag download URLs. Its count check is exactly four matching files, not a separate assertion that each extension occurs once. It signs `linux-release.json` and immediately verifies it against the compiled public key, so a mismatched signing secret stops publication. It also writes `SHA256SUMS` for packages, standalone `harlo-*-linux-*` tools, and the signed manifest.[^manifests]

**The signed manifest and the checksum listing have different authority.** The manifest authenticates its package metadata; `SHA256SUMS` is a plain checksum file, and standalone tools are included there rather than in the signed package asset list. Changing an unsigned checksum listing is not equivalent to producing a signature accepted by the desktop. Conversely, signing authenticates the output of the trusted release process, not the correctness of its build: a compromise that can use the private signing key remains inside that trust boundary.[^manifests][^signing][^workflow]

### Canonical bytes and verification

Both sides call `signing_payload`; they do not sign the serialized JSON text. The byte layout is the domain `harlo-linux-release-manifest\0v1\0`, then version, tag, repository, followed by each asset's name, SHA-256 string and download URL. Every field ends in a NUL byte, and assets are sorted by name. Consequently JSON formatting and the order of distinctly named assets do not change the payload; changing a covered field does. The signature field itself is excluded.[^signing][^signer]

The serializer's documented field-boundary assumption is that fields contain no NULs; `signing_payload` itself merely appends bytes and separators and does not validate that assumption. It is a shared encoding primitive, not a general manifest schema validator. Likewise, cryptographic verification alone does not enforce release identity: `Release::verified_by` separately requires repository `ArcticLatent/harlo` and tag `v{version}` after signature verification, and only then lowercases hashes for comparison.[^signing][^core]

Verification decodes a 32-byte public key and a 64-byte signature from base64 and checks Ed25519 over the canonical payload. `VerifyError` distinguishes malformed encodings/key material from a signature mismatch. The desktop rejects unsigned manifests, fetches the selected package from the verified release, compares its downloaded digest with the signed hash, and deletes a mismatched download before installation. A verified AppImage is replaced via a sibling `.new` file and rename; native packages are passed to the appropriate installer/elevation path.[^signing][^core][^app]

The compiled `PUBLIC_KEY_B64` is the normal trust anchor. A nonblank `HARLO_UPDATE_PUBLIC_KEY` replaces it, while `HARLO_UPDATE_MANIFEST_URL` redirects manifest retrieval: control of the desktop process environment is therefore a separate trust boundary, useful for tests but not an independent guarantee of production-key verification. The key-rotation instructions require shipping the new public key before signing releases with its private half; clients retaining the old public key cannot accept new-key signatures.[^signing][^core]

## Build packages without assuming a fully static desktop

`packaging/linux/build-in-container.sh` runs rootless Podman with a reusable Ubuntu 22.04 image and persistent Cargo/target volumes; `REBUILD` forces image rebuilding. It runs the bundler and nfpm to produce AppImage, Debian, RPM and Arch artifacts in `dist/`, with checksums. This local build path does **not** call the release-manifest signer. The container defaults to Qt 6.12.0 and nfpm 2.43.0, matching the release workflow's configured versions.[^container-build][^container][^workflow]

`bundle.sh` requires `QMAKE` pointing to Qt 6.9 or newer and builds the desktop plus helper programs. It bundles dynamically linked Qt, imported QML modules, and platform/theme/image/multimedia plugins through linuxdeploy, then produces `dist/AppDir`, an x86_64 AppImage, and `dist/opt/harlo`. AppImage build tools require FUSE or `APPIMAGE_EXTRACT_AND_RUN=1`. Licenses and third-party notices are copied into the bundle.[^bundle]

The release desktop is x86_64, built on Ubuntu 22.04's glibc 2.35 baseline, **not** a portable static binary. Native package dependencies retain system graphics, font, C/C++ runtime libraries and Git; the Debian/RPM metadata explicitly requires glibc 2.35. Dictation compilation disables build-host CPU specialization but enables AVX, AVX2, FMA and F16C, so bundling Qt does not remove the CPU requirement. nfpm installs the bundled tree at `/opt/harlo`, links `/usr/bin/harlo`, and adds menu entries, icons, and package-specific `install-kind` markers.[^workflow][^nfpm][^bundle]

The helper distinction matters: `harlo-engines` is built without default features to omit dictation, while remote copies of `harlo-agent` and `harlo-index` are built with musl. CI builds ARM helpers on an ARM runner and feeds them into the x86_64 bundle; it also publishes standalone static agent/index files for both architectures. ARM helpers are not an ARM desktop package.[^bundle][^workflow]

AUR recipes are separate distribution routes. `harlo` builds a tagged source release and `harlo-git` builds Git HEAD against Arch's Qt; both disable makepkg LTO because Rust's linker cannot consume GCC-bytecode C++ objects. `harlo-bin` extracts the release AppImage and removes `install-kind` so pacman owns updates. The tagged and binary recipes currently contain `sha256sums=('SKIP')` and instruct maintainers to set `pkgver` and run `updpkgsums`; they should not be mistaken for the desktop's signed-manifest verification path.[^aur-source][^aur-git][^aur-bin]

## Validate before publishing

CI checks helper presence/static linkage, starts the bundle offscreen, exercises AppImage version output, and renders a diagram. Its install matrix uses native package managers on Ubuntu 22.04/24.04, Debian 12, Fedora 42, openSUSE Tumbleweed and Arch, then requires `check-start: ok`, rejects missing-module/library diagnostics, and checks diagram rendering. These are installation/startup gates, not proof that every interactive desktop or graphics configuration works.[^workflow]

For local package checks, run `packaging/linux/test-install.sh dist`; `ONLY="debian:12 fedora:42"` narrows its container set. It mounts packages read-only, installs copies, starts offscreen with temporary home/runtime directories, and retains logs on failure. The signing library's unit tests independently cover valid signatures, changed hashes, wrong keys, malformed signatures, order-independent assets, field boundaries, and validity of the built-in public key.[^install-test][^signing]

## Site purpose and download behavior

`site/` is the public `harlo.run` website: plain HTML/CSS with no build step, local screenshots and fonts, and browser-side scripts. Root `wrangler.jsonc` serves it through Cloudflare Workers static assets, maps HTML paths such as `/download`, and uses `404.html` for missing pages; its documented deploy command is `npx wrangler deploy` on pushes to main.[^site][^wrangler]

The download script uses browser hints to highlight a distribution, fetches the latest release from GitHub's API, and substitutes matching package asset URLs, names, sizes and version text. If fetching fails, it silently keeps the page's latest-release fallback links. It neither loads `linux-release.json` nor performs its signature/hash checks: website download discovery and the authenticated desktop updater are intentionally different paths in the implementation.[^download][^site][^core][^app]

[^release]: packaging/release.sh
[^changelog]: packaging/changelog.sh
[^workflow]: .github/workflows/release.yml
[^manifests]: packaging/release-manifests.sh
[^signer]: crates/harlo-manifest-signer/src/main.rs
[^signer-cargo]: crates/harlo-manifest-signer/Cargo.toml
[^signing]: crates/harlo-update-signing/src/lib.rs
[^core]: crates/harlo-core/src/updates.rs
[^app]: crates/harlo-app/src/updates.rs
[^bundle]: packaging/linux/bundle.sh
[^container-build]: packaging/linux/build-in-container.sh
[^container]: packaging/linux/Containerfile
[^nfpm]: packaging/linux/nfpm.yaml
[^install-test]: packaging/linux/test-install.sh
[^aur-source]: packaging/aur/harlo/PKGBUILD
[^aur-git]: packaging/aur/harlo-git/PKGBUILD
[^aur-bin]: packaging/aur/harlo-bin/PKGBUILD
[^site]: site/README.md
[^download]: site/download.js
[^wrangler]: wrangler.jsonc
