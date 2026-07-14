# Dependencies and Integrations

Grindbot shells out to external tools rather than linking them as libraries.
These are the runtime tools the supervisor needs to operate; Rust crate
dependencies live in `Cargo.toml` and are not listed here.

## Runtime tools

- **`gh`** — GitHub CLI. Lists issues, fetches comments, and posts comments.
  Authentication and permissions are delegated to the user's `gh` install (run
  `gh auth login`); repo and author allowlist are configured under `[github]`.
- **`jj`** — Jujutsu. Manages workspaces, revisions, conflicts, rebase,
  bookmarks, push, and `handoff done` validation. Implementer agents also run
  `jj` directly inside their workspaces.
- **`polytoken`** — Spawns implementer sessions via `polytoken new --no-attach`
  (binary configurable as `polytoken.binary`, default `polytoken`). Each session
  exposes an authenticated loopback HTTP API at `http://127.0.0.1:{port}`.
  Endpoints used: `/facet`, `/adventurous-handoff`, `/permission-monitor`,
  `/goal`, `/prompt`, `/state`, `/terminate`.
- **Bash** — Generated stop hooks are Bash scripts that use `cat`/`rm` and read
  `$POLYTOKEN_PROJECT_DIR`. They are spawned by Polytoken, not by the Rust
  supervisor.

## Filesystem

Each implementer workspace contains:

- `.grindbot/base_commit` — base revision for `handoff done` validation.
- `.grindbot/result.json` — agent completion/feedback result.
- `.grindbot/stop_counter` — failed stop-attempt counter.
- `.polytoken/hooks.json` — stop hook config.
- `.polytoken/permissions.yaml` — command deny rules.

The workspaces directory (default `.grindbot-workspaces/`) is appended to the
main repo's `.gitignore`.

Supervisor state is stored at
`$HOME/.local/share/grindbot/{owner}/{repo}/state.json`, or
`./.local/share/grindbot/{owner}/{repo}/state.json` when `HOME` is unset.

## Dev/test requirements

Real `jj` and Bash are required by the handoff and stop-hook tests. Supervisor
integration tests use mock I/O and need no live services. `proptest` and
`tempfile` are dev dependencies (see `Cargo.toml`) used for property tests and
filesystem fixtures.

## Installation

End users can install prebuilt release binaries with
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall grindbot
```

The `[package.metadata.binstall]` section in `Cargo.toml` tells cargo-binstall
where to find release assets. Prebuilt binaries are published automatically when
a `v*` tag is pushed (see the Releasing section in `README.md`). Alternatively,
build from source with `cargo install --path .`.
