"""Schedules: one made in Settings → Schedules is on OpenCode's server,
pauses, resumes and is deleted."""

from _common import open_project_session, settings

# OpenCode's own config folder is where schedules are allowed, so Harlo
# attaches as to the shared server.
SHARED = True


def run(h, s):
    # A project to send it to: the sandbox's, once it has a session.
    open_project_session(h, title="Scheduled")
    settings(h, "Schedules")
    h.wait("Allow schedules", timeout=10)
    h.shot("not-allowed")
    if not h.find("New schedule"):
        h.click("Allow schedules", role="Checkbox")
    h.wait("New schedule", timeout=15)
    h.shot("empty")

    s.step("made")
    h.click("New schedule")
    h.fill("Name", "Morning check", where=lambda e: e.role == "TextInput")
    h.click("Runs: Every")
    h.fill("Time", "30 minutes")
    h.fill("Prompt", "Check the build.")
    if h.find("Choose one"):
        h.click("Change", index=0)
        h.settle(0.5)
        h.click("project", where=lambda e: e.x > 300)
    h.shot("form")
    h.click("Create schedule")
    h.wait_says("Every 30 minutes", timeout=15)
    h.shot("made")

    s.step("paused and resumed")
    h.click("Morning check", role=None, where=lambda e: e.x > 560)
    h.click("Pause")
    h.wait_says("Paused", timeout=10)
    h.click("Resume")
    h.settle(1)

    s.step("deleted")
    h.click("Delete")
    h.click("Delete for good")
    h.wait_says("No schedules yet", timeout=10)
