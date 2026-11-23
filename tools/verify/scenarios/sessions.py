"""Sessions: a new one from the start screen gets a reply and a title,
shows in the sidebar and a tab; a second opens beside it; tabs switch and
close; the filter finds a session by its title."""

from _common import COMPOSER, COMPOSER_NEW, open_project_session, send, sidebar


def run(h, s):
    s.step("the start screen")
    h.wait("What are we working on?")
    h.shot("start")

    s.step("a new session answers")
    send(h, "say hello from the scripted model", composer_label=COMPOSER_NEW)
    h.wait_says("hello from the scripted model", timeout=30)
    s.expect(h.find("say hello from the scripted model", role="Groupbox"), "the prompt's bubble")
    h.wait(COMPOSER, role="TextInput")
    h.shot("replied")

    s.step("a second session, opened from the sidebar")
    open_project_session(h, title="Second session")
    send(h, "say second answer")
    h.wait_says("second answer", timeout=30)
    s.expect(h.find("Second session", role="Tab"), "its tab")
    h.shot("second")

    s.step("a click on a tab shows its session")
    first = h.find(None, role="Tab", where=lambda e: e.label != "Second session")
    s.expect(first, "the first session's tab", [e.label for e in h.find(None, role="Tab")])
    h.click(first[0].label, role="Tab")
    h.until("the first session to show", lambda: h.says("hello from the scripted model"), timeout=5)
    h.click("Second session", role="Tab")
    h.until("the second session to show", lambda: h.says("second answer"), timeout=5)

    s.step("the filter finds it")
    h.fill("Filter sessions", "Second")
    s.expect(h.find("Second session", where=sidebar),
             "the session matching the filter")
    h.fill("Filter sessions", "")

    s.step("tabs close with Ctrl+W")
    s.expect(h.find("Second session", role="Tab"), "its tab before")
    h.key("ctrl+w")
    h.until("the tab to close", lambda: not h.find("Second session", role="Tab"), timeout=5)
    s.expect(h.find("Second session", where=sidebar), "the session still in the sidebar")
    h.shot("closed")
