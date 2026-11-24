"""Files and the editor: the Files panel lists the project, a file opens
in an editor tab, an edit marks it unsaved, and Ctrl+S saves it to disk."""

ENGINE = True

from _common import open_project_session, send


def run(h, s):
    open_project_session(h, title="Files")
    send(h, "say ready")
    h.wait_says("ready", timeout=30)

    s.step("the Files panel lists the project")
    h.click("Files", where=lambda e: e.x > 1400)
    h.wait("b.txt", timeout=15)
    s.expect(not h.shows(".git"), "git's own folder left out")
    h.shot("listed")

    s.step("a file opens in the editor")
    h.click("b.txt", role=None, where=lambda e: e.x > 1000)
    editor = h.wait(None, role="TextInput", where=lambda e: "b.txt" in e.label, timeout=15)
    h.shot("opened")

    s.step("edited and saved")
    h.fill(None, "beta, edited\n", where=lambda e: e.role == "TextInput" and "b.txt" in e.label)
    h.wait("Unsaved changes", timeout=5)
    h.shot("unsaved")
    h.key("ctrl+s")
    h.until("b.txt saved", lambda: open(f"{h.project}/b.txt").read() == "beta, edited\n", timeout=10)
    h.wait_gone("Unsaved changes", timeout=5)
    h.shot("saved")
