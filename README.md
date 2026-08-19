# Zed Mojo

Mojo language support for Zed, including current-syntax parsing, highlighting,
outline and text objects, snippets, run tasks, `mojo-lsp-server`, and
`lldb-dap`-based debugging.

The extension follows the Mojo sources in the adjacent Modular repository and
uses the maintained
[`shuklaayush/tree-sitter-mojo`](https://github.com/shuklaayush/tree-sitter-mojo)
grammar fork. Mojo currently supports macOS and Linux.

## Install for development

1. Install a current Mojo/MAX SDK using Pixi or uv, or activate an existing SDK
   so `mojo` and `mojo-lsp-server` are on the worktree shell `PATH`.
2. In Zed, run **Extensions: Install Dev Extension** and select this directory.
3. Open a worktree whose root contains `pixi.toml`, a Pixi-enabled
   `pyproject.toml`, `uv.lock`, or the activated Mojo environment.

The extension pins the newest published Zed extension API supported by both
Zed Stable and Zed Preview. Do not replace it with Zed `main`: unpublished API
versions load only in Zed Dev and Nightly.

Zed's extension registry already assigns the public `mojo` ID to
`vadim-su/zed_mojo`. This repository can override that extension locally for
development, but publishing these changes under the same ID requires an
upstream contribution or repository handoff.

## Language server

Discovery order is:

1. `lsp.mojo-lsp-server.binary.path` configured in Zed;
2. the default environment of a root Pixi project;
3. the `.venv` of a root uv/virtual-environment project;
4. `mojo-lsp-server` from the worktree shell `PATH`.

Project discovery is worktree-root based. For a named Pixi environment, a
nested project, or another SDK layout, activate the environment or configure
the executable explicitly:

```json
{
  "lsp": {
    "mojo-lsp-server": {
      "binary": {
        "path": "/absolute/path/to/mojo-lsp-server",
        "arguments": ["-I", "/absolute/path/to/mojo/modules"],
        "env": {
          "CUSTOM_VARIABLE": "value"
        }
      },
      "initialization_options": {},
      "settings": {}
    }
  }
}
```

The extension does not add the obsolete `--skip-docstring-checks` option. The
current server's docstring check is opt-in with `-check-docstrings`.

## Format and run

For an activated SDK:

```json
{
  "languages": {
    "Mojo": {
      "formatter": {
        "external": {
          "command": "mojo",
          "arguments": ["format", "-"]
        }
      }
    }
  }
}
```

Pixi projects can use `pixi run --frozen --no-progress --executable mojo
format -`; uv projects can use `uv run --frozen mojo format -`.

The bundled tasks provide activated-PATH, frozen Pixi, and frozen uv variants
for running the current file. A top-level `def main` receives the `mojo-main`
runnable tag; select the task matching the project's SDK when clicking its run
indicator. The Pixi and uv variants run from the worktree root so Mojo package
imports and root project discovery agree. The `mojo-source` debug locator
converts all three task shapes into source-debug scenarios. It carries Mojo
compiler options before the source into `buildArgs`, except optimization and
debug-level overrides that would make breakpoints unreliable. It preserves
arguments after the source as debuggee arguments and applies the task
environment to both compilation and the debuggee, so project tasks such as
`mojo run -I package "$ZED_FILE" --flag` keep their import path and program
arguments while debugging.

If Pixi cannot discover the intended manifest from the worktree root—for
example, the manifest is nested inside the worktree or lives in an unrelated
directory—override the runnable for that worktree in `.zed/tasks.json`:

```json
[
  {
    "label": "Mojo: run current file (external Pixi manifest)",
    "command": "pixi",
    "args": [
      "run",
      "--manifest-path",
      "/path/to/pixi.toml",
      "--frozen",
      "--no-progress",
      "--executable",
      "mojo",
      "run",
      "$ZED_FILE"
    ],
    "cwd": "$ZED_WORKTREE_ROOT",
    "tags": ["mojo-main"]
  }
]
```

Worktree task bindings take precedence over the extension defaults. An
external-manifest run override does not configure source debugging; activate
that Pixi environment before launching Zed if the debugger also needs it.

## Debug

The debugger prefers the project SDK's configured adapter, normally
`mojo-lldb-dap` for a Pixi/Conda SDK or `lldb-dap` for a Python wheel. It loads
the matching Mojo LLDB plugin and visualizers, supplies the SDK environment,
and deliberately does not select an unrelated system `lldb-dap`.

Debug a prebuilt executable:

```json
[
  {
    "adapter": "mojo-lldb",
    "label": "Mojo binary",
    "request": "launch",
    "program": "${ZED_WORKTREE_ROOT}/build/my-program",
    "args": []
  }
]
```

Debug a source file directly:

```json
[
  {
    "adapter": "mojo-lldb",
    "label": "Mojo source",
    "request": "launch",
    "mojoFile": "${ZED_WORKTREE_ROOT}/main.mojo",
    "buildArgs": []
  }
]
```

Source launch builds an executable in a private, unique temporary directory
with `mojo build --no-optimization --debug-level full`, using the configured
`cwd`; on macOS it also applies ad-hoc signing with the `get-task-allow`
entitlement. The manifest declares narrowly matched `process:exec`
capabilities for temporary-directory creation, building, and signing; source
launch fails if you remove process execution from Zed's
`granted_extension_capabilities`.
Attach configurations accept `pid`, `program`, core-file, command, or
remote-target fields from the schema. Configurations with `tcp_connection`
connect to an already-running adapter and do not spawn a second process. They
must use a prebuilt `program` visible to that adapter; local `mojoFile` builds
and local SDK plugin injection are intentionally rejected for external TCP.

## Development

The complete local check is:

```sh
(cd grammars/mojo && npm ci)
GRAMMAR_DIR=grammars/mojo \
TREE_SITTER=/path/to/tree-sitter \
scripts/check-all.sh
```

It checks formatting, the `wasm32-wasip2` target, Clippy, host tests, release
Wasm, Cargo metadata, TOML/JSON assets, the exact grammar revision, grammar
corpus and bindings, bundled current examples, every supported tree-sitter
query, current-Mojo syntax fixtures, and whitespace errors. See
[AGENTS.md](AGENTS.md) for the dependency and grammar update procedure.

## License

MIT. See [LICENSE](LICENSE).
