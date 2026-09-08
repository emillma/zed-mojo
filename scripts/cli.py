#!/usr/bin/env python3
"""Zed Mojo extension checks (Typer CLI replacing check-all.sh / check-grammar.sh / check-queries.sh).

Subcommands:
  check-all     full gate: cargo fmt/check/clippy/test/build + metadata, the
                extension-metadata validator, grammar + query checks, git diff --check.
  check-grammar validate the pinned Mojo grammar checkout (corpus, examples,
                bindings, lint) and that its revision matches extension.toml.
  check-queries compile every Mojo query against each syntax fixture and parse
                every fixture.

Env knobs: GRAMMAR_DIR (default <repo>/grammars/mojo), TREE_SITTER (default
`tree-sitter`).
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

import typer

app = typer.Typer(help=__doc__, no_args_is_help=True)

SCRIPTS_DIR = Path(__file__).resolve().parent          # programs/zed-mojo/scripts
REPO_ROOT = SCRIPTS_DIR.parent                         # programs/zed-mojo
MONO_ROOT = SCRIPTS_DIR.parents[2]                      # mono root


def _grammar_dir() -> Path:
    return Path(os.environ.get("GRAMMAR_DIR", str(REPO_ROOT / "grammars" / "mojo")))


def _tree_sitter() -> str:
    return os.environ.get("TREE_SITTER", "tree-sitter")


def _run(cmd: list[str], cwd: Path | None = None, env: dict | None = None) -> int:
    """Run a command; stream output to the terminal and return its exit code."""
    proc = subprocess.run(cmd, cwd=str(cwd) if cwd else None, env=env)
    return proc.returncode


def _fail(msg: str) -> typer.Exit:
    print(msg, file=sys.stderr)
    return typer.Exit(1)


def check_grammar() -> int:
    grammar_dir = _grammar_dir()
    tree_sitter = _tree_sitter()

    if not (grammar_dir / "src" / "parser.c").is_file():
        raise _fail(f"grammar parser not found at {grammar_dir}")
    if not shutil.which(tree_sitter):
        raise _fail(f"tree-sitter CLI not found: {tree_sitter}")

    if subprocess.run(["git", "-C", str(grammar_dir), "rev-parse", "--is-inside-work-tree"],
                      capture_output=True).returncode != 0:
        raise _fail(f"grammar checkout is not a Git worktree: {grammar_dir}")
    status = subprocess.run(["git", "-C", str(grammar_dir), "status", "--porcelain"],
                           capture_output=True, text=True).stdout.strip()
    if status:
        print("grammar checkout has uncommitted changes:", file=sys.stderr)
        print(status, file=sys.stderr)
        raise typer.Exit(1)

    with (REPO_ROOT / "extension.toml").open("rb") as f:
        expected_revision = tomllib.load(f)["grammars"]["mojo"]["rev"]
    actual_revision = subprocess.run(["git", "-C", str(grammar_dir), "rev-parse", "HEAD"],
                                     capture_output=True, text=True).stdout.strip()
    if actual_revision != expected_revision:
        raise _fail(f"grammar checkout {actual_revision} does not match extension.toml rev {expected_revision}")

    env = {**os.environ, "XDG_CACHE_HOME": os.environ.get("XDG_CACHE_HOME", "/tmp/zed-mojo-tree-sitter-cache"),
           "TREE_SITTER": tree_sitter}
    for cmd in (
        [tree_sitter, "test"],
        ["script/check-errors.sh"],          # the grammar clone's own copy (separate from src/tree-sitter-mojo)
        ["npm", "run", "lint"],
        ["npm", "test"],
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "test", "--locked"],
    ):
        rc = _run(cmd, cwd=grammar_dir, env=env)
        if rc != 0:
            return rc

    print("grammar corpus, examples, bindings, lint, and revision are valid")
    return 0


def check_queries() -> int:
    grammar_dir = _grammar_dir()
    tree_sitter = _tree_sitter()

    if not (grammar_dir / "src" / "parser.c").is_file():
        raise _fail(f"grammar parser not found at {grammar_dir}")

    env = {**os.environ, "XDG_CACHE_HOME": os.environ.get("XDG_CACHE_HOME", "/tmp/zed-mojo-tree-sitter-cache")}
    fixtures = sorted((REPO_ROOT / "tests" / "fixtures").glob("*.mojo"))
    queries = sorted((REPO_ROOT / "languages" / "mojo").glob("*.scm"))

    for fixture in fixtures:
        for query in queries:
            rc = _run([tree_sitter, "query", str(query), str(fixture), "--quiet"], cwd=grammar_dir, env=env)
            if rc != 0:
                return rc
        rc = _run([tree_sitter, "parse", "--quiet", str(fixture)], cwd=grammar_dir, env=env)
        if rc != 0:
            return rc

    print("all Mojo queries compile and every syntax fixture parses")
    return 0


@app.command()
def check_all() -> None:
    """Run the full extension gate (cargo + metadata + grammar + queries)."""
    steps = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "check", "--locked", "--target", "wasm32-wasip2"],
        ["cargo", "clippy", "--locked", "--target", "wasm32-wasip2", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "test", "--locked"],
        ["cargo", "build", "--locked", "--release", "--target", "wasm32-wasip2"],
    ]
    for cmd in steps:
        rc = _run(cmd, cwd=REPO_ROOT)
        if rc != 0:
            raise typer.Exit(rc)

    # cargo metadata --no-deps (stdout discarded), then the extension validator.
    proc = subprocess.run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
                         cwd=str(REPO_ROOT), stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    if proc.returncode != 0:
        raise typer.Exit(proc.returncode)

    rc = _run([sys.executable, str(SCRIPTS_DIR / "check-extension.py")], cwd=REPO_ROOT)
    if rc != 0:
        raise typer.Exit(rc)

    for fn in (check_grammar, check_queries):
        try:
            rc = fn()
        except typer.Exit as e:
            raise e
        if rc != 0:
            raise typer.Exit(rc)

    rc = _run(["git", "diff", "--check"], cwd=REPO_ROOT)
    raise typer.Exit(rc)


@app.command("check-grammar")
def check_grammar_cmd() -> None:
    """Validate the pinned Mojo grammar checkout and its revision."""
    rc = check_grammar()
    raise typer.Exit(rc)


@app.command("check-queries")
def check_queries_cmd() -> None:
    """Compile every Mojo query against each fixture and parse every fixture."""
    rc = check_queries()
    raise typer.Exit(rc)


@app.command("ensure-mojo-debugger-libs")
def ensure_mojo_debugger_libs() -> None:
    """Self-heal the pixi Mojo env: symlink the system libbsd into the env's lib dir.

    The nightly `mojo` conda package ships mojo-lldb-dap linked against libbsd but
    doesn't declare that dependency, and libbsd isn't on conda-forge. On NixOS we
    bridge the gap by symlinking the system's libbsd into the env (the DAP binary
    finds it via its $ORIGIN/../lib rpath). Idempotent — no-op if already linked.
    """
    import glob

    lib_dir = MONO_ROOT / ".pixi" / "envs" / "default" / "lib"
    link = lib_dir / "libbsd.so.0"
    if link.exists() or link.is_symlink():
        return  # already linked (or present)

    lib_dir.mkdir(parents=True, exist_ok=True)
    # Prefer the newest libbsd in the nix store (highest version number); fall
    # back to an empty glob. The nix store path embeds the version after the
    # "libbsd-" marker.
    candidates = []
    for p in glob.glob("/nix/store/*-libbsd-*/lib/libbsd.so.0"):
        m = re.search(r"libbsd-(\d+(?:\.\d+)*)", p)
        if m:
            candidates.append((tuple(int(x) for x in m.group(1).split(".")), p))
    src = max(candidates, key=lambda c: c[0])[1] if candidates else None
    if not src:
        print("mojo env: no /nix/store libbsd found; mojo-lldb-dap will fail to start", file=sys.stderr)
        return  # exit 0, matching the original script's non-fatal behaviour

    link.symlink_to(src)
    print(f"mojo env: linked {link} -> {src}", file=sys.stderr)


if __name__ == "__main__":
    app()
