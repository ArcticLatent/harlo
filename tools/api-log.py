#!/usr/bin/env python3
"""Records what Harlo asks a server and what comes back.

    tools/api-log.py <listen-port> <target-url> <log.jsonl>

A reverse proxy for a sandbox server: each request and its response go to
the log as one JSON line ({"t", "method", "path", "status", "request",
"response"}); an event stream is passed through as it arrives and logged
one event per line ({"t", "event", "data"}). WebSocket upgrades are piped
through untouched and logged by path only. Only for sandboxes: it keeps
request bodies, which can hold anything typed.
"""

import http.client
import json
import socket
import sys
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1])
TARGET = urllib.parse.urlsplit(sys.argv[2])
LOG = open(sys.argv[3], "a", buffering=1)
LOCK = threading.Lock()
# Bodies longer than this are cut in the log (not in what passes through).
KEEP = 64 * 1024
HOP = {"connection", "keep-alive", "transfer-encoding", "upgrade", "proxy-connection"}


def log(entry):
    entry["t"] = round(time.time(), 3)
    with LOCK:
        LOG.write(json.dumps(entry) + "\n")


def body_of(raw):
    text = raw[:KEEP].decode("utf-8", "replace")
    try:
        return json.loads(text)
    except ValueError:
        return text + ("…" if len(raw) > KEEP else "")


class Proxy(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def tunnel(self):
        """Pipes a WebSocket upgrade through, byte for byte."""
        upstream = socket.create_connection((TARGET.hostname, TARGET.port))
        head = f"{self.command} {self.path} HTTP/1.1\r\n" + "".join(
            f"{k}: {v}\r\n" for k, v in self.headers.items()) + "\r\n"
        upstream.sendall(head.encode())
        log({"method": self.command, "path": self.path, "websocket": True})
        client = self.connection

        def pipe(src, dst):
            try:
                while data := src.recv(65536):
                    dst.sendall(data)
            except OSError:
                pass
            finally:
                for s in (src, dst):
                    try:
                        s.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass

        other = threading.Thread(target=pipe, args=(upstream, client), daemon=True)
        other.start()
        pipe(client, upstream)
        other.join()
        self.close_connection = True

    def forward(self):
        if self.headers.get("Upgrade", "").lower() == "websocket":
            return self.tunnel()
        length = int(self.headers.get("Content-Length") or 0)
        sent = self.rfile.read(length) if length else b""
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP | {"host"}}
        upstream = http.client.HTTPConnection(TARGET.hostname, TARGET.port, timeout=3600)
        upstream.request(self.command, self.path, body=sent or None, headers=headers)
        reply = upstream.getresponse()
        kind = reply.getheader("Content-Type", "")
        self.send_response(reply.status)
        for k, v in reply.getheaders():
            if k.lower() not in HOP | {"content-length"}:
                self.send_header(k, v)
        if "text/event-stream" in kind:
            self.send_header("Connection", "close")
            self.end_headers()
            log({"method": self.command, "path": self.path, "status": reply.status, "stream": True})
            event = None
            while line := reply.readline():
                self.wfile.write(line)
                self.wfile.flush()
                text = line.decode("utf-8", "replace").rstrip("\r\n")
                if text.startswith("event:"):
                    event = text[6:].strip()
                elif text.startswith("data:"):
                    log({"path": self.path, "event": event, "data": body_of(text[5:].strip().encode())})
            self.close_connection = True
            return
        data = reply.read()
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
        log({"method": self.command, "path": self.path, "status": reply.status,
             "request": body_of(sent) if sent else None,
             "response": body_of(data) if data else None})

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = forward


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), Proxy).serve_forever()
