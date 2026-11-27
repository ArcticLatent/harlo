#!/usr/bin/env python3
"""Goal recovery and suggested notes, tried on a real model.

    tools/real/memory-goals.py [--plan DIR] [--model openai/gpt-6.1-sol] [--only goals|notes]

It runs on the ChatGPT sign-in kept in the scratch folder DIR (made with
`tools/bench/cold-cache.py --plan DIR --sign-in`; never the one Harlo itself
holds), on a private server with scratch HOME and XDG folders, a random
password and a session bus that leads nowhere. The agent's commands run in
the server's sandbox with the real home folder hidden.

Goals. Two sessions, each in a project of its own. The first does something
unrelated (a file, a command), then both get a goal. The server is killed
while both work, the first goal is cut down to what an earlier Harlo stored
(no start mark, no checkpoint), and after the restart both are resumed.
Checked: both wait paused; neither is told the other's work or what its
session did before the goal; both end with their tests passing.

Notes. With Memory on, a person asks for a change and then corrects how it is
done. Checked: the review suggests notes whose source is a message of the
person's (or the agent kept them itself); nothing said while Memory was off
is suggested; a session driven by /goal alone is reviewed with the goal as
its source; and a new session knows a kept note.

Missed. The same correction with the agent not allowed to keep notes, and
the person going on at once. Checked: the review catches the correction; a
turn that ends during a review is reviewed without another message; two
decisions on one suggestion at once count once (one kept, one turned down);
and neither is suggested again, in the same or other words, when the
person repeats the correction.

What a real model says can't be scripted, so each check says what was seen
and "not exercised" when the model gave it nothing to check. The report goes
to target/real/. What the run left on the scratch server (notes, the Memory
switch, its projects) is removed at the end.
"""

import argparse
import datetime
import importlib.util
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
from pathlib import Path

TOOLS = Path(__file__).resolve().parent.parent
REPORTS = TOOLS.parent / "target/real"
STOPPED = "The server stopped while the agent worked on it."


def load_bench():
    """The benchmark's private server, reused as it is."""
    spec = importlib.util.spec_from_file_location("cold_cache", TOOLS / "bench/cold-cache.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BENCH = load_bench()

STATS = '''"""Small statistics helpers."""


def mean(xs):
    """The arithmetic mean of xs."""
    if not xs:
        raise ValueError("mean of an empty list")
    return sum(xs) / len(xs)
'''
STATS_TESTS = '''import unittest

import stats


class MeanTests(unittest.TestCase):
    def test_mean(self):
        self.assertEqual(stats.mean([1, 2, 3]), 2)

    def test_empty(self):
        with self.assertRaises(ValueError):
            stats.mean([])


if __name__ == "__main__":
    unittest.main()
'''
TEXT = '''"""Small text helpers."""


def shout(text):
    """text in capitals."""
    return text.upper()
'''
TEXT_TESTS = '''import unittest

import textutil


class ShoutTests(unittest.TestCase):
    def test_shout(self):
        self.assertEqual(textutil.shout("hi"), "HI")


if __name__ == "__main__":
    unittest.main()
'''


class Run:
    """One run: the server, what was checked, and what is left to remove."""

    def __init__(self, plan, model):
        self.plan = Path(plan)
        self.model = model
        self.server = None
        self.checks = []
        self.notes = []
        self.projects = []
        self.project_ids = set()
        self.start()

    def start(self):
        self.server = BENCH.Server(keep=self.plan)
        self.configure()

    def configure(self, keeps_notes=True, slow_review=False):
        """Writes the server's configuration; it's read again for every turn.

        Nothing asks: the project's files, commands in the sandbox with the
        real home hidden, and (unless `keeps_notes` is off) the agent's own
        notes. With `slow_review` the small jobs beside a session, the
        review for notes among them, think hard: a review then takes long
        enough for a short turn to end during it.
        """
        allowed = ["read", "edit", "glob", "grep", "shell", "search_sessions"]
        if keeps_notes:
            allowed.append("remember")
        self.server.configure({
            "model": self.model,
            **({"small_model": f"{self.model}#high"} if slow_review else {}),
            "permissions": [
                {"action": "*", "resource": "*", "effect": "deny"},
                *({"action": action, "resource": "*", "effect": "allow"} for action in allowed),
            ],
            "sandbox": {"hidden": [str(Path.home())]},
        })

    def kill(self):
        """Stops the server as a crash would."""
        os.killpg(self.server.server.pid, signal.SIGKILL)
        self.server.server.wait()

    def call(self, method, path, body=None):
        return self.server.call(method, path, body)

    def check(self, name, ok, seen=""):
        """Keeps one result: True, False, or None for not exercised."""
        word = {True: "ok", False: "FAILED", None: "not exercised"}[ok]
        self.checks.append((name, ok, seen))
        print(f"  {word:14} {name}" + (f"\n{' ' * 17}{seen}" if seen and ok is not True else ""), flush=True)

    def say(self, text):
        self.notes.append(text)
        print(text, flush=True)

    def project(self, files):
        project = Path(tempfile.mkdtemp(prefix="real", dir=self.plan / "projects"))
        for name, text in files.items():
            (project / name).write_text(text)
        git = ["git", "-c", "user.name=real", "-c", "user.email=real@example.invalid"]
        subprocess.run(["git", "init", "-q"], cwd=project, check=True)
        subprocess.run(["git", "add", "."], cwd=project, check=True)
        subprocess.run([*git, "commit", "-qm", "start"], cwd=project, check=True)
        self.projects.append(project)
        return project

    def session(self, project):
        made = self.call("POST", "/api/session", {"location": {"directory": str(project)}})["data"]
        self.project_ids.add(made["projectID"])
        return made

    def messages(self, session):
        return self.call("GET", f"/api/session/{session}/message")["data"]

    def refuse_asks(self, session):
        for asked in self.call("GET", f"/api/session/{session}/permission")["data"]:
            self.call("POST", f"/api/session/{session}/permission/{asked['id']}/reply",
                      {"decision": "reject", "message": "Not in this run: work inside the project."})

    def turn(self, session, text, limit=600):
        """Sends a message of the person's and waits until the session is idle."""
        idles = sum(1 for m in self.messages(session) if m.get("type") == "idle")
        self.call("POST", f"/api/session/{session}/prompt", {"text": text, "delivery": "queue"})
        return self.idle_after(session, idles, limit)

    def idle_after(self, session, idles, limit=600):
        """Waits until the session has gone idle more than `idles` times; its messages."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline:
            self.refuse_asks(session)
            now = self.messages(session)
            if sum(1 for m in now if m.get("type") == "idle") > idles:
                return now
            time.sleep(0.5)
        raise SystemExit(f"a turn didn't finish in {limit} s")

    def goal(self, session):
        return self.call("POST", "/api/rpc/goal/get", {"input": {"sessionID": session}})["output"]

    def command(self, session, text):
        self.call("POST", f"/api/session/{session}/command", {"name": "goal", "text": text, "delivery": "queue"})

    def goal_ends(self, session, limit=900):
        """Waits until the session's goal is no longer worked on; the goal."""
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline:
            self.refuse_asks(session)
            goal = self.goal(session)
            last = (self.messages(session) or [{}])[-1]
            if goal and goal.get("status") != "active" and last.get("type") == "idle":
                return goal
            time.sleep(0.5)
        raise SystemExit(f"a goal didn't end in {limit} s")

    def setting(self, key):
        """A setting of the scratch server, read from its store."""
        store = sqlite3.connect(f"file:{self.plan / 'data/harlo/server.db'}?mode=ro", uri=True, timeout=10)
        try:
            row = store.execute("SELECT value FROM setting WHERE key = ?", (key,)).fetchone()
        finally:
            store.close()
        return row[0] if row else None

    def tests_pass(self, project):
        """Runs the project's tests where they can change nothing else."""
        done = subprocess.run(
            ["bwrap", "--ro-bind", "/", "/", "--tmpfs", "/home", "--tmpfs", "/tmp", "--dev", "/dev",
             "--bind", str(project), str(project), "--unshare-net", "--chdir", str(project),
             sys.executable, "-I", "-m", "unittest", "-q"],
            capture_output=True, text=True, timeout=120)
        return done.returncode == 0, (done.stderr or done.stdout).strip().splitlines()[-1:]


def texts(message):
    """What a message says, as one string."""
    if message.get("type") in ("user", "synthetic"):
        return message.get("text", "")
    return "\n".join(p.get("text", "") for p in message.get("content", []) if p.get("type") == "text")


def calls(messages):
    """The tool calls of `messages`, as name and input."""
    return [(p.get("name"), p.get("state", {}).get("input", {})) for m in messages
            for p in m.get("content", []) if p.get("type") == "tool"]


def goals(run):
    run.say("\nGoals: two sessions, a crash while both work, one goal in an earlier Harlo's form")
    first = run.project({"stats.py": STATS, "test_stats.py": STATS_TESTS})
    second = run.project({"textutil.py": TEXT, "test_textutil.py": TEXT_TESTS})
    a, b = run.session(first)["id"], run.session(second)["id"]

    # Something that has nothing to do with the goal: a file and a command.
    before = run.turn(a, "Create NOTES.md with the single line `A small statistics helper.` Then run `ls` "
                         "and tell me the files you see.")
    did = calls(before)
    run.check("the session did unrelated work first (a file, a command)",
              (first / "NOTES.md").exists() and any(name == "shell" for name, _ in did),
              f"calls: {[name for name, _ in did]}")

    run.command(a, "stats.py has median(xs) and mode(xs); each raises ValueError on an empty list; "
                   "test_stats.py tests both; `python3 -m unittest -q` passes.")
    run.command(b, "textutil.py has slugify(text) (lower case, words joined by single hyphens, nothing but "
                   "letters, digits and hyphens) and truncate(text, width) (cut at a word boundary, ending in "
                   "an ellipsis when cut); test_textutil.py tests both; `python3 -m unittest -q` passes.")

    # Killed while both work: each has begun changing its file.
    deadline, midway = time.monotonic() + 600, False
    while time.monotonic() < deadline:
        begun = "def median" in (first / "stats.py").read_text() and "def slugify" in (second / "textutil.py").read_text()
        active = [run.goal(s).get("status") for s in (a, b)]
        if begun or "active" not in active:
            midway = begun and active == ["active", "active"]
            break
        time.sleep(0.2)
    run.kill()
    run.check("the server was killed while both goals were being worked on", midway or None,
              f"statuses at the kill: {active}")

    # The first goal as an earlier Harlo kept it.
    store = sqlite3.connect(run.plan / "data/harlo/server.db", timeout=10)
    kept = json.loads(store.execute("SELECT value FROM setting WHERE key = ?", (f"goal/{a}",)).fetchone()[0])
    had = sorted(key for key in ("from", "checkpoint") if key in kept)
    for key in ("from", "checkpoint"):
        kept.pop(key, None)
    store.execute("UPDATE setting SET value = ? WHERE key = ?", (json.dumps(kept), f"goal/{a}"))
    store.commit()
    store.close()
    run.say(f"  the first goal was cut down to the old form (it had: {', '.join(had) or 'neither'})")

    run.start()
    stopped = [run.goal(s) for s in (a, b)]
    run.check("after the restart both goals wait, paused, saying the server stopped",
              all(g.get("status") == "paused" and g.get("reason") == STOPPED for g in stopped),
              str([(g.get("status"), g.get("reason")) for g in stopped]))

    run.command(a, "resume")
    run.command(b, "resume")
    ended = [run.goal_ends(s) for s in (a, b)]
    told = []
    for session in (a, b):
        resumes = [m for m in run.messages(session) if m.get("type") == "synthetic"
                   and texts(m).startswith("Pick the goal up again where it stopped.")]
        told.append(texts(resumes[-1]) if resumes else "")
    run.say("  what the first (old-form) goal was told on resume:\n" + "\n".join("    " + l for l in told[0].splitlines()))
    run.say("  what the second goal was told on resume:\n" + "\n".join("    " + l for l in told[1].splitlines()))

    run.check("the old-form goal is told nothing of what its session did before the goal",
              bool(told[0]) and "NOTES.md" not in told[0] and "`ls" not in told[0])
    run.check("the old-form goal is told the work its own rounds did before the crash",
              "stats.py" in told[0] or None, "its resume prompt names no file of its own")
    run.check("neither goal is told the other session's work",
              bool(told[1]) and "textutil" not in told[0] and "slugify" not in told[0]
              and "stats.py" not in told[1] and "median" not in told[1] and "NOTES.md" not in told[1])
    for name, goal in (("first", ended[0]), ("second", ended[1])):
        run.check(f"the {name} goal ends complete, by its reviewer's word", goal.get("status") == "complete",
                  f"{goal.get('status')}: {goal.get('reason')}")
    completed = json.dumps(ended[0].get("checkpoint", {}))
    run.check("the old-form goal's checkpoint holds none of the earlier work",
              "NOTES.md" not in completed and "`ls" not in completed, completed[:300])
    run.say(f"  the first goal's last checkpoint: {completed[:700]}")
    for name, project in (("first", first), ("second", second)):
        passed, last = run.tests_pass(project)
        run.check(f"the {name} project's tests pass", passed, " ".join(last))
    run.check("the file made before the goal is still there", (first / "NOTES.md").exists())


def reviewed(run, session, since, limit=180):
    """Waits until the review has moved past `since` for `session`; where it stands."""
    deadline = time.monotonic() + limit
    while time.monotonic() < deadline:
        mark = run.setting(f"proposals/{session}")
        if mark and mark != since:
            return mark
        time.sleep(1)
    return None


def proposals(run, project_id, session=None):
    found = run.call("GET", f"/harlo/proposals?projectID={project_id}")["data"]
    return [p for p in found if session is None or p["source"]["session"] == session]


def at_once(run, first, second):
    """Sends two requests at the same moment; whether each was taken."""
    start, taken = threading.Barrier(2), [None, None]

    def send(index, request):
        start.wait()
        try:
            run.call(*request)
            taken[index] = True
        except urllib.error.HTTPError as why:
            taken[index] = False if why.code == 400 else f"HTTP {why.code}"

    threads = [threading.Thread(target=send, args=(i, r)) for i, r in enumerate((first, second))]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return taken


def notes(run):
    run.say("\nNotes: a correction the agent may not keep, Memory off and on, a goal-only session")
    project = run.project({"stats.py": STATS, "test_stats.py": STATS_TESTS})
    run.call("PUT", "/harlo/memory", {"enabled": True})
    made = run.session(project)
    a, project_id = made["id"], made["projectID"]

    run.turn(a, "Add median(xs) to stats.py with a test in test_stats.py, and run the tests.")
    correction = ("Two things about this project, please stick to them from now on: we never import the "
                  "statistics module here, these helpers are written by hand; and tests are run with "
                  "`python3 -m unittest -q`, never pytest. Redo median that way if it needs it.")
    history = run.turn(a, correction)
    corrected = next(m["id"] for m in history if m.get("type") == "user" and m.get("text") == correction)
    mark = reviewed(run, a, None)
    run.check("the stretch was reviewed by the model and its answer was a list", mark is not None,
              "no review finished in 3 minutes")
    kept_by_agent = run.call("GET", f"/harlo/notes?projectID={project_id}")["data"]
    run.say(f"  notes the agent kept itself with `remember`: {[n['text'] for n in kept_by_agent]}")
    suggested = proposals(run, project_id, a)
    for p in suggested:
        run.say(f"  suggested ({p['kind']}, {p['scope']}): {p['text']}\n    from: {p['excerpt'][:140]}")
    if suggested:
        run.check("each suggestion names a message of the person's in that session as its source",
                  all(p["source"]["message"] == corrected or
                      any(m["id"] == p["source"]["message"] and m.get("type") == "user" for m in history)
                      for p in suggested), str([p["source"] for p in suggested]))
        run.check("a suggestion comes from the correction itself",
                  any(p["source"]["message"] == corrected for p in suggested))
    else:
        run.check("the review suggested the correction (or the agent had kept it itself)",
                  True if kept_by_agent else None,
                  "the model suggested nothing and the agent kept nothing")

    # Said with Memory off, then Memory is on again and the person goes on.
    run.call("PUT", "/harlo/memory", {"enabled": False})
    secret = "For your information only: the staging host is tulip-staging-42.internal. Just reply OK."
    hidden = run.turn(a, secret)
    hidden_id = next(m["id"] for m in hidden if m.get("type") == "user" and m.get("text") == secret)
    run.call("PUT", "/harlo/memory", {"enabled": True})
    rule = ("One more rule for this project: every public function in stats.py has a one-line docstring. "
            "Add the ones that are missing and run the tests.")
    run.turn(a, rule)
    again = reviewed(run, a, mark)
    run.check("the stretch after Memory came back on was reviewed", again is not None)
    later = [p for p in proposals(run, project_id, a) if p["id"] not in {q["id"] for q in suggested}]
    for p in later:
        run.say(f"  suggested ({p['kind']}, {p['scope']}): {p['text']}\n    from: {p['excerpt'][:140]}")
    everything = json.dumps(proposals(run, project_id) + run.call("GET", f"/harlo/notes?projectID={project_id}")["data"])
    run.check("nothing said while Memory was off is suggested or kept",
              "tulip" not in everything.lower() and hidden_id not in everything)

    # Two decisions on one suggestion at once.
    pending = proposals(run, project_id, a)
    count = lambda: len(run.call("GET", f"/harlo/notes?projectID={project_id}")["data"])
    if pending:
        one, held = pending[0], count()
        taken = at_once(run, ("POST", f"/harlo/proposals/{one['id']}/accept", {}),
                        ("POST", f"/harlo/proposals/{one['id']}/accept", {}))
        run.check("kept twice at once: one is taken, one refused, one new note",
                  sorted(map(str, taken)) == ["False", "True"] and count() == held + 1, f"{taken}, notes {held} -> {count()}")
    else:
        run.check("two decisions on one suggestion at once", None, "nothing was suggested to decide on")
    if len(pending) > 1:
        two, held = pending[1], count()
        taken = at_once(run, ("POST", f"/harlo/proposals/{two['id']}/accept", {}),
                        ("DELETE", f"/harlo/proposals/{two['id']}"))
        run.check("kept and dismissed at once: exactly one is taken, at most one new note",
                  sorted(map(str, taken)) == ["False", "True"] and count() == held + (1 if taken[0] is True else 0),
                  f"{taken}, notes {held} -> {count()}")
    for left in proposals(run, project_id, a):
        run.call("POST", f"/harlo/proposals/{left['id']}/accept", {})

    # A session with no message of the person's: a goal alone.
    g = run.session(project)["id"]
    run.command(g, "stats.py has mode(xs), raising ValueError on an empty list, with a test in test_stats.py, "
                   "and the tests pass. In this project a tie is always broken by the smallest value.")
    goal = run.goal_ends(g)
    history = run.messages(g)
    run.check("the goal-only session did real work and ended complete",
              goal.get("status") == "complete" and "def mode" in (project / "stats.py").read_text(),
              f"{goal.get('status')}: {goal.get('reason')}")
    run.check("it holds no message of the person's", all(m.get("type") != "user" for m in history))
    looked = reviewed(run, g, None)
    run.check("the goal-only session was reviewed", looked is not None, "no review finished in 3 minutes")
    set_by_person = next((m["id"] for m in history if m.get("type") == "synthetic"
                          and (m.get("metadata") or {}).get("source") == "goal"
                          and "round" not in (m.get("metadata") or {})), None)
    from_goal = proposals(run, project_id, g)
    for p in from_goal:
        run.say(f"  suggested ({p['kind']}, {p['scope']}): {p['text']}\n    from: {p['excerpt'][:140]}")
    run.check("its suggestions name the goal the person set as their source",
              all(p["source"]["message"] == set_by_person for p in from_goal) if from_goal else None,
              str([p["source"] for p in from_goal]) if from_goal else "the model suggested nothing from it")
    for left in from_goal:
        run.call("POST", f"/harlo/proposals/{left['id']}/accept", {})

    # A new session knows what was kept.
    kept = run.call("GET", f"/harlo/notes?projectID={project_id}")["data"]
    run.say("  notes that hold now:\n" + "\n".join(f"    - {n['text']}" for n in kept if not n.get("supersededBy")))
    asked = ("Without reading any file or running anything: how are tests run in this project, and is there "
             "a module we don't import here? Two short lines.")
    c = run.session(project)["id"]
    answer = "\n".join(texts(m) for m in run.turn(c, asked) if m.get("type") == "assistant")
    run.say("  a new session's answer:\n" + "\n".join("    " + l for l in answer.strip().splitlines()))
    knows = "unittest" in answer and "statistics" in answer
    told = any("unittest" in n["text"] or "statistics" in n["text"] for n in kept)
    run.check("a new session answers from the kept notes", knows if told else None,
              "no note about the tests or the module was kept" if not told else answer[:200])
    passed, last = run.tests_pass(project)
    run.check("the project's tests pass", passed, " ".join(last))


def same_point(one, other):
    """Whether two suggestions make the same point: most words of the shorter are in the longer.

    Rough, and only for this report: Harlo itself compares whole texts and
    leaves other wordings to the model.
    """
    words = [set("".join(c.lower() if c.isalnum() else " " for c in text).split()) for text in (one, other)]
    short, long = sorted(words, key=len)
    return len(short) >= 3 and len(short & long) / len(short) >= 0.7


class Watch:
    """Follows where the review of a session stands, with the time of each move."""

    def __init__(self, run, session):
        self.moves, self.done = [], threading.Event()
        self.thread = threading.Thread(target=self.follow, args=(run, session), daemon=True)
        self.thread.start()

    def follow(self, run, session):
        last = run.setting(f"proposals/{session}")
        while not self.done.is_set():
            try:
                now = run.setting(f"proposals/{session}")
            except sqlite3.Error:
                now = last
            if now != last:
                self.moves.append((time.monotonic(), now))
                last = now
            time.sleep(0.1)

    def stop(self):
        self.done.set()
        self.thread.join()
        return self.moves


def missed(run):
    run.say("\nMissed: the agent can't keep notes, so the review has to catch the correction")
    run.configure(keeps_notes=False, slow_review=True)
    project = run.project({"stats.py": STATS, "test_stats.py": STATS_TESTS})
    run.call("PUT", "/harlo/memory", {"enabled": True})
    made = run.session(project)
    a, project_id = made["id"], made["projectID"]

    run.turn(a, "Add variance(xs) (the population variance) to stats.py with a test in test_stats.py, and "
                "run the tests.")
    first = reviewed(run, a, None)
    watch = Watch(run, a)
    correction = ("Two things about this project, please stick to them from now on: we never import the "
                  "statistics module here, these helpers are written by hand; and tests are run with "
                  "`python3 -m unittest -q`, never pytest. Redo variance that way if it needs it.")
    history = run.turn(a, correction)
    corrected = next(m["id"] for m in history if m.get("type") == "user" and m.get("text") == correction)
    # The person goes on at once, while the review of that turn is under way.
    quick = "Run the tests once more and give me the result in one line."
    history = run.turn(a, quick)
    went_idle = time.monotonic()
    last_step = [m["id"] for m in history if m.get("type") == "assistant"][-1]
    deadline = time.monotonic() + 240
    while time.monotonic() < deadline and (run.setting(f"proposals/{a}") or "") < last_step:
        time.sleep(0.5)
    moves = watch.stop()
    stands = run.setting(f"proposals/{a}") or ""
    run.say(f"  the review moved {len(moves)} time(s) after the correction; the quick turn went idle "
            + ", ".join(f"{went_idle - at:+.1f} s after move {n + 1}" for n, (at, _) in enumerate(moves)))
    during = bool(moves) and moves[0][0] > went_idle and moves[0][1] < last_step
    run.check("the quick turn ended while the review of the correction was still running", during or None,
              "the review had ended first, or read both turns in one look")
    run.check("the quick turn was reviewed too, with no further message from the person",
              (len(moves) >= 2 and stands >= last_step) if during else (True if stands >= last_step else None),
              f"the review stands at {stands or 'nothing'}, the quick turn ended at {last_step}")

    kept_by_agent = run.call("GET", f"/harlo/notes?projectID={project_id}")["data"]
    run.check("the agent kept no note itself", not kept_by_agent, str([n["text"] for n in kept_by_agent]))
    suggested = proposals(run, project_id, a)
    for p in suggested:
        run.say(f"  suggested ({p['kind']}, {p['scope']}): {p['text']}\n    from: {p['excerpt'][:140]}")
    people = {m["id"] for m in history if m.get("type") == "user"}
    run.check("the review suggested notes from the correction the agent couldn't keep",
              any(p["source"]["message"] == corrected for p in suggested),
              f"{len(suggested)} suggestion(s); first review at {first}")
    run.check("every suggestion names a message of the person's as its source",
              all(p["source"]["message"] in people for p in suggested) if suggested else None)
    run.check("nothing is a note until the person keeps it",
              not run.call("GET", f"/harlo/notes?projectID={project_id}")["data"])

    count = lambda: len(run.call("GET", f"/harlo/notes?projectID={project_id}")["data"])
    decisions = []
    if suggested:
        one, held = suggested[0], count()
        taken = at_once(run, ("POST", f"/harlo/proposals/{one['id']}/accept", {}),
                        ("POST", f"/harlo/proposals/{one['id']}/accept", {}))
        run.check("kept twice at once: one is taken, one refused, one new note",
                  sorted(map(str, taken)) == ["False", "True"] and count() == held + 1,
                  f"{taken}, notes {held} -> {count()}")
        decisions.append("kept")
    else:
        run.check("two decisions on one suggestion at once", None, "nothing was suggested to decide on")
    if len(suggested) > 1:
        two, held = suggested[1], count()
        taken = at_once(run, ("DELETE", f"/harlo/proposals/{two['id']}"),
                        ("DELETE", f"/harlo/proposals/{two['id']}"))
        run.check("turned down twice at once: one is taken, one refused, no new note",
                  sorted(map(str, taken)) == ["False", "True"] and count() == held,
                  f"{taken}, notes {held} -> {count()}")
        decisions.append("turned down")
    else:
        run.check("turned down twice at once", None, "there was no second suggestion")

    # The same correction again: what was decided isn't suggested a second time.
    mark = run.setting(f"proposals/{a}")
    run.turn(a, "To repeat, since it matters: no statistics module here, and tests run with "
                "`python3 -m unittest -q`. Check stats.py and test_stats.py once more against that and run them.")
    again = reviewed(run, a, mark)
    repeated = [p for p in proposals(run, project_id, a) if p["id"] not in {q["id"] for q in suggested}]
    for p in repeated:
        run.say(f"  suggested after the repeat ({p['kind']}): {p['text']}")
    run.say(f"  the first two suggestions were decided as: {decisions}")
    run.check("a suggestion already decided isn't made again, in the same or other words, when the "
              "person repeats themselves",
              (not any(same_point(p["text"], q["text"]) for p in repeated for q in suggested[:2])) if again else None,
              "the repeat wasn't reviewed in 3 minutes" if not again else str([p["text"] for p in repeated]))
    run.configure()


def clean_up(run):
    """Removes what the run left on the scratch server."""
    try:
        for project_id in run.project_ids:
            for note in run.call("GET", f"/harlo/notes?projectID={project_id}")["data"]:
                run.call("DELETE", f"/harlo/notes/{note['id']}")
            for left in run.call("GET", f"/harlo/proposals?projectID={project_id}")["data"]:
                run.call("DELETE", f"/harlo/proposals/{left['id']}")
        run.call("PUT", "/harlo/memory", {"enabled": False})
    except (urllib.error.URLError, ConnectionError, KeyError) as why:
        print(f"couldn't clean up: {why}", file=sys.stderr)
    run.server.close()
    for project in run.projects:
        shutil.rmtree(project, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--plan", default=str(Path.home() / "scratch/harlo-plan"),
                        help="the scratch folder holding the ChatGPT sign-in")
    parser.add_argument("--model", default="openai/gpt-6.1-sol")
    parser.add_argument("--only", choices=["goals", "notes", "missed"])
    args = parser.parse_args()
    if not BENCH.SERVER.exists():
        raise SystemExit(f"no harlo-server at {BENCH.SERVER}: cargo build -p harlo-server")
    if Path(args.plan).resolve().is_relative_to(TOOLS.parent):
        raise SystemExit("--plan DIR holds a sign-in: keep it outside the repository")
    if not (Path(args.plan) / "data/harlo/server.db").exists():
        raise SystemExit(f"no sign-in in {args.plan}: tools/bench/cold-cache.py --plan {args.plan} --sign-in")

    began = time.monotonic()
    run = Run(args.plan, args.model)
    try:
        if args.model.split("/", 1)[1] not in BENCH.offered(run.server, args.model.split("/", 1)[0]):
            raise SystemExit(f"the sign-in doesn't offer {args.model}")
        print(f"model: {args.model}", flush=True)
        for name, part in (("goals", goals), ("notes", notes), ("missed", missed)):
            if args.only in (None, name):
                part(run)
    finally:
        clean_up(run)

    failed = [name for name, ok, _ in run.checks if ok is False]
    skipped = [name for name, ok, _ in run.checks if ok is None]
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    REPORTS.mkdir(parents=True, exist_ok=True)
    report = REPORTS / f"memory-goals-{stamp}.md"
    lines = [f"# Goal recovery and suggested notes on {args.model}", "",
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
