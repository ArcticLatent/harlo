"""Drives a real Harlo window for the verify scenarios.

Each `Harlo` runs its own sandbox through tools/gui-drive.py (a private
Harlo server with the scripted model, an Xvfb display, a private D-Bus,
scratch HOME and XDG folders) with Harlo built with Slint's MCP server
(`gui-drive.py start --mcp`). Elements are found by what they say to
assistive technology: their accessible label and role. That's what a person
reads on screen, so scenarios keep working when a layout moves.

    with Harlo("sessions", shots_dir) as h:
        h.click("New session")
        h.fill("Ask anything", "say hello")
        h.key("Return")
        h.wait("hello", role="Text")
        h.shot("replied")

Keys and typing go through xdotool (gui-drive.py), as a person's would;
finding, clicking, filling and screenshots go through the MCP server.
"""

import base64
import json
import os
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
GUI = os.path.join(REPO, "tools", "gui-drive.py")

# How long a wait lasts unless a scenario says otherwise.
WAIT = 15.0


class Failed(Exception):
    """A step the scenario relies on didn't happen."""


class Element:
    """An element on screen: its handle and what it says about itself."""

    def __init__(self, handle, props):
        self.handle = handle
        self.label = props.get("accessibleLabel", "")
        self.role = props.get("accessibleRole", "")
        self.value = props.get("accessibleValue", "")
        self.description = props.get("accessibleDescription", "")
        self.checked = props.get("accessibleChecked")
        self.enabled = props.get("accessibleEnabled", True)
        self.placeholder = props.get("accessiblePlaceholderText", "")
        at = props.get("absolutePosition", {})
        size = props.get("size", {})
        self.x, self.y = at.get("x", 0.0), at.get("y", 0.0)
        self.w, self.h = size.get("width", 0.0), size.get("height", 0.0)
        self.opacity = props.get("computedOpacity", 1.0)
        self.types = [t.get("typeName", "") for t in props.get("typeNamesAndIds", [])]

    @property
    def center(self):
        return (self.x + self.w / 2, self.y + self.h / 2)

    def __repr__(self):
        return f"<{self.role or '?'} {self.label!r} at {round(self.x)},{round(self.y)} {round(self.w)}x{round(self.h)}>"


class Harlo:
    """A Harlo window in its own sandbox."""

    def __init__(self, name, shots_dir, size=(1588, 947), shared=False, env=None, preferences=None):
        """`preferences` are Harlo's saved preferences at start (JSON fields;
        by default only that the setup guide was done)."""
        self.name = name
        self.preferences = {"setup_done": True, **(preferences or {})}
        self.shots_dir = shots_dir
        self.size = size
        self.shared = shared
        self.extra_env = env or {}
        self.state_file = tempfile.mktemp(prefix=f"harlo-verify-{name}.", suffix=".json")
        self.info = {}
        self.shots = []
        self.window = None
        self.mcp = None

    # ---- lifecycle -------------------------------------------------------

    def gui(self, *args, check=True):
        env = dict(os.environ, HARLO_GUI_STATE=self.state_file, HARLO_GUI_NOBUILD="1")
        env.pop("WAYLAND_DISPLAY", None)
        env.update(self.extra_env)
        if self.shared:
            env["HARLO_GUI_SHARED"] = "1"
        env["HARLO_GUI_PREFS"] = json.dumps(self.preferences)
        p = subprocess.run(["python3", GUI, *args], cwd=REPO, env=env, capture_output=True, text=True)
        if check and p.returncode != 0:
            raise Failed(f"gui-drive {args[0]}: {p.stderr.strip() or p.stdout.strip()}")
        return p.stdout

    def start(self):
        self.gui("start", "--mcp", "--size", f"{self.size[0]}x{self.size[1]}")
        for line in self.gui("env").splitlines():
            key, _, value = line.partition("=")
            self.info[key] = value
        self.mcp = f"http://127.0.0.1:{self.info['mcp_port']}/mcp"
        deadline = time.time() + 30
        while True:
            try:
                handles = self.call("list_windows")["windowHandles"]
                if handles:
                    self.window = handles[0]
                    break
            except (OSError, Failed, KeyError):
                pass
            if time.time() > deadline:
                raise Failed("Harlo's MCP server didn't answer")
            time.sleep(0.2)
        # The first frames settle (fonts, the connection, the sidebar).
        self.wait("Settings", role="Button", timeout=30)
        return self

    def stop(self):
        if os.path.exists(self.state_file):
            self.gui("stop", check=False)
        try:
            os.remove(self.state_file)
        except FileNotFoundError:
            pass

    def __enter__(self):
        return self.start()

    def __exit__(self, *_):
        self.stop()

    def kill(self):
        """Ends Harlo's process at once, as a crash or a closed laptop
        would; the sandbox's server keeps running. Returns the PID."""
        out = self.gui("kill")
        self.window = None
        return int(out.split()[-1])

    def revive(self):
        """Starts Harlo again after `kill`."""
        self.gui("revive")
        self._come_back()

    def restart_server(self):
        """Restarts the sandbox's server on the same port; Harlo keeps
        running and has to reconnect."""
        self.gui("server-restart")

    def restart(self):
        """Restarts Harlo only; its saved layout and settings stay."""
        self.gui("restart")
        self._come_back()

    def _come_back(self):
        self.window = None
        deadline = time.time() + 30
        while self.window is None:
            try:
                handles = self.call("list_windows")["windowHandles"]
                self.window = handles[0] if handles else None
            except (OSError, Failed, KeyError):
                pass
            if time.time() > deadline:
                raise Failed("Harlo didn't come back")
            time.sleep(0.2)
        # It comes back at its own size; the run's size stays.
        if getattr(self, "size", None):
            self.resize(*self.size)
        self.wait("Settings", role="Button", timeout=30)

    # ---- the MCP server --------------------------------------------------

    def call(self, tool, **args):
        if self.window is not None and tool in WINDOW_TOOLS:
            args.setdefault("windowHandle", self.window)
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                           "params": {"name": tool, "arguments": args}}).encode()
        request = urllib.request.Request(self.mcp, body, {
            "Content-Type": "application/json", "Accept": "application/json, text/event-stream"})
        with urllib.request.urlopen(request, timeout=30) as r:
            answer = json.load(r)
        if "error" in answer:
            raise Failed(f"{tool}: {answer['error'].get('message')}")
        result = answer["result"]
        content = result.get("content") or [{}]
        if result.get("isError"):
            raise Failed(f"{tool}: {content[0].get('text', '')}")
        first = content[0]
        if first.get("type") == "text":
            try:
                return json.loads(first["text"])
            except ValueError:
                return first["text"]
        return first

    def root(self):
        return self.call("get_window_properties")["rootElementHandle"]

    # ---- finding ---------------------------------------------------------

    def query(self, under, stack):
        handles = self.call("query_element_descendants", elementHandle=under,
                            findAll=True, queryStack=stack).get("elementHandles", [])
        return list({(h["index"], h["generation"]): h for h in handles}.values())

    def find(self, label, role=None, visible=True, where=None):
        """Every element labelled `label` (any label with None; of `role`,
        and that `where` accepts) on screen, top to bottom. An element a
        scroll view clips is cut to what shows of it (`shown`), and left out
        once nothing does."""
        stack = [{"matchDescendants": True}]
        if label is not None:
            stack.append({"matchElementAccessibleLabel": label})
        if role:
            stack.append({"matchElementAccessibleRole": role})
        found = {}
        for handle in self.query(self.root(), stack):
            try:
                found[(handle["index"], handle["generation"])] = Element(
                    handle, self.call("get_element_properties", elementHandle=handle))
            except Failed:
                continue
        for e in found.values():
            e.shown = (e.x, e.y, e.x + e.w, e.y + e.h)
            e.scroller = None
        if found:
            # Cut each to the scroll views it's inside; the innermost (the
            # smallest) is the one to scroll.
            for f in self.scrollers():
                for handle in self.query(f.handle, stack):
                    e = found.get((handle["index"], handle["generation"]))
                    if e is None:
                        continue
                    x0, y0, x1, y1 = e.shown
                    e.shown = (max(x0, f.x), max(y0, f.y), min(x1, f.x + f.w), min(y1, f.y + f.h))
                    if e.scroller is None or f.w * f.h < e.scroller.w * e.scroller.h:
                        e.scroller = f
        out = [e for e in found.values() if (not visible or self.visible(e))
               and (where is None or where(e))]
        out.sort(key=lambda e: (round(e.y), e.x))
        return out

    def properties(self, handles):
        """Each handle's element, leaving out any gone since the query (a
        streaming answer replaces its rows as it grows)."""
        out = []
        for handle in handles:
            try:
                out.append(Element(handle, self.call("get_element_properties", elementHandle=handle)))
            except Failed:
                continue
        return out

    def scrollers(self):
        return self.properties(self.query(self.root(), [{"matchDescendants": True},
                                                         {"matchElementTypeNameOrBase": "Flickable"}]))

    def visible(self, e):
        width, height = self.size
        x0, y0, x1, y1 = getattr(e, "shown", (e.x, e.y, e.x + e.w, e.y + e.h))
        x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, width), min(y1, height)
        return e.opacity > 0.01 and x1 - x0 > 0.5 and y1 - y0 > 0.5

    def seek(self, present, turns=12):
        """Scrolls each scroll view down, then back up, a wheel turn at a
        time, until `present()` finds something; whether it did."""
        for f in sorted(self.scrollers(), key=lambda f: f.w * f.h):
            for direction in (-1, 1):
                for _ in range(turns if direction < 0 else 2 * turns):
                    self.call("scroll_element", elementHandle=f.handle, deltaX=0,
                              deltaY=direction * f.h * 0.7)
                    time.sleep(0.12)
                    if present():
                        return True
        return bool(present())

    @staticmethod
    def clipped(e):
        x0, y0, x1, y1 = e.shown
        return y1 - y0 < e.h - 0.5 or x1 - x0 < e.w - 0.5

    def shows(self, label, role=None):
        return bool(self.find(label, role))

    def wait(self, label, role=None, timeout=WAIT, where=None):
        """The first element labelled `label`, once one shows."""
        deadline = time.time() + timeout
        while True:
            found = self.find(label, role, where=where)
            if found:
                return found[0]
            if time.time() > deadline:
                raise Failed(f"nothing showed {label!r}" + (f" ({role})" if role else "") + f" in {timeout:g} s")
            time.sleep(0.2)

    def says(self, words):
        """The first text on screen with `words` in it, or None."""
        return next((t for t in self.texts() if words in t), None)

    def wait_says(self, words, timeout=WAIT):
        """Waits until a text on screen has `words` in it; returns that text."""
        return self.until(f"a text saying {words!r}", lambda: self.says(words), timeout)

    def wait_gone(self, label, role=None, timeout=WAIT, where=None):
        deadline = time.time() + timeout
        while self.find(label, role, where=where):
            if time.time() > deadline:
                raise Failed(f"{label!r} still shows after {timeout:g} s")
            time.sleep(0.2)

    def until(self, what, test, timeout=WAIT):
        """Waits until `test()` is true; `what` says what was awaited."""
        deadline = time.time() + timeout
        while True:
            value = test()
            if value:
                return value
            if time.time() > deadline:
                raise Failed(f"{what}: not within {timeout:g} s")
            time.sleep(0.25)

    def texts(self, roles=("Text", "Groupbox")):
        """Every visible text's words, top to bottom: labels, answers and
        prompts (a prompt's bubble reads out as a group)."""
        found = []
        for role in roles:
            stack = [{"matchDescendants": True}, {"matchElementAccessibleRole": role}]
            for e in self.properties(self.query(self.root(), stack)):
                if self.visible(e) and e.label:
                    found.append(e)
        found.sort(key=lambda e: (round(e.y), e.x))
        return [e.label for e in found]

    # ---- acting ----------------------------------------------------------

    def element(self, label, role, index, reveal=True, where=None):
        """The `index`th element labelled `label` (that `where` accepts),
        scrolled fully into view."""
        def present():
            return [e for e in self.find(label, role, visible=False, where=where)
                    if self.visible(e) or e.scroller]

        # Shown, or about to show; else scrolled out of sight (the MCP
        # server leaves out what a scroll view hides entirely): look for it.
        try:
            self.until(f"{label!r} to show", present, timeout=3)
        except Failed:
            if not self.seek(present):
                self.until(f"{label!r} to show", present, timeout=WAIT)
        for _ in range(40):
            found = self.find(label, role, visible=False, where=where)
            on_screen = [e for e in found if self.visible(e)]
            # Shown ones first; then ones a scroll view hides, to reveal.
            ordered = on_screen + [e for e in found if not self.visible(e) and e.scroller]
            if index >= len(ordered):
                raise Failed(f"only {len(ordered)} of {label!r} show")
            e = ordered[index]
            if not reveal or not e.scroller or not self.clipped(e):
                return e
            f = e.scroller
            # Bigger than its scroll view (a long editor): shown is enough.
            if self.visible(e) and (e.w > f.w or e.h > f.h):
                return e
            if e.y < f.y:
                dy = min(f.y - e.y + 8, f.h * 0.8)
            else:
                dy = -min(e.y + e.h - (f.y + f.h) + 8, f.h * 0.8)
            self.call("scroll_element", elementHandle=f.handle, deltaX=0, deltaY=dy)
            time.sleep(0.25)
        raise Failed(f"couldn't scroll {label!r} into view")

    def click(self, label, role="Button", index=0, button="Left", double=False, where=None):
        """Clicks the element labelled `label`, as the pointer would."""
        e = self.element(label, role, index, where=where)
        self.call("click_element", elementHandle=e.handle, button=button,
                  action="DoubleClick" if double else "SingleClick")
        self.settle()
        return e

    def press(self, label, role="Button", index=0, where=None):
        """Activates the element as assistive technology does (its default action)."""
        e = self.element(label, role, index, reveal=False, where=where)
        self.call("invoke_accessibility_action", elementHandle=e.handle, action="Default_")
        self.settle()
        return e

    def hover(self, label, role=None, index=0, where=None):
        e = self.element(label, role, index, where=where)
        self.call("hover_element", elementHandle=e.handle)
        self.settle()
        return e

    def scroll(self, label, dy, role=None, index=0):
        """A wheel turn over the element: a negative `dy` reveals what's below."""
        e = self.element(label, role, index, reveal=False)
        self.call("scroll_element", elementHandle=e.handle, deltaX=0, deltaY=dy)
        self.settle()

    def fill(self, label, text, index=0, where=None):
        """Puts `text` in the field labelled `label`, as typing it would."""
        e = self.element(label, "TextInput", index, where=where)
        self.call("click_element", elementHandle=e.handle, button="Left", action="SingleClick")
        self.call("set_element_value", elementHandle=e.handle, value=text)
        self.settle()
        return e

    def key(self, *combos):
        """Presses keys, such as `Return`, `ctrl+comma` or `Escape`."""
        self.gui("key", *combos)
        self.settle()

    def type(self, text):
        self.gui("type", text)
        self.settle()

    def resize(self, width, height):
        self.gui("resize", str(width), str(height))
        self.size = (width, height)
        time.sleep(0.6)

    def settle(self, seconds=0.35):
        """Lets an animation or a reply to a click finish."""
        time.sleep(seconds)

    # ---- looking ---------------------------------------------------------

    def shot(self, name):
        """A screenshot of the window, kept with the scenario's results."""
        image = self.call("take_screenshot")
        os.makedirs(self.shots_dir, exist_ok=True)
        path = os.path.join(self.shots_dir, f"{len(self.shots) + 1:02d}-{name}.png")
        with open(path, "wb") as f:
            f.write(base64.b64decode(image["data"]))
        self.shots.append(path)
        return path

    # ---- the sandbox -----------------------------------------------------

    def api(self, method, path, body=None):
        """Calls the sandbox's server."""
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.info["url"] + path, data, method=method)
        token = base64.b64encode(f"opencode:{self.info['password']}".encode()).decode()
        request.add_header("Authorization", f"Basic {token}")
        if data is not None:
            request.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(request, timeout=30) as r:
                text = r.read().decode()
        except urllib.error.HTTPError as e:
            raise Failed(f"{method} {path}: {e.code} {e.read().decode()[:300]}") from e
        return json.loads(text) if text.strip() else None

    @property
    def project(self):
        return self.info["project"]

    def env(self):
        """The environment Harlo runs in: its scratch HOME and config, for
        running git or other tools as Harlo would."""
        root = self.info["gui_root"]
        env = {k: v for k, v in os.environ.items() if not k.startswith(("XDG_", "GIT_"))}
        env.update(HOME=f"{root}/home", XDG_CONFIG_HOME=f"{root}/config", GIT_CONFIG_NOSYSTEM="1")
        return env

    def file(self, path, text):
        """Writes a file in the sandbox's project."""
        full = os.path.join(self.project, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as f:
            f.write(text)

    def log(self, lines=60):
        return self.gui("log", str(lines), check=False)

    def clipboard(self):
        """The text on the window's display's clipboard."""
        return self.gui("clip-read", check=False)

    def notifications(self):
        return self.gui("notifications", check=False)


# The tools that take the window's handle.
WINDOW_TOOLS = {"get_window_properties", "find_elements_by_id", "take_screenshot",
                "move_pointer", "dispatch_pointer_scroll", "dispatch_key_event"}
