#!/usr/bin/env python3
"""Validate the extension metadata and JSON assets without network access."""

from __future__ import annotations

import json
import pathlib
import re
import tomllib


ROOT = pathlib.Path(__file__).resolve().parent.parent


def load_toml(path: str) -> dict:
    with (ROOT / path).open("rb") as file:
        return tomllib.load(file)


def load_json(path: pathlib.Path) -> object:
    with path.open(encoding="utf-8") as file:
        return json.load(file)


extension = load_toml("extension.toml")
cargo = load_toml("Cargo.toml")
cargo_lock = load_toml("Cargo.lock")
toolchain = load_toml("rust-toolchain.toml")
language = load_toml("languages/mojo/config.toml")

assert extension["version"] == cargo["package"]["version"]
assert cargo["package"]["rust-version"] == toolchain["toolchain"]["channel"]
assert "wasm32-wasip2" in toolchain["toolchain"]["targets"]
zed_api_version = cargo["dependencies"]["zed_extension_api"]
assert isinstance(zed_api_version, str), "zed_extension_api must be a published release"
zed_api_packages = [
    package
    for package in cargo_lock["package"]
    if package["name"] == "zed_extension_api"
]
assert len(zed_api_packages) == 1
assert zed_api_packages[0]["version"] == zed_api_version
assert zed_api_packages[0]["source"].startswith("registry+")
assert extension["language_servers"]["mojo-lsp-server"]["languages"] == ["Mojo"]
assert "mojo-source" in extension["debug_locators"]
assert language["debuggers"] == ["mojo-lldb"]
assert re.fullmatch(r"[0-9a-f]{40}", extension["grammars"]["mojo"]["rev"])
capabilities = extension["capabilities"]
assert capabilities[0] == {
    "kind": "process:exec",
    "command": "/usr/bin/mktemp",
    "args": ["-d", "/tmp/zed-mojo.XXXXXXXXXX"],
}
assert capabilities[1] == {
    "kind": "process:exec",
    "command": "/bin/sh",
    "args": [
        "-c",
        'set -eu\ncd "$1"\nshift\nexec "$@"',
        "zed-mojo-build",
        "*",
        "*",
        "build",
        "--no-optimization",
        "--debug-level",
        "full",
        "**",
    ],
}
assert capabilities[2]["kind"] == "process:exec"
assert capabilities[2]["command"] == "/bin/sh"
assert capabilities[2]["args"][0] == "-c"
assert capabilities[2]["args"][2:] == ["zed-mojo-codesign", "*"]

for relative in extension.get("snippets", []):
    snippets = load_json(ROOT / relative)
    serialized = json.dumps(snippets)
    for stale in ("fn ", "alias ", "__del__", "deinit take"):
        assert stale not in serialized, f"stale Mojo syntax in snippets: {stale}"
tasks = load_json(ROOT / "languages/mojo/tasks.json")
assert isinstance(tasks, list)
assert len(tasks) == 3
assert all(task.get("tags") == ["mojo-main"] for task in tasks)
tasks_by_command = {task["command"]: task for task in tasks}
assert set(tasks_by_command) == {"mojo", "pixi", "uv"}
assert tasks_by_command["mojo"]["args"] == ["run", "$ZED_FILE"]
assert tasks_by_command["pixi"]["args"] == [
    "run",
    "--frozen",
    "--no-progress",
    "--executable",
    "mojo",
    "run",
    "$ZED_FILE",
]
assert tasks_by_command["uv"]["args"] == [
    "run",
    "--frozen",
    "mojo",
    "run",
    "$ZED_FILE",
]
assert all(task["cwd"] == "$ZED_WORKTREE_ROOT" for task in tasks)
load_json(ROOT / extension["debug_adapters"]["mojo-lldb"]["schema_path"])

print("extension metadata and JSON assets are valid")
