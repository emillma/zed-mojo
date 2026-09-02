# Mojo in pixi + the Zed extension — environment notes

How the Mojo toolchain is wired into this monorepo for Zed (LSP, tasks,
debugging), and the two NixOS-specific fixes that make it work.

## Layout

- **`pixi.toml` → `[feature.mojo]`** — nightly Mojo toolchain from
  `https://conda.modular.com/max-nightly` (`mojo = "*"`; run
  `pixi update -e default mojo` to pick up a newer nightly).
  Included in the **default** environment because the Zed extension
  hardcodes `.pixi/envs/default` as its toolchain location.
- **`zed_mojo/`** — our forks: `my-zed-mojo` (extension) and
  `my-tree-sitter-mojo` (grammar, with nightly inferred-member syntax
  `.red` / `.float64`). The extension pins the grammar rev in
  `extension.toml`.
- **`zed_mojo/zed_isolated`** — launches a sandboxed Zed (separate config
  via XDG overrides + own TMPDIR) for safely testing dev extensions.

## How the extension finds the toolchain

For a pixi project (worktree has `pixi.toml`), `my-zed-mojo` looks for
binaries in `.pixi/envs/default/bin/` inside the worktree root and spawns
the LSP / debugger with `CONDA_PREFIX`, `MODULAR_HOME` and `PATH` pointed
there. Tasks ("Mojo: run current file (Pixi)") shell out to
`pixi run --executable mojo run <file>`.

## Fix 1: sysroot glibc too old for linking

`mojo build` (used by the debugger to build the target) links a standalone
executable with the conda toolchain's sysroot. The `compilers` package
pulls in `sysroot_linux-64` 2.28, but Mojo's runtime libs are built against
glibc ≥ 2.34 → floods of `undefined reference to pthread_*@GLIBC_2.34`
from `x86_64-conda-linux-gnu-ld`.

**Fix:** `sysroot_linux-64 = "2.39.*"` in `[feature.mojo.dependencies]`.
(conda-forge has 2.39; no conflict with `compilers`.)

Note: plain `mojo run` worked even without this — only standalone
`mojo build` (debug flow) needs it.

## Fix 2: missing libbsd for mojo-lldb-dap

The nightly `mojo` package ships `mojo-lldb-dap` linked against libbsd but
doesn't declare the dependency, and `libbsd` isn't on conda-forge. The
debugger died instantly with
`libbsd.so.0: cannot open shared object file`.

**Fix:** symlink the NixOS system libbsd into the env:

```
.pixi/envs/default/lib/libbsd.so.0 -> /nix/store/…-libbsd-0.12.2/lib/libbsd.so.0
```

This is **not** captured by `pixi.lock` (`.pixi/` is gitignored), so it's
self-healed by `zed_mojo/ensure-mojo-debugger-libs.sh`, wired as a pixi
activation script in `[feature.mojo.activation]`. Any `pixi run` /
`pixi shell` re-creates the link if the env was wiped. The script exits
silently on non-NixOS systems (no /nix/store) — there the proper fix is
upstream: the `mojo` conda package should declare `libbsd`.

## Testing safely

Use `zed_mojo/zed_isolated` (never install untested dev extensions into
your daily editor — Zed is the LLM access point). Details in the script
header; key gotcha: `nix-shell` overrides `TMPDIR` with a session dir it
deletes on exit, which breaks wasi-sdk clang during dev-extension grammar
compiles — the script re-pins `TMPDIR` on the launched command.
