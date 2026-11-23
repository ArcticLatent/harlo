"""A narrow window: the session stays readable with a panel open, and the
sidebar makes way."""

from _common import open_project_session, send

SIZE = (1000, 760)


def run(h, s):
    open_project_session(h, title="Narrow")
    send(h, "say a reply long enough to wrap in a narrow window, so the transcript's "
            "column is seen to fit the space it has rather than running under the panel")
    h.wait_says("long enough to wrap", timeout=30)
    h.shot("narrow")

    s.step("with a panel open")
    h.click("Changes", where=lambda e: e.x > 900 and e.w < 60)
    h.settle(1)
    h.shot("panel")

    s.step("narrower still")
    h.resize(760, 700)
    h.settle(1.5)
    h.shot("narrower")
    s.expect(h.find(None, role="TextInput", where=lambda e: e.role == "TextInput" and e.x + e.w <= 760),
             "the composer inside the window")
