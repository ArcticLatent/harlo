"""A real model asks to run a command: the approval card waits, and once
allowed the command runs in the project."""

import os

from _common import open_project_session, send
from real import pick_model


def run(h, s):
    open_project_session(h, title="Real approval")
    pick_model(h)
    send(h, "Run this shell command in the project: echo approved > r.txt. Then say done.")
    s.step("the approval card")
    h.wait("Allow once", role="Button", timeout=180)
    h.shot("asked")
    h.click("Allow once")
    s.step("the command ran")
    h.until("r.txt", lambda: os.path.exists(os.path.join(h.project, "r.txt")), timeout=120)
    h.settle(3)
    h.shot("ran")
