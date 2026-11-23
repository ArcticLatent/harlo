"""The Keyboard shortcuts and Stats pages open from the rail and close
with Escape."""


def rail(e):
    return e.x < 60


def run(h, s):
    s.step("keyboard shortcuts")
    h.click("Keyboard shortcuts", where=rail)
    h.wait("Start a new session", timeout=10)
    h.shot("shortcuts")
    h.element("Save the file", "Text", 0)
    h.shot("shortcuts-end")
    h.key("Escape")
    h.wait_gone("Start a new session")

    s.step("stats")
    h.click("Stats", where=rail)
    h.wait("All time", timeout=10)
    h.settle(1.5)
    h.shot("stats")
    h.key("Escape")
    h.wait_gone("All time")
