"""Approvals: a command the agent wants to run waits for Allow; allowed
once, it runs and the answer follows. A second one is denied with a
reason, which the agent is told."""

ENGINE = True

from _common import open_project_session, send


def run(h, s):
    open_project_session(h, title="Approvals")

    s.step("a command waits for approval")
    send(h, 'tool shell {"command":"echo hi >> c.txt"} ;; say ran it')
    h.wait("Allow once", role="Button", timeout=30)
    h.shot("asked")

    s.step("allowed once, it runs")
    h.click("Allow once")
    h.wait_says("ran it", timeout=30)
    s.expect(open(f"{h.project}/c.txt").read().strip() == "hi", "the command ran")
    h.shot("allowed")

    s.step("denied with a reason")
    send(h, 'tool shell {"command":"rm -rf build"} ;; say understood')
    h.wait("Deny with a reason…", role="Button", timeout=30)
    h.click("Deny with a reason…")
    h.fill(None, "not now", where=lambda e: e.placeholder.startswith("Tell ") or e.label.startswith("Tell "))
    h.shot("reason")
    h.key("Return")
    h.wait_gone("Allow once", role="Button", timeout=30)
    h.shot("denied")
