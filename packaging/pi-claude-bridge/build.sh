#!/bin/sh
# Builds harlo's built-in Claude subscription extension for pi into <folder>:
#
#   <folder>/node_modules/pi-claude-bridge   the extension pi loads (-e)
#   <folder>/sdk.json                        the Agent SDK harlo fetches on
#                                            first use: { version, url, integrity,
#                                            cli: { <platform>: { url, integrity } } }
#
# pi-claude-bridge and its packages are MIT and ship with harlo. Anthropic's
# Agent SDK isn't redistributed: harlo downloads it the first time (checked
# against the integrity pinned here). Its 300 MB copy of Claude Code is left
# out too: a small stand-in under its package name starts the user's own
# Claude Code, or, when they have none, the SDK's copy harlo downloads then
# (sdk.json's cli). Needs npm and node.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?usage: build.sh <folder>}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$here/package.json" "$here/package-lock.json" "$here/.npmrc" "$tmp/"
(cd "$tmp" && npm ci --omit=optional --ignore-scripts --no-audit --no-fund >/dev/null)
node -e '
const all = require(process.argv[1]).packages, l = all["node_modules/@anthropic-ai/claude-agent-sdk"];
const cli = {};
for (const p of ["linux-x64", "linux-x64-musl", "linux-arm64", "linux-arm64-musl"]) {
  const c = all["node_modules/@anthropic-ai/claude-agent-sdk-" + p];
  if (c) cli[p] = { url: c.resolved, integrity: c.integrity };
}
process.stdout.write(JSON.stringify({ version: l.version, url: l.resolved, integrity: l.integrity, cli }) + "\n");
' "$tmp/package-lock.json" > "$tmp/sdk.json"
rm -rf "$tmp/node_modules/@anthropic-ai/claude-agent-sdk" "$tmp/node_modules/.package-lock.json"
stand_in="$tmp/node_modules/@anthropic-ai/claude-agent-sdk-linux-x64"
mkdir -p "$stand_in"
printf '{"name":"@anthropic-ai/claude-agent-sdk-linux-x64","version":"0.0.0-harlo","private":true}\n' > "$stand_in/package.json"
install -m755 "$here/claude" "$stand_in/claude"
rm -rf "$out"
mkdir -p "$out"
mv "$tmp/node_modules" "$tmp/sdk.json" "$out/"
echo "pi-claude-bridge in $out ($(du -sh "$out" | cut -f1))"
