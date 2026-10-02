#!/usr/bin/env python3
"""Ask `hobby-research-lsp` for workspace symbols over stdio and print them, one
per line as `name<TAB>kind<TAB>container<TAB>path:line` (line 1-based) — the
request Helix sends on `space S`, made scriptable.

Usage: lsp-symbols.py <root> <query> [--server CMD]
Exits 1 on a timeout or an error response.
"""

import json
import subprocess
import sys
import threading
from pathlib import Path

KINDS = {
    1: "file", 5: "class", 6: "method", 10: "enum", 11: "interface", 12: "function",
    13: "variable", 14: "constant", 23: "struct", 4: "package",
}
TIMEOUT = 10


def send(proc, message):
    body = json.dumps(message).encode()
    proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    proc.stdin.flush()


def read(proc):
    length = None
    while True:
        line = proc.stdout.readline()
        if not line:
            raise EOFError("server closed its output")
        line = line.strip()
        if not line:
            break
        key, _, value = line.decode().partition(":")
        if key.lower() == "content-length":
            length = int(value)
    return json.loads(proc.stdout.read(length))


def response_for(proc, request_id):
    while True:
        message = read(proc)
        if message.get("id") == request_id and "method" not in message:
            return message


def run(root, query, server):
    proc = subprocess.Popen(
        server, cwd=root, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    timer = threading.Timer(TIMEOUT, proc.kill)
    timer.start()
    try:
        uri = Path(root).resolve().as_uri()
        send(proc, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": None, "rootUri": uri, "capabilities": {},
            "workspaceFolders": [{"uri": uri, "name": "root"}]}})
        response_for(proc, 1)
        send(proc, {"jsonrpc": "2.0", "method": "initialized", "params": {}})
        send(proc, {"jsonrpc": "2.0", "id": 2, "method": "workspace/symbol",
                    "params": {"query": query}})
        reply = response_for(proc, 2)
        if "error" in reply:
            raise RuntimeError(reply["error"])
        symbols = reply.get("result") or []
        send(proc, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
        response_for(proc, 3)
        send(proc, {"jsonrpc": "2.0", "method": "exit"})
        return symbols
    finally:
        timer.cancel()
        proc.kill()
        proc.wait()


def main(argv):
    default = Path(__file__).resolve().parent.parent / "lsps/hobby-research-lsp/target/debug/hobby-research-lsp"
    server = [str(default)]
    if "--server" in argv:
        i = argv.index("--server")
        server = argv[i + 1].split()
        del argv[i:i + 2]
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    root, query = argv
    try:
        symbols = run(root, query, server)
    except (EOFError, RuntimeError, OSError, ValueError) as err:
        print(f"lsp-symbols: {err}", file=sys.stderr)
        return 1
    for s in symbols:
        path = s["location"]["uri"].removeprefix("file://")
        line = s["location"]["range"]["start"]["line"] + 1
        kind = KINDS.get(s["kind"], str(s["kind"]))
        print(f"{s['name']}\t{kind}\t{s.get('containerName', '')}\t{path}:{line}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
