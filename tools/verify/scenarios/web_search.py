"""Web search: the first search asks whether to search the web, with which
service, or not at all; the answer goes back and the turn goes on."""

from _common import open_project_session, send

QUESTION = "to search the web for up-to-date information?"


def run(h, s):
    open_project_session(h, title="Web search")
    send(h, 'tool websearch {"query": "release notes"} ;; say searched or not')

    s.step("the question card")
    h.wait_says(QUESTION, timeout=30)
    s.expect(h.says("Disable web search"), "its choices", h.texts()[:30])
    h.shot("asked")

    s.step("turned off")
    # (Turning it off sends nothing to a real service.)
    h.click("Disable web search")
    if h.find("Send answer", role="Button"):
        h.click("Send answer")
    h.wait_says("searched or not", timeout=30)
    s.expect(not h.says(QUESTION), "the card gone")
    h.shot("answered")
