"""A scripted OpenAI-compatible model server for Harlo's tests.

    python3 -I tools/fake-model.py <port> <log.jsonl>

With port 0 it takes a free port and prints its number, alone on the first
line of its output. Tests start it that way: a port found free beforehand
may be handed to another test before this server takes it.

GET  /v1/models            one model, "fake-coder"
POST /v1/chat/completions  streamed (SSE) or plain JSON
POST /v1/messages          the same script, streamed as Anthropic's Messages
POST /v1/responses         the same script, streamed as OpenAI's Responses

Reasoning comes back signed ("fake-signature") or encrypted ("fake-
encrypted"), so a test can see it sent back.

The last user message is a script. Steps are separated by ";;". Which
step runs is the number of tool results after that user message, so
each step answers one model request. Parts inside a step are separated
by " | " and run in order:

    think <text>          reasoning text
    say <text>            answer text
    tool <name> <json>    a tool call (ends the step with tool_calls)
    long <n>              n blocks of Markdown: prose, a code block, a list
    pace <ms>             delay between streamed chunks from here on
    fail <code> <text>    an HTTP error with that status and message instead
    flaky <n> <code> <text>  that error for the step's first n requests, then
                          the rest of the step

A subagent's prompt starts with the server's line telling it it's one, and a
side question (/btw) with the lines asking for a quick answer; the
script, or the plain words kept in scripts.json, is what follows.

A prompt can also be plain words, with its script in scripts.json beside
the log: {"<prompt>": <script>}, the script either a string as above or a
list of steps, each a list of [word, text] parts (so " | " and ";;" can be
part of an answer). The transcript then shows the prompt a person would
type.

A goal's reviewer (/goal) gets the verdict kept in scripts.json under
"<reviewer>", or one that finds the goal met. The agent's first prompt
for a goal runs the script kept under "<goal>", when there is one; a
round the reviewer sends it back for runs "<round>", and a goal taken up
again runs "<resume>".

When the steps run out, the reply is "done.". A message without a
script gets "ok.". A request to summarize the conversation (compaction)
gets a filled-in summary in the server's template. Every request body is appended to the log, so a
test can see which tools the server offered and what it sent back.
"""

import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, LOG = int(sys.argv[1]), sys.argv[2]

LOREM = (
    "Harlo keeps the transcript readable while the agent works, wrapping long "
    "lines of prose so the reader never scrolls sideways to follow an argument. "
)


# How the server starts its request to summarize a session.
SUMMARIZE = "You MUST summarize the conversation"

# The line the server puts before the prompt it gives a subagent.
HARLO_SUMMARIZE = "Summarize the conversation so far for the agent"
HARLO_SUBAGENT_PREAMBLE = "You are a subagent: another session handed you this task."
SUBAGENT_PREAMBLE = "You are a subagent spawned by another session."

# How the server asks the model to review a goal (/goal).
REVIEWER = "Stop acting as the assistant. You are now the reviewer for the session's goal."
HARLO_REVIEWER = "Review the session's goal now, as its reviewer rather than its assistant."

# How the agent's first prompt for a goal starts.
GOAL_START = "This session now has a goal."
GOAL_ROUND = "The reviewer says the goal isn't met yet"
GOAL_RESUME = "Pick the goal up again where it stopped."

# The verdict a goal's reviewer gives unless scripts.json keeps another.
MET = '{"verdict":"done","reason":"The work is done and shown."}'

# How a side question (/btw) starts; the question follows a blank line.
ASIDE_PREAMBLE = "The user is asking a quick side question about the conversation so far."


def text_of(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return ""


SUMMARY = """## Objective
- Exercise Harlo against a scripted model.

## Requirements
- (none)

## Decisions
- (none)

## Work State
### Completed
- (none)

### Active
- (none)

### Blocked
- (none)

## Next Move
1. (none)

## Relevant Files
(none)

## Important Context
- (none)
"""


# How the server asks for a session's title.
TITLE_REQUEST = "The conversation's first message:"

# How the server opens the message that carries a tool's pictures.
TOOL_PICTURES = "The pictures the tools returned:"


def chat_usage():
    """A step's tokens: 10 in and 5 out, or the prompt's from scripts.json's
    "<prompt_tokens>" (a context bar with something in it)."""
    prompt = stored_scripts().get("<prompt_tokens>", 10)
    return {"prompt_tokens": prompt, "completion_tokens": 5, "total_tokens": prompt + 5}


def stored_scripts():
    """The scripts kept by prompt in scripts.json beside the log."""
    path = os.path.join(os.path.dirname(os.path.abspath(LOG)), "scripts.json")
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def plan(messages):
    """Returns the parts of the step to run for this request."""
    # A user message right after a tool's result carries what the tool
    # returned, such as a picture it read; the prompt is the one before.
    last_user = max((i for i, m in enumerate(messages) if m.get("role") == "user"
                     and not (i > 0 and messages[i - 1].get("role") == "tool"
                              and text_of(m.get("content")).startswith(TOOL_PICTURES))), default=-1)
    if last_user < 0:
        return [("say", "ok.")]
    # A request to summarize, wherever the server puts the instruction.
    if any(text_of(m.get("content")).lstrip().startswith((SUMMARIZE, HARLO_SUMMARIZE))
           for m in messages):
        return [("say", SUMMARY)]
    script = text_of(messages[last_user].get("content")).strip()
    # A request to name the session (unless a script answers it): its
    # first message is its title.
    if script.startswith(TITLE_REQUEST) and "<message>" in script and script not in stored_scripts():
        first = script.split("<message>", 1)[1].split("</message>", 1)[0].strip()
        return [("say", first.splitlines()[0] if first else "New session")]
    if script.startswith((REVIEWER, HARLO_REVIEWER)):
        return [("say", stored_scripts().get("<reviewer>", MET))]
    for preamble in (SUBAGENT_PREAMBLE, HARLO_SUBAGENT_PREAMBLE):
        if script.startswith(preamble):
            script = script[len(preamble) :].strip()
    if script.startswith(ASIDE_PREAMBLE):
        script = script.partition("\n\n")[2].strip()
    known_scripts = stored_scripts()
    for start, key in ((GOAL_START, "<goal>"), (GOAL_ROUND, "<round>"), (GOAL_RESUME, "<resume>")):
        if script.startswith(start) and key in known_scripts:
            script = key
    if script in known_scripts:
        script = known_scripts[script]
        if isinstance(script, list):
            done = sum(1 for m in messages[last_user + 1 :] if m.get("role") == "tool")
            if done >= len(script):
                return [("say", "done.")]
            return [(word, text) for word, text in script[done]]
    if script.startswith("You MUST summarize the conversation") or script.startswith(
        "The previous response did not fill in the required summary template"
    ):
        return [("say", SUMMARY)]
    steps = [s.strip() for s in script.split(";;") if s.strip()]
    known = ("think ", "say ", "tool ", "long ", "pace ", "fail ", "flaky ")
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


def as_chat(body, path):
    """The request's conversation as chat completions' messages: enough for
    plan(), which counts the tool results after the last prompt."""
    if path.endswith("/messages"):
        out = []
        for m in body.get("messages", []):
            content = m.get("content")
            if m.get("role") != "user" or isinstance(content, str):
                out.append({"role": m.get("role"), "content": text_of(content)})
                continue
            results = [b for b in content if b.get("type") == "tool_result"]
            out.extend({"role": "tool"} for _ in results)
            said = text_of([b for b in content if b.get("type") == "text"])
            if said or not results:
                out.append({"role": "user", "content": said})
        return out
    out = []
    for item in body.get("input", []):
        if item.get("role") == "user":
            content = item.get("content")
            said = content if isinstance(content, str) else "".join(
                p.get("text", "") for p in content if isinstance(p, dict))
            out.append({"role": "user", "content": said})
        elif item.get("type") == "function_call_output":
            out.append({"role": "tool"})
        else:
            out.append({"role": "assistant"})
    return out


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


# How many requests each flaky step has had, by its request's last message.
FLAKY = {}


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
        path = self.path.rstrip("/")
        wire = next((w for w in ("chat/completions", "messages", "responses") if path.endswith("/" + w)), None)
        if wire is None:
            self.send_json(404, {"error": {"message": "only chat completions, messages and responses"}})
            return
        messages = body.get("messages", []) if wire == "chat/completions" else as_chat(body, path)
        parts = plan(messages)
        failed = next((rest for word, rest in parts if word == "fail"), None)
        flaky = next((rest for word, rest in parts if word == "flaky"), None)
        if flaky is not None:
            times, _, error = flaky.partition(" ")
            key = json.dumps(messages[-1:], sort_keys=True) + str(len(messages))
            FLAKY[key] = FLAKY.get(key, 0) + 1
            if FLAKY[key] <= int(times):
                failed = error
            parts = [(w, r) for w, r in parts if w != "flaky"]
        if failed is not None:
            code, _, message = failed.partition(" ")
            self.send_json(int(code), {"error": {"message": message or "failed",
                                                 "type": "invalid_request_error"}})
            return
        if wire == "messages":
            self.stream_messages(parts)
        elif wire == "responses":
            self.stream_responses(parts)
        elif body.get("stream"):
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
            "usage": chat_usage(),
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
            self.send(chunk(usage=chat_usage()))
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

    def open_stream(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()

    def event(self, obj, pace=0.0):
        if pace:
            time.sleep(pace)
        self.wfile.write(f"event: {obj['type']}\ndata: ".encode() + json.dumps(obj).encode() + b"\n\n")
        self.wfile.flush()

    def stream_messages(self, parts):
        """The step as Anthropic's Messages events."""
        self.open_stream()
        pace, calls, index = 0.0, 0, 0
        try:
            self.event({"type": "message_start", "message": {"usage": {
                "input_tokens": 10, "cache_read_input_tokens": 0, "output_tokens": 1}}})
            for word, rest in parts:
                if word == "pace":
                    pace = int(rest) / 1000
                    continue
                if word == "think":
                    self.event({"type": "content_block_start", "index": index,
                                "content_block": {"type": "thinking", "thinking": ""}})
                    for p in pieces(rest):
                        self.event({"type": "content_block_delta", "index": index,
                                    "delta": {"type": "thinking_delta", "thinking": p}}, pace)
                    self.event({"type": "content_block_delta", "index": index,
                                "delta": {"type": "signature_delta", "signature": "fake-signature"}})
                elif word in ("say", "long"):
                    text = long_markdown(int(rest)) if word == "long" else rest
                    self.event({"type": "content_block_start", "index": index,
                                "content_block": {"type": "text", "text": ""}})
                    for p in pieces(text):
                        self.event({"type": "content_block_delta", "index": index,
                                    "delta": {"type": "text_delta", "text": p}}, pace)
                elif word == "tool":
                    call = self.call(calls, rest)
                    self.event({"type": "content_block_start", "index": index, "content_block": {
                        "type": "tool_use", "id": call["id"], "name": call["function"]["name"], "input": {}}})
                    for p in pieces(call["function"]["arguments"], 16):
                        self.event({"type": "content_block_delta", "index": index,
                                    "delta": {"type": "input_json_delta", "partial_json": p}}, pace)
                    calls += 1
                else:
                    continue
                self.event({"type": "content_block_stop", "index": index})
                index += 1
            self.event({"type": "message_delta", "delta": {"stop_reason": "tool_use" if calls else "end_turn"},
                        "usage": {"output_tokens": 5}})
            self.event({"type": "message_stop"})
        except (BrokenPipeError, ConnectionResetError):
            pass

    def stream_responses(self, parts):
        """The step as OpenAI's Responses events."""
        self.open_stream()
        pace, output = 0.0, []
        try:
            for word, rest in parts:
                at = len(output)
                if word == "pace":
                    pace = int(rest) / 1000
                elif word == "think":
                    self.event({"type": "response.output_item.added", "output_index": at, "item": {"type": "reasoning"}})
                    for p in pieces(rest):
                        self.event({"type": "response.reasoning_summary_text.delta", "output_index": at,
                                    "summary_index": 0, "delta": p}, pace)
                    item = {"type": "reasoning", "encrypted_content": "fake-encrypted"}
                    self.event({"type": "response.output_item.done", "output_index": at, "item": item})
                    output.append(item)
                elif word in ("say", "long"):
                    text = long_markdown(int(rest)) if word == "long" else rest
                    self.event({"type": "response.output_item.added", "output_index": at, "item": {"type": "message"}})
                    for p in pieces(text):
                        self.event({"type": "response.output_text.delta", "output_index": at, "delta": p}, pace)
                    item = {"type": "message"}
                    self.event({"type": "response.output_item.done", "output_index": at, "item": item})
                    output.append(item)
                elif word == "tool":
                    call = self.call(at, rest)
                    item = {"type": "function_call", "call_id": call["id"], "name": call["function"]["name"]}
                    self.event({"type": "response.output_item.added", "output_index": at, "item": item})
                    for p in pieces(call["function"]["arguments"], 16):
                        self.event({"type": "response.function_call_arguments.delta", "output_index": at,
                                    "delta": p}, pace)
                    self.event({"type": "response.output_item.done", "output_index": at, "item": item})
                    output.append(item)
            self.event({"type": "response.completed", "response": {"status": "completed", "output": output,
                        "usage": {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 0},
                                  "output_tokens": 5}}})
        except (BrokenPipeError, ConnectionResetError):
            pass

    def send(self, obj, pace=0.0):
        if pace:
            time.sleep(pace)
        self.wfile.write(b"data: " + json.dumps(obj).encode() + b"\n\n")
        self.wfile.flush()


SERVER = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
if PORT == 0:
    # The caller left the choice of port to the system: say which it is.
    print(SERVER.server_address[1], flush=True)
SERVER.serve_forever()
