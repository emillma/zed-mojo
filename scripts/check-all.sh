#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

cargo fmt --all -- --check
cargo check --locked --target wasm32-wasip2
cargo clippy --locked --target wasm32-wasip2 --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release --target wasm32-wasip2
cargo metadata --locked --no-deps --format-version 1 >/dev/null
python3 scripts/check-extension.py
scripts/check-grammar.sh
scripts/check-queries.sh
git diff --check
