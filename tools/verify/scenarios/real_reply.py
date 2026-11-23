"""A real model replies: a session on Claude answers a prompt, and the
answer reads as text in the transcript."""

from _common import open_project_session, send
from real import pick_model


def answered(h, word, prompt):
    return any(word in t.lower() and t != prompt for t in h.texts())


def run(h, s):
    open_project_session(h, title="Real reply")
    pick_model(h)
    prompt = "Reply with the single word pineapple and nothing else."
    send(h, prompt)
    s.step("the answer")
    h.until("an answer", lambda: answered(h, "pineapple", prompt), timeout=120)
    h.shot("answered")
