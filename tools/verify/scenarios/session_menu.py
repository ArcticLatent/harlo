"""The session menu: Fork copies the session into a new one beside it,
Archive takes a session out of the list (the server keeps it), and Delete
for good removes it from the server."""

from _common import open_project_session, send, sidebar


def menu(h, item):
    h.click("Session actions")
    h.wait(item, role=None, timeout=10)
    h.click(item, role=None)


def run(h, s):
    first = open_project_session(h, title="Talk")
    send(h, "say one")
    h.wait_says("one", timeout=30)
    # Named after the reply, so a generated title can't race the check.
    h.api("PATCH", f"/api/session/{first}", {"title": "Lexer"})
    h.wait("Lexer", role=None, where=sidebar, timeout=15)

    s.step("Fork")
    menu(h, "Fork")
    h.wait("Lexer (fork #1)", role=None, where=sidebar, timeout=15)
    forks = [x for x in h.api("GET", "/api/session")["data"]
             if x.get("fork", {}).get("sessionID") == first]
    s.expect(forks, "a session forked from the first", forks)
    h.shot("forked")

    s.step("Archive")
    h.click("Lexer (fork #1)", role=None, where=sidebar)
    h.settle(0.5)
    menu(h, "Archive")
    h.wait_gone("Lexer (fork #1)", role=None, where=sidebar, timeout=15)
    kept = h.api("GET", f"/api/session/{forks[0]['id']}")["data"]
    s.expect(kept.get("time", {}).get("archived"), "the server keeps it, archived", kept)
    h.shot("archived")

    s.step("Delete for good")
    h.click("Lexer", role=None, where=sidebar)
    h.settle(0.5)
    menu(h, "Delete…")
    h.wait("Delete for good", role=None, timeout=10)
    h.click("Delete for good", role=None)
    h.wait_gone("Lexer", role=None, where=sidebar, timeout=15)
    left = [x["id"] for x in h.api("GET", "/api/session")["data"]]
    s.expect(first not in left, "gone from the server", left)
    h.shot("deleted")
