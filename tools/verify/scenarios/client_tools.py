"""Harlo's own tools: the server takes them over one connection, for every
folder, which stays open (no retry every few seconds), and Settings →
Tools for agents shows no refusal."""

import json
import os
import time

from _common import settings

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
        s.step("Settings offers the tools, with no refusal")
        settings(h, "Tools for agents")
        h.wait("Past sessions", timeout=20)
        h.settle(3)
        s.expect(not h.shows("Not available on this server"), "not refused")
        s.expect(not h.shows("Not offered"), "no error")
        h.shot("offered")

        s.step("the connection stays open")
        first = attempts()
        time.sleep(12)
        again = attempts()
        s.expect(first == 1, "one connection", f"{first} attempts")
        s.expect(again == first, "no reconnecting", f"{first} then {again}")
    finally:
        if os.path.exists(LOG):
            os.remove(LOG)
