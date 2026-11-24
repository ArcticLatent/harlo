"""First start: the setup guide opens on its own and walks through its
steps (OpenCode, models, git, theme); skipped, it doesn't come back after
a restart."""

PREFERENCES = {"setup_done": False}


def run(h, s):
    s.step("the guide opens on its own")
    h.wait("Set up Harlo", timeout=20)
    s.expect(h.says("Connected"), "the server's status", h.texts()[:20])
    h.shot("opencode")

    for step in ("Providers", "Git", "Theme"):
        s.step(f"the {step} step")
        h.click("Next")
        h.settle(0.6)
        h.shot(step.lower())

    s.step("skipped, it stays closed")
    h.click("Skip setup")
    h.wait_gone("Set up Harlo")
    h.restart()
    h.settle(2)
    s.expect(not h.shows("Set up Harlo"), "no guide after a restart")
