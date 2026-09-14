#!/usr/bin/env python3
"""Check mojo-lsp-server goto-definition resolution for a stdlib symbol.

Spawns the mono pixi env's mojo-lsp-server, opens a tiny Mojo file using
`Dict`, and prints what `textDocument/definition` resolves to. Pass
`--stdlib` to add `-I <mono>/programs/modular/Mojo/stdlib` so definitions
should land in the stdlib source clone instead of dead-ending on the
compiled `.mojoc`.

Exit code 0 = expectation met (source file with --stdlib, non-source
without it).

Usage: python3 scripts/test-lsp-goto-def.py [--stdlib]
"""
import json
import os
import subprocess
import sys

MONO = "/home/emil/mono"
LSP = MONO + "/.pixi/envs/default/bin/mojo-lsp-server"
STDLIB = MONO + "/programs/modular/Mojo/stdlib"
TEST_DIR = "/tmp/mojo-lsp-goto-def"
TEST_PATH = TEST_DIR + "/test.mojo"

SOURCE = (
    "from std.collections import Dict\n"
    "\n"
    "\n"
    "def main():\n"
    "    var d: Dict[String, Int] = Dict[String, Int]()\n"
    '    d["a"] = 1\n'
    '    print(d["a"])\n'
)


def main() -> int:
    with_stdlib = "--stdlib" in sys.argv
    os.makedirs(TEST_DIR, exist_ok=True)
    with open(TEST_PATH, "w") as f:
        f.write(SOURCE)

    env = dict(os.environ)
    env["CONDA_PREFIX"] = MONO + "/.pixi/envs/default"
    # LSP_MODULAR_HOME overrides the SDK home so a shadow modular.cfg
    # (import_path pointed at a stdlib source clone) can be tested.
    env["MODULAR_HOME"] = os.environ.get(
        "LSP_MODULAR_HOME", MONO + "/.pixi/envs/default/share/max"
    )

    extra = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    args = [LSP] + (["-I", STDLIB] if with_stdlib else []) + extra
    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None if extra else subprocess.PIPE,
        env=env,
    )

    def send(obj):
        body = json.dumps(obj).encode()
        proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        proc.stdin.flush()

    def recv():
        headers = {}
        while True:
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError(
                    "LSP died. stderr tail:\n"
                    + proc.stderr.read().decode(errors="replace")[-3000:]
                )
            if line in (b"\r\n", b"\n"):
                break
            key, value = line.decode().split(":", 1)
            headers[key.strip().lower()] = value.strip()
        return json.loads(proc.stdout.read(int(headers["content-length"])))

    def recv_response(want_id):
        while True:
            msg = recv()
            if msg.get("id") == want_id:
                return msg

    uri = "file://" + TEST_PATH
    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "processId": os.getpid(), "rootUri": "file://" + TEST_DIR,
        "capabilities": {}}})
    recv_response(1)
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": {"uri": uri, "languageId": "mojo", "version": 1,
                         "text": SOURCE}}})

    # `Dict` in `    var d: Dict[String, Int]` -> line 4, col 11 (0-based)
    send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/definition",
          "params": {"textDocument": {"uri": uri},
                     "position": {"line": 4, "character": 11}}})
    result = recv_response(2).get("result")
    print("DEFINITION:", json.dumps(result))

    send({"jsonrpc": "2.0", "id": 3, "method": "textDocument/hover",
          "params": {"textDocument": {"uri": uri},
                     "position": {"line": 4, "character": 11}}})
    hover = recv_response(3).get("result") or {}
    contents = hover.get("contents", {})
    text = contents.get("value", "") if isinstance(contents, dict) else str(contents)
    print("HOVER:", text[:300])
    proc.terminate()

    target = ""
    if isinstance(result, list) and result:
        result = result[0]
    if isinstance(result, dict):
        target = result.get("targetUri", result.get("uri", ""))
    ok = bool(target) and "mojopkg" not in target and target.endswith(".mojo")
    print("RESOLVED TO SOURCE FILE:", "yes" if ok else "no")
    return 0 if (ok if with_stdlib else not ok) else 1


if __name__ == "__main__":
    sys.exit(main())
