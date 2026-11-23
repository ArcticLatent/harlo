"""A real model asks a question with its question tool; the card takes the
answer and the model uses it."""

from _common import open_project_session, send
from real import pick_model


def run(h, s):
    open_project_session(h, title="Real question")
    pick_model(h)
    send(h, "Use your tool for asking the user a question to ask me which colour I prefer, "
            "with the options Red and Blue. After I answer, reply with the colour I chose.")
    s.step("the question card")
    h.wait("Blue", timeout=180)
    h.shot("asked")
    h.click("Blue")
    if h.find("Send answer", role="Button"):
        h.click("Send answer")
    s.step("the model uses the answer")
    h.until("the colour back", lambda: any("blue" in t.lower() and "Red and Blue" not in t
                                           for t in h.texts() if "prefer" not in t), timeout=120)
    h.shot("answered")
