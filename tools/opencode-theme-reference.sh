#!/usr/bin/env bash
# Writes the colours OpenCode's desktop app gives each bundled theme, for
# Harlo's theme tests: crates/harlo-core/tests/fixtures/opencode-resolved.json.
#
#   tools/opencode-theme-reference.sh
#
# It fetches OpenCode's own theme code (resolve.ts, color.ts, types.ts) at the
# pinned version into a scratch folder and runs it with Node on every theme in
# crates/harlo-core/themes/opencode. Harlo's port of that code must give the
# same colours. opencode-bump.sh runs it once the themes and version are new.
set -euo pipefail

repo=anomalyco/opencode
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(<"$root/crates/opencode-client/OPENCODE_VERSION")
work=$(mktemp -d "${TMPDIR:-/tmp}/opencode-themes.XXXXXX")
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/theme"
for name in resolve color types; do
  gh api -H 'Accept: application/vnd.github.raw' \
    "repos/$repo/contents/packages/ui/src/theme/$name.ts?ref=v$version" > "$work/theme/$name.ts"
done
# Node runs TypeScript as it is, but wants the file extensions spelled out.
sed -i -E 's#from "\./(color|types)"#from "./\1.ts"#' "$work/theme/resolve.ts" "$work/theme/color.ts"

cd "$work"
node "$root/tools/opencode-theme-reference.mjs" "$work/theme/resolve.ts" \
  "$root/crates/harlo-core/themes/opencode" \
  > "$root/crates/harlo-core/tests/fixtures/opencode-resolved.json"
echo "wrote crates/harlo-core/tests/fixtures/opencode-resolved.json (OpenCode $version)"
