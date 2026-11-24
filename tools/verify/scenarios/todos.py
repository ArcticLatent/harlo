"""Todos: with `experimental.todo` on, the agent writes its todo list with
todowrite, and the session shows it."""

import json
import os

from _common import open_project_session, send

TODOS = {"todos": [
    {"content": "Write the parser", "status": "in_progress"},
    {"content": "Test the parser", "status": "pending"},
]}


def run(h, s):
    path = os.path.join(h.info["sandbox"], "config", "harlo", "harlo.json")
    with open(path) as f:
        config = json.load(f)
    config.setdefault("experimental", {})["todo"] = True
    with open(path, "w") as f:
        json.dump(config, f)
    h.restart_server()
    h.restart()

    open_project_session(h, title="Todos")
    send(h, f"tool todowrite {json.dumps(TODOS)} ;; say Listed.")
    h.wait_says("Listed.", timeout=30)
    h.wait("Write the parser", timeout=15)
    s.expect(h.find("0 of 2"), "the progress above the composer", h.texts()[:40])
    h.shot("listed")

    s.step("the whole list")
    h.click("Todos", role="Button")
    h.wait("Test the parser", timeout=10)
    h.shot("opened")
