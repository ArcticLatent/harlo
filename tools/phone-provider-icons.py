#!/usr/bin/env python3
"""Writes the phone app's model-provider logos from Lobe Icons.

    python3 -I tools/phone-provider-icons.py

It downloads the pinned `@lobehub/icons-static-svg` package (MIT) from npm,
checks its checksum, and writes one image set per provider into
`ios/Harlo/Shared/Shared.xcassets` as `Maker-<id>`, where `<id>` is the
logo's name as the computer sends it to the phone
(`crates/harlo-core/src/makers.rs`).

A logo in one colour is kept as a vector and tinted by the app; one in its
own colours is drawn to PNGs with `rsvg-convert` (librsvg), so the app shows
exactly what this script drew. The tile colour behind each logo is in
`ios/HarloKit/Sources/HarloKit/Features.swift` (`Palette.makerTile`).

Providers Lobe Icons doesn't carry keep the image set they have.
"""

import hashlib
import io
import json
import re
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

VERSION = "1.106.0"
PACKAGE = f"https://registry.npmjs.org/@lobehub/icons-static-svg/-/icons-static-svg-{VERSION}.tgz"
SHA256 = "fb1900cf0cfddf6a5ef947db57f951335602ad422bffff03ba9dc31390572b34"

ASSETS = Path(__file__).resolve().parent.parent / "ios/Harlo/Shared/Shared.xcassets"

# Provider logo id -> (Lobe icon, drawn in its own colours, the colour of
# the parts a coloured logo leaves to its surroundings: black when None).
ICONS = {
    "anthropic": ("anthropic", False, None),
    "bailian-coding-plan": ("bailian-color", True, None),
    "cerebras": ("cerebras", False, None),
    "claude-code": ("claudecode", False, None),
    "cline": ("cline", False, None),
    "command-code": ("commandcode", False, None),
    "cursor": ("cursor", False, None),
    "deepseek": ("deepseek", False, None),
    "fireworks": ("fireworks", False, None),
    "github-copilot": ("githubcopilot", False, None),
    "google": ("google-color", True, None),
    "groq": ("groq", False, None),
    # Its letter is drawn on black, so it is white; the dot keeps its blue.
    "kimi-for-coding": ("kimi-color", True, "#fff"),
    "llmapi": ("llmapi-color", True, None),
    "llmapi-detailed": ("llmapi-color", True, None),
    "lmstudio": ("lmstudio", False, None),
    "minimax-cn-coding-plan": ("minimax", False, None),
    "minimax-coding-plan": ("minimax", False, None),
    "mistral": ("mistral", False, None),
    "ollama": ("ollama", False, None),
    "openai": ("openai", False, None),
    "opencode": ("opencode", False, None),
    "openrouter": ("openrouter", False, None),
    "together": ("together-color", True, None),
    "vllm": ("vllm-color", True, None),
    "wafer.ai": ("wafer", False, None),
    "xai": ("xai", False, None),
    "zai-coding-plan": ("zai", False, None),
    "zhipuai-coding-plan": ("zhipu", False, None),
}

# A coloured logo's PNG sizes: 32 points at 1x, 2x and 3x.
POINTS = 32


def package() -> dict[str, str]:
    """The package's icons, by name."""
    with urllib.request.urlopen(PACKAGE, timeout=120) as reply:
        data = reply.read()
    found = hashlib.sha256(data).hexdigest()
    if found != SHA256:
        sys.exit(f"the package's checksum is {found}, not the pinned one")
    icons = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        for member in tar:
            path = Path(member.name)
            if member.isfile() and path.parent.name == "icons" and path.suffix == ".svg":
                icons[path.stem] = tar.extractfile(member).read().decode()
    return icons


def clean(svg: str, fill: str | None) -> str:
    """The icon as a plain 24-point SVG: no CSS sizes, no title."""
    head, body = re.match(r"<svg([^>]*)>(.*)</svg>\s*$", svg, re.S).groups()
    body = re.sub(r"<title>.*?</title>", "", body, flags=re.S)
    attributes = 'xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"'
    if fill:
        attributes += f' fill="{fill}"'
    if "fill-rule" in head:
        attributes += ' fill-rule="evenodd"'
    return f"<svg {attributes}>{body}</svg>\n"


def write(maker: str, svg: str, coloured: bool, fill: str | None) -> None:
    """Writes the image set `Maker-<maker>`."""
    folder = ASSETS / f"Maker-{maker}.imageset"
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.iterdir():
        old.unlink()
    properties = {"template-rendering-intent": "original" if coloured else "template"}
    if coloured:
        images = []
        for scale in (1, 2, 3):
            name = f"maker-{maker}@{scale}x.png"
            side = str(POINTS * scale)
            png = subprocess.run(
                ["rsvg-convert", "-w", side, "-h", side],
                input=clean(svg.replace("currentColor", fill or "#000"), fill).encode(),
                capture_output=True,
                check=True,
            ).stdout
            (folder / name).write_bytes(png)
            images.append({"filename": name, "idiom": "universal", "scale": f"{scale}x"})
    else:
        name = f"maker-{maker}.svg"
        (folder / name).write_text(clean(svg.replace("currentColor", "#000"), "#000"))
        images = [{"filename": name, "idiom": "universal"}]
        properties["preserves-vector-representation"] = True
    contents = {"images": images, "info": {"author": "xcode", "version": 1}, "properties": properties}
    (folder / "Contents.json").write_text(json.dumps(contents, indent=2, sort_keys=True) + "\n")


def main() -> None:
    icons = package()
    for maker, (icon, coloured, fill) in sorted(ICONS.items()):
        write(maker, icons[icon], coloured, fill)
    print(f"{len(ICONS)} provider logos from Lobe Icons {VERSION} in {ASSETS}")


if __name__ == "__main__":
    main()
