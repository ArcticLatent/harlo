#!/usr/bin/env bash
# Moves Harlo to an OpenCode release.
#
#   tools/opencode-bump.sh          # the latest release (highest vX.Y.Z tag)
#   tools/opencode-bump.sh 2.4.0    # a specific release
#
# Fetches that tag's OpenAPI schema, prints what changed against the pinned one,
# installs the release's verified Linux binary as the test server, then updates
# OPENCODE_VERSION, schema/openapi.json, the generated types and the bundled
# desktop themes (with OpenCode's licence). It commits
# nothing: review the output, run the tests, and commit the bump on its own.
set -euo pipefail

repo=anomalyco/opencode
root=$(cd "$(dirname "$0")/.." && pwd)
client=$root/crates/opencode-client
current=$(<"$client/OPENCODE_VERSION")

if [[ $# -ge 1 ]]; then
  target=${1#v}
else
  target=$(gh api "repos/$repo/releases?per_page=50" \
    --jq '.[] | select(.draft | not) | select(.prerelease | not) | .tag_name' \
    | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sed 's/^v//' | sort -V | tail -n 1)
fi
[[ $target =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "not a release version: $target" >&2; exit 1; }
echo "OpenCode $current -> $target"

work=$(mktemp -d "${TMPDIR:-/tmp}/opencode-bump.XXXXXX")
trap 'rm -rf "$work"' EXIT

gh api -H 'Accept: application/vnd.github.raw' \
  "repos/$repo/contents/packages/protocol/openapi.json?ref=v$target" > "$work/openapi.json"
(cd "$root" && cargo run -q -p opencode-gen -- diff "$client/schema/openapi.json" "$work/openapi.json")

# The test server: the release's own binary, checked against its SHA256SUMS.
bin_dir=${HARLO_DEV_DIR:-$HOME/.local/share/harlo-dev}/opencode/$target
if [[ ! -x $bin_dir/opencode ]]; then
  case $(uname -m) in
    x86_64) arch=x64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *) echo "no OpenCode build for $(uname -m)" >&2; exit 1 ;;
  esac
  asset=opencode-linux-$arch.tar.gz
  gh release download "v$target" -R "$repo" -p "$asset" -p SHA256SUMS -D "$work"
  (cd "$work" && grep -E "[[:space:]]\*?$asset\$" SHA256SUMS | sha256sum -c --quiet -)
  mkdir -p "$work/unpacked" "$bin_dir"
  tar -xzf "$work/$asset" -C "$work/unpacked"
  found=$(find "$work/unpacked" -type f -name opencode | head -n 1)
  [[ -n $found ]] || { echo "no opencode binary in $asset" >&2; exit 1; }
  install -m 755 "$found" "$bin_dir/opencode"
fi
# Ask the version with a scratch home so nothing touches the real OpenCode setup.
mkdir -p "$work/home"
reported=$(env HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" XDG_DATA_HOME="$work/home/.local/share" \
  XDG_STATE_HOME="$work/home/.local/state" XDG_CACHE_HOME="$work/home/.cache" TMPDIR="$work" \
  "$bin_dir/opencode" --version)
reported=${reported##* }   # "opencode v2.3.2" -> "v2.3.2"
reported=${reported#v}
[[ $reported == "$target" ]] || { echo "binary reports $reported, expected $target" >&2; exit 1; }
echo "test server: $bin_dir/opencode ($reported)"

# The desktop app's built-in themes, which Harlo bundles; fetched before
# anything changes, so a failed download leaves the old set in place.
themes=$root/crates/harlo-core/themes/opencode
mkdir -p "$work/themes"
for name in $(gh api "repos/$repo/contents/packages/ui/src/theme/themes?ref=v$target" \
    --jq '.[] | select(.type == "file") | .name' | grep -E '^[a-z0-9-]+\.json$'); do
  gh api -H 'Accept: application/vnd.github.raw' \
    "repos/$repo/contents/packages/ui/src/theme/themes/$name?ref=v$target" > "$work/themes/$name"
done
gh api -H 'Accept: application/vnd.github.raw' "repos/$repo/contents/LICENSE?ref=v$target" > "$work/themes/LICENSE"
echo "themes: $(ls "$work/themes" | grep -c '\.json$')"

cp "$work/openapi.json" "$client/schema/openapi.json"
rm -f "$themes"/*.json
cp "$work/themes"/* "$themes/"
printf '%s\n' "$target" > "$client/OPENCODE_VERSION"
(cd "$root" && cargo run -q -p opencode-gen -- "$client/schema/openapi.json" "$client/src/generated.rs" \
  && rustfmt --edition 2024 "$client/src/generated.rs")
# The colours OpenCode's own theme code gives the new themes, which Harlo's
# port of it is tested against.
"$root/tools/opencode-theme-reference.sh"

echo
echo "Next: cargo test --workspace, update the client (and the theme port, if its"
echo "colours now differ) for any changes above, then commit the bump."
