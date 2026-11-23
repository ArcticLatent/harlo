"""Changes: an edit the agent makes shows in the Changes panel, this
session's and the project's uncommitted ones, with the diff a click away;
committed from the panel, it's a commit in the project's git."""

import subprocess

from _common import close_sheet, open_project_session, send, settings


def git(h, *args):
    """Runs git in the project as Harlo's sandbox does (its scratch HOME)."""
    return subprocess.run(["git", "-C", h.project, *args], capture_output=True, text=True,
                          env=h.env()).stdout


def rail(e):
    return e.x > 1400


def run(h, s):
    open_project_session(h, title="Changes")

    s.step("the agent edits a file")
    send(h, 'tool edit {"path":"a.txt","oldString":"alpha","newString":"ALPHA"} ;; say edited a.txt')
    h.wait_says("edited a.txt", timeout=30)
    s.expect(open(f"{h.project}/a.txt").read() == "ALPHA\n", "a.txt changed on disk")

    s.step("the Changes panel lists it")
    h.click("Changes", where=rail)
    h.wait("a.txt", timeout=15)
    s.expect(h.shows("Uncommitted") or h.shows("This session"), "the groups", h.texts()[:30])
    h.shot("listed")

    s.step("its diff opens")
    h.click("a.txt", role=None, where=lambda e: e.x > 1000)
    h.settle(0.8)
    s.expect(h.says("ALPHA"), "the new line in the diff", h.texts()[:40])
    h.shot("diff")

    s.step("without a name, the commit says where to set one")
    h.click("Commit", role="Button")
    h.fill("Commit message", "Shout alpha")
    h.click("Commit", role="Button")
    h.wait_says("Git needs your name and email", timeout=15)
    h.shot("needs-name")

    s.step("the name set in Settings → Git")
    settings(h, "Git")
    h.fill("Name", "Verify Person", where=lambda e: e.role == "TextInput")
    h.click("Save Name")
    h.fill("Email", "verify@example.invalid", where=lambda e: e.role == "TextInput")
    h.click("Save Email")
    h.until("the scratch git config", lambda: "Verify Person" in git(h, "config", "--global", "user.name"),
            timeout=10)
    h.shot("named")
    close_sheet(h)

    s.step("committed from the panel")
    h.click("Commit", role="Button")
    h.until("the commit", lambda: "Shout alpha" in git(h, "log", "--oneline", "-1"), timeout=20)
    s.expect(h.says("Committed"), "the panel says so", h.texts()[-20:])
    h.shot("committed")
