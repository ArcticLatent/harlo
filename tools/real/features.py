#!/usr/bin/env python3
"""Harlo's main features, each tried once on a real model.

    tools/real/features.py [--plan DIR] [--model openai/gpt-6.1-sol] [--only NAME[,NAME…]]

It runs like tools/real/memory-goals.py: on the ChatGPT sign-in kept in the
scratch folder DIR, on a private server with scratch HOME and XDG folders,
with the agent's commands in the sandbox and the real home folder hidden.
Each case has a project and a session of its own:

    approvals   a command asks leave: allowed once it runs; refused with a reason, the agent goes on
    question    the agent asks the person, and uses the answer
    subagent    a subagent does part of the work in a session of its own and reports back
    compaction  after the conversation is summarized, the agent still knows what was said
    plan        the plan agent plans and changes nothing
    stop        a run is stopped in the middle of a command; the next message is answered
    steer       a message sent during a run changes what the run does
    undo        a turn's changes to files are taken back
    recall      a new session finds what an earlier one settled
    skill       a project's skill is loaded when the task calls for it
    mcp         an MCP server's tool is called, plainly and from a script
    webfetch    a public page is fetched and read
    image       a picture sent with a message is seen
    aside       a side question about the session is answered without joining it
    title       the session gets a title from its first message
    schedule    the agent schedules a prompt for later and a repeating one, they fire, it removes them
    plugin      a plugin's tool, added and allowed, is called by the agent; its command becomes a prompt
    background  a command runs in the background, is moved there, and is cancelled; results come back
    worktree    a session in a worktree changes and commits there, and the main checkout stays as it was
    websearch   the web is searched and the answer names its sources
    phone       a phone pairs, sends a message, allows the edit it is asked about, reads the answer
    searchform  the first search asks the person in a form; a chosen service is kept for every project,
                and "disable" turns web search off
    events      a plugin's code runs each time a turn ends; a failure there is shown on the plugin
    cron        the agent makes a cron schedule, it fires on the minute, the agent pauses it
    schedulecmd the agent schedules commands: they run in the sandbox, a failed one is kept as failed,
                and none is made where commands ask first
    schedulescript  the agent schedules a script that keeps a count between runs, sends a prompt, ends
    scheduleoutside a scheduled command the rules let out of the sandbox leaves it; another stays inside
    eventtrigger    asked for a schedule on an MCP server's event, the agent says truthfully what it can't do
    fork        a session is forked before a message, exported and imported elsewhere, and moved to a folder
    always      "always" on a command saves an approval: no ask in a new session, asked again once removed
    model       the model, then its reasoning level, is changed in the middle of a session
    custom      a project's own agent (its instructions and rules) and its own command
    switches    the optional switches: todo list, outline reads, the session's tools from a script,
                cached instructions
    bang        a command the person runs in the session is known to the agent afterwards
    queue       messages sent while a run is busy wait in line, one is cancelled, the rest are answered
    everywhere  a search over every project's sessions, and a note about the person, each asked about first
    commit      the Changes diff of what the agent did, and a commit message written from it
    mcpremote   an MCP server over HTTP, one behind a browser sign-in, and its resources read by the agent
                and sent with a message that mentions one
    mcpretry    an MCP server that couldn't be reached is connected by the next turn half a minute on
    searchover  the search service the config names is limited, and the search goes to another
    usage       after a turn on the plan, the server tells the usage meter the plan is signed in, where its
                limits show, and that no limit was reached
    ssh         the server is followed over SSH (a private sshd on a loopback port), and a session is run
                through the tunnel (the ignored Rust test real_ssh)

The `mcp` case also sets a rule on one tool of a server: a script's call
of it is refused, or asked about, as the rule says.

The report goes to target/real/. The projects are removed at the end.
"""

import argparse
import base64
import datetime
import importlib.util
import json
import os
import shutil
import sqlite3
import struct
import subprocess
import sys
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
REPORTS = TOOLS.parent / "target/real"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BASE = load("memory_goals", HERE / "memory-goals.py")
texts, calls = BASE.texts, BASE.calls

# The tools a case may use without asking. An MCP server's tools ask, and
# the run answers for the person.
BUILT_IN = ["read", "edit", "glob", "grep", "shell", "question", "subagent", "skill", "webfetch",
            "todowrite", "execute", "search_sessions"]

UNITS = r'''import json, sys
TOOLS = [{"name": "convert",
          "description": "Converts a length between units. Returns the value and a check token.",
          "inputSchema": {"type": "object", "required": ["value", "from", "to"], "additionalProperties": False,
                          "properties": {
                              "value": {"type": "number", "description": "The length to convert."},
                              "from": {"type": "string", "enum": ["miles", "kilometres", "feet", "metres"]},
                              "to": {"type": "string", "enum": ["miles", "kilometres", "feet", "metres"]},
                              "format": {"type": "object", "description": "How to write the result.",
                                         "properties": {"digits": {"type": "integer", "minimum": 0, "maximum": 8},
                                                        "unit": {"type": "boolean"}}}}}}]
METRES = {"miles": 1609.344, "kilometres": 1000.0, "feet": 0.3048, "metres": 1.0}
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"], "capabilities": {"tools": {}},
                  "serverInfo": {"name": "TOKEN", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": TOOLS}
    elif method == "tools/call":
        a = msg["params"]["arguments"]
        value = a["value"] * METRES[a["from"]] / METRES[a["to"]]
        digits = (a.get("format") or {}).get("digits", 3)
        result = {"content": [{"type": "text", "text": f"{value:.{digits}f} {a['to']} (check token TOKEN)"}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32601, "message": "no"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
'''


def png(red, green, blue, size=96):
    """A square picture of one colour."""
    row = b"\x00" + bytes([red, green, blue]) * size

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * size)) + chunk(b"IEND", b""))


class Case:
    """One feature's try: a project, a session, and what the run answers for the person."""

    def __init__(self, run, name):
        self.run, self.name = run, name
        self.asks = []
        # What asks is allowed once, but nothing is read or searched outside the project: those tools
        # aren't held by the sandbox, and the real home isn't the model's to look through.
        # A project's own settings are the test's own: they are allowed for good.
        self.policy = lambda asked: (("reject", "Stay inside the project folder.")
                                     if asked.get("action") == "external_directory"
                                     else ("always", None) if asked.get("action") == "project_config"
                                     else ("once", None))
        self.project = run.project({"stats.py": BASE.STATS, "test_stats.py": BASE.STATS_TESTS})
        self.configure()
        made = run.session(self.project)
        self.session, self.project_id = made["id"], made["projectID"]

    def configure(self, asking=(), extra=None, rules=()):
        """The server's configuration for this case: `asking` tools ask first; `rules` come last."""
        rules = [{"action": "*", "resource": "*", "effect": "ask"},
                 *({"action": action, "resource": "*", "effect": "ask" if action in asking else "allow"}
                   for action in BUILT_IN),
                 *({"action": rule[0], "resource": rule[2] if len(rule) > 2 else "*", "effect": rule[1]}
                   for rule in rules)]
        self.run.server.configure({"model": self.run.model, "permissions": rules,
                                   "sandbox": {"hidden": [str(Path.home())]}, **(extra or {})})

    def check(self, name, ok, seen=""):
        self.run.check(f"{self.name}: {name}", ok, seen)

    def say(self, text):
        self.run.say(f"  [{self.name}] {text}")

    def messages(self):
        return self.run.messages(self.session)

    def idles(self):
        return sum(1 for m in self.messages() if m.get("type") == "idle")

    def answer_asks(self):
        for asked in self.run.call("GET", f"/api/session/{self.session}/permission")["data"]:
            decision, message = self.policy(asked)
            self.asks.append((asked, decision))
            body = {"decision": decision}
            if message:
                body["message"] = message
            self.run.call("POST", f"/api/session/{self.session}/permission/{asked['id']}/reply", body)

    def send(self, text, delivery="queue", **more):
        self.run.call("POST", f"/api/session/{self.session}/prompt", {"text": text, "delivery": delivery, **more})

    def wait(self, idles, limit=420, meanwhile=None):
        """Waits until the session has gone idle more than `idles` times; its messages."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline:
            self.answer_asks()
            if meanwhile:
                meanwhile()
            now = self.messages()
            if sum(1 for m in now if m.get("type") == "idle") > idles:
                return now
            time.sleep(0.4)
        # What it was at, for the report.
        for part in tool_parts(self.messages())[-6:]:
            state = part.get("state", {})
            self.say(f"at the end: {part.get('name')} ({status(part)}): {json.dumps(state.get('input'))[:300]} "
                     f"-> {json.dumps(state.get('content') or state.get('error'))[:500]}")
        for kind in ("permission", "form"):
            waiting = self.run.call("GET", f"/api/session/{self.session}/{kind}")["data"]
            self.say(f"waiting {kind}s: {json.dumps(waiting)[:500]}")
        self.run.call("POST", f"/api/session/{self.session}/interrupt")
        raise TimeoutError(f"the session wasn't idle after {limit} s")

    def turn(self, text, **more):
        """Sends a message and waits for the run to end; the messages it added."""
        before = self.messages()
        idles = sum(1 for m in before if m.get("type") == "idle")
        self.send(text, **more)
        return self.wait(idles)[len(before):]

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.project, capture_output=True, text=True).stdout


def where(project):
    """The query that names a project's folder."""
    return urllib.parse.urlencode({"location[directory]": str(project)})


def said(messages):
    """What the agent said in `messages`."""
    return "\n".join(texts(m) for m in messages if m.get("type") == "assistant").strip()


def tool_parts(messages, name=None):
    return [p for m in messages for p in m.get("content", [])
            if p.get("type") == "tool" and (name is None or p.get("name") == name)]


def status(part):
    state = part.get("state", {})
    return state.get("status") or state.get("type") or ""


def approvals(c):
    c.configure(asking=("shell",))
    new = c.turn("Run `python3 -m unittest -q` and tell me the result in one line.")
    asked = [a for a, _ in c.asks]
    c.say(f"asked: {json.dumps(asked[:1])[:400]}")
    ran = tool_parts(new, "shell")
    c.check("the command asked leave before it ran", len(asked) >= 1, f"{len(asked)} asks")
    c.check("allowed once, it ran and the agent told the result",
            any(status(p) == "completed" for p in ran) and bool(said(new)), f"{[status(p) for p in ran]}: {said(new)[:200]}")

    c.asks.clear()
    c.policy = lambda asked: ("reject", "Not that command. The branch name is in the file .git/HEAD: read it with your read tool instead.")
    new = c.turn("Now run `git branch --show-current` and tell me the branch name.")
    ran = tool_parts(new, "shell")
    c.say(f"after the refusal the agent said: {said(new)[:300]}")
    c.check("refused with a reason, the command did not run",
            bool(c.asks) and not any(status(p) == "completed" for p in ran), f"{[status(p) for p in ran]}")
    c.check("the agent went on as told and still answered",
            bool(tool_parts(new, "read")) and any(w in said(new) for w in ("master", "main")),
            f"calls: {[p.get('name') for p in tool_parts(new)]}")


def question(c):
    form = {}

    def answer():
        if form:
            return
        for found in c.run.call("GET", f"/api/session/{c.session}/form")["data"]:
            form.update(found)
            answers = {}
            for field in found.get("fields", []):
                options = [o.get("value") or o.get("label") for o in field.get("options", [])]
                comma = next((o for o in options if "comma" in str(o).lower()), options[-1] if options else "comma")
                answers[field.get("key") or field.get("id")] = comma
            c.run.call("POST", f"/api/session/{c.session}/form/{found['id']}/reply", {"answer": answers})

    before = c.messages()
    c.send("Add show(x) to stats.py: it returns the number x as text with one decimal. Before you write "
           "anything, ask me with your question tool which decimal separator to use (the options: point, comma). "
           "Then write it and a test, and run the tests.")
    new = c.wait(sum(1 for m in before if m.get("type") == "idle"), meanwhile=answer)[len(before):]
    c.say(f"the form: {json.dumps(form)[:500]}")
    c.check("the agent asked the person with a form", bool(form) and bool(tool_parts(new, "question")))
    shown = subprocess.run(
        ["bwrap", "--ro-bind", "/", "/", "--tmpfs", "/home", "--tmpfs", "/tmp", "--dev", "/dev", "--bind",
         str(c.project), str(c.project), "--unshare-net", "--chdir", str(c.project), sys.executable, "-I", "-c",
         "import sys; sys.path.insert(0, '.'); import stats; print(stats.show(1.5))"],
        capture_output=True, text=True, timeout=60)
    c.check("what it wrote uses the answer (a comma)", shown.stdout.strip() == "1,5",
            (shown.stdout + shown.stderr).strip()[-200:])


def subagent(c):
    new = c.turn("Use a subagent to find out which functions test_stats.py tests and how many test methods it "
                 "has. Don't read the file yourself. Then tell me what the subagent reported.")
    started = tool_parts(new, "subagent")
    children = [s for s in c.run.call("GET", f"/api/session?parentID={c.session}").get("data", [])
                if s.get("parentID") == c.session] if started else []
    # A subagent works in the background: its answer comes back to the
    # parent as a message of its own, and the parent then runs again.
    deadline = time.monotonic() + 300
    while children and time.monotonic() < deadline:
        c.answer_asks()
        now = c.messages()
        back = [i for i, m in enumerate(now) if m.get("type") == "synthetic" and "background-result" in texts(m)]
        if back and now[-1].get("type") == "idle" and any(m.get("type") == "assistant" for m in now[back[-1]:]):
            new = now
            break
        time.sleep(0.5)
    c.say(f"the parent said: {said(new)[-400:]}")
    c.check("the agent started a subagent", bool(started), f"calls: {[p.get('name') for p in tool_parts(new)]}")
    c.check("the subagent worked in a session of its own", bool(children) or None,
            "the list of sessions shows no child (the listing may not take parentID)")
    if children:
        inside = c.run.messages(children[0]["id"])
        c.check("the child read the file itself", any(p.get("name") in ("read", "grep", "shell", "execute") for p in tool_parts(inside)),
                f"{[p.get('name') for p in tool_parts(inside)]}")
    c.check("the parent didn't read the file and reported the child's findings",
            not any(p.get("name") == "read" for p in tool_parts(new)) and "mean" in said(new).lower()
            and any(word in said(new).lower() for word in ("2", "two")), said(new)[-300:])


def compaction(c):
    c.turn("For later in this conversation: the release codename is HERON-17 and the reviewer is called Odalys. "
           "Now read stats.py and test_stats.py and describe each in one line.")
    before = c.messages()
    c.run.call("POST", f"/api/session/{c.session}/compact", {})
    deadline = time.monotonic() + 240
    while time.monotonic() < deadline:
        now = c.messages()
        summarized = [m for m in now if m.get("type") == "compaction"]
        if summarized and summarized[-1].get("status") not in ("pending", "running") \
                and not c.run.call("GET", "/api/session/active")["data"].get(c.session):
            break
        time.sleep(0.5)
    time.sleep(1)
    summary = json.dumps(summarized[-1])[:900] if summarized else ""
    c.say(f"the summary: {summary}")
    c.check("the conversation was summarized by the model",
            bool(summarized) and summarized[-1].get("status") not in ("pending", "running", "failed", "error")
            and "HERON-17" in summary, summary[:300])
    new = c.turn("Without reading anything: what is the release codename, and who reviews? Then put the "
                 "codename in a comment on the first line of stats.py.")
    c.check("after it, the agent still knows what was said before",
            "HERON-17" in said(new) and "Odalys" in said(new), said(new)[:300])
    c.check("and goes on working", "HERON-17" in (c.project / "stats.py").read_text().splitlines()[0],
            (c.project / "stats.py").read_text()[:120])


def plan(c):
    c.run.call("POST", f"/api/session/{c.session}/agent", {"agent": "plan"})
    c.policy = lambda asked: ("reject", "Plan only.")
    new = c.turn("Plan how to add median(xs) and mode(xs) to stats.py with tests. Don't ask me anything.")
    changed = c.git("status", "--porcelain").strip()
    c.check("the plan agent changed no file", not changed and not any(
        status(p) == "completed" for p in tool_parts(new, "edit") + tool_parts(new, "write")), changed)
    c.check("it gave a plan", "median" in said(new).lower() and len(said(new)) > 200, said(new)[:200])


def stop(c):
    before = c.messages()
    c.send("Run `sleep 40; echo finished-sleeping` and then tell me it finished.")
    deadline, running = time.monotonic() + 120, False
    while time.monotonic() < deadline and not running:
        running = any(status(p) == "running" for p in tool_parts(c.messages()[len(before):], "shell"))
        time.sleep(0.3)
    began = time.monotonic()
    c.run.call("POST", f"/api/session/{c.session}/interrupt")
    new = c.wait(sum(1 for m in before if m.get("type") == "idle"), limit=30)[len(before):]
    took = time.monotonic() - began
    ended = [{k: v for k, v in p.get("state", {}).items() if k != "input"} for p in tool_parts(new, "shell")]
    c.say(f"the stopped command: {json.dumps(ended)[:500]}; the run ended as: {json.dumps(new[-1])[:200]}")
    c.check("stopped in the middle of a command, the session is idle at once",
            running and took < 10 and "finished-sleeping" not in json.dumps(ended) and not said(new),
            f"running: {running}, {took:.1f} s, said: {said(new)[:100]}")
    asleep = subprocess.run(["pgrep", "-f", "sleep 40"], capture_output=True, text=True).stdout.split()
    c.check("and the command itself was ended", not asleep, f"still running: {asleep}")
    new = c.turn("Never mind that. Reply with the single word ready.")
    c.check("the next message is answered", "ready" in said(new).lower(), said(new)[:200])


def steer(c):
    before = c.messages()
    c.send("Run `sleep 10; echo waited` first. After that, create a file named a.txt containing the word alpha.")
    deadline, running = time.monotonic() + 120, False
    while time.monotonic() < deadline and not running:
        running = any(status(p) == "running" for p in tool_parts(c.messages()[len(before):], "shell"))
        time.sleep(0.3)
    c.send("Change of plan: name the file b.txt and put the word beta in it. No a.txt.", delivery="steer")
    new = c.wait(sum(1 for m in before if m.get("type") == "idle"))[len(before):]
    made = sorted(p.name for p in c.project.glob("?.txt"))
    c.check("the message was sent while the command ran", running)
    c.check("the run did what the later message said, without a new run",
            made == ["b.txt"] and "beta" in (c.project / "b.txt").read_text()
            and sum(1 for m in new if m.get("type") == "idle") == 1,
            f"files: {made}; idles: {sum(1 for m in new if m.get('type') == 'idle')}")


def undo(c):
    original = [(c.project / name).read_text() for name in ("stats.py", "test_stats.py")]
    text = "Add median(xs) to stats.py and a test for it in test_stats.py."
    new = c.turn(text)
    asked = next(m["id"] for m in c.messages() if m.get("type") == "user" and m.get("text") == text)
    c.check("the agent changed both files", "median" in (c.project / "stats.py").read_text()
            and "median" in (c.project / "test_stats.py").read_text(), c.git("status", "--porcelain"))
    staged = c.run.call("POST", f"/api/session/{c.session}/revert/stage", {"messageID": asked, "files": True})["data"]
    c.say(f"to be taken back: {json.dumps(staged)[:300]}")
    c.run.call("POST", f"/api/session/{c.session}/revert/commit", {})
    c.check("taken back, the files the agent changed are as they were",
            [(c.project / name).read_text() for name in ("stats.py", "test_stats.py")] == original,
            c.git("status", "--porcelain"))
    # What a command of the turn left behind has no writer Harlo can name:
    # it stays, and the person is told.
    left = c.git("status", "--porcelain").strip()
    c.check("what a command left behind stays and is reported, not guessed at",
            (all(line.split()[-1].rstrip("/") in json.dumps(staged.get("report", staged)) for line in left.splitlines())
             if left else None), f"left: {left!r}; report: {json.dumps(staged.get('report'))[:300]}")
    c.check("and the turn is gone from the session", not any(m.get("id") == asked for m in c.messages()))
    new = c.turn("Does stats.py have a median function? Read it and answer yes or no.")
    c.check("the agent sees the files as they are now", "no" in said(new).lower()[:80], said(new)[:200])


def recall(c):
    c.turn("We have settled this for the project: its configuration lives in a TOML file named "
           "harlo-stats.toml, never JSON. Create it with the one line `precision = 3`.")
    (c.project / "harlo-stats.toml").unlink(missing_ok=True)
    other = c.run.session(c.project)["id"]
    first, c.session = c.session, other
    new = c.turn("In an earlier session here we settled where this project's configuration lives and in which "
                 "format. The file isn't there any more, so look it up in our earlier sessions and tell me the "
                 "file's name and format.")
    searched = tool_parts(new, "search_sessions")
    c.say(f"searches: {[p.get('state', {}).get('input') for p in searched]}")
    c.check("the new session searched the earlier ones", bool(searched), f"calls: {[p.get('name') for p in tool_parts(new)]}")
    c.check("and found what was settled", "harlo-stats.toml" in said(new) and "toml" in said(new).lower(), said(new)[:300])
    c.session = first


def skill(c):
    folder = c.project / ".harlo/skills/changelog"
    folder.mkdir(parents=True)
    (folder / "SKILL.md").write_text(
        "---\nname: changelog\ndescription: How this project writes CHANGELOG.md entries. Use it whenever an "
        "entry is added or changed.\n---\n\nEvery entry is one line. It starts with the date as [YYYY-MM-DD] and "
        "ends with the token (ref: ZEBRA-9). Newest entry first, under the heading `# Changes`.\n")
    listed = c.run.call("GET", f"/api/skill?{where(c.project)}").get("data", [])
    c.check("the project's skill is listed", any("changelog" in json.dumps(s) for s in listed), json.dumps(listed)[:200])
    new = c.turn("Add an entry to CHANGELOG.md (create the file) saying that mean now refuses an empty list.")
    written = (c.project / "CHANGELOG.md").read_text() if (c.project / "CHANGELOG.md").exists() else ""
    c.say(f"CHANGELOG.md: {written!r}")
    c.check("the agent loaded the skill unasked", bool(tool_parts(new, "skill")), f"calls: {[p.get('name') for p in tool_parts(new)]}")
    c.check("and followed it", "(ref: ZEBRA-9)" in written and "# Changes" in written and "[20" in written, written[:200])


def mcp(c):
    def serve(name, token, extra):
        script = c.project.parent / f"{c.project.name}-{name}.py"
        script.write_text(UNITS.replace("TOKEN", token))
        c.run.extra.append(script)
        c.configure(extra={"mcp": {"servers": {name: {"type": "local", "command": ["python3", "-I", str(script)], **extra}}}})
        listed, deadline = [], time.monotonic() + 30
        while time.monotonic() < deadline:
            listed = c.run.call("GET", f"/api/mcp?{where(c.project)}").get("data", [])
            if any(s.get("name") == name and "connected" in json.dumps(s) for s in listed):
                break
            time.sleep(0.5)
        c.say(f"servers: {json.dumps(listed)[:300]}")

    def seen(new):
        c.say(f"asks: {[(a.get('action'), a.get('resources')) for a, _ in c.asks]}")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:300]}")
        c.asks.clear()

    # A server whose tools are the model's own tools.
    serve("units", "QUOKKA", {"codemode": False})
    new = c.turn("Use the units server's convert tool to convert 3.5 miles to kilometres with 4 digits, and tell "
                 "me exactly what it returned, check token included.")
    seen(new)
    c.check("a tool of an MCP server, offered as a tool of the model's own, was called and answered",
            "QUOKKA" in said(new) and "5.6327" in said(new) and not tool_parts(new, "execute"), said(new)[:300])
    # A server whose tools are called from scripts (as servers are unless set otherwise).
    serve("lengths", "NUMBAT", {})
    new = c.turn("Now use the lengths server's convert tool for 1200 feet to metres and for 2 miles to feet, and "
                 "tell me both results with their check tokens.")
    seen(new)
    c.check("a server's tools called from a script answered", "NUMBAT" in said(new) and "365.76" in said(new)
            and "10560" in said(new) and bool(tool_parts(new, "execute")), said(new)[:300])
    # A rule on one tool of that server holds for a script's call of it.
    serve_rules = {"mcp": {"servers": {"lengths": {"type": "local", "command": [
        "python3", "-I", str(c.project.parent / f"{c.project.name}-lengths.py")]}}}}
    c.configure(extra=serve_rules, rules=[("lengths_convert", "deny")])
    new = c.turn("Use the lengths server's convert tool for 50 feet to metres. If the tool can't be used, say "
                 "exactly why in one line and don't work the number out yourself.")
    seen(new)
    c.check("a tool a rule refuses isn't run from a script either", "NUMBAT" not in json.dumps(new)
            and "15.24" not in said(new), said(new)[:300])
    c.configure(extra=serve_rules, rules=[("lengths_convert", "ask")])
    new = c.turn("The rule changed. Try the lengths server's convert tool once more for 50 feet to metres and "
                 "give me the result with its check token.")
    asked = [a.get("action") for a, _ in c.asks]
    seen(new)
    c.check("a tool a rule asks about is asked about under its own name when a script calls it",
            "lengths_convert" in asked and "NUMBAT" in said(new) and "15.24" in said(new),
            f"asked: {asked}; {said(new)[:200]}")


def webfetch(c):
    new = c.turn("Fetch https://example.com and tell me the exact text of the page's main heading.")
    fetched = tool_parts(new, "webfetch")
    c.check("the page was fetched", any(status(p) == "completed" for p in fetched),
            f"calls: {[(p.get('name'), status(p)) for p in tool_parts(new)]}")
    c.check("and read", "Example Domain" in said(new), said(new)[:200])


def image(c):
    picture = base64.b64encode(png(214, 40, 40)).decode()
    new = c.turn("What single colour fills this picture? Answer with one word.",
                 files=[{"uri": f"data:image/png;base64,{picture}", "source": {"type": "inline"}, "name": "swatch.png"}])
    c.check("a picture sent with the message is seen", "red" in said(new).lower(), said(new)[:200])


def aside(c):
    c.turn("Add a one-line module comment to test_stats.py saying what it tests.")
    before = len(c.messages())
    answer = c.run.call("POST", f"/api/session/{c.session}/generate",
                        {"prompt": "In one short line: which file did the agent just change?"})["data"]["text"]
    c.check("a side question is answered from the session", "test_stats.py" in answer, answer[:200])
    c.check("and leaves the session as it was", len(c.messages()) == before)


def title(c):
    c.turn("Explain in two lines what the mean function in stats.py does with an empty list.")
    found = ""
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        found = c.run.call("GET", f"/api/session/{c.session}")["data"].get("title") or ""
        if found and found.lower() != "new session":
            break
        time.sleep(1)
    c.check("the session got a title from the model", bool(found) and found.lower() != "new session"
            and len(found.split()) <= 12, found)
    c.say(f"title: {found}")


def until(what, limit=120, every=0.5):
    """Waits until `what()` is truthy; its value, or what it last gave."""
    deadline, found = time.monotonic() + limit, None
    while time.monotonic() < deadline:
        found = what()
        if found:
            return found
        time.sleep(every)
    return found


def schedule(c):
    c.configure(extra={"automations": True})

    def listed():
        return [a for a in c.run.call("GET", "/api/experimental/automation")["data"]
                if a.get("sessionID") == c.session]

    def lines(name):
        path = c.project / name
        return path.read_text().splitlines() if path.exists() else []

    try:
        new = c.turn("Schedule a one-off automation 20 seconds from now. Its prompt: \"Append the line "
                     "one-off-fired to the file ticks.txt in this project and reply with the single word "
                     "done.\" Then tell me its name and when it runs. Don't write the file yourself.")
        made = listed()
        c.say(f"asks: {[(a.get('action'), d) for a, d in c.asks]}")
        c.say(f"automations: {json.dumps(made)[:600]}")
        c.check("the agent scheduled a prompt for later", len(made) == 1 and made[0]["trigger"]["type"] == "at"
                and made[0]["action"]["type"] == "prompt" and not lines("ticks.txt"), said(new)[:300])
        idles = c.idles()
        fired = c.wait(idles, limit=180)
        c.check("it fired on its own and the agent did what it said", lines("ticks.txt") == ["one-off-fired"],
                f"ticks.txt: {lines('ticks.txt')}; {said(fired[-4:])[:200]}")
        left = listed()
        c.check("and is done, not due again", all(a.get("status") == "done" for a in left),
                json.dumps([(a.get("name"), a.get("status")) for a in left]))

        new = c.turn("Now schedule a repeating automation, every 1 minute, with the prompt: \"Append the line "
                     "beat to the file beats.txt in this project and reply with the single word done.\" "
                     "Tell me its name.")
        repeating = [a for a in listed() if a["trigger"]["type"] == "every"]
        c.check("the agent scheduled a repeating prompt", len(repeating) == 1 and repeating[0]["status"] == "active",
                said(new)[:300])
        idles = c.idles()
        c.wait(idles, limit=200)
        beats = len(lines("beats.txt"))
        c.check("it fired after a minute", beats == 1, f"beats.txt: {lines('beats.txt')}")
        new = c.turn("Remove that repeating automation now, and any other automation of this session.")
        c.check("the agent removed it", not listed(), f"left: {json.dumps(listed())[:300]}; {said(new)[:200]}")
        time.sleep(75)
        c.answer_asks()
        c.check("and it fires no more", len(lines("beats.txt")) == beats and not c.run.call(
            "GET", "/api/session/active")["data"].get(c.session), f"beats.txt: {lines('beats.txt')}")
    finally:
        for left in listed():
            c.run.call("DELETE", f"/api/experimental/automation/{left['id']}")


def plugin(c):
    repo = c.project.parent / f"{c.project.name}-starter"
    shutil.copytree(TOOLS.parent / "docs/plugins/starter", repo)
    c.run.extra.append(repo)
    git = ["git", "-c", "user.name=real", "-c", "user.email=real@example.invalid"]
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True)
    subprocess.run([*git, "commit", "-qm", "the starter plugin"], cwd=repo, check=True)
    try:
        added = c.run.call("POST", "/harlo/plugins", {"src": str(repo)})
        added = added.get("data", added)
        c.say(f"added: state {added.get('state')}, asks {added.get('asks')}")
        new = c.turn("Use the starter plugin's count_words tool on stats.py and give me its three numbers. If "
                     "there is no such tool, say so in one line and don't count by any other means.")
        text = (c.project / "stats.py").read_text()
        words = str(len(text.split()))
        c.check("a plugin nobody allowed yet gives the agent no tool",
                added.get("state") == "waiting" and words not in said(new), said(new)[:300])
        c.run.call("POST", "/harlo/plugins/starter/approve")
        c.asks.clear()
        new = c.turn("The starter plugin is allowed now. Use its count_words tool on stats.py and give me the "
                     "lines, words and characters it returned.")
        c.say(f"asks: {[(a.get('action'), d) for a, d in c.asks]}")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:300]}")
        want = [str(len(text.split(chr(10)))), words, str(len(text))]
        c.check("once allowed, the agent called the plugin's tool and got the file's real counts",
                all(n in said(new).replace(",", "") for n in want), f"wanted {want}: {said(new)[:300]}")
        before = c.messages()
        c.run.call("POST", f"/api/session/{c.session}/command",
                   {"name": "standup", "text": "the word counts", "delivery": "queue"})
        new = c.wait(sum(1 for m in before if m.get("type") == "idle"))[len(before):]
        c.check("the plugin's command became a prompt the agent answered",
                len(said(new)) > 40 and "count" in said(new).lower(), said(new)[:300])
    finally:
        c.run.call("DELETE", "/harlo/plugins/starter")
        left = c.run.call("GET", "/harlo/plugins").get("data", [])
        c.check("the plugin is removed from the scratch server again",
                not any(p.get("name") == "starter" for p in left), json.dumps(left)[:200])


def background(c):
    def jobs():
        return c.run.call("GET", f"/api/experimental/job?sessionID={c.session}")["data"]

    def of(kind):
        return [j for j in jobs() if j.get("status") == kind]

    # Asked for in the background: the turn ends while the command runs.
    before, idles = c.messages(), c.idles()
    c.send("Run this command in the background: `sleep 25; echo build-finished > built.txt`. While it runs, "
           "tell me in one line how many functions stats.py defines. When the command has ended, tell me so.")
    first = c.wait(idles)[len(before):]
    shells = tool_parts(first, "shell")
    c.say(f"shell calls: {[json.dumps(p.get('state', {}).get('input'))[:160] for p in shells]}")
    c.say(f"jobs: {json.dumps(jobs())[:400]}")
    c.check("the agent ran the command in the background and answered meanwhile",
            any(p.get("state", {}).get("input", {}).get("background") for p in shells)
            and not (c.project / "built.txt").exists() and bool(of("running")) and bool(said(first)),
            f"{said(first)[:200]}; built.txt: {(c.project / 'built.txt').exists()}; running: {len(of('running'))}")
    later = c.wait(idles + 1, limit=120)[len(before) + len(first):]
    c.check("when it ended, its result came back and the agent said so",
            (c.project / "built.txt").exists() and bool(of("completed")) and bool(said(later)),
            f"{said(later)[:200]}; jobs: {[(j.get('status'), j.get('title')) for j in jobs()]}")

    # Started in the foreground, moved to the background by the person.
    before, idles = c.messages(), c.idles()
    c.send("Run `sleep 30; echo moved-finished > moved.txt` in the foreground (not in the background) and then "
           "tell me what moved.txt holds.")
    running = until(lambda: [p for p in tool_parts(c.messages()[len(before):], "shell")
                             if status(p) == "running"], limit=90, every=0.3)
    if running:
        call = running[0].get("id") or running[0].get("callID")
        moved = c.run.call("POST", f"/api/session/{c.session}/tool/{call}/background")
        c.say(f"moved: {json.dumps(moved)[:200]}")
    began = time.monotonic()
    first = c.wait(idles)[len(before):]
    took = time.monotonic() - began
    c.check("a running command the person moved to the background lets the turn go on at once",
            bool(running) and took < 20 and not (c.project / "moved.txt").exists(),
            f"running: {bool(running)}, {took:.0f} s; {said(first)[:200]}")
    later = c.wait(idles + 1, limit=120)[len(before) + len(first):]
    c.check("and its result came back when it ended",
            (c.project / "moved.txt").exists() and "moved-finished" in said(later) + json.dumps(later),
            said(later)[:200])

    # Cancelled by the person.
    before, idles = c.messages(), c.idles()
    first = c.turn("Run `sleep 240; echo never > never.txt` in the background and reply with the single word "
                   "started.")
    going = of("running")
    for job in going:
        c.run.call("POST", f"/api/experimental/job/{job['id']}/cancel")
    ended = until(lambda: not of("running"), limit=30)
    asleep = subprocess.run(["pgrep", "-f", "sleep 240"], capture_output=True, text=True).stdout.split()
    c.say(f"jobs: {[(j.get('status'), j.get('title')) for j in jobs()]}")
    c.check("a background command is cancelled, and the command itself ended",
            bool(going) and bool(ended) and bool(of("cancelled")) and not asleep
            and not (c.project / "never.txt").exists(), f"going: {len(going)}, still running: {asleep}")
    until(lambda: not c.run.call("GET", "/api/session/active")["data"].get(c.session), limit=90)


def worktree(c):
    main = c.git("rev-parse", "--abbrev-ref", "HEAD").strip()
    # (The scratch home has no git identity; a commit needs one.)
    c.git("config", "user.name", "Real Check")
    c.git("config", "user.email", "real@example.invalid")
    made = c.run.call("POST", "/api/worktree", {"projectID": c.project_id, "name": "median-work"})
    folder = Path(made.get("data", made)["directory"])
    c.say(f"worktree: {folder}")
    try:
        listed = json.dumps(c.run.call("GET", f"/api/worktree?projectID={c.project_id}"))
        c.check("a worktree is made on a branch of its own, outside the project's folder",
                folder.is_dir() and not folder.is_relative_to(c.project) and str(folder) in listed
                and "median-work" in c.git("branch", "--list", "median-work"), listed[:300])
        there = c.run.session(folder)
        c.check("a session there belongs to the same project", there["projectID"] == c.project_id,
                f"{there['projectID']} / {c.project_id}")
        c.session = there["id"]
        new = c.turn("Add a function median(values) to stats.py with a test for it in test_stats.py, run the "
                     "tests, and commit the change with the message \"Add median\". Tell me the branch you "
                     "committed on.")
        c.say(f"asks: {[(a.get('action'), a.get('resources'), d) for a, d in c.asks]}")
        c.say(f"said: {said(new)[-400:]}")
        log = subprocess.run(["git", "log", "--oneline", "median-work"], cwd=c.project,
                             capture_output=True, text=True).stdout
        c.check("the agent changed the worktree's files", "def median" in (folder / "stats.py").read_text())
        c.check("and committed on the worktree's branch", "Add median" in log, log[:200])
        c.check("the main checkout's files and branch stayed as they were",
                "def median" not in (c.project / "stats.py").read_text() and not c.git("status", "--porcelain")
                and c.git("rev-parse", "--abbrev-ref", "HEAD").strip() == main
                and "Add median" not in c.git("log", "--oneline", main),
                f"status: {c.git('status', '--porcelain')!r}")
        passed, last = c.run.tests_pass(folder)
        c.check("the tests pass in the worktree", passed, str(last))
    finally:
        c.run.call("DELETE", "/api/worktree", {"projectID": c.project_id, "directory": str(folder), "force": True})
        c.check("the worktree is removed again", not folder.exists())


def websearch(c):
    c.configure(extra={"websearch": {"provider": "random"}})
    new = c.turn("Search the web: what is the latest stable release of the Rust programming language? Give me "
                 "the version number and the link of one page you found it on.")
    searched = tool_parts(new, "websearch")
    for part in tool_parts(new):
        c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
    c.say(f"said: {said(new)[:400]}")
    c.check("the web was searched", any(status(p) == "completed" for p in searched),
            f"calls: {[(p.get('name'), status(p), json.dumps(p.get('state', {}))[:200]) for p in tool_parts(new)]}")
    answer = said(new)
    c.check("and the answer gives a version and a link", "http" in answer and "1." in answer, answer[:300])


def phone(c):
    c.configure(asking=("edit",))
    server = c.run.server
    env = {**os.environ, "HARLO_REAL_URL": f"http://127.0.0.1:{server.port}",
           "HARLO_REAL_PASSWORD": server.password, "HARLO_REAL_SESSION": c.session,
           "HARLO_REAL_PROJECT": str(c.project)}
    ran = subprocess.run(["cargo", "test", "-q", "-p", "harlo-server", "--test", "real_phone", "--",
                          "--ignored", "--nocapture"], cwd=TOOLS.parent, env=env, capture_output=True,
                         text=True, timeout=900)
    out = ran.stdout + ran.stderr
    for line in out.splitlines():
        if line.startswith(("PHONE", "user ", "markdown ", "tool ", "thinking ")) or "panicked" in line:
            c.say(line[:300])
    c.check("a phone paired, sent a message, allowed the edit it was asked about, read the answer as it came "
            "and asked a side question", ran.returncode == 0, out[-900:])
    c.check("the edit the phone allowed is in the file", "def double" in (c.project / "stats.py").read_text())
    state = c.run.call("GET", "/harlo/phone")
    state = state.get("data", state)
    c.check("phone access is off again and no phone stays paired",
            not state.get("devices") and not state.get("port"), json.dumps(state)[:300])
    # A check that failed halfway leaves the phone paired and access on: neither stays.
    for device in state.get("devices") or []:
        c.run.call("DELETE", f"/harlo/phone/device/{device['id']}")
    if state.get("port") or (state.get("wanted") or {}).get("on"):
        c.run.call("PUT", "/harlo/phone", {**state["wanted"], "on": False})


def ssh(c):
    server = c.run.server
    env = {**os.environ, "HARLO_REAL_URL": f"http://127.0.0.1:{server.port}",
           "HARLO_REAL_PASSWORD": server.password, "HARLO_REAL_PROJECT": str(c.project)}
    before = {s["id"] for s in c.run.call("GET", f"/api/session?{where(c.project)}").get("data", [])}
    ran = subprocess.run(["cargo", "test", "-q", "-p", "harlo-core", "--test", "real_ssh", "--",
                          "--ignored", "--nocapture"], cwd=TOOLS.parent, env=env, capture_output=True,
                         text=True, timeout=900)
    out = ran.stdout + ran.stderr
    for line in out.splitlines():
        if line.startswith("SSH") or "panicked" in line:
            c.say(line[:300])
    c.check("Harlo followed the server over SSH, started a session through the tunnel, got its live events "
            "and the model's answer", ran.returncode == 0, out[-900:])
    c.check("the file the model wrote for that session is in the project",
            (c.project / "tunnel.txt").exists() and (c.project / "tunnel.txt").read_text().strip() == "through")
    made = [s["id"] for s in c.run.call("GET", f"/api/session?{where(c.project)}").get("data", [])
            if s["id"] not in before]
    c.check("the session it started is on the server, as for any client", len(made) == 1, str(made))


SEARCH_CHOICE = "websearch:provider"


def forget_search_choice(run):
    """Takes the kept answer about web search out of the stopped scratch server's store."""
    store = sqlite3.connect(str(run.plan / "data/harlo/server.db"), timeout=10)
    try:
        store.execute("DELETE FROM setting WHERE key = ?", (SEARCH_CHOICE,))
        store.commit()
    finally:
        store.close()


def searchform(c):
    if c.run.setting(SEARCH_CHOICE) is not None:
        c.check("the scratch server keeps no answer about web search yet", None, c.run.setting(SEARCH_CHOICE))
        return
    c.run.after.append(lambda: forget_search_choice(c.run))
    forms, picks = [], {}

    def answer():
        for found in c.run.call("GET", f"/api/session/{c.session}/form")["data"]:
            forms.append(found)
            field = found["fields"][0]
            c.run.call("POST", f"/api/session/{c.session}/form/{found['id']}/reply",
                       {"answer": {field["key"]: picks[field["key"]]}})

    def search(text):
        before = c.messages()
        c.send(text)
        return c.wait(sum(1 for m in before if m.get("type") == "idle"), meanwhile=answer)[len(before):]

    ask = ("Search the web for the name of the current stable Python release and tell me the version number "
           "with one link. If web search can't be used, say exactly why in one line and don't answer from memory.")
    picks.update(choice="choose", provider="exa")
    new = search(ask)
    c.say(f"forms: {json.dumps([(f.get('title'), [o['value'] for o in f['fields'][0]['options']]) for f in forms])}")
    c.say(f"said: {said(new)[:300]}")
    c.check("the first search asked the person whether to search, then which service",
            [f.get("title") for f in forms] == ["Web Search", "Choose a web search provider"]
            and [o["value"] for o in forms[0]["fields"][0]["options"]] == ["allow", "choose", "disable"],
            json.dumps(forms)[:400])
    c.check("the search then ran on the chosen service and the answer names a link",
            any(status(p) == "completed" for p in tool_parts(new, "websearch")) and "http" in said(new),
            f"{[(p.get('name'), status(p)) for p in tool_parts(new)]}; {said(new)[:200]}")
    c.check("the answer is kept", c.run.setting(SEARCH_CHOICE) == json.dumps("exa"), str(c.run.setting(SEARCH_CHOICE)))

    # Another project: no form, the same service.
    first, seen = c.session, len(forms)
    c.project = c.run.project({"notes.txt": "Nothing yet.\n"})
    c.session = c.run.session(c.project)["id"]
    new = search(ask)
    c.check("a session of another project searches without being asked again",
            len(forms) == seen and any(status(p) == "completed" for p in tool_parts(new, "websearch")),
            f"forms: {len(forms) - seen}; "
            f"{[(p.get('name'), status(p), json.dumps(p.get('state', {}).get('error'))[:300]) for p in tool_parts(new)]}")

    # The answer forgotten (with the server stopped), the person turns web search off.
    c.run.server.close()
    forget_search_choice(c.run)
    c.run.start()
    c.configure()
    c.session, seen = first, len(forms)
    picks.update(choice="disable")
    new = search(ask)
    c.say(f"said: {said(new)[:300]}")
    c.check("asked again after the answer was forgotten, and told no, the search doesn't run",
            len(forms) == seen + 1 and not any(status(p) == "completed" for p in tool_parts(new, "websearch"))
            and c.run.setting(SEARCH_CHOICE) == "false",
            f"forms: {len(forms) - seen}; kept: {c.run.setting(SEARCH_CHOICE)}; "
            f"{[(p.get('name'), status(p)) for p in tool_parts(new)]}")
    c.check("and the agent says web search is off instead of answering from memory",
            "off" in said(new).lower() or "disabled" in said(new).lower(), said(new)[:300])
    seen = len(forms)
    new = search("Try the web search once more for the same thing. If you have no web search tool, say so in "
                 "one line.")
    c.check("after that the agent has no search tool, and nobody is asked",
            len(forms) == seen and not tool_parts(new, "websearch"),
            f"forms: {len(forms) - seen}; {[(p.get('name'), status(p)) for p in tool_parts(new)]}; {said(new)[:200]}")


LEDGER = {"name": "ledger", "description": "Keeps how each turn ended.", "version": "0.1.0",
          "tools": [{"name": "turns", "description": "Lists how the last turns ended, oldest first.",
                     "parameters": {"type": "object", "properties": {}}}],
          "events": ["turn.ended"]}

LEDGER_JS = """harlo.tool("turns", async () => (await harlo.storage.get("turns")) || []);
harlo.on("turn.ended", async (event, context) => {
  const turns = (await harlo.storage.get("turns")) || [];
  turns.push({ outcome: event.outcome, session: event.session.id, folder: context.session.directory });
  await harlo.storage.set("turns", turns.slice(-20));
});
"""


def events(c):
    repo = c.project.parent / f"{c.project.name}-ledger"
    repo.mkdir()
    c.run.extra.append(repo)
    git = ["git", "-c", "user.name=real", "-c", "user.email=real@example.invalid"]
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True)

    def publish(code):
        (repo / "plugin.json").write_text(json.dumps(LEDGER))
        (repo / "plugin.js").write_text(code)
        subprocess.run(["git", "add", "-A"], cwd=repo, check=True)
        subprocess.run([*git, "commit", "-qm", "a change"], cwd=repo, check=True)

    def shown():
        listed = c.run.call("GET", "/harlo/plugins").get("data", [])
        return next((p for p in listed if p.get("name") == "ledger"), {})

    publish(LEDGER_JS)
    try:
        added = c.run.call("POST", "/harlo/plugins", {"src": str(repo)})["data"]
        c.say(f"added: state {added.get('state')}, asks {added.get('asks')}")
        c.check("a plugin that runs when turns end says so before it is allowed",
                added.get("state") == "waiting" and "turn ends" in json.dumps(added.get("asks")),
                json.dumps(added)[:300])
        c.run.call("POST", "/harlo/plugins/ledger/approve")
        c.turn("Reply with the single word ready.")
        # A turn stopped by the person ends too.
        before = c.messages()
        c.send("Run `sleep 40; echo slept` and then tell me it finished.")
        until(lambda: [p for p in tool_parts(c.messages()[len(before):], "shell") if status(p) == "running"],
              limit=90, every=0.3)
        c.run.call("POST", f"/api/session/{c.session}/interrupt")
        c.wait(sum(1 for m in before if m.get("type") == "idle"), limit=30)
        time.sleep(2)
        new = c.turn("Use the ledger plugin's turns tool. Tell me how many turns it lists and each one's outcome, "
                     "in order, exactly as the tool gave them.")
        results = [json.dumps(p.get("state", {}).get("content"))[:600] for p in tool_parts(new, "execute")]
        c.say(f"the tool returned: {results}")
        c.say(f"said: {said(new)[:300]}")
        together = " ".join(results)
        c.check("the plugin's code ran when each turn ended, with how it ended and for which session",
                "succeeded" in together and "interrupted" in together and c.session in together
                and str(c.project) in together, together[:400])
        c.check("and the agent read that through the plugin's tool",
                "succeeded" in said(new).lower() and "interrupted" in said(new).lower(), said(new)[:300])
        c.check("nothing is wrong on the plugin's page", shown().get("eventError") is None,
                json.dumps(shown())[:300])

        publish(LEDGER_JS.replace("const turns", 'throw new Error("the ledger is full"); const turns'))
        updated = c.run.call("POST", "/harlo/plugins/ledger/update")["data"]
        c.turn("Reply with the single word again.")
        why = until(lambda: shown().get("eventError"), limit=20)
        c.check("when its code fails at a turn's end, the plugin's page says why and the turn is unharmed",
                updated.get("state") == "active" and "the ledger is full" in json.dumps(why), json.dumps(why)[:300])
        publish(LEDGER_JS)
        c.run.call("POST", "/harlo/plugins/ledger/update")
        c.turn("Reply with the single word fine.")
        cleared = until(lambda: shown().get("eventError") is None, limit=20)
        c.check("and no longer once it runs through again", bool(cleared), json.dumps(shown())[:300])
    finally:
        c.run.call("DELETE", "/harlo/plugins/ledger")
        left = c.run.call("GET", "/harlo/plugins").get("data", [])
        c.check("the plugin is removed from the scratch server again",
                not any(p.get("name") == "ledger" for p in left), json.dumps(left)[:200])


def automations_of(c):
    return [a for a in c.run.call("GET", "/api/experimental/automation")["data"]
            if a.get("sessionID") == c.session]


def remove_automations(c):
    for left in automations_of(c):
        c.run.call("DELETE", f"/api/experimental/automation/{left['id']}")


def lines_of(c, name):
    path = c.project / name
    return path.read_text().splitlines() if path.exists() else []


def cron(c):
    c.configure(extra={"automations": True})
    try:
        new = c.turn("Schedule a repeating automation with the cron schedule `* * * * *` (every minute). Its "
                     "prompt: \"Append the line cron-beat to the file cron.txt in this project and reply with "
                     "the single word done.\" Tell me its name and when it next runs.")
        made = automations_of(c)
        c.say(f"automations: {json.dumps(made)[:500]}")
        c.say(f"said: {said(new)[:300]}")
        c.check("the agent made a cron schedule, due on the minute",
                len(made) == 1 and made[0]["trigger"]["type"] == "cron"
                and made[0]["trigger"].get("cron") == "* * * * *" and made[0].get("nextDueAt", 1) % 60000 == 0
                and bool(made[0].get("tz") or made[0]["trigger"].get("tz")), json.dumps(made)[:300])
        due = made[0]["nextDueAt"] / 1000 if made else 0
        c.wait(c.idles(), limit=150)
        fired = automations_of(c)
        late = fired[0].get("lastRunAt", 0) / 1000 - due if fired else -1
        c.check("it fired at that minute and the agent did what the prompt said",
                lines_of(c, "cron.txt") == ["cron-beat"] and 0 <= late < 5,
                f"cron.txt: {lines_of(c, 'cron.txt')}; {late:.1f} s after it was due")
        new = c.turn("Pause that automation now; don't remove it.")
        paused = automations_of(c)
        beats = len(lines_of(c, "cron.txt"))
        c.check("the agent paused it", len(paused) == 1 and paused[0]["status"] == "paused",
                f"{json.dumps([(a['name'], a['status']) for a in paused])}; {said(new)[:200]}")
        time.sleep(70)
        c.check("and a paused schedule doesn't fire", len(lines_of(c, "cron.txt")) == beats
                and not c.run.call("GET", "/api/session/active")["data"].get(c.session),
                f"cron.txt: {lines_of(c, 'cron.txt')}")
    finally:
        remove_automations(c)


def schedulecmd(c):
    c.configure(extra={"automations": True})
    outside = c.project.parent / f"{c.project.name}-outside.txt"
    c.run.extra.append(outside)
    try:
        before = len(c.messages())
        new = c.turn("Schedule two one-off shell-command automations (commands the server runs, not prompts). "
                     f"The first, 15 seconds from now, runs exactly: `echo in > inside.txt; echo out > "
                     f"../{outside.name}`. The second, 20 seconds from now, runs exactly: `ls /no-such-folder`. "
                     "Don't run the commands yourself. Tell me their names.")
        made = automations_of(c)
        c.say(f"asks: {[(a.get('action'), d) for a, d in c.asks]}")
        c.check("the agent scheduled two commands", len(made) == 2 and all(a["action"]["type"] == "shell"
                                                                           for a in made), said(new)[:300])
        ended = until(lambda: len(automations_of(c)) == 2 and all(a.get("lastOutcome") for a in automations_of(c))
                      and automations_of(c), limit=90) or []
        for a in ended:
            c.say(f"{a['action']['command']!r}: {a.get('lastOutcome')}, {a.get('status')}: "
                  f"{json.dumps(a.get('lastOutput'))[:200]}")
        wrote = next((a for a in ended if "inside.txt" in a["action"]["command"]), {})
        failed = next((a for a in ended if "no-such-folder" in a["action"]["command"]), {})
        c.check("the first ran in the project", lines_of(c, "inside.txt") == ["in"], str(lines_of(c, "inside.txt")))
        c.check("in the sandbox: what it tried to write beside the project wasn't written",
                not outside.exists() and "Permission denied" in str(wrote.get("lastOutput")),
                f"outside: {outside.exists()}; {wrote.get('lastOutput')}")
        c.check("the second is kept as failed, with what it printed and its exit code",
                failed.get("lastOutcome") == "failed" and "Exit code: 2" in str(failed.get("lastOutput"))
                and "no-such-folder" in str(failed.get("lastOutput")), json.dumps(failed)[:300])
        time.sleep(3)
        c.check("a scheduled command sends the session nothing", len(c.messages()) == before + len(new)
                and not c.run.call("GET", "/api/session/active")["data"].get(c.session))
        remove_automations(c)

        # Where a command asks first, there is nobody to ask later: none is made.
        c.configure(asking=("shell",), extra={"automations": True})
        new = c.turn("Schedule one more shell-command automation, 15 seconds from now, running `echo late > "
                     "late.txt`. If it can't be made, tell me exactly why in one line.")
        c.say(f"said: {said(new)[:300]}")
        time.sleep(20)
        c.check("with commands set to ask first, the agent can't schedule one, and says why",
                not [a for a in automations_of(c) if a["action"]["type"] == "shell"]
                and not (c.project / "late.txt").exists()
                and any(word in said(new).lower() for word in ("allow", "approv", "ask")),
                f"{json.dumps(automations_of(c))[:200]}; {said(new)[:200]}")
    finally:
        remove_automations(c)


def schedulescript(c):
    c.configure(extra={"automations": True})
    try:
        new = c.turn("Schedule a script automation (a script the server runs, not a prompt) that runs every 1 "
                     "minute. The script keeps a count of its runs between runs. On its second run it sends this "
                     "session the prompt \"Reply with the single word counted.\" and then the automation "
                     "finishes, so there is no third run. Tell me its name.")
        made = automations_of(c)
        c.say(f"asks: {[(a.get('action'), d) for a, d in c.asks]}")
        c.say(f"the script: {json.dumps([a['action'] for a in made])[:900]}")
        c.say(f"said: {said(new)[:300]}")
        c.check("the agent scheduled a script", len(made) == 1 and made[0]["action"]["type"] == "code"
                and made[0]["trigger"]["type"] == "every", json.dumps(made)[:300])
        before = len(c.messages())
        ended = until(lambda: [a for a in automations_of(c) if a.get("runCount", 0) >= 2 and a.get("lastOutcome")],
                      limit=170, every=1) or automations_of(c)
        for a in ended:
            c.say(f"after its runs: runCount {a.get('runCount')}, status {a.get('status')}, state "
                  f"{json.dumps(a.get('state'))[:200]}, last: {a.get('lastOutcome')}: "
                  f"{json.dumps(a.get('lastOutput'))[:300]}")
        c.check("it ran twice and kept its count between the runs",
                bool(ended) and ended[0].get("runCount") == 2 and "2" in json.dumps(ended[0].get("state")),
                json.dumps([(a.get("runCount"), a.get("state"), a.get("lastOutput")) for a in ended])[:400])
        c.check("then it finished", bool(ended) and ended[0].get("status") == "done",
                json.dumps([(a.get("status"), a.get("nextDueAt")) for a in ended]))
        got = until(lambda: "counted" in said(c.messages()[before:]).lower(), limit=90)
        c.check("and its prompt reached the session, which answered", bool(got), said(c.messages()[before:])[:200])
    finally:
        remove_automations(c)


def scheduleoutside(c):
    free = c.project.parent / f"{c.project.name}-free.txt"
    held = c.project.parent / f"{c.project.name}-held.txt"
    c.run.extra += [free, held]
    leaves, stays = f"echo out > ../{free.name}", f"echo out > ../{held.name}"
    # A command is decided part by part: what it runs, and each file it writes through a redirection.
    # `echo` writing that one file may leave the sandbox without asking; nothing else may.
    c.configure(extra={"automations": True}, rules=[("outside_sandbox", "allow", "echo *"),
                                                    ("outside_sandbox", "allow", f"> ../{free.name}")])
    try:
        new = c.turn("Schedule two one-off shell-command automations (commands the server runs, not prompts), "
                     f"both 40 seconds from now. The first runs exactly: `{leaves}`. The second runs exactly: "
                     f"`{stays}`. Don't run the commands yourself and don't change them. Tell me their names.")
        made = automations_of(c)
        commands = sorted(a["action"].get("command", "") for a in made)
        c.say(f"asks: {[(a.get('action'), a.get('resources'), d) for a, d in c.asks]}")
        exact = commands == sorted([leaves, stays])
        if not exact:
            # Not what was asked for: nothing of the model's own wording runs outside the sandbox here.
            remove_automations(c)
        c.check("the agent scheduled the two commands as given", exact, json.dumps(commands)[:300])
        if not exact:
            return
        ended = until(lambda: all(a.get("lastOutcome") for a in automations_of(c)) and automations_of(c),
                      limit=120) or []
        for a in ended:
            c.say(f"{a['action']['command']!r}: {a.get('lastOutcome')}: {json.dumps(a.get('lastOutput'))[:200]}")
        out = next((a for a in ended if a["action"]["command"] == leaves), {})
        inside = next((a for a in ended if a["action"]["command"] == stays), {})
        c.check("the command the rules let out of the sandbox wrote beside the project",
                free.exists() and out.get("lastOutcome") == "fired", f"{free.exists()}; {json.dumps(out)[:200]}")
        c.check("the other one, scheduled the same way, stayed in the sandbox",
                not held.exists() and "Permission denied" in str(inside.get("lastOutput")),
                f"{held.exists()}; {json.dumps(inside)[:200]}")
        c.check("nobody was asked about leaving the sandbox",
                not any(a.get("action") == "outside_sandbox" for a, _ in c.asks))
    finally:
        remove_automations(c)


def eventtrigger(c):
    script = c.project.parent / f"{c.project.name}-units.py"
    script.write_text(UNITS.replace("TOKEN", "QUOKKA"))
    c.run.extra.append(script)
    c.configure(extra={"automations": True, "mcp": {"servers": {"units": {
        "type": "local", "command": ["python3", "-I", str(script)], "codemode": False}}}})
    until(lambda: any(s.get("name") == "units" and "connected" in json.dumps(s)
                      for s in c.run.call("GET", f"/api/mcp?{where(c.project)}").get("data", [])), limit=30)
    try:
        new = c.turn("Schedule an automation that fires whenever the units MCP server sends the event "
                     "`conversion.done`; its prompt: \"Reply with the single word converted.\" If that can't be "
                     "made, tell me in one or two lines exactly why, and don't make a different kind instead.")
        for part in tool_parts(new):
            state = part.get("state", {})
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(state.get('input'))[:260]} -> "
                  f"{json.dumps(state.get('content') or state.get('error'))[:320]}")
        c.say(f"said: {said(new)[:400]}")
        c.check("no schedule on an MCP server's event can be made", not automations_of(c),
                json.dumps(automations_of(c))[:300])
        told = said(new).lower()
        c.check("and the agent says why truthfully: the server doesn't act on such events, not that no MCP "
                "server is connected", "none is connected" not in told and "no mcp server" not in told
                and ("support" in told or "can't" in told or "cannot" in told or "isn't" in told), said(new)[:300])
    finally:
        remove_automations(c)


def asked_by(c, text):
    """The ID of the person's message `text` in the case's session."""
    return next(m["id"] for m in c.messages() if m.get("type") == "user" and m.get("text") == text)


def fork(c):
    c.turn("Keep this for the session: the release codename is HERON. Reply with the single word ok.")
    second = "Keep this too: the build number is 4471. Reply with the single word ok."
    c.turn(second)
    original, count = c.session, len(c.messages())
    forked = c.run.call("POST", f"/api/session/{original}/fork", {"before": asked_by(c, second)})["data"]
    c.say(f"fork: {json.dumps(forked)[:300]}")
    c.session = forked["id"]
    new = c.turn("What is the release codename, and is there a build number in this conversation? If there is "
                 "none, say none. One line.")
    c.check("a fork taken before a message knows what came before it and not what came after",
            "HERON" in said(new) and "4471" not in said(new), said(new)[:200])
    c.check("the session it was taken from is as it was", len(c.run.messages(original)) == count)
    c.session = forked["id"]
    new = c.turn("Once more: the release codename, in one word.")
    kept_fork = "HERON" in said(new)

    exported = c.run.call("GET", f"/api/experimental/session/{original}/export")["data"]
    elsewhere = c.run.project({"notes.txt": "Nothing yet.\n"})
    bring = {"info": exported["info"], "messages": exported["messages"], "location": {"directory": str(elsewhere)}}
    try:
        c.run.call("POST", "/api/experimental/session/import", bring)
        c.check("a session that is still here isn't imported over itself", False, "it was imported")
    except urllib.error.HTTPError as why:
        told = why.read().decode(errors="replace")
        c.say(f"importing a session that is still here: HTTP {why.code}: {told[:200]}")
        c.check("a session that is still here isn't imported over itself", why.code == 409, told[:200])
    # As on another computer: the session isn't here any more.
    c.run.call("DELETE", f"/api/session/{original}")
    brought = c.run.call("POST", "/api/experimental/session/import", bring)["data"]
    c.run.project_ids.add(brought.get("projectID", ""))
    c.say(f"exported {len(exported['messages'])} messages; imported as {brought['id']} in "
          f"{brought['location']['directory']}")
    c.session = brought["id"]
    new = c.turn("What are the release codename and the build number? One line.")
    c.check("an exported session, imported in another folder, carries its conversation",
            brought["location"]["directory"] == str(elsewhere)
            and "HERON" in said(new) and "4471" in said(new), said(new)[:200])

    c.check("the fork lives on after the session it came from is deleted",
            kept_fork and "HERON" in json.dumps(c.run.messages(forked["id"])))
    # The fork is moved to the other folder: its next turn works there.
    c.session = forked["id"]
    try:
        c.run.call("POST", f"/api/session/{c.session}/move", {"directory": str(elsewhere)})
        moved = c.run.call("GET", f"/api/session/{c.session}")["data"]["location"]["directory"]
    except urllib.error.HTTPError as why:
        c.check("a session is moved to another folder", False, f"HTTP {why.code}: {why.read()[:300]}")
        return
    new = c.turn("Create a file named here.txt in the current folder holding the single word moved, then tell "
                 "me the folder you are working in.")
    c.check("a moved session works in its new folder, with its conversation",
            moved == str(elsewhere) and (elsewhere / "here.txt").exists() and not (c.project / "here.txt").exists(),
            f"moved to {moved}; {said(new)[:200]}")
    new = c.turn("Once more, the release codename? One word.")
    c.check("and still knows what was said before the move", "HERON" in said(new), said(new)[:100])


def always(c):
    def saved():
        return [a for a in c.run.call("GET", "/api/permission/saved")["data"]
                if c.project_id in json.dumps(a)]

    c.configure(asking=("shell",))
    answer = {"shell": "always"}
    c.policy = lambda asked: (answer.get(asked.get("action"), "once"), None)
    tests = "Run `python3 -m unittest -q` and tell me the result in one line."
    try:
        c.turn(tests)
        first = [(a.get("action"), a.get("resources"), a.get("save"), d) for a, d in c.asks]
        c.say(f"asked: {first}")
        kept = saved()
        c.say(f"saved: {json.dumps(kept)[:400]}")
        c.check("answered \"always\", the command's approval is saved for the project",
                any(d == "always" for *_, d in first) and len(kept) >= 1, json.dumps(kept)[:300])
        answer["shell"] = "once"
        c.asks.clear()
        c.session = c.run.session(c.project)["id"]
        c.turn(tests)
        c.check("a new session of the project runs that command without asking",
                not any(a.get("action") == "shell" for a, _ in c.asks),
                str([(a.get("action"), a.get("resources")) for a, _ in c.asks]))
        c.turn("Run `git status --short` and tell me in one line whether the tree is clean.")
        c.check("another command still asks", any(a.get("action") == "shell" for a, _ in c.asks),
                str([(a.get("action"), a.get("resources")) for a, _ in c.asks]))
        for approval in saved():
            c.run.call("DELETE", f"/api/permission/saved/{approval['id']}")
        c.asks.clear()
        c.session = c.run.session(c.project)["id"]
        c.turn(tests)
        c.check("with the saved approval removed, the command asks again",
                not saved() and any(a.get("action") == "shell" for a, _ in c.asks),
                str([(a.get("action"), a.get("resources")) for a, _ in c.asks]))
    finally:
        for approval in saved():
            c.run.call("DELETE", f"/api/permission/saved/{approval['id']}")


def steps(messages):
    """The model and reasoning level of each of the agent's steps in `messages`."""
    return [(m.get("model", {}).get("id") if isinstance(m.get("model"), dict) else m.get("model"),
             m.get("variant") or (m.get("model", {}).get("variant") if isinstance(m.get("model"), dict) else None),
             (m.get("tokens") or {}).get("reasoning"))
            for m in messages if m.get("type") == "assistant"]


def model(c):
    provider, name = c.run.model.split("/", 1)
    other = "gpt-6.1-terra" if name != "gpt-6.1-terra" else "gpt-6.1-sol"
    c.turn("The word of the day is LANTERN. Reply with the single word ok.")
    c.run.call("POST", f"/api/session/{c.session}/model", {"model": {"id": other, "providerID": provider}})
    new = c.turn("What is the word of the day? One word.")
    c.say(f"steps after the change: {steps(new)}")
    c.check("after the model is changed, the next turn runs on the new model and knows the conversation",
            all(step[0] == other for step in steps(new)) and bool(steps(new)) and "LANTERN" in said(new),
            f"{steps(new)}; {said(new)[:100]}")
    shown = c.run.call("GET", f"/api/session/{c.session}")["data"]
    c.say(f"the session's model: {json.dumps(shown.get('model'))}")
    # The same question at two levels, each in a session that hasn't seen its answer.
    used = {}
    for level in ("low", "high"):
        c.session = c.run.session(c.project)["id"]
        c.turn("Reply with the single word ready.")
        c.run.call("POST", f"/api/session/{c.session}/model",
                   {"model": {"id": name, "providerID": provider, "variant": level}})
        new = c.turn("A farmer has 17 fields of 23 rows; each row loses 2 plants in 9 and started with 45. How "
                     "many plants are left in all? Reply with the number only.")
        used[level] = (steps(new), said(new)[:40])
        c.say(f"reasoning {level}: {steps(new)} -> {said(new)[:40]}")
    # 17 * 23 * 35 plants are left.
    c.check("the reasoning level is changed for the next turn and recorded on its steps",
            all(step[1] == level for level, (taken, _) in used.items() for step in taken)
            and all("13685" in answer.replace(",", "") for _, answer in used.values()), json.dumps(used)[:400])
    c.check("the higher level thought more", (used["high"][0][0][2] or 0) > (used["low"][0][0][2] or 0),
            json.dumps(used)[:300])


OWL = """---
description: Reviews code and changes nothing
mode: all
permissions:
  - action: edit
    resource: "*"
    effect: deny
---
You review code. You never change files. End every reply with a last line that reads exactly REVIEWED-BY-OWL.
"""

SHOUT = """---
description: Says the words back in capitals
---
Reply with exactly these words in upper case and nothing else: $ARGUMENTS
"""


def custom(c):
    (c.project / ".harlo/agents").mkdir(parents=True)
    (c.project / ".harlo/commands/team").mkdir(parents=True)
    (c.project / ".harlo/agents/owl.md").write_text(OWL)
    (c.project / ".harlo/commands/team/shout.md").write_text(SHOUT)
    agents = [a.get("id") for a in c.run.call("GET", f"/api/agent?{where(c.project)}")["data"]]
    commands = [k.get("name") for k in c.run.call("GET", f"/api/command?{where(c.project)}")["data"]]
    c.say(f"agents: {agents}; commands: {commands}")
    c.check("the project's own agent and command are listed", "owl" in agents and "team/shout" in commands,
            f"{agents}; {commands}")
    before = (c.project / "stats.py").read_text()
    c.run.call("POST", f"/api/session/{c.session}/agent", {"agent": "owl"})
    new = c.turn("Add a one-line comment above mean() in stats.py saying what it returns. If you can't, say why "
                 "in one line.")
    c.say(f"asks: {[(a.get('action'), d) for a, d in c.asks]}")
    c.say(f"the agent said: {said(new)[-300:]}")
    c.check("the session's turn ran as that agent: its instructions were followed",
            said(new).strip().endswith("REVIEWED-BY-OWL"), said(new)[-200:])
    c.check("and its rules held: the file wasn't changed", (c.project / "stats.py").read_text() == before)
    c.run.call("POST", f"/api/session/{c.session}/agent", {"agent": "build"})
    before = c.messages()
    c.run.call("POST", f"/api/session/{c.session}/command",
               {"name": "team/shout", "text": "quiet harbour", "delivery": "queue"})
    new = c.wait(sum(1 for m in before if m.get("type") == "idle"))[len(before):]
    ran_as = [m.get("agent") for m in new if m.get("type") == "assistant"]
    c.say(f"the command's turn ran as {ran_as} and said: {said(new)[:120]!r}")
    c.check("the project's command became its prompt, with what was typed after it, as the agent set again",
            "QUIET HARBOUR" in said(new) and ran_as and set(ran_as) == {"build"}, f"{ran_as}; {said(new)[:200]}")
    # Told that the agent's instructions ended, the model drops the ending they asked for.
    c.check("and the agent left behind no longer shapes the reply", "REVIEWED-BY-OWL" not in said(new),
            said(new)[:200])


def switches(c):
    # A todo list.
    c.configure(extra={"experimental": {"todo": True}})
    new = c.turn("Do these three things and keep a todo list while you work: add median(xs) to stats.py, add "
                 "spread(xs) (max minus min) to stats.py, add a test for each to test_stats.py.")
    lists = tool_parts(new, "todowrite")
    last = (lists[-1].get("state", {}).get("input", {}).get("todos") if lists else None) or []
    c.say(f"todo lists written: {len(lists)}; the last: {json.dumps(last)[:400]}")
    c.check("with the todo switch on, the agent kept a todo list and finished it",
            len(lists) >= 2 and len(last) >= 3 and all(t.get("status") == "completed" for t in last),
            json.dumps(last)[:300])
    passed, line = c.run.tests_pass(c.project)
    c.check("and did the work", passed and "spread" in (c.project / "stats.py").read_text(), str(line))
    c.configure()
    c.session = c.run.session(c.project)["id"]
    new = c.turn("Add a one-line comment at the top of test_stats.py saying what it tests. Keep a todo list if "
                 "you have a tool for one.")
    c.check("with the switch off there is no todo tool", not tool_parts(new, "todowrite"),
            str([p.get("name") for p in tool_parts(new)]))

    # Outline reads.
    body = "\n\n".join(f"def helper_{i}(x):\n    \"\"\"Helper number {i}.\"\"\"\n" +
                         "\n".join(f"    x = x + {j}" for j in range(12)) + "\n    return x"
                         for i in range(60))
    (c.project / "big.py").write_text(body + "\n\n\ndef zebra_crossing(x):\n    return x\n")
    ask = ("Which function does big.py declare last? Don't run any command and don't search: use the read "
           "tool, in whatever way costs the least. One word.")
    c.session = c.run.session(c.project)["id"]
    new = c.turn(ask)
    reads = [p.get("state", {}).get("input", {}) for p in tool_parts(new, "read")]
    c.say(f"reads, outline on: {json.dumps(reads)[:300]}")
    c.check("with outlines on, the agent asked for what the file declares and found the answer",
            any(r.get("outline") for r in reads) and "zebra_crossing" in said(new), said(new)[:100])
    c.configure(extra={"experimental": {"outline": False}})
    c.session = c.run.session(c.project)["id"]
    new = c.turn(ask)
    reads = [p.get("state", {}).get("input", {}) for p in tool_parts(new, "read")]
    c.say(f"reads, outline off: {json.dumps(reads)[:300]}")
    c.check("with outlines off, reads have no outline and the answer is still found",
            not any(r.get("outline") for r in reads) and "zebra_crossing" in said(new), said(new)[:100])

    # The session's own tools from a script.
    c.configure(extra={"experimental": {"scriptTools": True}})
    c.session = c.run.session(c.project)["id"]
    new = c.turn("In one execute script, use tools.project to list the .py files of this project and count the "
                 "lines of each, and return the counts as a map from file name to lines. Then tell me the "
                 "count for big.py.")
    scripts = tool_parts(new, "execute")
    lines = len((c.project / "big.py").read_text().splitlines())
    c.say(f"scripts: {[json.dumps(p.get('state', {}).get('input'))[:300] for p in scripts]}")
    c.check("with script tools on, a script read the project through the session's own tools",
            any("tools.project" in json.dumps(p.get("state", {}).get("input")) and status(p) == "completed"
                for p in scripts) and any(str(n) in said(new).replace(",", "") for n in (lines, lines + 1)),
            f"big.py has {lines} lines; {said(new)[:200]}")

    # Cached instructions: the same two turns with the switch off and on.
    used = {}
    for on in (False, True):
        c.configure(extra={"experimental": {"cachedInstructions": on}})
        c.session = c.run.session(c.project)["id"]
        c.turn("Reply with the single word one.")
        new = c.turn("Reply with the single word two.")
        tokens = [m.get("tokens") for m in new if m.get("type") == "assistant"]
        used[on] = (tokens, said(new)[:20])
        c.say(f"cached instructions {'on' if on else 'off'}: second turn's tokens {json.dumps(tokens)[:300]}")
    c.check("with cached instructions on, turns run as before", "two" in used[True][1].lower(), str(used[True]))
    read = {on: sum((t or {}).get("cache", {}).get("read", 0) for t in tokens) for on, (tokens, _) in used.items()}
    c.say(f"tokens read from the provider's cache on the second turn: off {read[False]}, on {read[True]}")


def bang(c):
    before = len(c.messages())
    c.run.call("POST", f"/api/session/{c.session}/shell",
               {"command": "echo tide-is-high > tide.txt; cat tide.txt; echo beside > ../bang-outside.txt"})
    ran = until(lambda: [m for m in c.messages()[before:] if m.get("type") == "shell" and m.get("status") != "running"],
                limit=30) or []
    c.say(f"the command's record: {json.dumps(ran)[:500]}")
    outside = c.project.parent / "bang-outside.txt"
    c.run.extra.append(outside)
    c.check("a command the person runs in the session is run and recorded",
            bool(ran) and "tide-is-high" in json.dumps(ran) and (c.project / "tide.txt").exists(),
            json.dumps(c.messages()[before:])[:300])
    c.say(f"it wrote beside the project: {outside.exists()} (the person's own command; where it runs is noted)")
    new = c.turn("What did the command I just ran myself print? Answer with its output only; don't run it again.")
    c.check("the agent knows what it printed without running anything",
            "tide-is-high" in said(new) and not tool_parts(new, "shell"), said(new)[:200])


def queue(c):
    def inbox():
        return c.run.call("GET", f"/api/session/{c.session}/inbox")["data"]

    before, idles = c.messages(), c.idles()
    c.send("Run `sleep 25; echo first-done` and then reply with the single word first.")
    until(lambda: [p for p in tool_parts(c.messages()[len(before):], "shell") if status(p) == "running"],
          limit=90, every=0.3)
    for word in ("second", "dropped", "third"):
        c.send(f"After that, also append the line {word} to the file queue.txt.")
    waiting = inbox()
    c.say(f"waiting: {json.dumps(waiting)[:500]}")
    texts_waiting = [w.get("text") or json.dumps(w) for w in waiting]
    c.check("messages sent during a run wait in line, in the order sent",
            len(waiting) == 3 and [("second" in t, "dropped" in t, "third" in t) for t in texts_waiting]
            == [(True, False, False), (False, True, False), (False, False, True)], json.dumps(waiting)[:300])
    drop = next((w for w in waiting if "dropped" in json.dumps(w)), None)
    if drop:
        c.run.call("DELETE", f"/api/session/{c.session}/inbox/{drop['id']}")
    c.check("one of them is taken back", len(inbox()) == 2, json.dumps(inbox())[:300])
    until(lambda: not inbox() and not c.run.call("GET", "/api/session/active")["data"].get(c.session)
          and c.idles() > idles, limit=240, every=1)
    c.answer_asks()
    new = c.messages()[len(before):]
    people = [texts(m) for m in new if m.get("type") == "user"]
    c.say(f"runs: {sum(1 for m in new if m.get('type') == 'idle')}; the person's messages in the session: "
          f"{[p[-40:] for p in people]}; queue.txt: {lines_of(c, 'queue.txt')}")
    c.check("after the run the waiting messages join the session in order, without the one taken back",
            len(people) == 3 and "second" in people[1] and "third" in people[2], str(people))
    c.check("and the agent did what both asked, in order", lines_of(c, "queue.txt") == ["second", "third"],
            str(lines_of(c, "queue.txt")))


def everywhere(c):
    c.turn("We have settled this: the deploy region for this product is eu-north-7. Reply with the single word ok.")
    home, home_session = c.project, c.session
    other = c.run.project({"README.md": "Another project.\n"})
    made = c.run.session(other)
    c.project, c.session = other, made["id"]
    new = c.turn("In a session of another project of mine we settled the deploy region. Search all my sessions, "
                 "not only this project's, and tell me the region.")
    searches = [p.get("state", {}).get("input") for p in tool_parts(new, "search_sessions")]
    asked = [a.get("action") for a, _ in c.asks]
    c.say(f"searches: {searches}; asked: {asked}")
    c.check("a search over every project's sessions asks first", "search_all_sessions" in asked, str(asked))
    c.check("and, allowed, finds what another project's session settled",
            any(s and s.get("everywhere") for s in searches) and "eu-north-7" in said(new), said(new)[:200])

    # A note about the person, for every project.
    allowed = [(action, "allow") for action in ("remember",)]
    c.configure(rules=allowed)
    c.run.call("PUT", "/harlo/memory", {"enabled": True})
    try:
        c.asks.clear()
        new = c.turn("Keep a note about me, for all my projects and not only this one: I want British spelling "
                     "in everything written for me.")
        asked = [a.get("action") for a, _ in c.asks]
        notes = c.run.call("GET", f"/harlo/notes?projectID={made['projectID']}")["data"]
        c.say(f"asked: {asked}; notes: {json.dumps(notes)[:400]}")
        c.check("a note about the person asks first", "remember_user" in asked, str(asked))
        c.check("and is kept as a note about the person, not the project",
                any(n.get("scope") == "user" and "british" in json.dumps(n).lower() for n in notes),
                json.dumps(notes)[:300])
        c.project = home
        c.session = c.run.session(home)["id"]
        new = c.turn("Write me one sentence about the colour of autumn leaves, in my preferred spelling. If you "
                     "know of no preference, start the sentence with UNKNOWN.")
        c.check("a session of another project writes with it", "colour" in said(new).lower()
                and "UNKNOWN" not in said(new), said(new)[:200])
    finally:
        for project_id in list(c.run.project_ids):
            for note in c.run.call("GET", f"/harlo/notes?projectID={project_id}")["data"]:
                c.run.call("DELETE", f"/harlo/notes/{note['id']}")
        c.run.call("PUT", "/harlo/memory", {"enabled": False})
        c.project, c.session = home, home_session


def commit(c):
    c.turn("Add median(xs) to stats.py and a test for it in test_stats.py. Don't commit.")
    changed = c.run.call("GET", f"/api/vcs/diff?{where(c.project)}")["data"]
    in_session = c.run.call("GET", f"/api/session/{c.session}/diff")["data"]
    c.say(f"changed: {json.dumps(changed)[:500]}")
    def named(files):
        return sorted(f.get("file") or f.get("path") or "" for f in files)

    wrote_files = [f for f in changed if (f.get("file") or f.get("path")) in ("stats.py", "test_stats.py")]
    c.say(f"the folder's changes: {named(changed)}; the session's: {named(in_session)}")
    c.check("the folder's changes list the two files with their diffs",
            len(wrote_files) == 2 and all("median" in json.dumps(f) for f in wrote_files), str(named(changed)))
    c.check("the session's own changes are the files its tools wrote, nothing a command left",
            named(in_session) == ["stats.py", "test_stats.py"], str(named(in_session)))
    changed = wrote_files
    # The app's own question, read from its source, before each file's diff.
    source = (TOOLS.parent / "crates/harlo/src/app/commit_message.rs").read_text()
    literal = source.split("const ASK: &str = ", 1)[1].split(";\n", 1)[0]
    ask = "".join(part.strip(' "') + " " for part in literal.replace("\\\n", "\n").splitlines()).strip()
    prompt = ask + "\n" + "".join(
        f"\n--- {f.get('file') or f.get('path')} (+{f.get('additions', 0)} -{f.get('deletions', 0)})\n"
        f"{f.get('patch', '')}\n" for f in changed)
    provider, name = c.run.model.split("/", 1)
    try:
        wrote = c.run.call("POST", "/api/experimental/generate/text",
                           {"prompt": prompt, "model": {"id": name, "providerID": provider}})
    except urllib.error.HTTPError as why:
        c.check("a commit message is written from the diff", False, f"HTTP {why.code}: {why.read()[:400]}")
        return
    message = (wrote.get("data") or {}).get("text") if isinstance(wrote.get("data"), dict) else wrote.get("data")
    message = (message or "").strip()
    c.say(f"asked: {ask[:120]}…")
    c.say(f"the message:\n{message}")
    first = message.splitlines()[0] if message else ""
    c.check("a commit message is written from the diff: a summary line within 72 characters about the change",
            0 < len(first) <= 72 and "median" in message.lower(), first)
    c.check("with no fences, quotes or commentary around it",
            not message.startswith(("`", '"', "'")) and "```" not in message and "here" not in first.lower(), first)
    c.check("and nothing joined the session", not any("commit message" in texts(m).lower() for m in c.messages()))


def free_port():
    import socket
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def fetch(url):
    """What a browser gets at `url` without following where it is sent: status, headers, text.

    Status 0 when nothing answers there.
    """
    class Stay(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *_):
            return None

    try:
        with urllib.request.build_opener(Stay).open(url, timeout=20) as got:
            return got.status, got.headers, got.read().decode(errors="replace")
    except urllib.error.HTTPError as got:
        return got.code, got.headers, got.read().decode(errors="replace")
    except OSError:
        return 0, {}, ""


def mcpremote(c):
    port, callback = free_port(), free_port()
    base = f"http://127.0.0.1:{port}"
    log = c.project.parent / f"{c.project.name}-remote.log"
    log.write_text("")
    c.run.extra.append(log)
    stand_in = subprocess.Popen([sys.executable, "-I", str(HERE / "remote-mcp.py"), str(port), str(log)])
    c.run.after.append(stand_in.kill)
    until(lambda: fetch(f"{base}/.well-known/oauth-authorization-server")[0] == 200, limit=20, every=0.2)

    def seen():
        return [json.loads(line) for line in log.read_text().splitlines()]

    def servers():
        return {s.get("name"): s for s in c.run.call("GET", f"/api/mcp?{where(c.project)}").get("data", [])}

    def state(name):
        return json.dumps(servers().get(name, {}).get("status"))

    credential = None
    try:
        c.configure(extra={"mcp": {"servers": {
            "depot": {"type": "remote", "url": f"{base}/open/mcp"},
            # The same server with its tools listed one by one.
            "labels": {"type": "remote", "url": f"{base}/open/mcp", "codemode": False},
            "vault": {"type": "remote", "url": f"{base}/mcp", "oauth": {"callbackPort": callback}}}}})
        until(lambda: "connected" in state("depot") and "auth" in state("vault").lower(), limit=40)
        listed = servers()
        c.say(f"servers: {json.dumps(listed)[:500]}")
        c.check("a server over HTTP connects, and one behind a sign-in waits for it",
                "connected" in state("depot") and "auth" in state("vault").lower()
                and bool(listed.get("vault", {}).get("integrationID")), json.dumps(listed)[:300])

        new = c.turn("How many walnut boards and how many ash boards are in stock at the depot? Use the depot "
                     "server's tools, and tell me the two numbers.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        c.check("the agent called the HTTP server's tool and has its answers",
                "4173" in said(new).replace(",", "") and any(r.get("tool") == "stock_level" for r in seen()),
                said(new)[:200])
        new = c.turn("Which account is the vault signed in as? Use the vault server's tools. If you have no such "
                     "tool, reply NO-VAULT-TOOL.")
        c.check("before the sign-in the agent has no tool of the guarded server",
                "HX-58213" not in json.dumps(new) and not any(r.get("tool") == "whoami" for r in seen()),
                said(new)[:200])

        # The sign-in, as a browser does it.
        integration = listed["vault"]["integrationID"]
        info = c.run.call("GET", f"/api/integration/{integration}")["data"]
        method = next(m for m in info.get("methods", []) if m.get("type") == "oauth")
        attempt = c.run.call("POST", f"/api/integration/{integration}/connect/oauth", {"methodID": method["id"]})["data"]
        _, headers, _ = fetch(attempt["url"])
        back = headers.get("Location", "")
        _, _, page = fetch(back)
        settled = until(lambda: c.run.call(
            "GET", f"/api/integration/{integration}/connect/oauth/{attempt['attemptID']}")["data"].get("status")
            not in ("pending", None), limit=30)
        until(lambda: "connected" in state("vault"), limit=30)
        c.say(f"sign-in address: {attempt['url'][:90]}…; sent back to {back[:60]}…; page: {page[:80]!r}")
        registered = [r for r in seen() if r.get("path") == "/register"]
        c.check("the browser sign-in registers Harlo, comes back to it and connects the server",
                attempt["url"].startswith(f"{base}/authorize?") and back.startswith(f"http://127.0.0.1:{callback}/")
                and "connected" in page.lower() and "connected" in state("vault")
                and [r.get("client") for r in registered] == ["Harlo"],
                f"settled {settled}; {state('vault')}; registered {registered}")
        kept = c.run.call("GET", f"/api/integration/{integration}")["data"].get("connections", [])
        credential = next((k.get("id") for k in kept if k.get("type") == "credential"), None)
        c.say(f"kept: {json.dumps(kept)[:300]}")

        c.session = c.run.session(c.project)["id"]
        new = c.turn("Which account is the vault signed in as? Use the vault server's tools.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        c.check("after it, the agent calls the guarded server's tool, and the call carries the sign-in",
                "HX-58213" in said(new) and any(r.get("tool") == "whoami" and r.get("signed") for r in seen()),
                said(new)[:200])

        # What a server lists as resources.
        offered = c.run.call("GET", f"/api/mcp/resource?{where(c.project)}")["data"]
        c.say(f"resources: {json.dumps(offered)[:300]}")
        c.check("the server's resources are listed with the server they come from",
                [(r.get("uri"), r.get("server")) for r in offered.get("resources", []) if r.get("server") == "depot"]
                == [("depot://policy/reorder", "depot"), ("depot://label", "depot")]
                and "depot://bin/{number}" in [t.get("uriTemplate") for t in offered.get("templates", [])],
                json.dumps(offered)[:300])
        new = c.turn("The depot server offers a reorder policy as a resource. Below how many units is a board "
                     "reordered? If you have no way to read that resource, reply exactly CANNOT-READ-RESOURCE.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        reads = sum(1 for r in seen() if r.get("method") == "resources/read")
        c.check("the agent reads a resource the server offers, found from the tool's own description",
                "37" in said(new) and reads >= 1, said(new)[:200])
        new = c.turn("The labels server offers a shelf label as a picture resource. Read it with the labels "
                     "server's own tool, not from a script, look at it and tell me its colour in one word.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        c.check("a resource that is a picture is shown to the model, from a tool listed on its own",
                "red" in said(new).lower() and bool(tool_parts(new, "labels_read_resource")), said(new)[:200])
        c.session = c.run.session(c.project)["id"]
        new = c.turn("The depot server offers its shelf label as a picture resource. Read it from a script with "
                     "tools.depot and tell me its colour in one word; if you can't see the picture, say UNSEEN.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        c.check("a picture read from a script is shown to the model beside the script's result",
                "red" in said(new).lower() and bool(tool_parts(new, "execute"))
                and not tool_parts(new, "labels_read_resource"), said(new)[:200])
        new = c.turn("And by the depot's resource for a bin's contents: what does bin 12 hold? Give the number.")
        c.check("an address made from a resource template is read", "91" in said(new), said(new)[:200])
        reads = sum(1 for r in seen() if r.get("method") == "resources/read")
        # Mentioned in a message, as the composer's @ does, the resource comes with it.
        c.session = c.run.session(c.project)["id"]
        new = c.turn("On which weekday do orders go out, by the attached policy? One word. Don't call any tool.",
                     files=[{"uri": "depot://policy/reorder", "name": "Reorder policy"}])
        c.check("a resource mentioned in a message is read and sent with it",
                "thursday" in said(new).lower() and not tool_parts(new)
                and sum(1 for r in seen() if r.get("method") == "resources/read") > reads, said(new)[:200])
    finally:
        if credential:
            c.run.call("DELETE", f"/api/credential/{credential}")
        c.configure()
        stand_in.kill()


def mcpretry(c):
    port = free_port()
    base = f"http://127.0.0.1:{port}"
    log = c.project.parent / f"{c.project.name}-retry.log"
    log.write_text("")
    c.run.extra.append(log)

    def state():
        listed = c.run.call("GET", f"/api/mcp?{where(c.project)}").get("data", [])
        return next((s.get("status") or {} for s in listed if s.get("name") == "depot"), {})

    stand_in = None
    try:
        # Nothing listens there yet, as when the computer wakes before its network.
        c.configure(extra={"mcp": {"servers": {"depot": {"type": "remote", "url": f"{base}/open/mcp"}}}})
        until(lambda: state().get("status") == "failed", limit=40)
        c.say(f"with nothing there: {json.dumps(state())}")
        c.check("a server that can't be reached is failed, in plain words",
                state() == {"status": "failed", "error": f"Couldn't reach {base}/open/mcp."}, json.dumps(state()))
        stand_in = subprocess.Popen([sys.executable, "-I", str(HERE / "remote-mcp.py"), str(port), str(log)])
        c.run.after.append(stand_in.kill)
        until(lambda: fetch(f"{base}/.well-known/oauth-authorization-server")[0] == 200, limit=20, every=0.2)
        failed_at = time.monotonic()
        new = c.turn("Reply with the single word early.")
        c.check("a turn right after the failure doesn't try it yet", state().get("status") == "failed"
                and not log.read_text().strip(), f"{json.dumps(state())}; {log.read_text()[:200]}")
        time.sleep(max(0.0, 32 - (time.monotonic() - failed_at)))
        c.check("and nothing tries it while no turn starts", state().get("status") == "failed", json.dumps(state()))
        new = c.turn("How many cedar boards are in stock at the depot? Use the depot server's tools and tell me "
                     "the number. If you have no such tool, reply NO-DEPOT-TOOL.")
        for part in tool_parts(new):
            c.say(f"call {part.get('name')} ({status(part)}): {json.dumps(part.get('state', {}).get('input'))[:200]}")
        c.check("half a minute on, the next turn connects it by itself and the agent uses its tool",
                state().get("status") == "connected" and "268" in said(new), f"{json.dumps(state())}; {said(new)[:200]}")
    finally:
        c.configure()
        if stand_in:
            stand_in.kill()


def searchover(c):
    # Exa's free allowance is a few searches a minute: a burst of them runs into it.
    c.configure(extra={"websearch": {"provider": "exa"}})
    subjects = ("Rust", "Python", "Go", "Node.js", "PostgreSQL", "Ruby", "PHP", "Kotlin")
    handed, failed, done, new = [], [], 0, []
    for _ in range(3):
        new = c.turn("Search the web for the latest stable release of each of these, one search per item, all "
                     f"of the searches at once in a single step: {', '.join(subjects)}. Then list item and "
                     "version, one per line.")
        for part in tool_parts(new, "websearch"):
            done += 1
            output = json.dumps(part.get("state", {}))
            if "is limited for now; searched with" in output:
                handed.append(output[output.index("is limited for now") - 5:][:60])
            if status(part) != "completed":
                failed.append(output[:300])
        if handed or failed:
            break
    c.say(f"searches: {done}; handed on: {len(handed)} {handed[:3]}; failed: {failed[:2]}")
    c.check("no search failed for the chosen service being limited", not failed, str(failed)[:400])
    c.check("the chosen service, once limited, handed the search to another, and the result said so",
            True if handed else None, f"Exa answered all {done} searches: nothing to hand on")
    if handed:
        c.check("and the agent still answered every item",
                sum(1 for subject in subjects if subject.lower() in said(new).lower()) >= 6, said(new)[:300])


def usage(c):
    c.turn("Explain in one line what the mean function in stats.py returns for [2, 4].")
    read = c.run.call("GET", "/harlo/usage")["data"]
    provider = c.run.model.split("/")[0]
    mine = next((entry for entry in read if entry.get("providerID") == provider), None)
    c.check("the plan's provider is among the readings", mine is not None, json.dumps(read)[:300])
    if mine is None:
        return
    c.check("it is told as a plan, with the page that shows its limits",
            bool(mine.get("plan")) and str(mine.get("manage", "")).startswith("https://"), json.dumps(mine))
    c.check("no limit was reached by a turn that was answered", mine.get("reached") is False, json.dumps(mine))
    c.check("it claims no limit the provider doesn't tell", mine.get("limits") == [], json.dumps(mine))
    c.say(f"usage: {json.dumps(mine)}")


CASES = {"approvals": approvals, "question": question, "subagent": subagent, "compaction": compaction,
         "plan": plan, "stop": stop, "steer": steer, "undo": undo, "recall": recall, "skill": skill, "mcp": mcp,
         "webfetch": webfetch, "image": image, "aside": aside, "title": title, "schedule": schedule,
         "plugin": plugin, "background": background, "worktree": worktree, "websearch": websearch,
         "phone": phone, "searchform": searchform, "events": events, "cron": cron, "schedulecmd": schedulecmd,
         "schedulescript": schedulescript, "scheduleoutside": scheduleoutside, "eventtrigger": eventtrigger, "fork": fork, "always": always,
         "model": model, "custom": custom, "switches": switches, "bang": bang, "queue": queue,
         "everywhere": everywhere, "commit": commit, "mcpremote": mcpremote, "ssh": ssh, "mcpretry": mcpretry,
         "searchover": searchover, "usage": usage}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--plan", default=str(Path.home() / "scratch/harlo-plan"))
    parser.add_argument("--model", default="openai/gpt-6.1-sol")
    parser.add_argument("--only", help="comma-separated names of the cases to run")
    args = parser.parse_args()
    names = args.only.split(",") if args.only else list(CASES)
    unknown = [n for n in names if n not in CASES]
    if unknown:
        raise SystemExit(f"no such case: {', '.join(unknown)}; there are: {', '.join(CASES)}")
    if not BASE.BENCH.SERVER.exists():
        raise SystemExit(f"no harlo-server at {BASE.BENCH.SERVER}: cargo build -p harlo-server")
    if Path(args.plan).resolve().is_relative_to(TOOLS.parent):
        raise SystemExit("--plan DIR holds a sign-in: keep it outside the repository")

    began = time.monotonic()
    run = BASE.Run(args.plan, args.model)
    run.extra, run.after = [], []
    try:
        run.call("PUT", "/harlo/memory", {"enabled": False})
        print(f"model: {args.model}", flush=True)
        for name in names:
            run.say(f"\n{name}")
            try:
                CASES[name](Case(run, name))
            except (TimeoutError, urllib.error.URLError, KeyError, IndexError, StopIteration, OSError,
                    subprocess.SubprocessError) as why:
                run.check(f"{name}: ran to its end", False, f"{type(why).__name__}: {why}")
                traceback.print_exc()
    finally:
        BASE.clean_up(run)
        for after in run.after:
            after()
        for path in run.extra:
            shutil.rmtree(path) if path.is_dir() else path.unlink(missing_ok=True)

    failed = [name for name, ok, _ in run.checks if ok is False]
    skipped = [name for name, ok, _ in run.checks if ok is None]
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    REPORTS.mkdir(parents=True, exist_ok=True)
    report = REPORTS / f"features-{stamp}.md"
    lines = [f"# Harlo's features on {args.model}", "",
             f"{len(run.checks) - len(failed) - len(skipped)} ok, {len(failed)} failed, "
             f"{len(skipped)} not exercised, in {(time.monotonic() - began) / 60:.1f} min", ""]
    lines += [f"- {'ok' if ok else 'FAILED' if ok is False else 'not exercised'}: {name}"
              + (f" ({seen})" if seen and ok is not True else "") for name, ok, seen in run.checks]
    lines += ["", "## What was seen", "", "```", *run.notes, "```", ""]
    report.write_text("\n".join(lines))
    print(f"\n{lines[2]}\nreport: {report.relative_to(TOOLS.parent)}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
