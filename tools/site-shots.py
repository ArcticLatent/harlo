#!/usr/bin/env python3
"""Takes the website's and the README's screenshots in the real Harlo window.

    python3 -I tools/site-shots.py [--modes dark,light] [--sizes wide,narrow]
                                   [--only scene,…] [--keep]
    python3 -I tools/site-shots.py cut

Each colour mode gets its own Harlo in its own sandbox (tools/gui-drive.py:
a private server with the scripted model, a scratch HOME, an invisible
display), drawn at twice its size: a 1440×900 window as 2880×1800 pictures,
and a 760×940 one with larger text for the pictures a phone shows.
The sessions are sample work in two scratch projects, `webapp` and `api`;
nothing outside the scratch folders is read or changed, and no request
leaves this computer.

The pictures go to target/site-shots/<mode>-<size>/. `cut` then writes the site's
pictures from them (site/assets/*.webp; site/README.md says which is which).

Build first: `cargo build -p harlo-server` and
`cargo build -p harlo --features slint/mcp --target-dir target/mcp`.
"""

import json
import os
import subprocess
import sys
import time
import traceback

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
VERIFY = os.path.join(ROOT, "tools", "verify")
sys.path[:0] = [VERIFY, os.path.join(VERIFY, "scenarios")]

from driver import Failed, Harlo  # noqa: E402
from _common import composer, send, settings, sidebar  # noqa: E402

OUT = os.path.join(ROOT, "target", "site-shots")
# The windows as a person sees them, and how many pixels draw each of their
# points. The narrow one, with larger text and no sidebar, is for pictures
# shown on a phone.
SIZES = {"wide": (1440, 900), "narrow": (760, 940)}
SCALE = 2

# The model the sample sessions name. The scripted model answers for it.
MODEL = "GPT-6.2 Sol"
PROVIDER = "OpenAI"

GUARD = """import { getSession } from "./session";

const LOGIN = "/login";

/** Sends a visitor who isn't signed in to the login page. */
export function guard(request: Request): Response | null {
  const url = new URL(request.url);
  if (getSession(request)) return null;

  const returnTo = url.searchParams.get("returnTo") ?? url.pathname;
  if (returnTo === LOGIN) return null;
  return Response.redirect(`${LOGIN}?returnTo=${returnTo}`);
}
"""

GUARD_OLD = """  const returnTo = url.searchParams.get("returnTo") ?? url.pathname;
  if (returnTo === LOGIN) return null;
  return Response.redirect(`${LOGIN}?returnTo=${returnTo}`);
}
"""

GUARD_NEW = """  const returnTo = safePath(url.searchParams.get("returnTo") ?? url.pathname);
  if (returnTo === LOGIN) return null;
  return Response.redirect(`${LOGIN}?returnTo=${encodeURIComponent(returnTo)}`);
}

/** A path on this site, decoded; anything else goes home. */
function safePath(raw: string): string {
  const path = decodeURIComponent(raw);
  return path.startsWith("/") && !path.startsWith("//") ? path : "/";
}
"""

GUARD_TEST = """import { expect, test } from "vitest";
import { guard } from "../src/auth/guard";

const visit = (path: string) => guard(new Request(`https://shop.test${path}`));

test("an encoded login path doesn't loop", () => {
  expect(visit("/login?returnTo=%2Flogin")).toBeNull();
});

test("another site's address goes home", () => {
  const to = visit("/cart?returnTo=//evil.test")?.headers.get("location");
  expect(to).toBe("/login?returnTo=%2F");
});
"""

FOUND = (
    "The guard compares the raw `returnTo` with `/login`, so an encoded path "
    "(`%2Flogin`) slips past it and the login page sends you to itself. Two ways to fix it:\n\n"
    "1. Decode `returnTo` before checking it.\n"
    "2. Allow same-site paths only, which also closes an open redirect.\n\n"
    "I'd do both."
)

FIXED = (
    "Fixed. `guard` now decodes `returnTo` and keeps only paths on this site, "
    "so an encoded `/login` no longer loops and another site's address goes home.\n\n"
    "- `src/auth/guard.ts`: `safePath` decodes and checks the path\n"
    "- `test/return-to.test.ts`: the loop, and an address on another site\n\n"
    "Both tests pass. The changes are in the Changes panel when you want to read them."
)


def sh(folder, *argv):
    subprocess.run(argv, cwd=folder, check=True, capture_output=True,
                   env=dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1"))


def write(folder, path, text):
    full = os.path.join(folder, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w") as f:
        f.write(text)


def project(sandbox, name, files):
    """A git project in the sandbox with `files` committed."""
    folder = os.path.join(sandbox, name)
    os.makedirs(folder, exist_ok=True)
    for path, text in files.items():
        write(folder, path, text)
    sh(folder, "git", "init", "-q", "-b", "main")
    sh(folder, "git", "add", "-A")
    sh(folder, "git", "-c", "user.name=Sam", "-c", "user.email=sam@example.com",
       "commit", "-q", "-m", "Start")
    return folder


def name_model(sandbox):
    """Gives the scripted model the name the pictures show. The server reads
    the file again as it's saved."""
    path = os.path.join(sandbox, "config", "harlo", "harlo.json")
    with open(path) as f:
        config = json.load(f)
    fake = config["providers"]["fake"]
    fake["name"] = PROVIDER
    fake["models"]["fake-coder"]["name"] = MODEL
    with open(path, "w") as f:
        json.dump(config, f, indent=2)


def script(h, prompt, steps):
    """Keeps the scripted model's answer to `prompt` (tools/fake-model.py)."""
    path = os.path.join(h.info["sandbox"], "scripts.json")
    try:
        with open(path) as f:
            scripts = json.load(f)
    except (OSError, ValueError):
        scripts = {}
    scripts[prompt] = [[list(part) for part in step] for step in steps]
    with open(path, "w") as f:
        json.dump(scripts, f)


def session(h, folder, title):
    created = h.api("POST", "/api/session", {"location": {"directory": folder}})["data"]
    h.api("PATCH", f"/api/session/{created['id']}", {"title": title})
    return created["id"]


def ask(h, sid, prompt, steps):
    """Sends `prompt` to a session through the server, as another client would."""
    script(h, prompt, steps)
    h.api("POST", f"/api/session/{sid}/prompt", {"text": prompt})


def sidebar_shown(h):
    return bool(h.find("New session", role="Button", where=sidebar))


def open_session(h, title, narrow=False):
    """Opens a session from the sidebar; a narrow window shows the sidebar
    only for that."""
    if not sidebar_shown(h):
        h.click("Toggle sidebar")
        h.settle(0.6)
    h.wait(title, role=None, where=sidebar, timeout=20)
    h.settle(0.5)
    for attempt in range(2):
        h.click(title, role=None, where=sidebar)
        try:
            h.wait(None, role="TextInput", where=composer, timeout=8)
            break
        except Failed:
            if attempt:
                raise
    if narrow and sidebar_shown(h):
        h.click("Toggle sidebar")
        h.settle(0.6)


def leave_settings(h):
    """Closes Settings from any of its pages (Escape first leaves a page)."""
    for _ in range(4):
        if not h.find("Search settings", role="TextInput"):
            return
        h.key("Escape")
        h.settle(0.5)


def rail(h):
    return lambda e: e.x > h.size[0] - 70


class Sitting:
    """One Harlo in one colour mode, and the pictures it has taken."""

    def __init__(self, h, mode, size):
        self.h = h
        self.mode = mode
        self.narrow = size == "narrow"
        self.sandbox = h.info["sandbox"]

    def cap(self, name):
        self.h.settle(0.6)
        path = self.h.shot(name)
        final = os.path.join(os.path.dirname(path), f"{name}.png")
        os.replace(path, final)
        print(f"      {os.path.relpath(final, ROOT)}", flush=True)
        return final


def scene_sessions(s):
    """The two projects and their sessions: one fixed, one at work, one
    waiting for a yes."""
    h = s.h
    name_model(s.sandbox)
    webapp = project(s.sandbox, "webapp", {
        "package.json": '{\n  "name": "webapp",\n  "scripts": { "test": "node scripts/test.mjs" }\n}\n',
        "scripts/test.mjs": 'console.log("✓ test/return-to.test.ts (2 tests)\\n\\nTest Files  1 passed (1)\\n     Tests  2 passed (2)");\n',
        "src/auth/guard.ts": GUARD,
        "src/auth/session.ts": "export function getSession(request: Request) {\n"
                               "  return request.headers.get(\"cookie\")?.includes(\"session=\") ?? false;\n}\n",
        "README.md": "# webapp\n\nThe shop's front end.\n",
    })
    api = project(s.sandbox, "api", {
        "README.md": "# api\n\nThe shop's API.\n",
        "CHANGELOG.md": "# Changelog\n\n## 2.3\n\n- Orders can be exported.\n",
        ".github/workflows/ci.yml": "name: ci\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n"
                                    "    steps:\n      - uses: actions/checkout@v6\n      - run: cargo test\n",
    })
    s.webapp, s.api = webapp, api

    notes = session(h, api, "Write the 2.4 release notes")
    ask(h, notes, "Write the release notes for 2.4 from the commits since 2.3.",
        [[("say", "Here are the notes for 2.4, from the nine commits since 2.3.")]])
    cache = session(h, api, "Why is the CI cache missing?")
    ask(h, cache, "The CI cache misses on every run. Find out why.",
        [[("think", "The cache key first."), ("tool", 'read {"path":".github/workflows/ci.yml"}')],
         [("tool", 'shell {"command":"gh run list --limit 5","description":"List the last runs"}')],
         [("say", "The key has the run's number in it, so it never matches.")]])
    editor = session(h, webapp, "Upgrade the editor to React 19")
    ask(h, editor, "Upgrade the editor package to React 19.",
        [[("say", "Upgraded. The editor builds and its tests pass on React 19.")]])
    limits = session(h, webapp, "Add rate limiting to the API client")
    s.limits = limits
    s.fix = session(h, webapp, "Fix the login redirect loop")


def scene_hero(s):
    """The fix, read back: the agent's turn, and its two files in the
    Changes panel, while another session works."""
    h = s.h
    open_session(h, "Fix the login redirect loop", s.narrow)
    first = "After signing in, the login page sends me back to itself. Find out why."
    script(h, first, [
        [("think", "The redirect is built in the auth guard, so that's where to look."),
         ("tool", 'grep {"pattern":"returnTo"}')],
        [("tool", 'read {"path":"src/auth/guard.ts"}')],
        [("say", FOUND)]])
    send(h, first)
    h.wait_says("do both.", timeout=60)
    second = "Do both, and add tests for it."
    script(h, second, [
        [("think", "Decode first, then keep only this site's paths. Then a test for each."),
         ("tool", "edit " + json.dumps({"path": "src/auth/guard.ts", "oldString": GUARD_OLD,
                                        "newString": GUARD_NEW}))],
        [("tool", "write " + json.dumps({"path": "test/return-to.test.ts", "content": GUARD_TEST}))],
        [("tool", 'shell {"command":"npm test -- auth","description":"Run the auth tests"}')],
        [("say", FIXED)]])
    send(h, second)
    h.wait("Allow once", role="Button", timeout=60)
    s.cap("approval")
    h.click("Allow once")
    h.wait_says("Both tests pass.", timeout=60)
    # Another session at work while this one is read.
    ask(h, s.limits, "Add rate limiting to the API client, with a test.",
        [[("think", "Where requests are sent from."), ("pace", "900"), ("long", "40")]])
    h.settle(2)
    s.cap("chat")
    h.click("Changes", where=rail(h))
    h.wait("guard.ts", role=None, where=lambda e: e.x > h.size[0] / 2, timeout=15)
    h.settle(1)
    s.cap("hero")
    h.click("guard.ts", role=None, where=lambda e: e.x > h.size[0] / 2 and e.role != "Tab")
    h.settle(1)
    s.cap("diff")


def scene_diff(s):
    """The whole change, in the maximized Changes panel."""
    h = s.h
    if s.narrow:
        # The panel already has the window's width.
        h.click("Close Changes") if h.find("Close Changes") else None
        return
    h.click("Maximize")
    # The pointer off the button, so its name doesn't show.
    h.hover("Commit message", role="TextInput")
    h.settle(1.2)
    s.cap("diff-wide")
    h.click("Restore")
    h.settle(0.6)
    h.click("Close Changes", where=lambda e: e.y < 40) if h.find("Close Changes") else h.click(
        "Changes", where=rail(h))
    h.settle(0.6)


PLUGIN_JSON = """{
  "description": "Writes release notes from the commits since the last tag, and posts them.",
  "version": "1.4.0",
  "tools": [
    { "name": "commits_since", "description": "Lists the commits since a tag." },
    { "name": "post_notes", "description": "Posts release notes to the project's chat." }
  ],
  "settings": [{ "key": "token", "label": "Chat token", "secret": true }],
  "permissions": { "network": ["hooks.chat.example"], "project": ["read", "shell"] },
  "events": ["turn.ended"]
}
"""

PLUGIN_JS = """harlo.tool("commits_since", async (args) => {
  const out = await harlo.project.shell({ command: `git log --oneline ${args.tag}..HEAD` });
  return out;
});
harlo.tool("post_notes", async (args) => {
  await harlo.fetch("https://hooks.chat.example/post", { method: "POST", body: args.text });
  return "posted";
});
harlo.on("turn.ended", async () => {});
"""


def scene_plugin(s):
    """Settings → Extensions → Plugins: a plugin from a git address, with
    what it asks for. The address is rewritten to a folder in the sandbox,
    so nothing is fetched from outside."""
    h = s.h
    repos = os.path.join(s.sandbox, "repos")
    project(repos, "release-notes", {
        "plugin.json": PLUGIN_JSON,
        "plugin.js": PLUGIN_JS,
        "skills/release-notes/SKILL.md": "---\ndescription: How this project writes release notes.\n---\n"
                                         "Plain words, newest first.\n",
        "commands/notes.md": "---\ndescription: Write the next release's notes.\n---\nWrite the notes.\n",
    })
    write(os.path.join(s.sandbox, "home"), ".gitconfig",
          f'[url "{repos}/"]\n\tinsteadOf = https://github.com/acme/\n')
    settings(h, "Extensions")
    h.wait("MCP servers", timeout=15)
    # The page can still be settling, which swallows a click.
    for attempt in range(4):
        h.settle(1)
        try:
            h.click("Plugins")
            h.wait("Add Plugin", timeout=5)
            break
        except Failed:
            if attempt == 3:
                raise
    h.fill("Plugin", "https://github.com/acme/release-notes")
    h.click("Add Plugin")
    row = lambda e: e.role != "TextInput" and e.y > 150  # noqa: E731
    h.wait("release-notes", role=None, where=row, timeout=30)
    h.settle(1)
    s.cap("plugins")
    h.click("release-notes", role=None, where=row)
    h.wait("Allow", timeout=15)
    h.settle(1)
    s.cap("plugin")
    h.click("Allow")
    h.settle(1.5)
    leave_settings(h)


def scene_settings(s):
    h = s.h
    for section in ("Providers", "Local models", "Appearance", "Phone", "Server", "Git"):
        settings(h, section)
        h.settle(1.2)
        s.cap("settings-" + section.lower().replace(" ", "-"))
    leave_settings(h)


SCENES = [("sessions", scene_sessions), ("hero", scene_hero), ("diff", scene_diff),
          ("plugin", scene_plugin), ("settings", scene_settings)]


def sitting(mode, size, only, keep):
    shots = os.path.join(OUT, f"{mode}-{size}")
    os.makedirs(shots, exist_ok=True)
    window = SIZES[size]
    physical = (window[0] * SCALE, window[1] * SCALE)
    preferences = {"appearance": mode}
    if size == "narrow":
        preferences["text_size"] = "large"
    h = Harlo(f"site-{mode}-{size}", shots, size=physical, shared=True,
              env={"SLINT_SCALE_FACTOR": str(SCALE)}, preferences=preferences)
    failed = []
    try:
        h.start()
        # Elements are measured in the window's own points.
        h.size = window
        s = Sitting(h, mode, size)
        for name, scene in SCENES:
            if only and name not in only and name != "sessions":
                continue
            started = time.time()
            try:
                scene(s)
                print(f"    {name} ({time.time() - started:.0f} s)", flush=True)
            except Exception as e:  # a scene that fails leaves the rest to run
                failed.append((name, str(e)))
                print(f"    {name} failed: {e}", flush=True)
                traceback.print_exc(limit=3)
                try:
                    s.cap(f"failed-{name}")
                    for _ in range(3):
                        h.key("Escape")
                except Exception:
                    pass
        if keep:
            print(f"    kept: HARLO_GUI_STATE={h.state_file} python3 tools/gui-drive.py stop", flush=True)
            return failed
    finally:
        if not keep:
            h.stop()
    return failed


# The site's pictures: (file, size, shot, the part of the window in its own
# points or None for all of it, the widths to write or None for the cut's own).
# Each is written dark (`name.webp`) and light (`name-light.webp`).
PANE = (498, 1160)       # Settings' page, wide
PANE_NARROW = (282, 656)  # and narrow
COLUMN_NARROW = (56, 696)  # between the rails of the narrow window
CUTS = [
    ("desktop-hero", "wide", "hero", None, {"": 1102, "-2x": 2204}),
    ("desktop-approval", "wide", "approval", None, {"": 1440}),
    ("desktop-diff", "wide", "diff-wide", None, {"": 1440}),
    ("crop-diff", "wide", "diff-wide", (437, 36, 1384, 310), None),
    ("crop-approval", "wide", "approval", (456, 111, 1272, 760), None),
    ("crop-providers", "wide", "settings-providers", (PANE[0], 436, PANE[1], 774), None),
    ("crop-plugin", "wide", "plugin", (PANE[0], 124, PANE[1], 520), None),
    ("crop-server", "wide", "settings-server", (PANE[0], 124, PANE[1], 500), None),
    ("crop-hero-narrow", "narrow", "chat", (COLUMN_NARROW[0], 36, COLUMN_NARROW[1], 936), None),
    ("crop-approval-narrow", "narrow", "approval", (COLUMN_NARROW[0], 236, COLUMN_NARROW[1], 796), None),
    ("crop-diff-narrow", "narrow", "diff", (352, 36, 698, 690), None),
    ("crop-providers-narrow", "narrow", "settings-providers", (PANE_NARROW[0], 454, PANE_NARROW[1], 806), None),
    # The narrow window cuts these rows short, so the phone gets the wide one's list alone.
    ("crop-plugin-narrow", "wide", "plugin", (PANE[0], 262, PANE[0] + 496, 478), None),
    ("crop-server-narrow", "narrow", "settings-server", (PANE_NARROW[0], 150, PANE_NARROW[1], 568), None),
]


# The phone app's pictures: the iOS workflow's `screenshots` artifact,
# downloaded into target/ios-shots (`gh run download <run> --name screenshots
# --dir target/ios-shots`), by the name each is written under.
PHONE = os.path.join(ROOT, "target", "ios-shots")
PHONES = {"05-dashboard": "iphone-dashboard", "09-approvals": "iphone-approval", "12-diff": "iphone-diff"}
PHONE_WIDTH = 660


def cut():
    """Writes site/assets/*.webp from the pictures in target/site-shots, and
    the phone's from target/ios-shots when they are there."""
    from PIL import Image

    assets = os.path.join(ROOT, "site", "assets")
    for shot, name in PHONES.items():
        for mode, suffix in (("dark", ""), ("light", "-light")):
            source = os.path.join(PHONE, f"{shot}-{mode}.png")
            if not os.path.exists(source):
                print(f"no {os.path.relpath(source, ROOT)}: {name}{suffix}.webp is left as it is")
                continue
            image = Image.open(source).convert("RGB")
            out = image.resize((PHONE_WIDTH, round(image.height * PHONE_WIDTH / image.width)), Image.LANCZOS)
            path = os.path.join(assets, f"{name}{suffix}.webp")
            out.save(path, "WEBP", quality=84, method=6)
            print(f"{os.path.relpath(path, ROOT)}  {out.width}×{out.height}  {os.path.getsize(path) // 1024} KB")
    for name, size, shot, part, widths in CUTS:
        for mode, suffix in (("dark", ""), ("light", "-light")):
            source = os.path.join(OUT, f"{mode}-{size}", f"{shot}.png")
            image = Image.open(source).convert("RGB")
            if part:
                image = image.crop(tuple(v * SCALE for v in part))
            for tail, width in (widths or {"": image.width}).items():
                out = image
                if width != image.width:
                    out = image.resize((width, round(image.height * width / image.width)), Image.LANCZOS)
                path = os.path.join(assets, f"{name}{suffix}{tail}.webp")
                out.save(path, "WEBP", quality=84, method=6)
                print(f"{os.path.relpath(path, ROOT)}  {out.width}×{out.height}  "
                      f"{os.path.getsize(path) // 1024} KB")
    link_preview(assets)
    return 0


def link_preview(assets):
    """The picture links to the site show (og.png): the dark window laid over
    the window in the picture that's there, under its logo."""
    from PIL import Image, ImageDraw

    left, top, width, radius = 100, 170, 1000, 12
    path = os.path.join(assets, "og.png")
    card = Image.open(path).convert("RGB")
    window = Image.open(os.path.join(OUT, "dark-wide", "hero.png")).convert("RGB")
    window = window.resize((width, round(window.height * width / window.width)), Image.LANCZOS)
    # Rounded at the top, where the window's corners show.
    mask = Image.new("L", window.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, width - 1, window.height + radius), radius, fill=255)
    card.paste(window, (left, top), mask)
    ImageDraw.Draw(card).rounded_rectangle((left, top, left + width - 1, top + window.height + radius), radius,
                                           outline=(58, 60, 70))
    card.save(path, "PNG", optimize=True)
    print(f"{os.path.relpath(path, ROOT)}  {card.width}×{card.height}  {os.path.getsize(path) // 1024} KB")


def main():
    args = sys.argv[1:]
    if args[:1] == ["cut"]:
        return cut()
    opts = {"--modes": "dark,light", "--sizes": "wide,narrow", "--only": ""}
    keep = False
    while args:
        a = args.pop(0)
        if a in opts:
            opts[a] = args.pop(0)
        elif a == "--keep":
            keep = True
        else:
            print(__doc__)
            return 0 if a in ("-h", "--help") else 2
    only = [x for x in opts["--only"].split(",") if x]
    failed = []
    for mode in opts["--modes"].split(","):
        for size in opts["--sizes"].split(","):
            print(f"  {mode}, {size}", flush=True)
            failed += [(f"{mode}-{size}", *f) for f in sitting(mode, size, only, keep)]
    for mode, name, why in failed:
        print(f"not taken: {mode} {name}: {why}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
