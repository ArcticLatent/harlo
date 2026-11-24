"""Settings: every section opens and shows its rows; the search finds a
setting; Escape closes. Each section is kept as a screenshot."""

from _common import close_sheet, settings

SECTIONS = ["General", "Appearance", "Notifications", "Shortcuts", "Voice", "Projects", "Git",
            "Providers", "Models", "Local models", "Agents", "Extensions", "Tools for agents",
            "Schedules", "Server", "Phone", "About"]


def run(h, s):
    for section in SECTIONS:
        s.step(f"Settings → {section}")
        settings(h, section)
        h.settle(0.8)
        s.expect(h.find(section, role="Text", where=lambda e: e.x > 560 and e.y < 200)
                 or h.find(section, where=lambda e: e.x > 560 and e.y < 200),
                 f"{section}'s heading")
        h.shot(section.lower().replace(" ", "-"))

    s.step("the search finds a setting")
    h.fill("Search settings", "reopen")
    s.expect(h.shows("Reopen tabs"), "Reopen tabs listed")
    h.shot("search")
    h.fill("Search settings", "")

    s.step("Escape closes Settings")
    close_sheet(h)
