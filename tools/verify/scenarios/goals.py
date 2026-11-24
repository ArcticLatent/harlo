"""Goals: /goal sets one, which shows above the composer; the reviewer
finds it blocked and it pauses with why; Resume sends the agent back and
the goal is reached; Dismiss clears it."""

ENGINE = True

import json
import os

from _common import open_project_session, send


def run(h, s):
    scripts = os.path.join(h.info["sandbox"], "scripts.json")
    with open(scripts, "w") as f:
        json.dump({"<reviewer>": '{"verdict":"blocked","reason":"It needs the release date."}'}, f)

    open_project_session(h, title="Goals")
    send(h, "/goal Write the release notes")
    h.wait("Write the release notes", timeout=30)

    s.step("blocked, it pauses with why")
    h.wait("It needs the release date.", timeout=30)
    s.expect(h.find("Paused"), "paused", h.texts()[:40])
    h.shot("paused")

    s.step("resumed, it is reached")
    os.remove(scripts)
    h.click("Resume")
    h.wait("Reached", timeout=30)
    h.shot("reached")

    s.step("dismissed")
    h.click("Dismiss")
    h.wait_gone("Reached", timeout=15)
    h.shot("cleared")
