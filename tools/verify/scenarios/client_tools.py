"""Harlo's own tools on a server that can't take them: the server says
they're unavailable, Harlo stops asking (no retry every few seconds) and
Settings → Tools for agents says why. Only on harlo-server: OpenCode takes
them."""

import json
import os
import time

from _common import settings

ENGINE = "only"

LOG = f"/tmp/harlo-verify-client-tools.{os.getpid()}.jsonl"
ENV = {"HARLO_GUI_API_LOG": LOG}


def attempts():
    try:
        with open(LOG) as f:
            return sum(1 for line in f if "client-mcp" in json.loads(line).get("path", ""))
    except OSError:
        return 0


def run(h, s):
    try:
        s.step("Settings says why the tools aren't offered")
        settings(h, "Tools for agents")
        h.wait("Not available on this server", timeout=20)
        s.expect(h.shows("This server doesn't offer clients' tools to its agents yet."),
                 "the server's reason")
        h.shot("unavailable")

        s.step("Harlo stops asking")
        first = attempts()
        time.sleep(12)
        again = attempts()
        s.expect(first >= 1, "it asked once per folder", f"{first} attempts")
        s.expect(again == first, "no more attempts", f"{first} then {again}")
    finally:
        if os.path.exists(LOG):
            os.remove(LOG)
