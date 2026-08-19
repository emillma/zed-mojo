# Repository guide

This is a Rust Zed extension for the current Mojo language. Treat the adjacent
`../modular` checkout as the syntax, LSP, CLI, and debugger source of truth;
model memory and other Mojo extensions can be stale. Read `../modular/AGENTS.md`
before using that repository and obey any nearer instructions.

Build the extension for `wasm32-wasip2`. A host-only build does not validate a
Zed extension.

## Authorship

- Local Git commits use `Ayush Shukla <shuklaayush247@gmail.com>`.
- Cargo and extension metadata use `Ayush Shukla <ayush@shuklaayu.sh>`.
- Preserve upstream license notices when copying material; prefer deriving
  syntax fixtures and snippets directly from the Modular language reference.

## Updating every dependency surface

First inspect and preserve existing changes, then record the current pins:

```sh
git status --short
git config user.name
git config user.email
rustc --version
git ls-remote https://github.com/zed-industries/zed.git refs/heads/main
git ls-remote https://github.com/shuklaayush/tree-sitter-mojo.git refs/heads/main
```

### Rust and Cargo

1. Check the latest stable patch release with `rustup check` and the official
   Rust release channel.
2. Set the exact release in both `rust-toolchain.toml` and
   `package.rust-version` in `Cargo.toml`. Keep `rustfmt`, `clippy`, and the
   `wasm32-wasip2` target.
3. Run `cargo update`. The unversioned Zed Git dependency should advance to
   current Zed `main`.
4. Confirm the `zed_extension_api` source SHA in `Cargo.lock` equals the live
   Zed SHA. Never hand-edit the lockfile.
5. Adapt `src/mojo.rs` to the pinned API. Check Zed core behavior as well as
   public types: Zed merges the worktree environment and reapplies
   `LspSettings.binary` after the extension callback.
6. Finish with `cargo update --dry-run`; it should report zero updates.

Install the pinned toolchain if needed:

```sh
toolchain="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)"
rustup toolchain install "$toolchain" \
  --component clippy,rustfmt \
  --target wasm32-wasip2
```

### Mojo grammar

`extension.toml` uses `[grammars.mojo].rev`, not the legacy `commit` alias.
Updating the SHA alone is insufficient:

1. Fetch `shuklaayush/tree-sitter-mojo` and inspect `grammar.js`, generated
   parser files, `src/node-types.json`, scanner changes, and corpus tests.
2. Compare syntax against current valid sources under:
   - `../modular/mojo/stdlib/std`
   - `../modular/mojo/stdlib/benchmarks`
   - `../modular/mojo/examples`
   - `../modular/mojo/docs/code`
3. Require every syntactically valid source, including semantic compile-fail
   tests, to parse without tree-sitter `ERROR` or `MISSING` nodes. Exclude only
   fixtures that deliberately contain malformed syntax for parser recovery.
4. Regenerate `src/grammar.json`, `src/node-types.json`, `src/parser.c`, and
   tree-sitter support headers with the grammar's pinned CLI version.
5. Run the grammar corpus, parse the current Modular source sets, and compile
   every `languages/mojo/*.scm` query against the exact generated parser.
6. Treat the grammar's Node and Rust bindings as dependency surfaces too. Run
   `npm outdated`, update to the newest peer-compatible packages, and run
   `cargo update --dry-run`. Do not force a major upgrade through incompatible
   peer dependencies merely to make `npm outdated` empty.
7. Commit and push the grammar change first. Only then set `rev` to that
   reachable 40-character commit SHA and rerun the extension checks.

Current Mojo syntax must drive snippets and queries. In particular, use `def`,
`comptime`, `__deinit__`, and `deinit move`; do not reintroduce deprecated `fn`,
`alias`, `__del__`, or `deinit take` examples. Do not add `folds.scm` or
`embedding.scm`; the pinned Zed query loader does not load those names.

### GitHub Actions and other tooling

Actions are dependencies too. Check each workflow action's latest official
release, resolve the tag to a full immutable SHA with `git ls-remote`, and keep
the readable release tag in a comment. Update the pinned tree-sitter CLI in CI
when the grammar generator changes. Avoid unpinned action tags.

## Runtime invariants

- Use the language-server ID `mojo-lsp-server` and Zed's standard
  `binary.path`, `binary.arguments`, and `binary.env` settings.
- Project SDKs precede global PATH. Support root default Pixi and uv `.venv`
  layouts; activated Conda is PATH-based. Do not run unqualified `conda run`.
- Never add `--skip-docstring-checks`; current Mojo uses opt-in
  `-check-docstrings`.
- Prefer the SDK `lldb-dap`, pass `--repl-mode variable`, load the Mojo LLDB
  plugin/visualizers, and keep legacy `mojo-lldb-dap` only as fallback.
- A `mojoFile` must be compiled with `--no-optimization --debug-level full`
  before launch and codesigned with `get-task-allow` on macOS. LLDB cannot run a
  `.mojo` source file directly.
- Keep manifest `process:exec` capabilities synchronized with the exact
  temporary-directory, build, and codesign commands. Zed rejects undeclared
  commands before spawning them; do not broaden the argument patterns without
  a concrete need.
- Keep the `mojo-source` debug locator synchronized with every bundled run task
  shape so runnables can enter source debugging directly.
- A configured TCP adapter is external: connect with `command: None` rather
  than spawning an adapter that was never told to listen.

## Validation

With the exact grammar checkout available at `grammars/mojo`:

```sh
(cd grammars/mojo && npm ci)
GRAMMAR_DIR=grammars/mojo \
TREE_SITTER=/path/to/tree-sitter \
scripts/check-all.sh
```

The script runs:

```sh
cargo fmt --all -- --check
cargo check --locked --target wasm32-wasip2
cargo clippy --locked --target wasm32-wasip2 --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release --target wasm32-wasip2
cargo metadata --locked --no-deps --format-version 1
python3 scripts/check-extension.py
scripts/check-grammar.sh
scripts/check-queries.sh
git diff --check
```

The grammar gate verifies that a local Git checkout exactly matches the
manifest revision, then runs the focused tree-sitter corpus, bundled current
examples, Node lint/binding test, and Rust binding test. Also run
`cargo update --dry-run`, parse the current Modular source sets, and install
the repository as a Zed dev extension. Smoke-test highlighting,
top-level runnables, direct/Pixi/uv tasks, LSP initialization, prebuilt launch,
source launch, attach, SDK plugin summaries, and external TCP configuration.
State clearly which live SDK/UI checks were actually run.
