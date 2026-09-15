# Zed Mojo

Mojo language support for Zed, including current-syntax parsing, highlighting,
outline and text objects, snippets, run tasks, `mojo-lsp-server`, and
`lldb-dap`-based debugging.

Parsing uses
[`shuklaayush/tree-sitter-mojo`](https://github.com/shuklaayush/tree-sitter-mojo).
The extension supports macOS and Linux.

## Install for development

1. Install a current Mojo/MAX SDK using Pixi or uv, or activate an existing SDK
   so `mojo` and `mojo-lsp-server` are on the worktree shell `PATH`.
2. In Zed, run **Extensions: Install Dev Extension** and select this directory.
3. Open a worktree whose root contains `pixi.toml`, a Pixi-enabled
   `pyproject.toml`, `uv.lock`, or the activated Mojo environment.

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
      }
    }
  }
}
```

### Stdlib source navigation

The SDK ships the stdlib only as a compiled `std.mojoc`, so the language
server cannot jump to definitions of stdlib symbols (`Dict`, `List`, …).
The `stdlib_source` setting routes the built-in `std` imports to a stdlib
source tree instead: the extension derives a shadow `MODULAR_HOME` whose
`modular.cfg` points `import_path` at that directory, and definitions
resolve into the source files.

```json
{
  "lsp": {
    "mojo-lsp-server": {
      "settings": {
        "stdlib_source": "programs/modular/Mojo/stdlib"
      }
    }
  }
}
```

The path may be worktree-relative (as above) or absolute, and must be the
directory _containing_ the `std` package. The source tree must match the
installed SDK's Mojo version, or definitions and diagnostics will diverge
from the compiled stdlib. The shadow config is derived from the detected
SDK home (`MODULAR_HOME` of a root Pixi project or the activated
environment); if none is found or its `modular.cfg` is missing, the
language server fails to start with the reason in the error.

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

The bundled tasks provide activated-PATH, frozen Pixi, and frozen uv variants.
Top-level `def main` run indicators let you choose among them. Debugging
preserves compiler options before the source, program arguments after it, and
the task environment; optimization and debug-level overrides are ignored so
breakpoints remain reliable.

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

Worktree task bindings take precedence. External-manifest source debugging
also requires that environment to be active when Zed starts.

## Debug

Debugger discovery uses the project SDK: `mojo-lldb-dap` for Pixi/Conda or
`lldb-dap` for a Python wheel. It loads the matching Mojo plugin and
visualizers.

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

Source launch uses `mojo build --no-optimization --debug-level full` and
applies the required ad-hoc signing on macOS.

TCP connections require an already-running adapter and a prebuilt program
visible to it; source builds and local plugin injection are unavailable.

## Development

The complete local check is:

```sh
(cd grammars/mojo && npm ci)
GRAMMAR_DIR=grammars/mojo \
TREE_SITTER=/path/to/tree-sitter \
scripts/check-all.sh
```

See [AGENTS.md](AGENTS.md) for update and validation details.

## License

MIT. See [LICENSE](LICENSE).
