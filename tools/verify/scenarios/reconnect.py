"""Reconnecting: the server restarts while Harlo is open, in the middle of
a streaming answer. Harlo reconnects by itself, the session keeps its
history (with what had streamed of the cut-short answer), isn't left
looking busy, and answers the next prompt.

Runs on both servers: window:reconnect on OpenCode, engine:reconnect on
harlo-server. Only the sandbox's own server restarts."""

import json

from _common import open_project_session, send

ENGINE = True


def run(h, s):
    session = open_project_session(h, title="Reconnect")

    s.step("a first answer")
    send(h, "say Before the restart.")
    h.wait_says("Before the restart.", timeout=30)

    s.step("the server restarts during a streaming answer")
    send(h, "pace 40 | long 6")
    h.wait("Stop", timeout=20)
    h.settle(0.5)
    h.restart_server()
    h.shot("restarted")

    s.step("Harlo reconnects and the session isn't stuck")
    h.wait_gone("Stop", timeout=45)
    # The server menu's status line says when the window is connected again.
    h.click(h.wait_says("127.0.0.1:"), role=None)
    h.wait_says("Connected ·", timeout=30)
    h.shot("reconnected")
    h.key("Escape")
    # The transcript sits at its end, so the start is checked on the server.
    stored = json.dumps(h.api("GET", f"/api/session/{session}/message"))
    s.expect("Before the restart." in stored and "Section 0" in stored,
             "the history and what streamed before the restart are kept")
    s.expect(h.says("Harlo keeps the"), "the cut-short answer shows")

    s.step("the next prompt is answered")
    send(h, "say After the restart.")
    h.wait_says("After the restart.", timeout=45)
    h.shot("answered")
