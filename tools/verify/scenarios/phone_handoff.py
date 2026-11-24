"""The phone handoff: an edit waits for approval, Harlo's process is ended
(SIGKILL, not a hidden window), a paired phone approves the edit (sending
the same answer twice, as a retrying phone would), and Harlo, started
again, reconnects to a finished turn.

Asserts one file edit, one completed tool call and one approval decision,
and that the server's event history replays in order with one ask and one
answer.

The phone is a stand-in (crates/harlo-core/examples/phone_stub.rs) that
speaks the phone app's encrypted protocol over loopback. The relay stays
off: nothing leaves this computer."""

import json
import os
import subprocess
import threading
import time
import urllib.request
import base64

from _common import open_project_session, send, settings, close_sheet

# Phone access on, on a free port, relay off.
PREFERENCES = {"phone": True, "phone_relay": False}
ENV = {"HARLO_PHONE_PORT": "0"}

EDIT = 'tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega.'

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
STUB = os.path.join(REPO, "target", "debug", "examples", "phone_stub")


class Stub:
    """The stand-in phone, its lines collected as they come."""

    def __init__(self, link, session, go):
        self.lines = []
        self.proc = subprocess.Popen([STUB, link, session, go], stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True)
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.lines.append(line.strip())

    def wait(self, prefix, timeout=40):
        deadline = time.time() + timeout
        while time.time() < deadline:
            for line in self.lines:
                if line.startswith(prefix):
                    return line
                if line.startswith("failed"):
                    raise AssertionError(f"the phone failed: {line}")
            time.sleep(0.1)
        raise AssertionError(f"the phone never said {prefix!r}: {self.lines}")

    def stop(self):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait(timeout=5)


def replay(h):
    """The server's durable events from the start, as (id, type), read the
    way a reconnecting client asks for them."""
    request = urllib.request.Request(h.info["url"] + "/api/event")
    token = base64.b64encode(f"opencode:{h.info['password']}".encode()).decode()
    request.add_header("Authorization", f"Basic {token}")
    request.add_header("Last-Event-ID", "0")
    events, block = [], {}
    try:
        with urllib.request.urlopen(request, timeout=2) as r:
            for raw in r:
                line = raw.decode().rstrip("\n")
                if not line:
                    if "id" in block and "data" in block:
                        events.append((int(block["id"]), json.loads(block["data"])["type"]))
                    block = {}
                elif ":" in line:
                    key, _, value = line.partition(":")
                    block[key] = value.strip()
    except (TimeoutError, OSError):
        pass
    return events


def run(h, s):
    subprocess.run(["cargo", "build", "-q", "-p", "harlo-core", "--example", "phone_stub"],
                   cwd=REPO, check=True)
    go = f"/tmp/harlo-verify-handoff-go.{os.getpid()}"
    stub = None
    try:
        s.step("phone access runs on the server")
        settings(h, "Phone")
        h.wait("Listening on the server", timeout=20)
        h.shot("phone-on-server")
        close_sheet(h)

        s.step("a phone pairs")
        session = open_project_session(h, title="Handoff")
        h.click("Permissions", role="Button")
        h.click("Ask before edits and commands", role=None)
        h.settle(0.5)
        link = h.api("POST", "/harlo/phone/pair")["link"]
        stub = Stub(link, session, go)
        stub.wait("paired")

        s.step("the edit waits for approval on both")
        send(h, EDIT)
        h.wait("Allow once", role="Button", timeout=30)
        asked = stub.wait("asked ").split()[1]
        s.expect(open(f"{h.project}/a.txt").read() == "alpha\n", "nothing written before approval")
        h.shot("asked")

        s.step("Harlo's process ends")
        pid = h.kill()
        s.expect(not os.path.exists(f"/proc/{pid}") or
                 open(f"/proc/{pid}/stat").read().split()[2] == "Z", "the process is gone", str(pid))
        pending = h.api("GET", f"/api/session/{session}/permission")["data"]
        s.expect([p["id"] for p in pending] == [asked], "the approval still waits on the server")

        s.step("the phone approves, twice")
        open(go, "w").close()
        answered = stub.wait("answered")
        s.expect(answered == "answered true true", "both answers accepted", answered)
        stub.wait("cleared")
        deadline = time.time() + 30
        while open(f"{h.project}/a.txt").read() != "omega\n" and time.time() < deadline:
            time.sleep(0.2)
        s.expect(open(f"{h.project}/a.txt").read() == "omega\n", "edited once, with no window open")

        s.step("Harlo starts again and reconnects")
        h.revive()
        h.wait("Handoff", role="Tab", timeout=20)
        h.wait_says("Changed alpha to omega", timeout=20)
        s.expect(not h.find("Allow once", role="Button"), "no stale approval")
        h.shot("reconnected")

        s.step("one edit, one completed call, one decision")
        messages = h.api("GET", f"/api/session/{session}/message")["data"]
        tools = [p for m in messages if m.get("type") == "assistant"
                 for p in m.get("content", []) if p.get("type") == "tool"]
        s.expect(len(tools) == 1 and tools[0]["state"]["status"] == "completed",
                 "one completed tool call", json.dumps([t["state"]["status"] for t in tools]))
        events = replay(h)
        kinds = [k for _, k in events]
        ids = [i for i, _ in events]
        s.expect(ids == sorted(ids) and len(ids) == len(set(ids)), "replayed once, in order")
        s.expect(kinds.count("permission.asked") == 1, "one ask", str(kinds.count("permission.asked")))
        s.expect(kinds.count("permission.replied") == 1, "one decision",
                 str(kinds.count("permission.replied")))
        s.expect(kinds.count("session.tool.success") == 1, "one tool run",
                 str(kinds.count("session.tool.success")))
        s.expect(kinds.index("permission.asked") < kinds.index("permission.replied")
                 < kinds.index("session.tool.success"), "ask, answer, run, in that order")
        s.expect(not h.api("GET", f"/api/session/{session}/permission")["data"], "nothing waits")
    finally:
        if stub:
            stub.stop()
        if os.path.exists(go):
            os.remove(go)
