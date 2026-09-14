#!/usr/bin/env python3
"""Check mojo-lsp-server goto-definition resolution for a stdlib symbol.

Spawns the mono pixi env's mojo-lsp-server, opens a tiny Mojo file using
`Dict`, and prints what `textDocument/definition` resolves to. Pass
`--stdlib` to add `-I <mono>/programs/modular/Mojo/stdlib` so definitions
should land in the stdlib source clone instead of dead-ending on the
compiled `.mojoc`.

Exit code 0 = expectation met: a `.mojo` source file with `--stdlib` or
`--shadow`, a dead-end without them.

`--shadow` derives a shadow SDK home from the real modular.cfg with
import_path repointed at the stdlib clone (what the extension's
`stdlib_source` setting does), so the full mechanism is checkable from the
CLI without Zed.

Usage: python3 scripts/test-lsp-goto-def.py [--stdlib | --shadow] [-- extra LSP args]
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

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


def make_shadow_home() -> str:
    """Derive a shadow SDK home whose import_path is the stdlib source clone.

    Mirrors what the extension does for `stdlib_source`: copy the project
    SDK's modular.cfg into a temp dir with import_path repointed at STDLIB.
    """
    real_cfg = MONO + "/.pixi/envs/default/share/max/modular.cfg"
    home = tempfile.mkdtemp(prefix="zed-mojo-shadow.")
    with open(real_cfg) as f:
        text = f.read()
    text = re.sub(r"(?m)^import_path = .*$", "import_path = " + STDLIB, text)
    with open(home + "/modular.cfg", "w") as f:
        f.write(text)
    return home


def main() -> int:
    with_stdlib = "--stdlib" in sys.argv
    with_shadow = "--shadow" in sys.argv
    os.makedirs(TEST_DIR, exist_ok=True)
    with open(TEST_PATH, "w") as f:
        f.write(SOURCE)

    env = dict(os.environ)
    env["CONDA_PREFIX"] = MONO + "/.pixi/envs/default"
    # LSP_MODULAR_HOME overrides the SDK home so a shadow modular.cfg
    # (import_path pointed at a stdlib source clone) can be tested;
    # --shadow derives one from the real cfg for the full CLI check.
    env["MODULAR_HOME"] = os.environ.get(
        "LSP_MODULAR_HOME", MONO + "/.pixi/envs/default/share/max"
    )
    if with_shadow:
        env["MODULAR_HOME"] = make_shadow_home()

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
    if with_shadow:
        shutil.rmtree(env["MODULAR_HOME"], ignore_errors=True)
    expect_source = with_stdlib or with_shadow
    return 0 if (ok if expect_source else not ok) else 1


if __name__ == "__main__":
    sys.exit(main())
