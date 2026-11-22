#!/usr/bin/env python3
"""Drives the real Harlo window in a private X display, for hands-on UI checks.

Everything runs in scratch folders: a private OpenCode server with the
scripted model (tools/oc-sandbox.sh), an invisible Xvfb display, and Harlo
with HOME and every XDG folder inside the scratch folder, and a private D-Bus
session bus. The desktop session, its notifications and portals, the shared
OpenCode service and the real config are never touched. Notifications sent on
the private bus are logged instead of shown.

    tools/gui-drive.py start [--size 1588x947]   build, then start everything
    tools/gui-drive.py shot [name]               screenshot; prints the PNG path
    tools/gui-drive.py click X Y [--right|--middle] [--double]
    tools/gui-drive.py move X Y
    tools/gui-drive.py drag X1 Y1 X2 Y2
    tools/gui-drive.py scroll X Y up|down [N]
    tools/gui-drive.py type TEXT                 types into the focused widget
    tools/gui-drive.py key COMBO...              e.g. Return ctrl+Return Escape ctrl+v
    tools/gui-drive.py clip-text TEXT            puts text on the display's clipboard
    tools/gui-drive.py clip-image PNG            puts an image on the display's clipboard
    tools/gui-drive.py resize W H                resizes the Harlo window
    tools/gui-drive.py api METHOD PATH [JSON]    calls the private OpenCode API
    tools/gui-drive.py file PATH TEXT            writes a file in the sandbox project
    tools/gui-drive.py env                       prints the sandbox's URL, folders and display
    tools/gui-drive.py log [N]                   the last N lines of Harlo's output
    tools/gui-drive.py notifications             desktop notifications Harlo sent
    tools/gui-drive.py restart                   restarts Harlo only (its saved layout stays)
    tools/gui-drive.py stop                      stops everything and removes the scratch folders

State lives in $HARLO_GUI_STATE (default /tmp/harlo-gui.json). Each process
runs in a systemd user scope capped at 6 GB of memory.
"""

import base64
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE = os.environ.get("HARLO_GUI_STATE", "/tmp/harlo-gui.json")
MEMORY = "6G"


def fail(message):
    print(f"gui-drive: {message}", file=sys.stderr)
    sys.exit(1)


def load():
    try:
        with open(STATE) as f:
            return json.load(f)
    except FileNotFoundError:
        fail("not started; run `tools/gui-drive.py start`")


def save(state):
    with open(STATE, "w") as f:
        json.dump(state, f, indent=2)


def scoped(argv):
    """Runs argv in its own memory-capped user scope; the pid is argv's own."""
    return ["systemd-run", "--user", "--scope", "-q", "-p", f"MemoryMax={MEMORY}", *argv]


def spawn(argv, env, log):
    out = open(log, "ab")
    proc = subprocess.Popen(
        scoped(argv), env=env, stdout=out, stderr=subprocess.STDOUT,
        stdin=subprocess.DEVNULL, start_new_session=True,
    )
    return proc.pid


def x_env(state):
    env = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY",)}
    env["DISPLAY"] = state["display"]
    return env


def xdotool(state, *args):
    subprocess.run(["xdotool", *args], env=x_env(state), check=True)


def free_display():
    for n in range(90, 140):
        if not os.path.exists(f"/tmp/.X11-unix/X{n}") and not os.path.exists(f"/tmp/.X{n}-lock"):
            return f":{n}"
    fail("no free X display number")


def harlo_env(state):
    root = state["gui_root"]
    env = x_env(state)
    for key in list(env):
        if key.startswith("XDG_") and key != "XDG_RUNTIME_DIR":
            del env[key]
    env.update(
        HOME=f"{root}/home",
        XDG_CONFIG_HOME=f"{root}/config",
        XDG_DATA_HOME=f"{root}/data",
        XDG_STATE_HOME=f"{root}/state",
        XDG_CACHE_HOME=f"{root}/cache",
        HARLO_OPENCODE_PASSWORD=state["password"],
        DBUS_SESSION_BUS_ADDRESS=state["bus"],
        RUST_BACKTRACE="1",
    )
    return env


def start_harlo(state):
    state["harlo_pid"] = spawn(
        [state["binary"], "--server", state["url"]], harlo_env(state), f"{state['gui_root']}/harlo.log"
    )
    # Wait for the window to map.
    for _ in range(100):
        found = subprocess.run(
            ["xdotool", "search", "--onlyvisible", "--name", "Harlo"],
            env=x_env(state), capture_output=True, text=True,
        )
        if found.stdout.strip():
            state["window"] = found.stdout.split()[0]
            return
        time.sleep(0.1)
    fail(f"the Harlo window didn't appear; see {state['gui_root']}/harlo.log")


def cmd_start(args):
    if os.path.exists(STATE):
        fail(f"already started ({STATE}); run `stop` first")
    size = "1588x947"
    if args[:1] == ["--size"]:
        size = args[1]
    width, height = size.split("x")
    build = subprocess.run(scoped(["cargo", "build", "-q", "-p", "harlo"]), cwd=REPO)
    if build.returncode != 0:
        fail("cargo build failed")
    sandbox = subprocess.run(
        [f"{REPO}/tools/oc-sandbox.sh"], capture_output=True, text=True, check=True
    ).stdout
    values = dict(line.split("=", 1) for line in sandbox.split() if "=" in line)
    gui_root = tempfile.mkdtemp(prefix="harlo-gui.")
    for sub in ("home", "config", "data", "state", "cache", "shots"):
        os.makedirs(f"{gui_root}/{sub}")
    state = {
        "sandbox": values["DIR"], "url": values["URL"], "password": values["PASSWORD"],
        "project": values["PROJECT"], "gui_root": gui_root, "display": free_display(),
        "binary": f"{REPO}/target/debug/harlo", "shots": 0, "clip_pids": [],
    }
    state["xvfb_pid"] = spawn(
        ["Xvfb", state["display"], "-screen", "0", f"{width}x{height}x24", "-nolisten", "tcp"],
        dict(os.environ), f"{gui_root}/xvfb.log",
    )
    for _ in range(50):
        if os.path.exists(f"/tmp/.X11-unix/X{state['display'][1:]}"):
            break
        time.sleep(0.1)
    # A private session bus: notifications and portals never reach the desktop.
    socket = f"{gui_root}/bus"
    state["bus"] = f"unix:path={socket}"
    state["bus_pid"] = spawn(
        ["dbus-daemon", "--session", "--nofork", "--nopidfile", f"--address={state['bus']}"],
        dict(os.environ), f"{gui_root}/bus.log",
    )
    for _ in range(50):
        if os.path.exists(socket):
            break
        time.sleep(0.1)
    state["monitor_pid"] = spawn(
        ["dbus-monitor", "--address", state["bus"],
         "type='method_call',interface='org.freedesktop.Notifications',member='Notify'"],
        dict(os.environ), f"{gui_root}/notifications.log",
    )
    save(state)
    start_harlo(state)
    xdotool(state, "windowmove", state["window"], "0", "0")
    xdotool(state, "windowsize", state["window"], width, height)
    save(state)
    print(f"started: display {state['display']}, server {state['url']}, project {state['project']}")


def cmd_shot(args):
    state = load()
    state["shots"] += 1
    name = args[0] if args else f"{state['shots']:03}"
    path = f"{state['gui_root']}/shots/{name}.png"
    time.sleep(0.25)  # let the last input settle into a frame
    subprocess.run(["import", "-window", "root", path], env=x_env(state), check=True)
    save(state)
    print(path)


def cmd_click(args):
    state = load()
    button = "3" if "--right" in args else "2" if "--middle" in args else "1"
    x, y = [a for a in args if not a.startswith("--")][:2]
    xdotool(state, "mousemove", x, y)
    time.sleep(0.05)
    xdotool(state, "click", *(["--repeat", "2"] if "--double" in args else []), button)


def cmd_move(args):
    xdotool(load(), "mousemove", args[0], args[1])


def cmd_drag(args):
    state = load()
    x1, y1, x2, y2 = args
    xdotool(state, "mousemove", x1, y1, "mousedown", "1")
    xdotool(state, "mousemove", x2, y2, "mouseup", "1")


def cmd_scroll(args):
    state = load()
    x, y, way = args[:3]
    n = args[3] if len(args) > 3 else "3"
    xdotool(state, "mousemove", x, y)
    xdotool(state, "click", "--repeat", n, "--delay", "30", "4" if way == "up" else "5")


def focus(state):
    """Gives the window keyboard focus; with no window manager, nothing else does."""
    xdotool(state, "windowfocus", "--sync", state["window"])


def cmd_type(args):
    state = load()
    focus(state)
    xdotool(state, "type", "--delay", "15", "--", " ".join(args))


def cmd_key(args):
    state = load()
    focus(state)
    xdotool(state, "key", "--delay", "40", *args)


def own_clipboard(state, argv, stdin_data):
    proc = subprocess.Popen(argv, env=x_env(state), stdin=subprocess.PIPE, start_new_session=True)
    proc.stdin.write(stdin_data)
    proc.stdin.close()
    state["clip_pids"].append(proc.pid)
    save(state)


def cmd_clip_text(args):
    state = load()
    own_clipboard(state, ["xclip", "-selection", "clipboard"], " ".join(args).encode())


def cmd_clip_image(args):
    state = load()
    with open(args[0], "rb") as f:
        data = f.read()
    own_clipboard(state, ["xclip", "-selection", "clipboard", "-t", "image/png"], data)


def cmd_resize(args):
    state = load()
    xdotool(state, "windowsize", state["window"], args[0], args[1])


def cmd_api(args):
    state = load()
    method, path = args[0], args[1]
    body = args[2].encode() if len(args) > 2 else None
    request = urllib.request.Request(state["url"] + path, data=body, method=method)
    token = base64.b64encode(f"opencode:{state['password']}".encode()).decode()
    request.add_header("Authorization", f"Basic {token}")
    if body:
        request.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            print(response.read().decode())
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code}: {e.read().decode()}")


def cmd_file(args):
    state = load()
    path = os.path.join(state["project"], args[0])
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(" ".join(args[1:]).replace("\\n", "\n"))
    print(path)


def cmd_env(_args):
    state = load()
    for key in ("display", "url", "project", "sandbox", "gui_root"):
        print(f"{key}={state[key]}")


def cmd_log(args):
    state = load()
    n = int(args[0]) if args else 40
    with open(f"{state['gui_root']}/harlo.log", errors="replace") as f:
        print("".join(f.readlines()[-n:]), end="")


def cmd_notifications(_args):
    """Prints each notification sent on the private bus: its summary and body."""
    state = load()
    with open(f"{state['gui_root']}/notifications.log", errors="replace") as f:
        lines = f.read().splitlines()
    calls, current = [], None
    for line in lines:
        if "member=Notify" in line:
            current = []
            calls.append(current)
        elif current is not None and line.strip().startswith("string "):
            current.append(line.strip()[len("string "):].strip('"'))
    for strings in calls:
        # app name, icon, summary, body (then hint keys).
        if len(strings) >= 4:
            print(f"{strings[0]}: {strings[2]} — {strings[3]}")
    if not calls:
        print("none")


def kill(pid):
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass


def cmd_restart(_args):
    state = load()
    kill(state["harlo_pid"])
    time.sleep(0.5)
    start_harlo(state)
    xdotool(state, "windowmove", state["window"], "0", "0")
    save(state)
    print("restarted")


def cmd_stop(_args):
    state = load()
    for pid in [
        state.get("harlo_pid"), *state.get("clip_pids", []), state.get("xvfb_pid"),
        state.get("monitor_pid"), state.get("bus_pid"),
    ]:
        if pid:
            kill(pid)
    subprocess.run([f"{REPO}/tools/oc-sandbox.sh", "stop", state["sandbox"]], check=False)
    time.sleep(0.3)
    if os.path.basename(state["gui_root"]).startswith("harlo-gui."):
        shutil.rmtree(state["gui_root"], ignore_errors=True)
    os.remove(STATE)
    print("stopped")


COMMANDS = {
    "start": cmd_start, "shot": cmd_shot, "click": cmd_click, "move": cmd_move,
    "drag": cmd_drag, "scroll": cmd_scroll, "type": cmd_type, "key": cmd_key,
    "clip-text": cmd_clip_text, "clip-image": cmd_clip_image, "resize": cmd_resize,
    "api": cmd_api, "file": cmd_file, "env": cmd_env, "log": cmd_log,
    "notifications": cmd_notifications,
    "restart": cmd_restart, "stop": cmd_stop,
}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(__doc__)
        sys.exit(2)
    COMMANDS[sys.argv[1]](sys.argv[2:])
