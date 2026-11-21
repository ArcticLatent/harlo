"""A scripted OpenAI-compatible model server for Harlo's tests.

    python3 -I tools/fake-model.py <port> <log.jsonl>

GET  /v1/models            one model, "fake-coder"
POST /v1/chat/completions  streamed (SSE) or plain JSON

The last user message is a script. Steps are separated by ";;". Which
step runs is the number of tool results after that user message, so
each step answers one model request. Parts inside a step are separated
by " | " and run in order:

    think <text>          reasoning text
    say <text>            answer text
    tool <name> <json>    a tool call (ends the step with tool_calls)
    long <n>              n blocks of Markdown: prose, a code block, a list
    pace <ms>             delay between streamed chunks from here on

When the steps run out, the reply is "done.". A message without a
script gets "ok.". Every request body is appended to the log, so a
test can see which tools OpenCode offered and what it sent back.
"""

import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, LOG = int(sys.argv[1]), sys.argv[2]

LOREM = (
    "Harlo keeps the transcript readable while the agent works, wrapping long "
    "lines of prose so the reader never scrolls sideways to follow an argument. "
)


def text_of(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return ""


def plan(messages):
    """Returns the parts of the step to run for this request."""
    last_user = max((i for i, m in enumerate(messages) if m.get("role") == "user"), default=-1)
    if last_user < 0:
        return [("say", "ok.")]
    script = text_of(messages[last_user].get("content")).strip()
    steps = [s.strip() for s in script.split(";;") if s.strip()]
    known = ("think ", "say ", "tool ", "long ", "pace ")
    if not steps or not any(steps[0].startswith(k) for k in known):
        return [("say", "ok.")]
    done = sum(1 for m in messages[last_user + 1 :] if m.get("role") == "tool")
    if done >= len(steps):
        return [("say", "done.")]
    parts = []
    for raw in steps[done].split(" | "):
        word, _, rest = raw.strip().partition(" ")
        parts.append((word, rest))
    return parts


def long_markdown(n):
    blocks = []
    for i in range(n):
        blocks.append(
            f"### Section {i}\n\n{LOREM * 3}\n\n```rust\nfn section_{i}() -> u32 {{\n    let value = {i};\n    value * 2\n}}\n```\n\n"
            f"- first point about {i}\n- second point with `inline code`\n\n"
        )
    return "".join(blocks)


def chunk(delta=None, finish=None, usage=None):
    c = {"id": "fake", "object": "chat.completion.chunk", "created": int(time.time()), "model": "fake-coder", "choices": []}
    if delta is not None or finish is not None:
        c["choices"] = [{"index": 0, "delta": delta or {}, "finish_reason": finish}]
    if usage is not None:
        c["usage"] = usage
    return c


def pieces(text, size=24):
    return [text[i : i + size] for i in range(0, len(text), size)] or [""]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def send_json(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            self.send_json(200, {"object": "list", "data": [{"id": "fake-coder", "object": "model"}]})
        else:
            self.send_json(404, {"error": {"message": "no such path"}})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "body": body, "t": time.time()}) + "\n")
        if not self.path.rstrip("/").endswith("/chat/completions"):
            self.send_json(404, {"error": {"message": "only chat completions"}})
            return
        parts = plan(body.get("messages", []))
        if body.get("stream"):
            self.stream(parts)
        else:
            self.plain(parts)

    def plain(self, parts):
        text = "".join(rest for word, rest in parts if word == "say")
        calls = [self.call(i, rest) for i, (word, rest) in enumerate(parts) if word == "tool"]
        message = {"role": "assistant", "content": text or None}
        if calls:
            message["tool_calls"] = calls
        self.send_json(200, {
            "id": "fake", "object": "chat.completion", "model": "fake-coder",
            "choices": [{"index": 0, "message": message, "finish_reason": "tool_calls" if calls else "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
        })

    @staticmethod
    def call(index, rest):
        name, _, args = rest.partition(" ")
        return {"id": f"call_{int(time.time() * 1000)}_{index}", "type": "function",
                "function": {"name": name, "arguments": args or "{}"}}

    def stream(self, parts):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        pace = 0.0
        calls = 0
        try:
            self.send(chunk({"role": "assistant", "content": ""}))
            for word, rest in parts:
                if word == "pace":
                    pace = int(rest) / 1000
                elif word == "think":
                    for p in pieces(rest):
                        self.send(chunk({"reasoning_content": p}), pace)
                elif word in ("say", "long"):
                    text = long_markdown(int(rest)) if word == "long" else rest
                    for p in pieces(text):
                        self.send(chunk({"content": p}), pace)
                elif word == "tool":
                    call = self.call(calls, rest)
                    args = call["function"]["arguments"]
                    self.send(chunk({"tool_calls": [{"index": calls, "id": call["id"], "type": "function",
                                                     "function": {"name": call["function"]["name"], "arguments": ""}}]}))
                    for p in pieces(args, 16):
                        self.send(chunk({"tool_calls": [{"index": calls, "function": {"arguments": p}}]}), pace)
                    calls += 1
            self.send(chunk(finish="tool_calls" if calls else "stop"))
            self.send(chunk(usage={"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}))
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

    def send(self, obj, pace=0.0):
        if pace:
            time.sleep(pace)
        self.wfile.write(b"data: " + json.dumps(obj).encode() + b"\n\n")
        self.wfile.flush()


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
