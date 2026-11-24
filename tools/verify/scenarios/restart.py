"""After a restart Harlo opens the tabs that were open, with the session
that was showing."""

ENGINE = True

from _common import open_project_session, send


def tab(e):
    return e.role == "Tab"


def run(h, s):
    open_project_session(h, title="First tab")
    send(h, "say one")
    h.wait_says("one", timeout=30)
    open_project_session(h, title="Second tab")
    send(h, "say two")
    h.wait_says("two", timeout=30)

    s.step("restarted")
    h.restart()
    h.wait("Second tab", role="Tab", timeout=20)
    s.expect(h.find("First tab", role="Tab"), "the first tab too")
    h.wait_says("two", timeout=15)
    h.shot("reopened")
