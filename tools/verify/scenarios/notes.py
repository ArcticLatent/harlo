"""Notes: the Notes panel keeps a note per project, with todos; the note is
still there after Harlo restarts."""

from _common import open_project_session, send


def run(h, s):
    open_project_session(h, title="Notes")
    send(h, "say ready")
    h.wait_says("ready", timeout=30)

    s.step("a note is written")
    h.click("Notes", where=lambda e: e.x > 1400)
    h.wait("Note", role="TextInput", timeout=10)
    h.fill("Note", "Check the release checklist\n- [ ] update the changelog\n")
    h.settle(1.5)
    h.shot("written")

    s.step("kept across a restart")
    h.restart()
    h.settle(1)
    # The panel comes back as it was: open.
    s.expect(h.find("Note", role="TextInput"), "the Notes panel open again")
    if not h.find("Note", role="TextInput"):
        h.click("Notes", where=lambda e: e.x > 1400)
        h.settle(1)
    note = h.find("Note", role="TextInput")
    s.expect(note and "release checklist" in note[0].value, "the note's text",
             note[0].value if note else h.texts()[:20])
    h.shot("restarted")
