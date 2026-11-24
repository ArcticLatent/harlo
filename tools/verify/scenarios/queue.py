"""A busy session: a message sent while it works waits in the queue, with
Steer and Cancel; Stop ends the run."""

ENGINE = True

from _common import COMPOSER, open_project_session, send


def run(h, s):
    open_project_session(h, title="Queue")

    s.step("a slow run")
    send(h, "pace 250 | long 6")
    h.wait("Stop", timeout=20)

    s.step("a message waits in the queue")
    send(h, "say then update the changelog")
    h.wait_says("then update the changelog", timeout=10)
    s.expect(h.find("Steer", role="Button"), "Steer")
    h.shot("queued")

    s.step("Stop ends the run")
    h.click("Stop")
    h.settle(1.5)
    h.shot("stopped")
    s.expect(h.find(COMPOSER, role="TextInput"), "the composer back")
