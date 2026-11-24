"""Compaction: /compact summarizes the conversation; the row says so, and
the next prompt is answered as usual."""

ENGINE = True

from _common import composer, open_project_session, send


def run(h, s):
    open_project_session(h, title="Compaction")
    send(h, "say First answer.")
    h.wait_says("First answer.", timeout=30)

    s.step("/compact")
    h.click(None, role="TextInput", where=composer)
    h.type("/compact")
    h.wait("/compact", timeout=10)
    h.click("/compact")
    h.settle(0.5)
    if h.find(None, role="TextInput", where=composer)[0].value.strip():
        h.key("Return")
    h.wait("Summarized the conversation to free up context", timeout=30)
    h.shot("summarized")

    s.step("carries on")
    send(h, "say Second answer.")
    h.wait_says("Second answer.", timeout=30)
    h.shot("after")
