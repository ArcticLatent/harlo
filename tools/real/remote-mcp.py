#!/usr/bin/env python3
"""A stand-in for two remote MCP servers, for tools/real/features.py (the case `mcpremote`).

    remote-mcp.py PORT LOG

On 127.0.0.1:PORT it serves, over MCP's HTTP transport with plain JSON answers:

    /open/mcp   a server anyone may use: the tool `stock_level`, two resources (a text and a
                picture) and a resource template.
    /mcp        a server behind a sign-in: the tool `whoami`, answered only with the token its
                own authorization server hands out (metadata, client registration, an
                authorization page that sends the browser straight back with a code, a token
                endpoint).

Each MCP request is written to LOG as a line of JSON: its path, method, tool, and whether it
carried the token. The numbers the tools return can't be guessed, so an answer holding them
came from a call.
"""
import base64
import json
import struct
import sys
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlencode, urlsplit

TOKEN = "tok-real-1"
STOCK = {"walnut": 4173, "cedar": 268, "ash": 0}
POLICY = "Reorder a board when its stock falls below 37 units. Orders go out on Thursdays."


def label():
    """The depot's shelf label: a plain red square, as a PNG in base64."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    side = 96
    rows = b"".join(b"\x00" + bytes((220, 20, 20)) * side for _ in range(side))
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return base64.b64encode(png).decode()


def tools_of(path):
    if path == "/open/mcp":
        return [{"name": "stock_level", "description": "How many boards of a wood are in stock at the depot.",
                 "inputSchema": {"type": "object", "properties": {"wood": {"type": "string"}},
                                 "required": ["wood"]}}]
    return [{"name": "whoami", "description": "Says which account the vault is signed in as.",
             "inputSchema": {"type": "object", "properties": {}}}]


def called(path, name, arguments):
    if path == "/open/mcp" and name == "stock_level":
        wood = str(arguments.get("wood", "")).lower()
        if wood not in STOCK:
            return f"No such wood: {wood}. The depot keeps {', '.join(STOCK)}.", True
        return f"{STOCK[wood]} boards of {wood} are in stock.", False
    if path == "/mcp" and name == "whoami":
        return "The vault is signed in as account HX-58213.", False
    return f"No such tool: {name}", True


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def base(self):
        return f"http://127.0.0.1:{self.server.server_address[1]}"

    def send(self, status, body=None, headers=()):
        data = b"" if body is None else json.dumps(body).encode()
        self.send_response(status)
        for name, value in headers:
            self.send_header(name, value)
        if body is not None:
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def note(self, **seen):
        with open(self.server.log, "a") as log:
            log.write(json.dumps(seen) + "\n")

    def do_GET(self):
        url = urlsplit(self.path)
        base = self.base()
        if url.path.startswith("/.well-known/oauth-protected-resource"):
            self.send(200, {"resource": f"{base}/mcp", "authorization_servers": [base]})
        elif url.path == "/.well-known/oauth-authorization-server":
            self.send(200, {
                "issuer": base, "authorization_endpoint": f"{base}/authorize",
                "token_endpoint": f"{base}/token", "registration_endpoint": f"{base}/register",
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "code_challenge_methods_supported": ["S256"],
                "token_endpoint_auth_methods_supported": ["none"]})
        elif url.path == "/authorize":
            asked = {k: v[0] for k, v in parse_qs(url.query).items()}
            back = asked.get("redirect_uri", "") + "?" + urlencode(
                {"code": "the-code", "state": asked.get("state", "")})
            self.send(302, headers=[("Location", back)])
        elif url.path in ("/mcp", "/open/mcp"):
            self.send(405)
        else:
            self.send(404)

    def do_POST(self):
        url = urlsplit(self.path)
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        if url.path == "/register":
            asked = json.loads(body or b"{}")
            self.note(path=url.path, client=asked.get("client_name"))
            self.send(201, {"client_id": "harlo-real", "client_name": asked.get("client_name"),
                            "redirect_uris": asked.get("redirect_uris"),
                            "token_endpoint_auth_method": "none"})
        elif url.path == "/token":
            form = {k: v[0] for k, v in parse_qs(body.decode()).items()}
            if (form.get("grant_type") != "authorization_code" or form.get("code") != "the-code"
                    or not form.get("code_verifier")):
                self.send(400, {"error": "invalid_grant"})
                return
            self.send(200, {"access_token": TOKEN, "token_type": "Bearer", "expires_in": 3600,
                            "refresh_token": "ref-real-1"})
        elif url.path in ("/mcp", "/open/mcp"):
            self.mcp(url.path, json.loads(body or b"{}"))
        else:
            self.send(404)

    def mcp(self, path, message):
        signed = self.headers.get("Authorization") == f"Bearer {TOKEN}"
        method, params = message.get("method"), message.get("params") or {}
        self.note(path=path, method=method, tool=params.get("name"), signed=signed)
        if path == "/mcp" and not signed:
            challenge = f'Bearer resource_metadata="{self.base()}/.well-known/oauth-protected-resource/mcp"'
            self.send(401, headers=[("WWW-Authenticate", challenge)])
            return
        if "id" not in message:
            self.send(202)
            return
        if method == "initialize":
            capabilities = {"tools": {}, **({"resources": {}} if path == "/open/mcp" else {})}
            result = {"protocolVersion": params.get("protocolVersion"), "capabilities": capabilities,
                      "serverInfo": {"name": "depot" if path == "/open/mcp" else "vault", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": tools_of(path)}
        elif method == "tools/call":
            text, failed = called(path, params.get("name"), params.get("arguments") or {})
            result = {"content": [{"type": "text", "text": text}], "isError": failed}
        elif method == "resources/list" and path == "/open/mcp":
            result = {"resources": [{"uri": "depot://policy/reorder", "name": "Reorder policy",
                                     "mimeType": "text/plain"},
                                    {"uri": "depot://label", "name": "Shelf label", "mimeType": "image/png"}]}
        elif method == "resources/templates/list" and path == "/open/mcp":
            result = {"resourceTemplates": [{"uriTemplate": "depot://bin/{number}", "name": "A bin's contents"}]}
        elif method == "resources/templates/list":
            result = {"resourceTemplates": []}
        elif method == "resources/read" and params.get("uri") == "depot://policy/reorder":
            result = {"contents": [{"uri": params["uri"], "mimeType": "text/plain", "text": POLICY}]}
        elif method == "resources/read" and params.get("uri") == "depot://label":
            result = {"contents": [{"uri": params["uri"], "mimeType": "image/png", "blob": label()}]}
        elif method == "resources/read" and str(params.get("uri", "")).startswith("depot://bin/"):
            number = int("".join(ch for ch in params["uri"][12:] if ch.isdigit()) or 0)
            result = {"contents": [{"uri": params["uri"], "mimeType": "text/plain",
                                    "text": f"Bin {number} holds {number * 7 + 7} ash offcuts."}]}
        else:
            result = {}
        self.send(200, {"jsonrpc": "2.0", "id": message["id"], "result": result})


def main():
    server = ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler)
    server.log = sys.argv[2]
    server.serve_forever()


if __name__ == "__main__":
    main()
