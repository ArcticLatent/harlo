"""What several scenarios do: open a session in the sandbox's project, send
a script to the scripted model, open a Settings section.

The scripted model (tools/fake-model.py) runs what a prompt says: steps
separated by ";;", parts by " | ", such as `say hi`, `think …`,
`tool edit {…}`, `long 3` or `pace 40`.
"""

COMPOSER_NEW = "Ask, or describe what to build · Tab switches agents"
COMPOSER = "Send a message to continue this session · Tab switches agents"


def sidebar(e):
    """Whether an element is in the sidebar (left of the sessions' column)."""
    return e.x < 340


def open_project_session(h, title=None):
    """Makes a session in the sandbox's project through OpenCode and opens
    it from the sidebar; returns its ID."""
    created = h.api("POST", "/api/session", {"location": {"directory": h.project}})["data"]
    if title:
        h.api("PATCH", f"/api/session/{created['id']}", {"title": title})
    h.click(title or "Untitled session", role=None, where=sidebar)
    h.wait(None, role="TextInput", where=composer)
    return created["id"]


def composer(e):
    """Whether an element is a session's composer, idle or busy."""
    return e.role == "TextInput" and (e.label.startswith("Send a message") or
                                      e.label.startswith("Queue") or "Ctrl+Enter" in e.label
                                      or e.label == COMPOSER_NEW)


def send(h, script, composer_label=None):
    """Types `script` into the composer (the session's, or the start
    screen's) and sends it with Enter."""
    if composer_label:
        h.fill(composer_label, script)
    else:
        h.fill(None, script, where=composer)
    h.key("Return")


def settings(h, section):
    """Opens Settings at `section`."""
    if not h.find("Search settings", role="TextInput"):
        h.click("Settings", where=lambda e: e.x < 60)
        h.wait("Search settings", role="TextInput")
    h.click(section, where=lambda e: e.x > 60 and e.w > 100)


def close_sheet(h):
    h.key("Escape")
    h.wait_gone("Search settings", role="TextInput")
