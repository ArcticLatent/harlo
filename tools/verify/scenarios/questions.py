"""Questions: the agent asks with OpenCode's question tool; the card shows
the question and its options, and the chosen answer goes back to it."""

ENGINE = True

import json

from _common import open_project_session, send

QUESTION = {"questions": [{
    "question": "Which colour?", "header": "Colour",
    "options": [{"label": "Red", "description": "warm"}, {"label": "Blue", "description": "cool"}],
}]}


def run(h, s):
    open_project_session(h, title="Questions")
    send(h, f"tool question {json.dumps(QUESTION)} ;; say got the answer")

    s.step("the question card")
    h.wait("Which colour?", timeout=30)
    s.expect(h.says("has a question"), "who asks", h.texts()[:20])
    h.shot("asked")

    s.step("answered")
    h.click("Blue")
    h.shot("chosen")
    if h.find("Send answer", role="Button"):
        h.click("Send answer")
    h.wait_says("got the answer", timeout=30)
    s.expect(not h.find("Which colour?", role="Text"), "the card gone")
    h.shot("answered")
