#!/usr/bin/env python3
"""Measures what clearing a rested session's old tool results does, on a real model.

    tools/bench/cold-cache.py --fake      the scripted model: checks the plumbing, costs nothing
    tools/bench/cold-cache.py --model anthropic/claude-sonnet-5-5 [--reps 2]
                              [--wait 0] [--prices IN,WRITE,READ,OUT] [--note TEXT]

On a ChatGPT plan instead of a key, with a sign-in of the benchmark's own
kept in a scratch folder DIR (never the one Harlo itself holds):

    tools/bench/cold-cache.py --plan DIR --sign-in     prints a link to open; lists the models
    tools/bench/cold-cache.py --plan DIR --model openai/MODEL [--reps 2] …
    tools/bench/cold-cache.py --plan DIR --sign-out    forgets the sign-in, removes DIR

One session works through four prompts in a scratch project (copies of three
modules of Python's library): read two long files and take notes; change a
third file; change the first long file; change the second. It runs with
`compaction.prune` off (`false`) and on, `--reps` times each, every run in a project of
its own on a private server (scratch HOME and XDG folders, a random password,
a session bus that leads nowhere). With a key, only that key is passed in from
the environment, under the name the provider's own tools use (`--key-env` to
name another).

With `--wait 0` (the default) every prompt counts as coming after a rest
(`"prune": 0`), and nothing waits. With `--wait 310` the session really rests
that long before each prompt and `"prune": true` is used, so the provider's
cache has let go as it would for a person.

What is reported for each run: whether the three changes work (checked by
loading the changed files), the steps and file reads it took, the tokens the
provider counted, what they cost as billed, and what they cost when each
prompt after the first finds the cache cold (its first request charged whole
at the price of writing to the cache). Prices are US dollars per million
tokens (input, cache write, cache read, output), taken from Harlo's model
catalog unless `--prices` gives them.

Results are appended to tools/bench/HISTORY.md.
"""

import argparse
import base64
import datetime
import importlib.util
import json
import os
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

TOOLS = Path(__file__).resolve().parent.parent
SERVER = Path(os.environ.get("HARLO_SERVER_BIN", TOOLS.parent / "target/debug/harlo-server"))
HISTORY = Path(__file__).resolve().parent / "HISTORY.md"

# The provider's own names for its key.
KEY_ENV = {"anthropic": "ANTHROPIC_API_KEY", "openai": "OPENAI_API_KEY",
           "openrouter": "OPENROUTER_API_KEY", "opencode": "OPENCODE_API_KEY"}

CATALOG = TOOLS.parent / "crates/harlo-engine/catalog/models.json"


def prices(provider, name):
    """The catalog's prices for a model: input, cache write, cache read, output."""
    cost = json.loads(CATALOG.read_text()).get(provider, {}).get("models", {}).get(name, {}).get("cost")
    if not isinstance(cost, dict) or not all(isinstance(cost.get(k), (int, float)) for k in ("input", "output")):
        return None
    # Without prices of its own for the cache, a model charges its input price.
    return (cost["input"], cost.get("cache_write", cost["input"]),
            cost.get("cache_read", cost["input"]), cost["output"])


FILES = ["argparse.py", "difflib.py", "textwrap.py"]

PROMPTS = [
    "Read argparse.py and difflib.py from start to end; they are long, so read every part of "
    "both. Then write NOTES.md with ten short points: five on how argparse handles option "
    "strings that conflict, five on how difflib's SequenceMatcher finds matching blocks. "
    "Change no other file.",
    "In textwrap.py add a function shorten_middle(text, width, placeholder='...'). Text no "
    "longer than width comes back as it is. Longer text keeps its start and its end with the "
    "placeholder between them, so the result is exactly width characters; when the characters "
    "kept are an odd number, the start gets one more than the end. Add it to __all__.",
    "In argparse.py give ArgumentParser a method option_strings() that returns a sorted list "
    "of every option string the parser itself knows (such as '-h' and '--help').",
    "In difflib.py give SequenceMatcher a method longest() that returns the longest matching "
    "block over the whole of both sequences, as a Match.",
]

# `--task jobs`: the three changes and a question, each in a session of its own, with nothing read
# beforehand. This is where reading less of a long file can show.
JOBS = [
    *PROMPTS[1:],
    "Which two methods of the class ArgumentParser in argparse.py are the longest, counted in lines? "
    "Write their names and their lengths to LONGEST.md. Change no other file.",
]

# `--task list`: the three changes asked for at once, in one prompt. Several tasks in one message is
# where an agent that keeps a todo list writes one.
LIST = ["Make these three changes, one after the other.\n\n"
        + "\n\n".join(f"{number}. {text}" for number, text in enumerate(PROMPTS[1:], 1))]

# What the scripted model does for each prompt with --fake.
FAKE = [
    'tool read {"path":"argparse.py"} ;; tool read {"path":"difflib.py"} ;; '
    'tool read {"path":"argparse.py","offset":2000} ;; say Read.',
    'tool read {"path":"textwrap.py"} ;; say Changed.',
    "say Changed.",
    "say Changed.",
]


FAKE_LIST = ['tool read {"path":"textwrap.py"} ;; say Changed.']

FAKE_JOBS = ['tool read {"path":"textwrap.py"} ;; say Changed.', "say Changed.", "say Changed.",
             'tool read {"path":"argparse.py","outline":true} ;; say Found.']


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_textwrap(project):
    m = load(project / "textwrap.py", "bench_textwrap")
    return (m.shorten_middle("abcdefghij", 7) == "ab...ij"
            and m.shorten_middle("abcdefghij", 8) == "abc...ij"
            and m.shorten_middle("abc", 8) == "abc"
            and "shorten_middle" in m.__all__)


def check_argparse(project):
    m = load(project / "argparse.py", "bench_argparse")
    parser = m.ArgumentParser()
    parser.add_argument("-v", "--verbose")
    parser.add_argument("--out")
    return parser.option_strings() == ["--help", "--out", "--verbose", "-h", "-v"]


def check_difflib(project):
    m = load(project / "difflib.py", "bench_difflib")
    return tuple(m.SequenceMatcher(None, "abxcd", "abcd").longest()) == (0, 0, 2)


def check_longest(project):
    found = (project / "LONGEST.md").read_text()
    return "_parse_known_args" in found and "__init__" in found


CHECKS = [("textwrap", check_textwrap), ("argparse", check_argparse), ("difflib", check_difflib)]
JOB_CHECKS = [*CHECKS, ("longest", check_longest)]


class Server:
    """A private Harlo server in a scratch folder.

    `keep` names a folder that outlives the server (a plan's sign-in is kept
    there); without it the folder is a new one, removed on close.
    """

    def __init__(self, env_key=None, fake=False, keep=None):
        self.kept = keep is not None
        self.dir = Path(keep) if keep else Path(tempfile.mkdtemp(prefix="harlo-bench."))
        self.dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        for sub in ("home", "config/harlo", "data", "state", "cache", "projects"):
            (self.dir / sub).mkdir(parents=True, exist_ok=True)
        self.providers = None
        self.model = None
        if fake:
            self.model = subprocess.Popen(
                [sys.executable, "-I", str(TOOLS / "fake-model.py"), "0", str(self.dir / "model.jsonl")],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True)
            port = int(self.model.stdout.readline())
            self.providers = {"fake": {
                "name": "Fake",
                "settings": {"baseURL": f"http://127.0.0.1:{port}/v1", "apiKey": "fake-key"},
                "models": {"fake-coder": {"name": "Fake Coder", "limit": {"context": 128000, "output": 8192}}}}}
        self.configure({})
        self.password = secrets.token_hex(16)
        self.port = free_port()
        env = {"PATH": os.environ["PATH"], "HOME": str(self.dir / "home"),
               "XDG_CONFIG_HOME": str(self.dir / "config"), "XDG_DATA_HOME": str(self.dir / "data"),
               "XDG_STATE_HOME": str(self.dir / "state"), "XDG_CACHE_HOME": str(self.dir / "cache"),
               "HARLO_SERVER_PASSWORD": self.password,
               "HARLO_CLAUDE_PATH": str(self.dir / "bin/claude"),
               "HARLO_RELAY": "http://127.0.0.1:9",
               "HARLO_UPDATE_MANIFEST_URL": "http://127.0.0.1:9",
               "HARLO_UPDATE_CHANGELOG_URL": "http://127.0.0.1:9",
               "DBUS_SESSION_BUS_ADDRESS": f"unix:path={self.dir}/no-bus"}
        if env_key:
            env[env_key] = os.environ[env_key]
        log = open(self.dir / "server.log", "a")
        self.server = subprocess.Popen(
            [str(SERVER), "serve", "--hostname", "127.0.0.1", "--port", str(self.port)],
            cwd=self.dir / "projects", env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
            start_new_session=True)
        for _ in range(100):
            try:
                self.call("GET", "/api/info")
                return
            except (urllib.error.URLError, ConnectionError):
                time.sleep(0.1)
        self.close()
        raise SystemExit(f"the server didn't start; see {self.dir}/server.log")

    def configure(self, config):
        """Writes the server's configuration; it's read again for every turn."""
        if self.providers:
            config = {**config, "providers": self.providers}
        (self.dir / "config/harlo/harlo.json").write_text(json.dumps(config, indent=2))

    def project(self):
        """A new git project holding the three library files."""
        project = Path(tempfile.mkdtemp(prefix="p", dir=self.dir / "projects"))
        library = Path(argparse.__file__).parent
        for name in FILES:
            shutil.copy(library / name, project / name)
        git = ["git", "-c", "user.name=bench", "-c", "user.email=bench@example.invalid"]
        subprocess.run(["git", "init", "-q"], cwd=project, check=True)
        subprocess.run(["git", "add", "."], cwd=project, check=True)
        subprocess.run([*git, "commit", "-qm", "start"], cwd=project, check=True)
        return project

    def call(self, method, path, body=None):
        auth = base64.b64encode(f"harlo:{self.password}".encode()).decode()
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.port}{path}", method=method,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Authorization": f"Basic {auth}", "Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=60) as response:
            raw = response.read()
        return json.loads(raw) if raw else None

    def close(self, forget=False):
        for process in (self.server, self.model):
            if process is not None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                process.wait()
        if forget or not self.kept:
            shutil.rmtree(self.dir, ignore_errors=True)


def free_port():
    import socket
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def messages(server, session):
    return server.call("GET", f"/api/session/{session}/message")["data"]


def turn(server, session, text, limit):
    """Sends a prompt and waits for the run to end; refuses whatever it asks leave for."""
    idles = sum(1 for m in messages(server, session) if m.get("type") == "idle")
    server.call("POST", f"/api/session/{session}/prompt", {"text": text, "delivery": "queue"})
    deadline = time.monotonic() + limit
    while time.monotonic() < deadline:
        for asked in server.call("GET", f"/api/session/{session}/permission")["data"]:
            server.call("POST", f"/api/session/{session}/permission/{asked['id']}/reply",
                        {"decision": "reject", "message": "Not in this run: work inside the project with the tools you have."})
        now = messages(server, session)
        if sum(1 for m in now if m.get("type") == "idle") > idles:
            return now
        time.sleep(0.5)
    raise SystemExit(f"a prompt didn't finish in {limit} s")


def measure(history, prices):
    """Per prompt: steps, reads, tokens, and the cost as billed and with a cold start."""
    price_in, price_write, price_read, price_out = (p / 1e6 for p in prices)
    turns = []
    for message in history:
        if message.get("type") == "user":
            turns.append({"steps": 0, "reads": 0, "prompt": 0, "cached": 0, "output": 0, "billed": 0.0, "cold": 0.0})
        if message.get("type") != "assistant" or not turns:
            continue
        at = turns[-1]
        tokens = message.get("tokens") or {}
        cache = tokens.get("cache") or {}
        fresh, read, write = tokens.get("input", 0), cache.get("read", 0), cache.get("write", 0)
        out = tokens.get("output", 0)
        billed = fresh * price_in + write * price_write + read * price_read + out * price_out
        # After a rest the first request of a prompt finds nothing cached.
        first = at["steps"] == 0 and len(turns) > 1
        cold = (fresh + read + write) * price_write + out * price_out if first else billed
        at["steps"] += 1
        at["reads"] += sum(1 for p in message.get("content", []) if p.get("type") == "tool" and p.get("name") == "read")
        at["prompt"] += fresh + read + write
        at["cached"] += read
        at["output"] += out
        at["billed"] += billed
        at["cold"] += cold
    return turns


# What `--compare` can turn on besides clearing, and the `experimental` key each one sets.
SWITCHES = {"instructions": "cachedInstructions", "files": "scriptTools", "outline": "outline", "todo": "todo"}


def run(server, arm, args):
    prune = False if arm == "off" else (True if args.wait else 0)
    if args.compare in SWITCHES:
        # Clearing as Harlo has it by default, in both arms.
        prune = True
    config = {
        "model": args.model,
        # Nothing here asks: the files in the project and no more.
        "permissions": [
            {"action": "*", "resource": "*", "effect": "deny"},
            {"action": "read", "resource": "*", "effect": "allow"},
            {"action": "edit", "resource": "*", "effect": "allow"},
            {"action": "glob", "resource": "*", "effect": "allow"},
            {"action": "grep", "resource": "*", "effect": "allow"},
        ],
    }
    config["compaction"] = {"prune": prune}
    if args.compare in SWITCHES:
        config["experimental"] = {SWITCHES[args.compare]: arm == "on"}
    if args.compare == "todo":
        # The list is the agent's own: writing it asks nobody. Allowed in both arms; only the on arm has the tool.
        config["permissions"].append({"action": "todowrite", "resource": "*", "effect": "allow"})
    if args.compare == "files":
        # Scripts in both arms, as a session has them; the on arm adds the files to them.
        config["permissions"].append({"action": "execute", "resource": "*", "effect": "allow"})
    if args.shell:
        # A shell, as a real session has one, inside the sandbox for agents'
        # commands: they change the project and the temporary folders only.
        # The server's home is a scratch one, so the sandbox's own list of
        # secret places covers nothing real: the real home is hidden whole.
        config["permissions"].append({"action": "shell", "resource": "*", "effect": "allow"})
        config["sandbox"] = {"hidden": [str(Path.home())]}
    server.configure(config)
    project = server.project()
    session = server.call("POST", "/api/session", {"location": {"directory": str(project)}})["data"]["id"]
    history = []
    if args.task == "jobs":
        for number, text in enumerate(FAKE_JOBS if args.fake else JOBS):
            if number:
                session = server.call("POST", "/api/session", {"location": {"directory": str(project)}})["data"]["id"]
            history += turn(server, session, text, args.limit)
    else:
        prompts = (FAKE_LIST if args.fake else LIST) if args.task == "list" else (FAKE if args.fake else PROMPTS)
        for number, text in enumerate(prompts):
            if number and args.wait:
                time.sleep(args.wait)
            history = turn(server, session, text, args.limit)
    passed = {}
    for name, check in (JOB_CHECKS if args.task == "jobs" else CHECKS):
        try:
            passed[name] = bool(check(project))
        except Exception as why:  # a change that doesn't load is a failed change
            passed[name] = False
            passed[name + "_why"] = f"{type(why).__name__}: {why}"[:120]
    cleared = sum(1 for m in history for p in m.get("content", []) if isinstance(p, dict)
                  and "cleared" in ((p.get("state") or {}).get("metadata") or {}))
    failed = [str(m.get("error"))[:160] for m in history if m.get("type") == "assistant" and m.get("error")]
    tools = {}
    for m in history:
        for p in m.get("content", []) if m.get("type") == "assistant" else []:
            if isinstance(p, dict) and p.get("type") == "tool":
                name = p.get("name")
                if name == "read" and ((p.get("state") or {}).get("input") or {}).get("outline"):
                    name = "outline"
                tools[name] = tools.get(name, 0) + 1
    return {"arm": arm, "turns": measure(history, args.prices), "passed": passed, "cleared": cleared,
            "failed": failed, "tools": tools, "checks": JOB_CHECKS if args.task == "jobs" else CHECKS}


def offered(server, provider):
    """The models of `provider` the server can use now."""
    return sorted(m["id"] for m in server.call("GET", "/api/model")["data"]
                  if (m.get("providerID") or m.get("provider")) == provider)


def sign_in(server):
    """Signs the scratch server in to a ChatGPT plan: prints the link, waits for the browser."""
    begun = server.call("POST", "/api/integration/openai/connect/oauth", {"methodID": "chatgpt-token-sharing"})["data"]
    print(f"OPEN THIS LINK AND SIGN IN (10 minutes):\n{begun['url']}", flush=True)
    while True:
        status = server.call("GET", f"/api/integration/openai/connect/oauth/{begun['attemptID']}")["data"]
        if status["status"] not in ("pending", "waiting"):
            break
        time.sleep(1)
    print(f"sign-in: {status['status']} {status.get('message', '')}".rstrip(), flush=True)
    return status


def sign_out(server):
    """Forgets every credential the scratch server holds."""
    for credential in server.call("GET", "/api/credential")["data"]:
        try:
            server.call("DELETE", f"/api/credential/{credential['id']}")
        except urllib.error.HTTPError as why:
            print(f"couldn't remove {credential['id']}: {why}", file=sys.stderr)


def total(result, key):
    return sum(t[key] for t in result["turns"])


def line(result):
    passed = "".join("+" if result["passed"].get(name) else "-" for name, _ in result["checks"])
    cached = 100 * total(result, "cached") // max(1, total(result, "prompt"))
    return (f"| {result['arm']:>3} | {passed} | {total(result, 'steps'):>5} | {total(result, 'reads'):>5} "
            f"| {result['cleared']:>7} | {total(result, 'prompt'):>10,} | {cached:>3}% | {total(result, 'output'):>7,} "
            f"| {total(result, 'billed'):>7.3f} | {total(result, 'cold'):>7.3f} |")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model", help="provider/model, as Harlo's configuration names it")
    parser.add_argument("--key-env", help="the environment variable holding the provider's key")
    parser.add_argument("--plan", metavar="DIR", help="use the ChatGPT sign-in kept in this scratch folder")
    parser.add_argument("--sign-in", action="store_true", help="with --plan: sign in and list the models")
    parser.add_argument("--sign-out", action="store_true", help="with --plan: forget the sign-in, remove DIR")
    parser.add_argument("--reps", type=int, default=2)
    parser.add_argument("--compare", choices=["clearing", *SWITCHES], default="clearing",
                        help="what the on arm turns on: clearing old tool output (the default), sending the "
                             "system prompt as the conversation's opening message (instructions), letting "
                             "scripts read and search files (files), outlines of source files (outline), "
                             "or the agent's todo list (todo)")
    parser.add_argument("--task", choices=["session", "jobs", "list"], default="session",
                        help="one session of four prompts that starts by reading two long files (the default), "
                             "four jobs, each in a session of its own with nothing read beforehand, "
                             "or the three changes asked for at once in one prompt (list)")
    parser.add_argument("--shell", action="store_true",
                        help="let the agent run commands, in the sandbox, with the real home folder hidden")
    parser.add_argument("--wait", type=int, default=0, help="seconds of rest before each prompt")
    parser.add_argument("--limit", type=int, default=900, help="seconds a prompt may take")
    parser.add_argument("--prices", help="IN,WRITE,READ,OUT in dollars per million tokens")
    parser.add_argument("--note", default="")
    parser.add_argument("--fake", action="store_true")
    args = parser.parse_args()
    if not SERVER.exists():
        raise SystemExit(f"no harlo-server at {SERVER}: cargo build -p harlo-server")
    if args.plan and Path(args.plan).resolve().is_relative_to(TOOLS.parent):
        raise SystemExit("--plan DIR holds a sign-in: keep it outside the repository")

    if args.sign_in or args.sign_out:
        if not args.plan:
            raise SystemExit("--sign-in and --sign-out need --plan DIR")
        server = Server(keep=args.plan)
        try:
            if args.sign_out:
                sign_out(server)
            elif sign_in(server)["status"] in ("complete", "completed", "done", "success"):
                print("models:", " ".join(offered(server, "openai")))
        finally:
            server.close(forget=args.sign_out)
        return

    env_key = None
    if args.fake:
        args.model, args.prices = "fake/fake-coder", (0, 0, 0, 0)
    else:
        if not args.model or "/" not in args.model:
            raise SystemExit("--model provider/model is needed (or --fake)")
        provider, name = args.model.split("/", 1)
        if not args.plan:
            env_key = args.key_env or KEY_ENV.get(provider)
            if not env_key or not os.environ.get(env_key):
                raise SystemExit(f"the provider's key isn't in the environment ({env_key or '--key-env NAME'})")
        if args.prices:
            args.prices = tuple(float(p) for p in args.prices.split(","))
        elif (known := prices(provider, name)) is not None:
            args.prices = known
        else:
            raise SystemExit(f"no prices known for {name}: pass --prices IN,WRITE,READ,OUT")
        if args.wait and args.wait < 300:
            raise SystemExit("--wait under 300 leaves the provider's cache warm; use 0 or 300 and more")

    results = []
    server = Server(env_key=env_key, fake=args.fake, keep=args.plan)
    try:
        if args.plan and name not in offered(server, provider):
            raise SystemExit(f"{args.model} isn't offered with this sign-in: {' '.join(offered(server, provider))}")
        for rep in range(args.reps):
            for arm in ("off", "on"):
                print(f"run {rep + 1}/{args.reps}, {args.compare} {arm}…", file=sys.stderr, flush=True)
                results.append(run(server, arm, args))
    finally:
        server.close()

    revision = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=TOOLS, capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=TOOLS, capture_output=True, text=True).stdout.strip()
    paid = "a ChatGPT plan (the dollars are what a key would pay)" if args.plan else "a key"
    head = [
        f"## {datetime.datetime.now().strftime('%Y-%m-%d %H:%M')} · cold-cache · {args.model}",
        "",
        f"Revision {revision}{' with uncommitted changes' if dirty else ''}; paid by {paid}; rest before each prompt: "
        f"{args.wait or 'none (every prompt counts as rested)'}; prices {args.prices}; the on arm turns on: {args.compare}; "
        f"task: {args.task} ({', '.join(name for name, _ in (JOB_CHECKS if args.task == 'jobs' else CHECKS))})."
        + (" The agent has a shell, in the sandbox." if args.shell else "")
        + (f" {args.note}" if args.note else ""),
        "",
        "| arm | work done | steps | reads | cleared | prompt tokens | from cache | output | $ billed | $ if cold |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    rows = [line(r) for r in results]
    detail = ["", "Per prompt (steps / reads / prompt tokens / from cache / $ if cold):", ""]
    for r in results:
        cells = "; ".join(f"{t['steps']} / {t['reads']} / {t['prompt']:,} / {100 * t['cached'] // max(1, t['prompt'])}% / {t['cold']:.3f}" for t in r["turns"])
        detail.append(f"- {r['arm']}: {cells}")
    detail += ["", "Tool calls:", ""]
    detail += [f"- {r['arm']}: " + ", ".join(f"{name} {n}" for name, n in sorted(r["tools"].items())) for r in results]
    why = [f"- {r['arm']}: {k} {v}" for r in results for k, v in r["passed"].items() if k.endswith("_why")]
    why += [f"- {r['arm']}: a step failed: {f}" for r in results for f in r["failed"]]
    report = "\n".join([*head, *rows, *detail, *why, ""])
    print(report)
    if not args.fake:
        with open(HISTORY, "a") as history:
            history.write(report + "\n")


if __name__ == "__main__":
    main()
