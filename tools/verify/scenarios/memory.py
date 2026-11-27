"""Settings → Memory: notes between sessions. Turned on, a note the agent
keeps shows with the session it came from, and is corrected and then
forgotten from Settings. A correction the agent didn't keep is suggested
with the message it comes from, kept with its words changed, and another
is dismissed."""

import json
import os

from _common import close_sheet, open_project_session, send, settings



def edit_button(e):
    return e.label.startswith("Edit: ")


def run(h, s):
    s.step("Settings → Memory")
    settings(h, "Memory")
    h.until("the Memory page",
            lambda: h.find("Keep notes between sessions", role="Checkbox"))
    s.expect(h.says("None yet."), "no notes yet")

    s.step("turned on, the agent keeps a note")
    h.click("Keep notes between sessions", role="Checkbox")
    h.settle(1)
    close_sheet(h)
    session = open_project_session(h, title="Cache choice")
    send(h, 'tool remember {"text":"Cache in SQLite: one file, no server to run."} ;; say Kept.')
    h.wait_says("Kept.", timeout=30)

    s.step("the note shows with its source")
    settings(h, "Memory")
    # The page shows the note's text with its source under it (the
    # transcript behind the sheet has the text too, without the source).
    h.wait_says("Cache in SQLite: one file, no server to run.\nFrom “Cache choice”", timeout=15)
    h.shot("kept")

    s.step("corrected")
    h.click(None, role="Button", where=edit_button)
    h.fill("Note", "Cache in SQLite, one file per project.")
    h.click("Save Note")
    h.wait_says("Cache in SQLite, one file per project.\nFrom", timeout=15)
    s.expect(not h.says("no server to run.\nFrom"), "the old text is gone")
    h.shot("corrected")

    s.step("forgotten")
    h.click(None, role="Button", where=edit_button)
    h.click("Forget")
    h.until("the note gone", lambda: not h.says("one file per project.\nFrom"), timeout=15)
    s.expect(h.says("None yet."), "no notes left")
    h.shot("forgotten")

    s.step("a correction the agent missed is suggested")
    # (The Forget button took the focus with it, so Escape has nowhere to go.)
    h.click("Close")
    h.wait_gone("Search settings", role="TextInput")
    project = h.api("GET", f"/api/session/{session}")["data"]["projectID"]
    scripts = os.path.join(h.info["sandbox"], "scripts.json")
    correction = "No, cache in Postgres: the servers share it."
    with open(scripts, "w") as f:
        json.dump({
            correction: 'tool read {"path":"a.txt"} ;; say Postgres it is.',
            "<notes>": json.dumps([
                {"text": "Cache in Postgres: the servers share it.", "kind": "correction",
                 "message": "{person}"},
                {"text": "a.txt is read before every change.", "kind": "pitfall",
                 "message": "{person}"},
            ]),
        }, f)
    send(h, correction)
    h.wait_says("Postgres it is.", timeout=30)
    h.until("the suggestions",
            lambda: len(h.api("GET", f"/harlo/proposals?projectID={project}")["data"]) == 2,
            timeout=30)
    os.remove(scripts)
    settings(h, "Memory")
    h.wait_says("Suggested", timeout=15)
    h.wait_says("Cache in Postgres: the servers share it.\nFrom “Cache choice” · for this project",
                timeout=15)
    s.expect(h.says("A correction you gave"), "what kind it is")
    h.shot("suggested")

    s.step("looked at, it shows the message it comes from")
    h.click("Review: A correction you gave")
    h.wait_says(f"“{correction}”", timeout=15)
    h.shot("reviewed")

    s.step("kept with its words changed")
    h.fill("Note", "Cache in Postgres, shared by every server.")
    h.click("Keep Note")
    h.wait_says("Cache in Postgres, shared by every server.\nFrom “Cache choice”", timeout=15)
    s.expect(not h.says("A correction you gave"), "no longer suggested")

    s.step("the other is dismissed")
    h.click("Review: Something that went wrong")
    h.click("Dismiss")
    h.until("no suggestions left", lambda: not h.says("Suggested"), timeout=15)
    s.expect(not h.says("a.txt is read before every change."), "a dismissed one is no note")
    h.shot("decided")
