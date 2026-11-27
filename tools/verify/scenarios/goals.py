"""Goals: /goal sets one, which shows above the composer; the reviewer
finds it blocked and it pauses with why; Resume sends the agent back and
the goal is reached; Dismiss clears it. A goal the server stops in the
middle of a round is paused, and Resume gives the agent its checkpoint
with what the cut-short round had done.

Only the sandbox's own server restarts."""

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

    s.step("the server restarts in the middle of a round")
    with open(scripts, "w") as f:
        json.dump({
            "<reviewer>": '{"verdict":"continue","reason":"more.md is missing.",'
                          '"next_step":"Write more.md."}',
            "<goal>": 'tool write {"path":"first.md","content":"one\\n"} ;; say first.md is written.',
            "<round>": 'tool write {"path":"more.md","content":"two\\n"} ;; pace 60 | long 40',
        }, f)
    send(h, "/goal Write both files")
    h.wait("Write both files", timeout=30)
    h.until("the second round to be writing",
            lambda: os.path.exists(os.path.join(h.project, "more.md")) and h.says("Harlo keeps the"),
            timeout=45)
    h.restart_server()

    s.step("the goal waits, paused, and says why")
    h.wait("The server stopped while the agent worked on it.", timeout=45)
    s.expect(h.find("Paused"), "paused", h.texts()[:40])
    h.shot("stopped")

    s.step("resumed from its checkpoint, it is reached")
    with open(scripts, "w") as f:
        json.dump({"<resume>": "say Picking it up."}, f)
    h.click("Resume")
    h.wait("Reached", timeout=45)
    # What the agent was told is the resume prompt, in any request since.
    told = ""
    with open(os.path.join(h.info["sandbox"], "model.jsonl")) as f:
        for line in f:
            if "Pick the goal up again where it stopped." not in line:
                continue
            for message in json.loads(line)["body"]["messages"]:
                content = message.get("content")
                content = content if isinstance(content, str) else json.dumps(content)
                if "Pick the goal up again where it stopped." in content:
                    told = content
    for expected in ("<checkpoint>", "- changed first.md", "Still missing: more.md is missing.",
                     "Next: Write more.md.", "<since>", "- changed more.md",
                     "The server stopped before this step ended."):
        s.expect(expected in told, f"the agent is told: {expected}", told[:1500])
    h.shot("resumed")
    os.remove(scripts)
    h.click("Dismiss")
    h.wait_gone("Reached", timeout=15)
