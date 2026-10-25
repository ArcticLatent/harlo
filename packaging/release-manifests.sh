#!/usr/bin/env bash
# Writes and signs a release's update manifest, next to its files in a
# folder (the release workflow's publish job):
#
#   packaging/release-manifests.sh <dir> <version> <owner/repo>
#
#   linux-release.json   the .deb, .rpm and Arch packages and the AppImage,
#                        with their SHA-256 sums and addresses: what harlo
#                        reads to update itself
#   SHA256SUMS           every file's checksum, for people
#
# The manifest is signed with HARLO_UPDATE_SIGNING_KEY (crates/harlo-
# manifest-signer) and then checked against the public key built into
# harlo, so a wrong key stops the release instead of shipping an update
# nobody can install.
set -euo pipefail

DIR="${1:?usage: $0 <dir> <version> <owner/repo>}"
VERSION="${2:?version missing}"
REPOSITORY="${3:?repository missing}"
TAG="v$VERSION"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_URL="https://github.com/$REPOSITORY/releases/download/$TAG"

[[ -n "${HARLO_UPDATE_SIGNING_KEY:-}" ]] || {
  echo "HARLO_UPDATE_SIGNING_KEY is not set" >&2
  exit 1
}

sha() { sha256sum "$1" | awk '{print $1}'; }
signer() { cargo run --quiet --release --locked --manifest-path "$ROOT/Cargo.toml" -p harlo-manifest-signer -- "$@"; }

mapfile -t packages < <(find "$DIR" -maxdepth 1 -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.pkg.tar.zst' -o -name '*.AppImage' \) -printf '%f\n' | sort)
((${#packages[@]} == 4)) || {
  echo "expected a .deb, an .rpm, an Arch package and an AppImage in $DIR, found: ${packages[*]:-none}" >&2
  exit 1
}
for name in "${packages[@]}"; do
  [[ "$name" == *"$VERSION"* ]] || {
    echo "$name isn't version $VERSION" >&2
    exit 1
  }
done

for name in "${packages[@]}"; do
  jq -n --arg name "$name" --arg sha "$(sha "$DIR/$name")" --arg url "$BASE_URL/$name" \
    '{name: $name, sha256: $sha, download_url: $url}'
done | jq -s --arg version "$VERSION" --arg tag "$TAG" --arg repo "$REPOSITORY" \
  '{version: $version, tag: $tag, repository: $repo, assets: .}' >"$DIR/linux-release.json"

signer sign "$DIR/linux-release.json"
signer verify "$DIR/linux-release.json"

(cd "$DIR" && sha256sum "${packages[@]}" linux-release.json >SHA256SUMS)
echo "Manifest for $TAG:"
cat "$DIR/SHA256SUMS"
