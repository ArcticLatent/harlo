"""Notifications: with Harlo in the background, a finished run and an
approval send desktop notifications."""

from _common import open_project_session, send

# Under Xvfb the window keeps the focus, so notify while it has it.
PREFERENCES = {"setup_done": True, "notifications": {"while_focused": True}}


def run(h, s):
    open_project_session(h, title="Notified")
    s.step("a finished run in another tab")
    send(h, "pace 200 | long 1")
    h.key("ctrl+n")
    h.settle(5)
    seen = h.notifications()
    s.expect("Notified" in seen or "finished" in seen.lower(), "a notification", seen[-600:])
