"""Harlo's screenshot gallery: every view a person sees while using Harlo,
taken in the real window, for design critiques.

    python3 -I tools/verify/gallery.py [label] [--modes dark,light]
                                       [--sizes wide,narrow] [--only scene,…]

Each mode and size gets its own Harlo, in its own sandbox (the scripted
model, a scratch HOME, attached as to the shared server), which walks
through the scenes below: the start screen and setup guide, a session with
every kind of card, the menus, each panel, the terminal and editor, every
Settings section, the pages, phone pairing and schedules.

The pictures go to .impeccable/critique/shots/<date>-<label>/<mode>-<size>/
(local-only), with INDEX.md listing each picture and what it shows, and
index.html to browse them. Nothing is sent anywhere: the critique itself is
run by hand, with these pictures and PRODUCT.md and DESIGN.md as context.
"""

import datetime
import html
import json
import os
import sys
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path[:0] = [HERE, os.path.join(HERE, "scenarios")]

from driver import Failed, Harlo  # noqa: E402
from _common import close_sheet, composer, open_project_session, send, settings  # noqa: E402

SIZES = {"wide": (1588, 947), "narrow": (1000, 760)}
MODES = ("dark", "light")

SECTIONS = ["General", "Appearance", "Notifications", "Shortcuts", "Voice", "Projects", "Git",
            "Models", "Local models", "Agents", "Tools for agents", "Schedules", "Server", "Phone",
            "About"]

# A reply with every kind of Markdown block the transcript draws.
MARKDOWN = (
    "# Release notes\n\nThe **0.4** release brings *steering* and the `/` menu. "
    "See [the changelog](https://example.com).\n\n## What changed\n\n"
    "- Queued messages can be steered\n- The composer offers commands\n  - and files after @\n\n"
    "1. Bump the version\n2. Tag the release\n\n"
    "```rust\nfn main() {\n    println!(\"Hello, Harlo\");\n}\n```\n\n"
    "| Step | Status |\n|------|--------|\n| Build | done |\n| Tests | 405 passed |\n\n"
    "> Keep the changelog in plain words.\n\n---\n\nThat's everything for this release."
)


def ask(h, prompt, steps):
    """Sends `prompt` as a person would type it, with the scripted model's
    answer kept for it in the sandbox: `steps`, each a list of
    (word, text) parts (see tools/fake-model.py)."""
    path = os.path.join(h.info["sandbox"], "scripts.json")
    try:
        with open(path) as f:
            scripts = json.load(f)
    except (OSError, ValueError):
        scripts = {}
    scripts[prompt] = [[list(part) for part in step] for step in steps]
    with open(path, "w") as f:
        json.dump(scripts, f)
    send(h, prompt)


class Gallery:
    """One sitting: a Harlo in one mode and size, and what it has taken."""

    def __init__(self, h, folder):
        self.h = h
        self.folder = folder
        self.taken = []
        self.scene = ""

    def cap(self, name, what):
        """Takes a picture of the window now, saying what it shows."""
        path = self.h.shot(f"{self.scene}-{name}")
        self.taken.append({"scene": self.scene, "file": os.path.relpath(path, self.folder),
                           "what": what})

    def rail(self, e):
        """Whether an element is on the right rail."""
        return e.x > self.h.size[0] - 70

    def recover(self):
        for _ in range(3):
            self.h.key("Escape")
        self.h.settle(0.5)


# ---- the scenes ---------------------------------------------------------------
# Each takes the sitting and leaves the window much as it found it.


def scene_home(g):
    h = g.h
    g.cap("empty", "The start screen with no sessions yet: the composer, folder and model chips")
    for title in ("Fix the login redirect", "Write the release notes", "Tidy the settings page"):
        created = h.api("POST", "/api/session", {"location": {"directory": h.project}})["data"]
        h.api("PATCH", f"/api/session/{created['id']}", {"title": title})
    h.wait("Write the release notes", timeout=15)
    h.settle(1)
    g.cap("sidebar", "The start screen with a project and three sessions in the sidebar")


def scene_chat(g):
    h = g.h
    open_project_session(h, title="Release notes")
    ask(h, "Write the release notes for 0.4, with the steps and where the build is.",
        [[("think", "Looking at what the release needs: the changelog, the version and a tag."),
          ("say", MARKDOWN)]])
    h.wait_says("everything for this release", timeout=40)
    h.settle(1)
    # The end and the top are one frame when the whole answer fits.
    asked = h.find("Write the release notes for 0.4, with the steps and where the build is.")
    if not (asked and h.visible(asked[0])):
        g.cap("markdown-end", "The end of an answer: code, a table, a quote and a rule")
        for _ in range(4):
            h.scroll("Timeline", 300)
        h.settle(0.8)
    g.cap("markdown", "The prompt, the thought, and the answer's headings, emphasis, link and lists")
    todos = {"todos": [{"content": "Bump the version", "status": "completed", "priority": "high"},
                       {"content": "Tag the release", "status": "in_progress", "priority": "high"},
                       {"content": "Post the notes", "status": "pending", "priority": "medium"}]}
    ask(h, "Look at the text files, mark the first one edited, and plan the rest of the release.",
        [[("tool", 'glob {"pattern":"*.txt"}')], [("tool", 'read {"path":"a.txt"}')],
         [("tool", 'read {"path":"b.txt"}')],
         [("tool", 'edit {"path":"a.txt","oldString":"alpha","newString":"alpha, edited"}')],
         [("tool", "todowrite " + json.dumps(todos))],
         [("say", "Edited a.txt and planned the rest.")]])
    h.wait_says("planned the rest", timeout=40)
    h.settle(1)
    g.cap("tools", "Tool calls folded into a summary, an edit with its diff, and a todo list")
    summary = h.find(None, role="Button", where=lambda e: e.label.startswith(("Searched", "Read", "Ran")))
    if summary:
        h.click(summary[0].label)
        h.settle(0.8)
        g.cap("tools-open", "The tool summary opened: each call with its time")
        h.click(summary[0].label)
    prompt = h.find(None, role="Groupbox")
    if prompt:
        h.hover(prompt[-1].label, role="Groupbox")
        h.settle(0.6)
        g.cap("prompt-hover", "A prompt under the pointer, with its Undo button")
        if h.find("Undo", role="Button"):
            h.click("Undo")
            h.settle(0.8)
            g.cap("undo-card", "A staged undo: files back, Cancel undo or Undo")
            h.click("Cancel undo")
            h.settle(0.5)


def scene_approval(g):
    h = g.h
    open_project_session(h, title="Run the tests")
    ask(h, "Run the test suite.",
        [[("tool", 'shell {"command":"cargo test --workspace","description":"Run the tests"}')],
         [("say", "Tests ran.")]])
    h.wait("Allow once", role="Button", timeout=30)
    h.settle(0.8)
    g.cap("waiting", "A command waiting for approval: Allow once, Always allow, Deny, Deny with a reason")
    h.click("Deny with a reason…")
    h.settle(0.6)
    g.cap("reason", "Denying with a reason: the field for what to do instead")
    h.key("Escape")
    h.click("Deny")
    h.settle(1)


def scene_question(g):
    h = g.h
    open_project_session(h, title="Pick a colour")
    question = {"questions": [
        {"question": "Which colour should the accent be?", "header": "Accent",
         "options": [{"label": "Blue", "description": "calm, the default"},
                     {"label": "Green", "description": "fresh"},
                     {"label": "Orange", "description": "warm"}]},
        {"question": "Which extras?", "header": "Extras", "multiple": True,
         "options": [{"label": "Tests", "description": ""}, {"label": "Docs", "description": ""}]}]}
    ask(h, "Ask me what accent colour I'd like.",
        [[("tool", "question " + json.dumps(question))], [("say", "Thanks, Blue it is.")]])
    h.wait("Which colour should the accent be?", timeout=30)
    h.settle(0.8)
    g.cap("waiting", "A question card with options, and a second question choosing several")
    h.click("Blue")
    h.settle(0.4)
    g.cap("chosen", "The question card with an option chosen")
    if h.find("Send answer", role="Button"):
        h.click("Send answer")
    h.settle(1)


def scene_busy(g):
    h = g.h
    open_project_session(h, title="Long answer")
    ask(h, "Explain how the transcript stays readable, section by section.",
        [[("think", "planning a long answer"), ("pace", "120"), ("long", "8")]])
    h.wait("Stop", role="Button", timeout=20)
    h.settle(1.5)
    g.cap("streaming", "An answer streaming in, the session marked working")
    send(h, "And then update the changelog")
    h.settle(0.8)
    g.cap("queued", "A message queued while the session works, with Steer and cancel")
    h.click("Stop")
    h.settle(1.5)
    g.cap("stopped", "The run stopped")


def scene_menus(g):
    h = g.h
    open_project_session(h, title="Menus")
    h.click(None, role="TextInput", where=composer)
    h.type("/")
    h.wait("/compact", timeout=10)
    g.cap("slash", "The / menu: commands and skills over the composer")
    h.key("Escape")
    h.key("ctrl+a", "BackSpace")
    h.type("@")
    h.wait("@a.txt", timeout=10)
    g.cap("at", "The @ menu: files to mention")
    h.key("Escape")
    h.key("ctrl+a", "BackSpace")
    for chip, what in (("Build", "The agent menu"), ("default", "The model variant menu"),
                       ("Fake Coder", "The model menu, with its filter"),
                       ("Permissions", "The permissions (shield) menu")):
        found = h.find(chip, role="Button", where=lambda e: e.y > h.size[1] - 120)
        if not found:
            continue
        h.click(chip, where=lambda e: e.y > h.size[1] - 120)
        h.settle(0.8)
        g.cap(chip.lower().replace(" ", "-"), what)
        h.key("Escape")
        h.settle(0.4)
    # A mode other than the agent's own is named beside the shield.
    foot = lambda e: e.y > h.size[1] - 120
    if h.find("Permissions", role="Button", where=foot):
        h.click("Permissions", where=foot)
        h.settle(0.5)
        h.click("Don't ask")
        h.wait_says("Don't ask", timeout=10)
        h.settle(0.5)
        g.cap("dont-ask", "Don't ask chosen: the mode is named beside the shield")
        h.click("Permissions", where=foot)
        h.settle(0.5)
        h.click("Ask before commands")
        h.settle(0.5)


def scene_panels(g):
    h = g.h
    # The chat's session: it has an edit, todos and context to show.
    h.click("Release notes", role=None, where=lambda e: e.y < 34)
    h.wait(None, role="Text", where=lambda e: e.y < 110 and e.label == "Release notes" and e.y > 34,
           timeout=10)
    h.settle(1)
    for panel, what in (("Context", "The Context panel: how full the context window is, usage and files"),
                        ("Todos", "The Todos panel: the session's todo list"),
                        ("Changes", "The Changes panel: this session's and uncommitted changes"),
                        ("Files", "The Files panel: the project's tree"),
                        ("Notes", "The Notes panel: the project's note")):
        h.click(panel, where=g.rail)
        h.settle(1)
        if panel == "Changes" and h.find("a.txt"):
            g.cap("changes", what)
            h.click("a.txt", role=None, where=lambda e: e.x > h.size[0] / 2 and e.role != "Tab")
            h.settle(0.8)
            g.cap("changes-diff", "The Changes panel with a file's diff open")
            continue
        if panel == "Notes":
            h.fill("Note", "Release checklist\n- [x] bump the version\n- [ ] tag the release\n- [ ] post the notes\n")
            h.settle(1)
        g.cap(panel.lower(), what)
    if h.find("Maximize", role="Button"):
        h.click("Maximize")
        h.settle(1)
        g.cap("maximized", "A panel maximized over the session")
        h.key("Escape")
        h.settle(0.8)
    h.click("Notes", where=g.rail)
    h.settle(0.8)
    h.click("Toggle sidebar")
    h.settle(0.8)
    g.cap("no-sidebar", "The session with the sidebar hidden")
    h.click("Toggle sidebar")
    h.settle(0.5)


def scene_terminal(g):
    h = g.h
    h.click("Terminal", where=g.rail)
    h.settle(1.5)
    h.type("ls -la && git --no-pager log --oneline --stat\n")
    h.settle(1.5)
    g.cap("open", "The terminal under the session, with a command's output")
    if h.find("New terminal", role="Button"):
        h.click("New terminal")
        h.settle(1)
        g.cap("tabs", "Two terminal tabs")
    if h.find("Hide terminals", role="Button"):
        h.click("Hide terminals")
        h.settle(0.6)


def scene_editor(g):
    h = g.h
    h.file("src/main.rs", "use std::env;\n\n/// Says hello.\nfn main() {\n    let name = env::args().nth(1)"
                          ".unwrap_or_else(|| \"Harlo\".into());\n    println!(\"Hello, {name}\");\n}\n")
    h.click("Files", where=g.rail)
    h.settle(1)
    if h.find("src"):
        h.click("src", role=None, where=lambda e: e.x > h.size[0] / 2)
        h.settle(0.6)
    h.click("main.rs", role=None, where=lambda e: e.x > h.size[0] / 2)
    h.wait(None, role="TextInput", where=lambda e: e.role == "TextInput" and "main.rs" in e.label, timeout=15)
    h.settle(1)
    g.cap("open", "A file open in the editor tab, beside the Files panel")
    h.fill(None, "use std::env;\n\nfn main() {}\n",
           where=lambda e: e.role == "TextInput" and "main.rs" in e.label)
    h.settle(0.6)
    g.cap("unsaved", "The editor with unsaved changes")
    h.key("ctrl+s")
    h.settle(0.8)


def scene_settings(g):
    h = g.h
    for section in SECTIONS:
        settings(h, section)
        h.settle(0.8)
        g.cap(section.lower().replace(" ", "-"), f"Settings → {section}")
    settings(h, "Appearance")
    h.fill("Search settings", "theme")
    h.settle(0.6)
    g.cap("search", "Settings searched for “theme”")
    h.fill("Search settings", "")
    close_sheet(h)


def scene_pages(g):
    h = g.h
    left = lambda e: e.x < 60
    h.click("Keyboard shortcuts", where=left)
    h.settle(1)
    g.cap("shortcuts", "The keyboard shortcuts page")
    h.key("Escape")
    h.click("Stats", where=left)
    h.settle(1.5)
    g.cap("stats", "The stats page")
    h.key("Escape")
    h.settle(0.5)


def scene_phone(g):
    h = g.h
    settings(h, "Phone")
    h.click("Phone access", role="Checkbox")
    h.wait_says("Listening", timeout=15)
    h.click("Show a code")
    h.wait("Pairing QR code", timeout=10)
    h.settle(0.6)
    g.cap("pairing", "Settings → Phone: access on and a pairing QR code")
    h.click("Phone access", role="Checkbox")
    h.settle(0.5)
    g.recover()


def scene_schedules(g):
    h = g.h
    settings(h, "Schedules")
    if not h.find("New schedule"):
        h.click("Allow schedules", role="Checkbox")
    h.wait("New schedule", timeout=15)
    h.click("New schedule")
    h.fill("Name", "Morning build check", where=lambda e: e.role == "TextInput")
    h.click("Runs: Every")
    h.fill("Time", "1 day")
    h.fill("Prompt", "Check that the build and tests pass, and say what broke.")
    h.settle(0.6)
    g.cap("form", "A new schedule's form")
    h.click("Create schedule")
    h.wait_gone("Create schedule", role="Button", timeout=15)
    h.wait("Morning build check", where=lambda e: e.x > h.size[0] * 0.3 and e.role != "TextInput",
           timeout=15)
    h.settle(0.6)
    g.cap("detail", "A schedule just made: when it runs, what it sends, its runs, Pause and Delete")
    h.click("All schedules")
    h.wait("New schedule", timeout=10)
    h.settle(0.6)
    g.cap("list", "The schedules list")
    g.recover()


def scene_window(g):
    h = g.h
    h.restart()
    h.settle(2)
    g.cap("reopened", "After a restart: the tabs and panels that were open")


SCENES = [("home", scene_home), ("chat", scene_chat), ("approval", scene_approval),
          ("question", scene_question), ("busy", scene_busy), ("menus", scene_menus),
          ("panels", scene_panels), ("terminal", scene_terminal), ("editor", scene_editor),
          ("settings", scene_settings), ("pages", scene_pages), ("phone", scene_phone),
          ("schedules", scene_schedules), ("window", scene_window)]

# A sitting of its own: the guide shows only before setup is done.
GUIDE = ["OpenCode", "Models", "Git", "Theme"]


def guide_sitting(folder, mode, size):
    shots = os.path.join(folder, "setup-guide")
    taken = []
    with Harlo(f"gallery-guide-{mode}", shots, size=size, shared=True,
               preferences={"setup_done": False, "appearance": mode}) as h:
        h.wait("Set up Harlo", timeout=20)
        for i, step in enumerate(GUIDE):
            if i:
                h.click("Next")
            h.settle(0.8)
            path = h.shot(step.lower())
            taken.append({"scene": "setup-guide", "file": os.path.relpath(path, folder),
                          "what": f"The setup guide's {step} step, on first start"})
    return taken


def sitting(folder, mode, size, only):
    taken = []
    failed = []
    try:
        taken += guide_sitting(folder, mode, SIZES[size])
    except (Failed, OSError) as e:
        failed.append(("setup-guide", str(e)))
    shots = os.path.join(folder, "views")
    with Harlo(f"gallery-{mode}-{size}", shots, size=SIZES[size], shared=True,
               preferences={"appearance": mode}) as h:
        g = Gallery(h, folder)
        for name, scene in SCENES:
            if only and name not in only:
                continue
            g.scene = name
            started = time.time()
            try:
                scene(g)
                print(f"    {name} ({len([t for t in g.taken if t['scene'] == name])} pictures, "
                      f"{time.time() - started:.0f} s)", flush=True)
            except Exception as e:  # a scene that fails leaves the rest to run
                failed.append((name, f"{e}"))
                print(f"    {name} failed: {e}", flush=True)
                traceback.print_exc(limit=2)
                try:
                    g.cap("failed", f"Where the {name} scene stopped: {e}")
                    g.recover()
                except Exception:
                    pass
        taken += g.taken
    return taken, failed


def write_index(root, label, sittings):
    lines = [f"# Harlo screenshots: {label}", "",
             f"Taken {datetime.datetime.now():%Y-%m-%d %H:%M} by tools/verify/gallery.py, in the real "
             "window against the scripted model. For /impeccable critique: read PRODUCT.md and "
             "DESIGN.md, then the pictures below. Paths are relative to this folder.", ""]
    page = ["<!doctype html><meta charset=utf-8><title>Harlo screenshots</title><style>"
            "body{font:14px system-ui;margin:24px;background:#18181a;color:#e8e8e4}"
            "figure{display:inline-block;width:380px;margin:8px;vertical-align:top}"
            "img{width:100%;border:1px solid #333}figcaption{font-size:12px;color:#aaa}"
            "</style>", f"<h1>Harlo screenshots: {html.escape(label)}</h1>"]
    for (mode, size), (taken, failed) in sittings.items():
        lines += [f"## {mode}, {size} ({SIZES[size][0]}×{SIZES[size][1]})", ""]
        page.append(f"<h2>{mode}, {size}</h2>")
        for t in taken:
            path = f"{mode}-{size}/{t['file']}"
            lines.append(f"- `{path}` — {t['what']}")
            page.append(f"<figure><a href='{html.escape(path)}'><img loading=lazy src='{html.escape(path)}'>"
                        f"</a><figcaption>{html.escape(t['what'])}</figcaption></figure>")
        for scene, why in failed:
            lines.append(f"- **{scene} not taken:** {why}")
        lines.append("")
    with open(os.path.join(root, "INDEX.md"), "w") as f:
        f.write("\n".join(lines))
    with open(os.path.join(root, "index.html"), "w") as f:
        f.write("\n".join(page))


def main():
    # The Todos panel has a list to show only with OpenCode's todo tool on.
    os.environ["OC_SANDBOX_TODO"] = "1"
    args = sys.argv[1:]
    opts = {"--modes": ",".join(MODES), "--sizes": ",".join(SIZES), "--only": ""}
    label = "app"
    while args:
        a = args.pop(0)
        if a in opts:
            opts[a] = args.pop(0)
        elif a in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            label = a
    root = os.path.join(ROOT, ".impeccable", "critique", "shots",
                        f"{datetime.date.today():%Y-%m-%d}-{label}")
    if os.path.exists(root):
        root += datetime.datetime.now().strftime("-%H%M%S")
    only = [s for s in opts["--only"].split(",") if s]
    sittings = {}
    for mode in opts["--modes"].split(","):
        for size in opts["--sizes"].split(","):
            print(f"  {mode}, {size}", flush=True)
            folder = os.path.join(root, f"{mode}-{size}")
            sittings[(mode, size)] = sitting(folder, mode, size, only)
            write_index(root, label, sittings)
    count = sum(len(t) for t, _ in sittings.values())
    failed = sum(len(f) for _, f in sittings.values())
    print(f"{count} pictures, {failed} scenes not taken: {os.path.relpath(root, ROOT)}/INDEX.md")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
