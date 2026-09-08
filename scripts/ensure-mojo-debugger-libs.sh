#!/usr/bin/env bash
# Self-heal the pixi Mojo env: the nightly `mojo` conda package ships
# mojo-lldb-dap linked against libbsd but doesn't declare that dependency,
# and libbsd doesn't exist on conda-forge. On NixOS we bridge the gap by
# symlinking the system's libbsd into the env's lib dir (the DAP binary
# finds it via its $ORIGIN/../lib rpath).
#
# Wired up as a pixi activation script for [feature.mojo], so any
# `pixi run`/`pixi shell` in this workspace re-creates the link if the
# environment was wiped or recreated. See zed_mojo/mojo-env-notes.md.

set -euo pipefail

LIB_DIR=".pixi/envs/default/lib"
LINK="$LIB_DIR/libbsd.so.0"

if [ -e "$LINK" ]; then
    exit 0
fi

mkdir -p "$LIB_DIR"

# Prefer the newest libbsd in the nix store; fall back to an empty glob.
src=$(ls -d /nix/store/*-libbsd-*/lib/libbsd.so.0 2>/dev/null | sort -V | tail -1 || true)
if [ -z "$src" ]; then
    echo "mojo env: no /nix/store libbsd found; mojo-lldb-dap will fail to start" >&2
    exit 0
fi

ln -sfn "$src" "$LINK"
echo "mojo env: linked $LINK -> $src" >&2
