#!/usr/bin/env bash
# Writes Harlo's bundled colour schemes: crates/harlo-core/themes/tinted.json.
#
#   tools/themes-update.sh            # the pinned commit, in themes/TINTED_COMMIT
#   tools/themes-update.sh <commit>   # move to another commit of the collection
#
# The schemes are Tinted Theming's Base16 ones (MIT), from
# https://github.com/tinted-theming/schemes. Harlo bundles the widely used
# families listed below, not the whole collection. Each entry is the scheme's
# file name and the name Settings shows for it.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
themes=$root/crates/harlo-core/themes
commit=${1:-$(<"$themes/TINTED_COMMIT")}

schemes=(
  "github-dark|GitHub Dark"
  "github-dark-dimmed|GitHub Dark Dimmed"
  "github|GitHub Light"
  "onedark|One Dark"
  "one-light|One Light"
  "dracula|Dracula"
  "catppuccin-mocha|Catppuccin Mocha"
  "catppuccin-macchiato|Catppuccin Macchiato"
  "catppuccin-frappe|Catppuccin Frappé"
  "catppuccin-latte|Catppuccin Latte"
  "solarized-dark|Solarized Dark"
  "solarized-light|Solarized Light"
  "gruvbox-dark-medium|Gruvbox Dark"
  "gruvbox-light-medium|Gruvbox Light"
  "tokyo-night-dark|Tokyo Night"
  "tokyo-night-storm|Tokyo Night Storm"
  "tokyo-night-moon|Tokyo Night Moon"
  "tokyo-night-light|Tokyo Night Light"
  "nord|Nord"
  "nord-light|Nord Light"
  "kanagawa|Kanagawa"
  "kanagawa-dragon|Kanagawa Dragon"
  "everforest-dark-medium|Everforest Dark"
  "everforest-light-medium|Everforest Light"
  "nightfox|Nightfox"
  "dayfox|Dayfox"
  "flexoki-dark|Flexoki Dark"
  "flexoki-light|Flexoki Light"
  "rose-pine|Rosé Pine"
  "rose-pine-moon|Rosé Pine Moon"
  "rose-pine-dawn|Rosé Pine Dawn"
  "ayu-dark|Ayu Dark"
  "ayu-mirage|Ayu Mirage"
  "ayu-light|Ayu Light"
  "monokai|Monokai"
  "material|Material"
  "material-palenight|Material Palenight"
  "material-lighter|Material Lighter"
)

work=$(mktemp -d "${TMPDIR:-/tmp}/harlo-tinted.XXXXXX")
trap 'rm -rf "$work"' EXIT
git -C "$work" init -q schemes
git -C "$work/schemes" fetch -q --depth 1 https://github.com/tinted-theming/schemes "$commit"
git -C "$work/schemes" checkout -q FETCH_HEAD

python3 -I - "$work/schemes" "$themes/tinted.json" "${schemes[@]}" <<'EOF'
"""Reads the listed Base16 schemes and writes them as one JSON list."""
import json
import re
import sys

folder, out, *entries = sys.argv[1:]
line = re.compile(r'^\s*([A-Za-z0-9_-]+):\s*"([^"]*)"')
result = []
for entry in entries:
    slug, name = entry.split("|", 1)
    fields, palette = {}, {}
    with open(f"{folder}/base16/{slug}.yaml", encoding="utf-8") as f:
        for text in f:
            match = line.match(text)
            if not match:
                continue
            key, value = match.groups()
            if re.fullmatch(r"base0[0-9A-F]", key):
                palette[key] = value.lower()
            else:
                fields[key] = value
    if len(palette) != 16 or fields.get("variant") not in ("dark", "light"):
        sys.exit(f"{slug}: not a complete Base16 scheme")
    result.append({
        "id": slug,
        "name": name,
        "author": fields.get("author", ""),
        "variant": fields["variant"],
        "palette": palette,
    })
with open(out, "w", encoding="utf-8") as f:
    json.dump(result, f, indent=1, ensure_ascii=False)
    f.write("\n")
print(f"wrote {len(result)} schemes")
EOF
cp "$work/schemes/LICENSE" "$themes/TINTED-LICENSE"
printf '%s\n' "$commit" > "$themes/TINTED_COMMIT"
echo "tinted-theming/schemes at $commit"
