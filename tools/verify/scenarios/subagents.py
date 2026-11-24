"""Subagents: the agent hands a task to Explore in a child session; the
parent shows the subagent's card and its answer, and the child opens from
it."""

ENGINE = True

import json

from _common import open_project_session, send

CALL = {"agent": "explore", "description": "Find the file", "prompt": "say Found a.txt."}


def run(h, s):
    open_project_session(h, title="Subagents")
    send(h, f"tool subagent {json.dumps(CALL)} ;; say Parent done.")
    h.wait_says("Parent done.", timeout=30)
    h.wait_says("reported back", timeout=30)
    h.wait_says("Found a.txt.", timeout=10)
    h.settle(1.0)
    h.shot("parent")

    s.step("the child")
    h.click("Open", role="Button")
    h.wait_says("say Found a.txt.", timeout=15)
    h.shot("child")
