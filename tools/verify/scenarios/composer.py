"""The composer: "/" lists commands and skills, "@" files; the agent and
model menus open and close."""

ENGINE = True

from _common import composer, open_project_session


def run(h, s):
    open_project_session(h, title="Composer")

    s.step("the / menu")
    h.click(None, role="TextInput", where=composer)
    h.type("/")
    h.wait("/compact", timeout=10)
    h.shot("slash")
    h.key("Escape")
    h.wait_gone("/compact")
    h.key("ctrl+a", "BackSpace")

    s.step("the @ menu, and a file picked from it")
    h.type("@b")
    h.wait("@b.txt", timeout=10)
    h.shot("at")
    h.click("@b.txt")
    h.wait_gone("@b.txt", role="Button")
    field = h.find(None, role="TextInput", where=composer)
    s.expect(field and "b.txt" in field[0].value, "the file in the message",
             field[0].value if field else None)
    h.shot("picked")
    h.key("ctrl+a", "BackSpace")

    for menu in ("Build", "Fake Coder"):
        s.step(f"the {menu} menu")
        h.click(menu, where=lambda e: e.y > 600 and e.role == "Button")
        h.settle(0.8)
        h.shot(menu.lower().replace(" ", "-"))
        h.key("Escape")
        h.settle(0.5)
