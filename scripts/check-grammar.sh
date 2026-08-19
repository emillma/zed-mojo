#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
grammar_dir=${GRAMMAR_DIR:-$repo_root/grammars/mojo}
tree_sitter=${TREE_SITTER:-tree-sitter}

if [ ! -f "$grammar_dir/src/parser.c" ]; then
  echo "grammar parser not found at $grammar_dir" >&2
  exit 1
fi
if ! command -v "$tree_sitter" >/dev/null 2>&1; then
  echo "tree-sitter CLI not found: $tree_sitter" >&2
  exit 1
fi

if ! git -C "$grammar_dir" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "grammar checkout is not a Git worktree: $grammar_dir" >&2
  exit 1
fi
grammar_status=$(git -C "$grammar_dir" status --porcelain)
if [ -n "$grammar_status" ]; then
  echo "grammar checkout has uncommitted changes:" >&2
  echo "$grammar_status" >&2
  exit 1
fi
expected_revision=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["grammars"]["mojo"]["rev"])' "$repo_root/extension.toml")
actual_revision=$(git -C "$grammar_dir" rev-parse HEAD)
if [ "$actual_revision" != "$expected_revision" ]; then
  echo "grammar checkout $actual_revision does not match extension.toml rev $expected_revision" >&2
  exit 1
fi

XDG_CACHE_HOME=${XDG_CACHE_HOME:-/tmp/zed-mojo-tree-sitter-cache}
export XDG_CACHE_HOME

(
  cd "$grammar_dir"
  "$tree_sitter" test
  TREE_SITTER="$tree_sitter" script/check-errors.sh
  npm run lint
  npm test
  cargo fmt --all -- --check
  cargo test --locked
)

echo "grammar corpus, examples, bindings, lint, and revision are valid"
