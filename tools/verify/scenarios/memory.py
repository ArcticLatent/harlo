"""Settings → Memory: notes between sessions. On harlo-server: turned on
there, a note the agent keeps shows with the session it came from, and is
corrected and then forgotten from Settings. On OpenCode the page says notes
need Harlo's own server."""

from _common import close_sheet, open_project_session, send, settings

ENGINE = True

UNSUPPORTED = "Notes between sessions need Harlo's own server. This server doesn't keep them."


def edit_button(e):
    return e.label.startswith("Edit: ")


def run(h, s):
    s.step("Settings → Memory")
    settings(h, "Memory")
    shown = h.until("the Memory page",
                    lambda: h.says(UNSUPPORTED) or h.find("Keep notes between sessions",
                                                          role="Checkbox"))
    if shown == UNSUPPORTED:
        h.shot("unsupported")
        return
    s.expect(h.says("None yet."), "no notes yet")

    s.step("turned on, the agent keeps a note")
    h.click("Keep notes between sessions", role="Checkbox")
    h.settle(1)
    close_sheet(h)
    open_project_session(h, title="Cache choice")
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
