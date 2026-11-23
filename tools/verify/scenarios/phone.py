"""Phone access: turned on in Settings → Phone it listens, shows a pairing
code, and turns off again. (It runs on a free port; the relay stays off.)"""

from _common import settings


def run(h, s):
    settings(h, "Phone")
    h.wait("Phone access", timeout=10)
    h.shot("off")

    s.step("turned on")
    h.click("Phone access", role="Checkbox")
    h.wait_says("Listening", timeout=15)

    s.step("a pairing code")
    h.click("Show a code")
    h.wait("Pairing QR code", timeout=10)
    h.click("Copy", where=lambda e: e.x > 560)
    h.settle(0.5)
    link = h.clipboard()
    s.expect("/#p=" in link, "the pairing link copied", link[:200])
    h.shot("code")

    s.step("turned off")
    h.click("Phone access", role="Checkbox")
    h.until("not listening", lambda: not h.says("Listening"), timeout=10)
