#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
grammar_dir=${GRAMMAR_DIR:-$repo_root/grammars/mojo}
tree_sitter=${TREE_SITTER:-tree-sitter}

if [ ! -f "$grammar_dir/src/parser.c" ]; then
  echo "grammar parser not found at $grammar_dir" >&2
  exit 1
fi

XDG_CACHE_HOME=${XDG_CACHE_HOME:-/tmp/zed-mojo-tree-sitter-cache}
export XDG_CACHE_HOME
cd "$grammar_dir"

for fixture in "$repo_root"/tests/fixtures/*.mojo; do
  for query in "$repo_root"/languages/mojo/*.scm; do
    "$tree_sitter" query "$query" "$fixture" --quiet
  done
  "$tree_sitter" parse --quiet "$fixture"
done
echo "all Mojo queries compile and every syntax fixture parses"
