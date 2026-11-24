"""Shell commands: a person's `!` command runs with no model and shows its
output; a command the agent sends to the background shows above the
composer with Stop, and stopping it ends it."""

import json
import os

from _common import open_project_session, send


def run(h, s):
    # The agent's commands run without asking, so the job starts at once.
    path = os.path.join(h.info["sandbox"], "config", "harlo", "harlo.json")
    with open(path) as f:
        config = json.load(f)
    config.setdefault("permissions", []).append(
        {"action": "shell", "resource": "*", "effect": "allow"})
    with open(path, "w") as f:
        json.dump(config, f)
    h.restart_server()
    h.restart()

    open_project_session(h, title="Shell")
    send(h, "!echo hello-from-shell")
    h.wait("hello-from-shell", timeout=20)
    h.shot("own-command")

    s.step("a command in the background")
    send(h, 'tool shell {"command":"sleep 60","background":true} ;; say Started it.')
    h.wait_says("Started it.", timeout=30)
    h.wait("In the background", timeout=15)
    h.shot("running")
    h.click("Stop", role="Button")
    h.wait_gone("In the background", timeout=15)
    h.shot("stopped")
