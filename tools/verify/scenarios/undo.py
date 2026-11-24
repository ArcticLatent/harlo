"""Undo a turn from the window: the agent edits a file next to the
person's uncommitted work; Undo on the prompt puts the file back and shows
what it restored; Cancel undo brings the agent's edit back; Undo again
and keep it: the prompt and its answer go, the person's work stays."""

from _common import open_project_session, send

EDIT = 'tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega.'

# The prompt shows as sent; hovering it offers Undo.
PROMPT = EDIT


def content(h, name):
    with open(f"{h.project}/{name}") as f:
        return f.read()


def run(h, s):
    h.file("notes.txt", "my uncommitted notes\n")
    open_project_session(h, title="Undo me")

    s.step("the agent edits a.txt")
    send(h, EDIT)
    h.wait_says("Changed alpha to omega", timeout=30)
    s.expect(content(h, "a.txt") == "omega\n", "a.txt edited")

    s.step("Undo on the prompt restores the file")
    h.hover(PROMPT, role=None)
    h.click("Undo")
    h.wait("Files restored to before this prompt", role=None, timeout=20)
    s.expect(content(h, "a.txt") == "alpha\n", "a.txt back")
    s.expect(content(h, "notes.txt") == "my uncommitted notes\n", "the person's work untouched")
    h.shot("staged")

    s.step("Cancel undo brings the edit back")
    h.click("Cancel undo")
    h.wait_gone("Files restored to before this prompt", role=None, timeout=20)
    s.expect(content(h, "a.txt") == "omega\n", "the agent's edit is back")

    s.step("Undo kept: the prompt and its answer go")
    h.hover(PROMPT, role=None)
    h.click("Undo")
    h.wait("Files restored to before this prompt", role=None, timeout=20)
    h.click("Undo", where=lambda e: e.y > 300)
    h.wait_gone("Files restored to before this prompt", role=None, timeout=20)
    h.settle(1)
    s.expect(not h.says("Changed alpha to omega"), "the answer is gone")
    s.expect(content(h, "a.txt") == "alpha\n", "a.txt stays restored")
    s.expect(content(h, "notes.txt") == "my uncommitted notes\n", "the person's work stays")
    h.shot("kept")
