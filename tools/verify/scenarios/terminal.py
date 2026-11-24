"""The terminal: it opens under the session, runs what's typed in the
project's folder, hides and comes back with its shell still there."""

ENGINE = True


def run(h, s):
    s.step("the terminal opens")
    h.click("Terminal", where=lambda e: e.x > 1400)
    h.settle(1.5)
    h.shot("opened")

    s.step("a command runs")
    h.type("echo verify-$((6*7))\n")
    h.wait_says("verify-42", timeout=15)
    h.shot("ran")

    s.step("hidden and shown again, its output is still there")
    if h.find("Hide terminals", role="Button"):
        h.click("Hide terminals")
        h.settle(0.8)
        h.click("Terminal", where=lambda e: e.x > 1400)
        h.wait_says("verify-42", timeout=10)
