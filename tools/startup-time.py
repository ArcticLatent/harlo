#!/usr/bin/env python3
"""Times the release build's start and its idle processor use, for the
website's "native" figures.

    cargo build --release -p harlo -p harlo-server
    python3 -I tools/startup-time.py [--runs 5]

Everything runs in scratch folders, as tools/gui-drive.py sets them up: a
private server (the release build) with six sessions, a scratch HOME, a
private D-Bus and an invisible Xvfb display. Nothing of the real setup is
read.

The display is filmed at 100 pictures a second; the start is the time from
running `harlo` to the first picture that already looks like the settled
window (under 0.5% of it still to change, which leaves out the blinking
caret). It is timed twice: with the renderer Harlo picks itself (the
graphics driver's, where there is one), and with its software renderer, as
on a computer with no graphics card.

Idle use is the processor time of the window's process over ten seconds of
doing nothing, as a share of one core. It is read with the software
renderer only: a graphics driver runs timers of its own on a display with
no screen, which would be counted as Harlo's.

Run it under a memory cap (`systemd-run --user --scope -q -p MemoryMax=2G`).
"""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
RELEASE = os.path.join(ROOT, "target", "release")
SIZE = (1440, 900)
# Pictures a second the display is filmed at.
RATE = 100
# A picture this close to the settled one counts as the full window.
SETTLED = 0.005
# How long the window is left alone before its idle use is read, and for how long.
REST, IDLE = 4.0, 10.0


def gui():
    """tools/gui-drive.py as a module, for its sandbox and environment."""
    spec = importlib.util.spec_from_file_location("gui_drive", os.path.join(ROOT, "tools", "gui-drive.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cpu_seconds(pid):
    with open(f"/proc/{pid}/stat") as f:
        fields = f.read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def idle_share(pid):
    time.sleep(REST)
    before, started = cpu_seconds(pid), time.monotonic()
    time.sleep(IDLE)
    return (cpu_seconds(pid) - before) / (time.monotonic() - started) * 100


def stop(proc):
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


# The film is kept small: each picture is shrunk to this size, and no more
# than this many are kept, so a camera that misbehaves can't fill the memory.
FILM = (288, 180)
KEPT = 600


def filmed(display, env, argv, idle):
    """One start on the Xvfb display: seconds to the full window, and the idle
    share when `idle` asks for it (None otherwise)."""
    import numpy

    width, height = SIZE
    # `passthrough` hands on only the pictures really taken; without it
    # ffmpeg pads the film with thousands of copies.
    film = subprocess.Popen(
        ["ffmpeg", "-loglevel", "error", "-f", "x11grab", "-framerate", str(RATE), "-draw_mouse", "0",
         "-video_size", f"{width}x{height}", "-i", display, "-vf", f"scale={FILM[0]}:{FILM[1]}:flags=area",
         "-fps_mode", "passthrough", "-pix_fmt", "gray", "-f", "rawvideo", "-"],
        stdout=subprocess.PIPE, stdin=subprocess.DEVNULL)
    size = FILM[0] * FILM[1]
    frames = []
    # The camera is running before Harlo starts.
    for _ in range(10):
        film.stdout.read(size)
    started = time.monotonic()
    harlo = subprocess.Popen(argv, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        while time.monotonic() - started < 3.0 and len(frames) < KEPT:
            data = film.stdout.read(size)
            if len(data) < size:
                break
            frames.append((time.monotonic() - started, numpy.frombuffer(data, dtype=numpy.uint8)))
    finally:
        stop(film)
    if len(frames) >= KEPT:
        stop(harlo)
        raise SystemExit(f"the camera took {KEPT} pictures in {frames[-1][0]:.2f} s: it isn't keeping time")
    last = frames[-1][1].astype(numpy.int16)
    full = None
    for at, frame in frames:
        changed = numpy.count_nonzero(numpy.abs(frame.astype(numpy.int16) - last) > 12) / size
        if changed < SETTLED:
            full = at
            break
    blank = numpy.count_nonzero(last > 12) / size < 0.01
    share = idle_share(harlo.pid) if idle else None
    stop(harlo)
    if blank:
        raise SystemExit("the window never drew")
    return full, share


def main():
    args = sys.argv[1:]
    runs = int(args[args.index("--runs") + 1]) if "--runs" in args else 5
    for name in ("harlo", "harlo-server"):
        if not os.path.exists(os.path.join(RELEASE, name)):
            raise SystemExit(f"no {name} in target/release: cargo build --release -p harlo -p harlo-server")
    g = gui()
    state_file = tempfile.mktemp(prefix="harlo-startup.", suffix=".json")
    env = dict(os.environ, HARLO_GUI_STATE=state_file, HARLO_GUI_NOBUILD="1",
               HARLO_SERVER_BIN=os.path.join(RELEASE, "harlo-server"),
               HARLO_GUI_PREFS=json.dumps({"setup_done": True}))
    env.pop("WAYLAND_DISPLAY", None)
    drive = [sys.executable, os.path.join(ROOT, "tools", "gui-drive.py")]
    subprocess.run([*drive, "start", "--size", f"{SIZE[0]}x{SIZE[1]}"], env=env, check=True,
                   stdout=subprocess.DEVNULL)
    try:
        with open(state_file) as f:
            state = json.load(f)
        for n in range(6):
            made = subprocess.run([*drive, "api", "POST", "/api/session",
                                   json.dumps({"location": {"directory": state["project"]}})],
                                  env=env, check=True, capture_output=True, text=True).stdout
            sid = json.loads(made)["data"]["id"]
            subprocess.run([*drive, "api", "PATCH", f"/api/session/{sid}",
                            json.dumps({"title": f"Session {n + 1}"})], env=env, check=True,
                           stdout=subprocess.DEVNULL)
        time.sleep(1.5)
        # The debug window gui-drive started makes way for the release build.
        subprocess.run([*drive, "kill"], env=env, check=True, stdout=subprocess.DEVNULL)
        harlo_env = g.harlo_env(state)
        argv = [os.path.join(RELEASE, "harlo"), "--server", state["url"]]
        for renderer, extra in (("its own choice of renderer", {}),
                                ("the software renderer", {"SLINT_BACKEND": "winit-software"})):
            print(f"{renderer}:", flush=True)
            results = []
            # One start first that isn't counted: the files are read from disk once.
            for run in range(runs + 1):
                full, idle = filmed(state["display"], dict(harlo_env, **extra), argv, idle=bool(extra))
                if run:
                    results.append((full, idle))
                    start = "no full window seen" if full is None else f"full window in {full:.2f} s"
                    rest = "" if idle is None else f", idle {idle:.2f}% of one core"
                    print(f"  run {run}: {start}{rest}", flush=True)
            starts = sorted(r[0] for r in results if r[0] is not None)
            if not starts:
                raise SystemExit("no start could be timed")
            line = f"  start {starts[0]:.2f}–{starts[-1]:.2f} s (median {starts[len(starts) // 2]:.2f})"
            idles = sorted(r[1] for r in results if r[1] is not None)
            if idles:
                line += f", idle {idles[0]:.2f}–{idles[-1]:.2f}%"
            print(line, flush=True)
    finally:
        subprocess.run([*drive, "stop"], env=env, stdout=subprocess.DEVNULL)
        if os.path.exists(state_file):
            os.remove(state_file)


if __name__ == "__main__":
    main()
