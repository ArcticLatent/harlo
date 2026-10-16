#!/usr/bin/env python3
# harlo's MCP bridge on a remote host (installed by harlo; it replaces this
# file when it changes). An MCP server on stdin/stdout, as the agents start
# them, that hands each tool call to harlo through the socket harlo forwards
# here over SSH (argv[1]). The caller is HARLO_SESSION (set in each session
# harlo starts) or, for OpenCode's shared server, HARLO_OPENCODE. With no
# harlo at the other end it offers no tools.
import json
import os
import socket
import sys

SOCK = sys.argv[1] if len(sys.argv) > 1 else ""
SESSION = os.environ.get("HARLO_SESSION", "")
OPENCODE = bool(os.environ.get("HARLO_OPENCODE"))


def ask(tool, args):
    """harlo's answer to one call ({ok, result} or {ok: false, error}), or None."""
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(120)
        s.connect(SOCK)
        line = {"session": SESSION, "opencode": OPENCODE, "tool": tool, "args": args}
        s.sendall((json.dumps(line) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = s.recv(65536)
            if not chunk:
                break
            data += chunk
        s.close()
        return json.loads(data.decode())
    except Exception:
        return None


def reply(msg_id, result=None, error=None):
    out = {"jsonrpc": "2.0", "id": msg_id}
    if error is not None:
        out["error"] = error
    else:
        out["result"] = result
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()


for raw in sys.stdin:
    try:
        msg = json.loads(raw)
    except ValueError:
        continue
    # Notifications get no answer.
    if "id" not in msg:
        continue
    msg_id, method = msg["id"], msg.get("method", "")
    params = msg.get("params") or {}
    if method == "initialize":
        reply(msg_id, {
            "protocolVersion": params.get("protocolVersion") or "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "harlo", "version": "bridge"},
            "instructions": "harlo's sessions: start other coding agents, send them messages, read their replies.",
        })
    elif method == "ping":
        reply(msg_id, {})
    elif method == "tools/list":
        # harlo's tools, from harlo (none when it isn't there or is off).
        answer = ask("_tools", {})
        tools = answer["result"] if answer and answer.get("ok") else []
        reply(msg_id, {"tools": tools})
    elif method == "tools/call":
        answer = ask(params.get("name", ""), params.get("arguments") or {})
        if answer is None:
            text, failed = "harlo isn't running (or can't be reached from this machine).", True
        elif answer.get("ok"):
            text, failed = json.dumps(answer.get("result")), False
        else:
            text, failed = answer.get("error") or "harlo couldn't do that", True
        reply(msg_id, {"content": [{"type": "text", "text": text}], "isError": failed})
    else:
        reply(msg_id, error={"code": -32601, "message": "unknown method"})
