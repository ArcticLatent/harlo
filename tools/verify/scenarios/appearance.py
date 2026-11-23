"""Appearance: light and dark, a colour theme in each, seen across the
window with a session open."""

from _common import close_sheet, open_project_session, send, settings


def run(h, s):
    open_project_session(h, title="Looks")
    send(h, "say # Heading | say some **bold** text and `code`")
    h.wait_says("Heading", timeout=30)

    s.step("light")
    settings(h, "Appearance")
    h.click("Light or dark: Light")
    h.wait("Light themes", timeout=10)
    h.shot("light-settings")
    h.click("Solarized Light")
    h.settle(0.8)
    close_sheet(h)
    h.shot("light-window")

    s.step("dark")
    settings(h, "Appearance")
    h.click("Light or dark: Dark")
    h.wait("Dark themes", timeout=10)
    h.click("Dracula")
    h.settle(0.8)
    close_sheet(h)
    h.shot("dark-window")

    s.step("kept after a restart")
    h.restart()
    settings(h, "Appearance")
    s.expect(h.find("Dark themes"), "still dark", h.texts()[:30])
