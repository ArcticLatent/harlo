"""One whole turn: with "Ask before edits and commands" on, an edit waits
for approval and shows its diff; allowed once, the file changes, the agent
answers, and the change is listed in Changes. After a restart Harlo
reopens the same session with its history.

Runs on both servers: window:turn on OpenCode, engine:turn on
harlo-server."""

from _common import open_project_session, send

ENGINE = True

EDIT = 'tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega.'


def run(h, s):
    open_project_session(h, title="Edit a")

    s.step("edits ask in this session")
    h.click("Permissions", role="Button")
    h.click("Ask before edits and commands", role=None)
    h.settle(0.5)

    s.step("the edit waits for approval, with its diff")
    send(h, EDIT)
    h.wait("Allow once", role="Button", timeout=30)
    s.expect(h.find("omega") is not None, "the diff shows the new line")
    s.expect(open(f"{h.project}/a.txt").read() == "alpha\n", "nothing written before approval")
    h.shot("asked")

    s.step("allowed once, the file changes and the agent answers")
    h.click("Allow once")
    h.wait_says("Changed alpha to omega", timeout=30)
    s.expect(open(f"{h.project}/a.txt").read() == "omega\n", "a.txt changed")

    s.step("the change is listed in Changes")
    h.click("Changes", role=None)
    h.settle(1.5)
    s.expect(h.find("This session") is not None, "the session's changes")
    s.expect(h.find("a.txt") is not None, "a.txt among them")
    h.shot("changes")

    s.step("after a restart, the same session")
    h.restart()
    h.wait("Edit a", role="Tab", timeout=20)
    h.wait_says("Changed alpha to omega", timeout=15)
    h.shot("reopened")
