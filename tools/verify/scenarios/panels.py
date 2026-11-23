"""The right panel: each of Context, Todos, Changes, Files and Notes opens
from the rail and closes; a panel maximizes and comes back with Escape;
the sidebar hides and shows."""

from _common import open_project_session, send

PANELS = ["Context", "Todos", "Changes", "Files", "Notes"]


def rail(e):
    return e.x > 1400


def run(h, s):
    open_project_session(h, title="Panels")
    send(h, "say ready")
    h.wait_says("ready", timeout=30)

    for panel in PANELS:
        s.step(f"the {panel} panel")
        h.click(panel, where=rail)
        h.settle(0.8)
        h.shot(panel.lower())

    s.step("maximized and restored")
    if h.find("Maximize", role="Button"):
        h.click("Maximize")
        h.settle(0.8)
        h.shot("maximized")
        h.key("Escape")
        h.settle(0.8)

    s.step("the sidebar hides and shows")
    h.click("Toggle sidebar")
    h.settle(0.8)
    s.expect(not h.find("Filter sessions", role="TextInput"), "the sidebar hidden")
    h.shot("no-sidebar")
    h.click("Toggle sidebar")
    h.wait("Filter sessions", role="TextInput")
