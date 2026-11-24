"""Commands and skills: a project command runs from the composer with its
argument filled in, and a project skill attached with "/" reaches the
agent."""

from _common import composer, open_project_session, send


def run(h, s):
    h.file(".harlo/commands/greet.md",
           "---\ndescription: Says a greeting\n---\nsay Hello $1 from the command\n")
    h.file(".harlo/skills/release-notes/SKILL.md",
           "---\nname: release-notes\ndescription: Writes release notes.\n---\n\nKeep them short.\n")
    open_project_session(h, title="Commands")

    s.step("the / menu lists both")
    h.click(None, role="TextInput", where=composer)
    h.type("/")
    h.wait("/greet", timeout=10)
    h.shot("menu")
    h.type("rel")
    h.wait("/release-notes", timeout=10)
    h.key("Escape")
    h.key("ctrl+a", "BackSpace")

    s.step("a command runs")
    send(h, "/greet Bob")
    h.wait_says("Hello Bob from the command", timeout=30)
    h.shot("command")

    s.step("a skill goes with a prompt")
    send(h, "/release-notes Write the notes.")
    # The skill's text comes first, so the scripted model answers "ok.".
    h.wait_says("ok.", timeout=30)
    s.expect(h.find("release-notes"), "the skill on the message", h.texts()[:30])
    h.shot("skill")
