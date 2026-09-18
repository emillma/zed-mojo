# Upstream tracking — zed-mojo

- **Upstream repo**: https://github.com/shuklaayush/zed-mojo
  (remote `upstream-shuklaayush`)
- **Origin** (local fork): git@github.com:emillma/zed-mojo.git
- **Pinned base rev**: `a4dfd0d` (upstream `main` HEAD, "fix: follow upstream
  mojo grammar" — `main` stays pristine here, fast-forward only; the fork's
  old local-work main `afbaac8` was flattened 2026-09-18, its commits live on
  in `mono-local`'s history)
- **Local branch**: `mono-local` (all local work; `main` mirrors
  `upstream-shuklaayush/main`)
- **Local changes**: `mono-local` is 21 commits ahead of the shuklaayush base —
  the repo restructure (Typer CLI replaces check scripts, env tooling to
  `scripts/`, CI repointed), the shadow-home / LSP harness line, and the
  goto-def test path fix. See `git log upstream-shuklaayush/main..mono-local`.
  NOTE: `mono-local` still sits on the pre-flatten base — rebase onto the
  re-pinned `main` at the next sync.
- **Last synced**: 2026-09-18 (main re-pinned; mono-local rebase pending)
