"""Stopping a real model mid-answer: Stop ends the run and the composer is
ready for the next message."""

from _common import COMPOSER, open_project_session, send
from real import pick_model


def run(h, s):
    open_project_session(h, title="Real stop")
    pick_model(h)
    send(h, "Count from 1 to 400, one number per line, with no other text.")
    s.step("working")
    h.wait("Stop", role="Button", timeout=120)
    h.until("some numbers", lambda: any(t.strip().startswith("1") for t in h.texts()), timeout=120)
    h.shot("working")
    s.step("stopped")
    h.click("Stop")
    h.wait_gone("Stop", role="Button", timeout=30)
    h.wait(COMPOSER, role="TextInput", timeout=10)
    h.shot("stopped")
