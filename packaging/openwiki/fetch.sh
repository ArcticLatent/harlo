#!/usr/bin/env bash
# OpenWiki for harlo: the pinned Node (node.env) and the pinned openwiki
# package (package.json, package-lock.json), in one folder harlo starts it
# from:
#
#   packaging/openwiki/fetch.sh DEST          # e.g. target/openwiki
#   packaging/openwiki/fetch.sh --lock        # refresh package-lock.json
#
# DEST/bin/node, DEST/node_modules/openwiki/dist/cli/cli.js, and
# DEST/LICENSES (Node's, OpenWiki's, every package's). DEST/VERSION says
# what's in it; a DEST already holding these versions is left as it is.
# For this machine's architecture (x86_64 or aarch64). Needs curl, tar, xz
# and the network. The Node download is kept in
# ${XDG_CACHE_HOME:-~/.cache}/harlo-build and checked against node.env's
# SHA-256.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=node.env
. "$here/node.env"
arg=${1:?usage: fetch.sh DEST | --lock}

cache=${XDG_CACHE_HOME:-$HOME/.cache}/harlo-build
case $(uname -m) in
    x86_64) arch=x64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo "fetch.sh: no Node build for $(uname -m)" >&2; exit 1 ;;
esac
sha_var=NODE_SHA256_$arch
NODE_SHA256=${!sha_var}
tarball=node-v$NODE_VERSION-linux-$arch.tar.xz
mkdir -p "$cache"
if ! echo "$NODE_SHA256  $cache/$tarball" | sha256sum -c --status 2>/dev/null; then
    curl -fsSL -o "$cache/$tarball.part" "https://nodejs.org/dist/v$NODE_VERSION/$tarball"
    echo "$NODE_SHA256  $cache/$tarball.part" | sha256sum -c --status || { echo "fetch.sh: $tarball doesn't match node.env's SHA-256" >&2; exit 1; }
    mv "$cache/$tarball.part" "$cache/$tarball"
fi

# The whole Node (npm with it) in a scratch folder, for installing.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -xJf "$cache/$tarball" -C "$work"
nodedir=$work/node-v$NODE_VERSION-linux-$arch
export PATH="$nodedir/bin:$PATH" npm_config_cache="$work/npm-cache" npm_config_update_notifier=false npm_config_fund=false npm_config_audit=false

if [ "$arg" = --lock ]; then
    cp "$here/package.json" "$work/"
    (cd "$work" && npm install --package-lock-only --omit=dev >/dev/null)
    cp "$work/package-lock.json" "$here/package-lock.json"
    echo "package-lock.json: openwiki $(sed -n 's/.*"openwiki": "\(.*\)".*/\1/p' "$here/package.json" | head -n 1)"
    exit 0
fi

dest=$arg
openwiki=$(sed -n 's/.*"openwiki": "\(.*\)".*/\1/p' "$here/package.json" | head -n 1)
want="openwiki $openwiki, node $NODE_VERSION $arch, lock $(sha256sum "$here/package-lock.json" | cut -c1-16)"
if [ -f "$dest/VERSION" ] && [ "$(cat "$dest/VERSION")" = "$want" ] && [ -x "$dest/bin/node" ]; then
    exit 0
fi

rm -rf "$dest"
mkdir -p "$dest/bin" "$dest/LICENSES"
cp "$nodedir/bin/node" "$dest/bin/node"
cp "$nodedir/LICENSE" "$dest/LICENSES/node.txt"
cp "$here/package.json" "$here/package-lock.json" "$dest/"
# (Its own Node fetches better-sqlite3's prebuilt binary for this Node.)
(cd "$dest" && npm ci --omit=dev --no-audit --no-fund >/dev/null 2>&1)
[ -f "$dest/node_modules/better-sqlite3/build/Release/better_sqlite3.node" ] || { echo "fetch.sh: better-sqlite3's binary wasn't installed" >&2; exit 1; }
cp "$dest/node_modules/openwiki/LICENSE" "$dest/LICENSES/openwiki.txt"
# Every package's licence, in one file.
"$dest/bin/node" -e '
const fs = require("fs"), path = require("path");
const root = process.argv[1], out = [];
const seen = new Set();
(function walk(dir) {
  for (const name of fs.existsSync(dir) ? fs.readdirSync(dir).sort() : []) {
    if (name.startsWith(".")) continue;
    const p = path.join(dir, name);
    if (name.startsWith("@")) { walk(p); continue; }
    const pj = path.join(p, "package.json");
    if (!fs.existsSync(pj)) continue;
    const meta = JSON.parse(fs.readFileSync(pj, "utf8"));
    const id = meta.name + "@" + meta.version;
    if (!seen.has(id)) {
      seen.add(id);
      const lic = fs.readdirSync(p).find(f => /^(licen[cs]e|copying)/i.test(f));
      out.push("== " + id + " (" + (meta.license || "see below") + ")\n" + (lic ? fs.readFileSync(path.join(p, lic), "utf8").trim() : "") + "\n");
    }
    walk(path.join(p, "node_modules"));
  }
})(path.join(root, "node_modules"));
fs.writeFileSync(path.join(root, "LICENSES", "packages.txt"), out.join("\n"));
' "$dest"
# Smaller (about 440 MB to 250): what only builds and editors read (source
# maps, type declarations, docs), better-sqlite3's C sources once built,
# and Node's symbols.
rm -rf "$dest/node_modules/.cache" "$dest/node_modules/better-sqlite3/deps" "$dest/node_modules/better-sqlite3/src"
find "$dest/node_modules" -type f \( -name '*.map' -o -name '*.d.ts' -o -name '*.d.mts' -o -name '*.d.cts' -o -name '*.md' -o -name '*.markdown' \) ! -path '*/node_modules/openwiki/*' -delete
if command -v strip >/dev/null; then strip "$dest/bin/node"; fi
echo "$want" > "$dest/VERSION"
