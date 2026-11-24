#!/usr/bin/env python3
"""Harlo verify: the whole app, checked feature by feature (see items.toml).

    tools/verify/verify.sh all              everything
    tools/verify/verify.sh changed          what the changes since the last
                                            green run touch (impact.toml)
    tools/verify/verify.sh item <id>…       those features' checks
    tools/verify/verify.sh check <check>…   those checks, such as
                                            window:sessions or unit:harlo
    tools/verify/verify.sh list             the features and their checks
    tools/verify/verify.sh report [run]     writes a run's report again

The checks:

    build                   Harlo, and Harlo with Slint's MCP server
    lint:fmt, lint:clippy   cargo fmt --check; clippy with warnings denied
    unit:<crate>            the crate's tests; each test file is reported
                            as unit:<crate>:<file>
    window:<scenario>       a scenario in tools/verify/scenarios/ driving
                            the real window on harlo-server, with the
                            scripted model
    real:<scenario>         a scenario with a real model (Claude, through
                            the server's Claude Agent and Claude Code)

Each run writes target/verify/<date-time>/ (report.html, results.jsonl,
logs, screenshots) and points target/verify/latest at it; a run with
nothing failed is remembered in target/verify/last-green.json for
`changed`. Every check runs in scratch folders: its own `harlo-server`,
HOME and XDG folders, display and D-Bus; nothing of the user's sessions,
settings or shared service is opened.

Environment:
    VERIFY_NOBUILD=1        use the binaries as they are
    VERIFY_SKIP="real lint" kinds or checks left out
    JOBS=N                  window scenarios at once (default 3)
"""

import datetime
import fnmatch
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import tomllib
import traceback
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.join(ROOT, "tools", "verify")
SCENARIOS = os.path.join(HERE, "scenarios")
RUNS = os.environ.get("VERIFY_RUNS") or os.path.join(ROOT, "target", "verify")
CRATES = ["opencode-client", "harlo-core", "harlo", "harlo-engine", "harlo-server"]
KINDS = ["build", "lint", "unit", "window", "real"]

sys.path.insert(0, HERE)
sys.path.insert(0, SCENARIOS)


def git(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True).stdout.strip()


def load_items():
    with open(os.path.join(HERE, "items.toml"), "rb") as f:
        return tomllib.load(f)["item"]


def scenario_names(kind):
    """The scenarios of `kind` (window or real), by file name."""
    prefix = "real_" if kind == "real" else ""
    names = []
    for f in sorted(os.listdir(SCENARIOS)):
        if not f.endswith(".py") or f.startswith("_"):
            continue
        name = f[:-3]
        if (kind == "real") == name.startswith("real_"):
            names.append(name[len(prefix):])
    return names


def load_scenario(kind, name):
    file = os.path.join(SCENARIOS, ("real_" if kind == "real" else "") + name + ".py")
    spec = importlib.util.spec_from_file_location(f"scenario_{kind}_{name}", file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# ---- a run -------------------------------------------------------------------


class Run:
    def __init__(self, mode, args, plan):
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        self.dir = os.path.join(RUNS, stamp)
        os.makedirs(os.path.join(self.dir, "logs"), exist_ok=True)
        self.results = os.path.join(self.dir, "results.jsonl")
        self.lock = threading.Lock()
        self.meta = {
            "mode": mode, "args": args, "plan": plan.summary(),
            "commit": git("rev-parse", "HEAD"), "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "dirty": bool(git("status", "--porcelain", "--untracked-files=no")),
            "started": time.time(),
        }
        self.write_meta()
        latest = os.path.join(RUNS, "latest")
        if os.path.islink(latest) or os.path.exists(latest):
            os.remove(latest)
        os.symlink(stamp, latest)

    def write_meta(self):
        with open(os.path.join(self.dir, "meta.json"), "w") as f:
            json.dump(self.meta, f, indent=1)

    def record(self, check, status, seconds=0, log="", detail="", shots=None, steps=None):
        row = {"check": check, "status": status, "seconds": round(seconds, 1), "log": log,
               "detail": detail[-4000:]}
        if shots:
            row["shots"] = shots
        if steps:
            row["steps"] = steps
        mark = {"pass": "ok  ", "fail": "FAIL", "skip": "skip"}.get(status, status)
        first = detail.strip().splitlines()[0][:140] if detail.strip() else ""
        with self.lock:
            with open(self.results, "a") as f:
                f.write(json.dumps(row) + "\n")
            print(f"  {mark}  {check}" + (f"  ({round(seconds)} s)" if seconds >= 1 else "")
                  + (f"  {first}" if status != "pass" and first else ""), flush=True)

    def step(self, check, cmd, timeout=3600, fail_lines=r"^error|panicked|FAILED", env=None):
        """Runs `cmd`, its output to logs/<check>.log; recorded by its exit."""
        log = os.path.join("logs", re.sub(r"[^\w.@-]", "_", check) + ".log")
        started = time.time()
        ok, detail = run_logged(cmd, os.path.join(self.dir, log), timeout, env)
        if not ok:
            lines = [l for l in open(os.path.join(self.dir, log), errors="replace").read().splitlines()
                     if re.search(fail_lines, l)]
            detail = "\n".join(lines[-15:] + [detail]) if lines else detail
        self.record(check, "pass" if ok else "fail", time.time() - started, log, detail)
        return ok


def run_logged(cmd, log, timeout, env=None):
    e = dict(os.environ)
    e.update(env or {})
    try:
        with open(log, "w") as out:
            p = subprocess.run(cmd, cwd=ROOT, env=e, stdout=out, stderr=subprocess.STDOUT,
                               timeout=timeout, stdin=subprocess.DEVNULL)
        return p.returncode == 0, "" if p.returncode == 0 else f"exit {p.returncode}"
    except subprocess.TimeoutExpired:
        return False, f"timed out after {timeout} s"


# ---- what to run -------------------------------------------------------------


class Plan:
    def __init__(self):
        self.build = not os.environ.get("VERIFY_NOBUILD")
        self.lint = []
        self.unit = []
        self.window = []
        self.real = []
        self.why = []

    def add(self, check):
        kind, _, name = check.partition(":")
        if kind == "build":
            return
        if kind == "lint":
            for n in (["fmt", "clippy"] if name in ("", "*") else [name]):
                if n not in self.lint:
                    self.lint.append(n)
        elif kind == "unit":
            crate = name.split(":")[0]
            for c in CRATES if crate in ("", "*") else [crate]:
                if c not in self.unit:
                    self.unit.append(c)
        elif kind in ("window", "real"):
            target = getattr(self, kind)
            for sc in scenario_names(kind):
                if fnmatch.fnmatchcase(sc, name or "*") and sc not in target:
                    target.append(sc)
        else:
            sys.exit(f"unknown check {check}")

    def skip(self, skipped):
        for s in skipped:
            kind, _, name = s.partition(":")
            if kind in ("lint", "unit", "window", "real"):
                target = getattr(self, kind)
                target[:] = [] if not name else [x for x in target if not fnmatch.fnmatchcase(x, name)]
            elif s == "build":
                self.build = False

    def empty(self):
        return not (self.lint or self.unit or self.window or self.real)

    def summary(self):
        return (f"build: {'yes' if self.build else 'no'}; lint: {', '.join(self.lint) or '-'}; "
                f"unit: {', '.join(self.unit) or '-'}; window: {len(self.window)} scenarios; "
                f"real: {len(self.real)} scenarios")


def last_green():
    try:
        with open(os.path.join(RUNS, "last-green.json")) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def changed_paths():
    """What changed since the last green run: git's changes against its
    commit (committed, staged, unstaged and untracked), each newer than it."""
    green = last_green()
    base = green["commit"] if green else (git("merge-base", "main", "HEAD") or "HEAD")
    since = green["started"] if green else 0
    paths = set(filter(None, git("diff", "--name-only", base).splitlines()))
    paths |= set(filter(None, git("ls-files", "--others", "--exclude-standard").splitlines()))
    if green:
        paths = {p for p in paths if not os.path.exists(os.path.join(ROOT, p))
                 or os.path.getmtime(os.path.join(ROOT, p)) > since}
    return sorted(paths), (green["commit"][:8] if green else f"main ({base[:8]})")


def plan(mode, args):
    items = load_items()
    p = Plan()
    if mode == "all":
        for kind in ("lint", "unit", "window", "real"):
            p.add(f"{kind}:*")
    elif mode == "item":
        known = {it["id"]: it for it in items}
        for a in args:
            if a not in known:
                sys.exit(f"no item {a} (items: {', '.join(known)})")
            for c in known[a].get("checks", []):
                p.add(c)
    elif mode == "check":
        for a in args:
            p.add(a)
    else:  # changed
        with open(os.path.join(HERE, "impact.toml"), "rb") as f:
            impact = tomllib.load(f)
        paths, base = changed_paths()
        p.why.append(f"{len(paths)} paths changed since {base}")
        for path in paths:
            hit = [r for r in impact["rule"] if any(fnmatch.fnmatchcase(path, g) for g in r["paths"])]
            for r in hit:
                for c in r["checks"]:
                    p.add(c)
            if not hit:
                for c in impact["default"]:
                    p.add(c)
                p.why.append(f"no rule for {path}: the default checks")
    p.skip(os.environ.get("VERIFY_SKIP", "").split())
    return p


# ---- the tiers ---------------------------------------------------------------


def scoped(cmd, memory="12G"):
    return ["systemd-run", "--user", "--scope", "-q", "-p", f"MemoryMax={memory}",
            "-p", "MemorySwapMax=0", *cmd]


def t_build(run, p):
    steps = [("build", scoped(["cargo", "build", "-q", "-j", "6", "-p", "harlo", "-p", "harlo-server"]))]
    if p.window or p.real:
        steps.append(("build:mcp", scoped(["cargo", "build", "-q", "-j", "6", "-p", "harlo",
                                           "--features", "slint/mcp",
                                           "--target-dir", os.path.join(ROOT, "target", "mcp")])))
    for check, cmd in steps:
        if not run.step(check, cmd, timeout=3600, fail_lines=r"^error|^warning"):
            return False
    return True


def t_lint(run, names):
    for n in names:
        if n == "fmt":
            run.step("lint:fmt", ["cargo", "fmt", "--all", "--check"], timeout=600,
                     fail_lines=r"^Diff in")
        else:
            cmd = ["cargo", "clippy", "-q", "-j", "6", "--all-targets", "--all-features"]
            for c in CRATES:
                cmd += ["-p", c]
            run.step("lint:clippy", scoped(cmd + ["--", "-D", "warnings"]), timeout=3600,
                     fail_lines=r"^error|^warning|-->")


RUNNING = re.compile(r"^\s+Running (?:unittests )?(\S+)")
RESULT = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")


def t_unit(run, crates):
    """Each crate's tests, once; each test file reported on its own."""
    for crate in crates:
        log = os.path.join("logs", f"unit-{crate}.log")
        started = time.time()
        cmd = scoped(["cargo", "test", "-j", "6", "--no-fail-fast", "-p", crate])
        ok, why = run_logged(cmd, os.path.join(run.dir, log), 3600)
        text = open(os.path.join(run.dir, log), errors="replace").read()
        target, failures, seen = None, {}, 0
        lines = text.splitlines()
        for i, line in enumerate(lines):
            m = RUNNING.match(line)
            if m:
                path = m.group(1)
                target = "lib" if path.startswith("src/") else os.path.splitext(os.path.basename(path))[0]
                continue
            if line.startswith("   Doc-tests"):
                target = "doc"
                continue
            m = RESULT.match(line)
            if m and target:
                seen += 1
                passed, failed, ignored = int(m.group(2)), int(m.group(3)), int(m.group(4))
                names = [l.split("---- ")[1].split(" stdout")[0] for l in failures.get(target, [])]
                detail = f"{passed} passed, {failed} failed" + (f", {ignored} ignored" if ignored else "")
                if failed:
                    detail += "\n" + "\n".join(names)
                if passed == failed == 0 and target == "doc":
                    continue
                run.record(f"unit:{crate}:{target}", "pass" if m.group(1) == "ok" else "fail",
                           0, log, detail)
                target = None
            elif line.startswith("---- ") and target:
                failures.setdefault(target, []).append(line)
        if not ok and seen == 0:
            errors = [l for l in lines if re.search(r"^error|panicked", l)]
            run.record(f"unit:{crate}", "fail", time.time() - started, log, "\n".join(errors[-15:] + [why]))
        else:
            run.record(f"unit:{crate}", "pass" if ok else "fail", time.time() - started, log,
                       "" if ok else "a test file failed (above)")


class Steps:
    """What a scenario checked: its steps and expectations, in order."""

    def __init__(self):
        self.rows = []
        self.current = "start"

    def step(self, name):
        """Names what the scenario does next, for its report."""
        self.current = name
        self.rows.append({"step": name, "ok": True})

    def expect(self, ok, what, detail=""):
        """A check that doesn't stop the scenario when it fails."""
        self.rows.append({"step": self.current, "expect": what, "ok": bool(ok), "detail": str(detail)[:500]})
        return ok

    def failed(self):
        return [r for r in self.rows if not r["ok"]]


def run_scenario(run, kind, name, setup=None):
    from driver import Failed, Harlo
    check = f"{kind}:{name}"
    started = time.time()
    try:
        module = load_scenario(kind, name)
    except Exception:
        run.record(check, "fail", 0, "", "the scenario doesn't load:\n" + traceback.format_exc()[-1500:])
        return
    if kind == "real" and setup is None:
        run.record(check, "skip", 0, "", "no real model to use")
        return
    shots_dir = os.path.join(run.dir, "shots", kind, name)
    steps = Steps()
    env = dict(getattr(module, "ENV", {}))
    if setup:
        env.update(setup.env())
    h = Harlo(f"{kind}-{name}", shots_dir, size=getattr(module, "SIZE", (1588, 947)),
              shared=getattr(module, "SHARED", False), env=env,
              preferences=getattr(module, "PREFERENCES", None))
    status, detail = "pass", ""
    log_rel = os.path.join("logs", kind, f"{name}.log")
    try:
        h.start()
        if setup:
            setup.prepare(h)
        module.run(h, steps)
    except Failed as e:
        status, detail = "fail", f"{steps.current}: {e}"
    except Exception:
        status, detail = "fail", f"{steps.current}: " + traceback.format_exc()[-1500:]
    if status == "fail":
        try:
            h.shot("failed")
            detail += "\n\nOn screen: " + " | ".join(h.texts())[:1200]
        except Exception:
            pass
    soft = steps.failed()
    if soft and status == "pass":
        status = "fail"
        detail = "\n".join(f"{r['step']}: expected {r['expect']}" + (f" ({r['detail']})" if r["detail"] else "")
                           for r in soft)
    try:
        os.makedirs(os.path.join(run.dir, "logs", kind), exist_ok=True)
        with open(os.path.join(run.dir, log_rel), "w") as f:
            f.write(h.log(400) if h.info else "")
            f.write("\n---- notifications ----\n" + (h.notifications() if h.info else ""))
    except Exception:
        log_rel = ""
    h.stop()
    shots = [os.path.relpath(s, run.dir) for s in h.shots]
    run.record(check, status, time.time() - started, log_rel, detail, shots, steps.rows)


def t_scenarios(run, kind, names, setup=None):
    jobs = int(os.environ.get("JOBS", "3"))
    if kind == "real":
        # Real models are slow and cost money: one at a time.
        jobs = 1
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as pool:
        for _ in pool.map(lambda n: run_scenario(run, kind, n, setup), names):
            pass


def t_real(run, names):
    import real
    setup, why = real.Setup.find()
    if setup is None:
        for n in names:
            run.record(f"real:{n}", "skip", 0, "", why)
        return
    print(f"  real models: {setup.describe()}", flush=True)
    try:
        t_scenarios(run, "real", names, setup)
    finally:
        setup.close()


# ---- main --------------------------------------------------------------------


def results_of(run_dir):
    rows = {}
    path = os.path.join(run_dir, "results.jsonl")
    if os.path.exists(path):
        for line in open(path):
            row = json.loads(line)
            rows[row["check"]] = row
    return rows


def known_checks():
    """Every check there is: lints, each crate's test files, scenarios."""
    checks = ["lint:fmt", "lint:clippy"]
    for crate in CRATES:
        folder = os.path.join(ROOT, "crates", crate, "tests")
        for name in sorted(os.listdir(folder)):
            if name.endswith(".rs"):
                checks.append(f"unit:{crate}:{name[:-3]}")
    checks += [f"window:{name}" for name in scenario_names("window")]
    checks += [f"real:{name}" for name in scenario_names("real")]
    return checks


def list_items():
    """Prints each feature and its checks, then what no feature names and
    what a feature names that doesn't exist."""
    items = load_items()
    for it in items:
        print(f"{it['id']}: {it['title']}")
        for c in it.get("checks", []):
            print(f"    {c}")
    known = known_checks()
    named = [c for it in items for c in it.get("checks", [])]

    def covers(name, check):
        return name == check or fnmatch.fnmatchcase(check, name) or check.startswith(name + ":")

    loose = [c for c in known if not any(covers(n, c) for n in named)]
    missing = sorted({n for n in named if not any(covers(n, c) for c in known)})
    if loose:
        print("\nIn no feature: " + ", ".join(loose))
    if missing:
        print("\nNamed but not there: " + ", ".join(missing))
    if not loose and not missing:
        print(f"\n{len(known)} checks, each in a feature.")


def main():
    args = sys.argv[1:]
    mode = args[0] if args else "changed"
    rest = args[1:]
    if mode in ("-h", "--help", "help"):
        print(__doc__)
        return 0
    if mode == "list":
        list_items()
        return 0
    if mode == "report":
        import report
        run_dir = os.path.join(RUNS, rest[0] if rest else "latest")
        print(report.write(os.path.realpath(run_dir), load_items()))
        return 0
    if mode not in ("all", "changed", "item", "check"):
        sys.exit(f"unknown mode {mode}; see --help")
    p = plan(mode, rest)
    for why in p.why[:20]:
        print(f"  {why}")
    if p.empty():
        print("nothing to check")
        return 0
    run = Run(mode, rest, p)
    print(f"verify {mode}: {p.summary()}\n  run: {run.dir}", flush=True)
    if p.build and not t_build(run, p):
        p.lint = p.unit = p.window = p.real = []
    if p.lint:
        t_lint(run, p.lint)
    if p.unit:
        t_unit(run, p.unit)
    if p.window:
        t_scenarios(run, "window", p.window)
    if p.real:
        t_real(run, p.real)
    run.meta["finished"] = time.time()
    run.write_meta()
    import report
    path = report.write(run.dir, load_items())
    rows = results_of(run.dir).values()
    failed = [r["check"] for r in rows if r["status"] == "fail"]
    passed = sum(1 for r in rows if r["status"] == "pass")
    skipped = sum(1 for r in rows if r["status"] == "skip")
    print(f"\n{passed} passed, {len(failed)} failed, {skipped} skipped"
          f" in {round((run.meta['finished'] - run.meta['started']) / 60, 1)} min")
    if failed:
        print("failed: " + " ".join(failed))
    elif mode in ("all", "changed"):
        shutil.copy(os.path.join(run.dir, "meta.json"), os.path.join(RUNS, "last-green.json"))
    print(f"report: {path}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
