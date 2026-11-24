"""Agents: picked from the composer's agent menu, Plan refuses an edit and
says so on the tool card; back on Build the same edit runs."""

from _common import open_project_session, send

ENGINE = True

EDIT = 'tool edit {"path":"a.txt","oldString":"alpha","newString":"alpha beta"} ;; say Done.'


def agent_button(name):
    return lambda e: e.role == "Button" and e.y > 600 and e.label.startswith(name)


def pick(h, current, wanted):
    h.click(None, where=agent_button(current))
    h.settle(0.5)
    h.click(wanted, role=None, where=lambda e: e.y > 300)
    h.wait(None, where=agent_button(wanted), timeout=10)


def run(h, s):
    open_project_session(h, title="Agents")

    s.step("Plan from the agent menu")
    h.click(None, where=agent_button("Build"))
    h.settle(0.8)
    s.expect(h.says("Plan"), "Plan is offered")
    h.shot("menu")
    h.key("Escape")
    h.settle(0.3)
    pick(h, "Build", "Plan")

    s.step("Plan doesn't edit")
    send(h, EDIT)
    h.wait_says("Done.", timeout=30)
    with open(h.project + "/a.txt") as f:
        s.expect(f.read() == "alpha\n", "a.txt unchanged")
    h.shot("refused")

    s.step("Build edits")
    pick(h, "Plan", "Build")
    send(h, EDIT)
    h.until("a.txt edited", lambda: open(h.project + "/a.txt").read() == "alpha beta\n",
            timeout=30)
    h.shot("edited")
