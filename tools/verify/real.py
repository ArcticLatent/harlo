"""Real models for the real:* scenarios: Claude, through OpenCode's Claude
Agent, signed in with the user's own Claude Code sign-in.

Safety, since the sign-in is the user's real one:

- The sandbox's `claude` is a wrapper that runs the real Claude Code with
  the sandbox's scratch HOME and a dead session bus, so it never sees the
  real ~/.claude or the desktop keyring.
- At each start it reads only the current access token from
  ~/.claude/.credentials.json (read-only) and passes it as
  CLAUDE_CODE_OAUTH_TOKEN. The refresh token never leaves that file, so
  nothing here can refresh the sign-in: refreshing would rotate it and sign
  the real Claude Code out.
- Checks run only while the token has REQUIRED_MINUTES left; Claude Code
  refreshes it on its own schedule, and the next run picks up the new one.

GPT isn't used: its sign-in in OpenCode can only be used by refreshing it
once it has expired, which would sign the real OpenCode out. It's checked
and reported as the reason for the skip.
"""

import json
import os
import shutil
import time

CREDENTIALS = os.path.expanduser("~/.claude/.credentials.json")
OPENCODE_AUTH = os.path.expanduser("~/.local/share/opencode/auth.json")
REQUIRED_MINUTES = 30

# The cheapest model the Claude Agent offers; every check is a short turn.
MODEL = "Haiku"

WRAPPER = """#!/bin/sh
# Real Claude Code for a verify sandbox: scratch HOME, no keyring, and only
# the current access token (read-only), so it can't refresh the sign-in.
token=$(python3 -I - <<'EOF'
import json, sys, time
data = json.load(open({credentials!r}))["claudeAiOauth"]
if data["expiresAt"] / 1000 - time.time() < 120:
    sys.exit("the Claude sign-in is about to expire")
print(data["accessToken"])
EOF
) || exit 1
export HOME={home!r}
export XDG_CONFIG_HOME={home!r}/.config
export DBUS_SESSION_BUS_ADDRESS="unix:path={home}/no-bus"
export CLAUDE_CODE_OAUTH_TOKEN="$token"
export DISABLE_AUTOUPDATER=1
unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN
exec {claude!r} "$@"
"""


def minutes_left():
    """Minutes the Claude Code access token has left, or None without one."""
    try:
        with open(CREDENTIALS) as f:
            data = json.load(f)["claudeAiOauth"]
        return (data["expiresAt"] / 1000 - time.time()) / 60
    except (OSError, ValueError, KeyError):
        return None


def gpt_state():
    """Why GPT checks don't run: what OpenCode's own sign-in says."""
    try:
        with open(OPENCODE_AUTH) as f:
            openai = json.load(f).get("openai")
    except (OSError, ValueError):
        openai = None
    if not openai:
        return "no ChatGPT sign-in in OpenCode"
    expires = openai.get("expires")
    if expires and expires / 1000 < time.time():
        hours = round((time.time() - expires / 1000) / 3600)
        return (f"the ChatGPT sign-in expired {hours} h ago; using it would mean refreshing it, "
                "which signs OpenCode out; sign in again there to add GPT checks")
    return "GPT checks aren't written yet"


class Setup:
    """The real Claude Code, put into each scenario's sandbox."""

    def __init__(self, claude, left):
        self.claude = claude
        self.left = left

    @staticmethod
    def find():
        """The setup, or None and why the real checks are skipped."""
        claude = shutil.which("claude") or os.path.expanduser("~/.local/bin/claude")
        if not os.path.exists(claude):
            return None, "Claude Code isn't installed"
        left = minutes_left()
        if left is None:
            return None, "Claude Code isn't signed in"
        if left < REQUIRED_MINUTES:
            return None, (f"the Claude sign-in has {max(0, round(left))} min left and real checks need "
                          f"{REQUIRED_MINUTES}; Claude Code refreshes it itself, so try again later "
                          "(refreshing it here would sign Claude Code out)")
        return Setup(os.path.realpath(claude), left), None

    def describe(self):
        return f"Claude ({MODEL}) through OpenCode's Claude Agent; sign-in good for {round(self.left)} min"

    def env(self):
        return {}

    def prepare(self, h):
        """Puts the wrapper where the sandbox server runs `claude`, then
        connects OpenCode's Claude Agent from Settings → Models."""
        sandbox = h.info["sandbox"]
        home = os.path.join(sandbox, "home")
        os.makedirs(os.path.join(sandbox, "bin"), exist_ok=True)
        path = os.path.join(sandbox, "bin", "claude")
        with open(path, "w") as f:
            f.write(WRAPPER.format(credentials=CREDENTIALS, home=home, claude=self.claude))
        os.chmod(path, 0o700)

        from _common import close_sheet, settings
        settings(h, "Models")
        h.wait_says("Signed in", timeout=60)
        if not h.find("Disconnect"):
            h.click("Connect", where=lambda e: e.x > 560)
        h.wait("Disconnect", timeout=60)
        h.shot("claude-connected")
        close_sheet(h)

    def close(self):
        pass


def pick_model(h, name=MODEL):
    """Chooses the Claude model `name` from the composer's model chip."""
    h.click("Fake Coder", where=lambda e: e.y > 600)
    h.fill("Filter models", name.lower())
    match = lambda e: name.lower() in e.label.lower() and e.role == "Button"
    row = h.wait(None, role="Button", where=lambda e: match(e) and e.y < 880, timeout=30)
    h.click(None, where=lambda e: match(e) and e.y < 880)
    h.wait(None, role="Button", where=lambda e: match(e) and e.y > 880, timeout=10)
