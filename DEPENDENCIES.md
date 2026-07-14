# Dependencies and Integrations

Versions in `Cargo.toml`; resolved graph in `Cargo.lock`.

## Rust dependencies

| Crate | Version/features | Role |
|---|---|---|
| `clap` | `4`, `derive` | CLI parsing/dispatch. |
| `serde` | `1`, `derive` | Serialization. |
| `serde_json` | `1` | Result files, state, HTTP payloads. |
| `tokio` | `1`, `full` | Async runtime, subprocesses, timers. |
| `reqwest` | `0.12`, `json` | Polytoken session HTTP API. |
| `tracing` | `0.1` | Structured logging. |
| `tracing-subscriber` | `0.3`, `env-filter` | Log subscriber, `RUST_LOG`. |
| `chrono` | `0.4`, `serde` | UTC timestamps, GitHub dates. |
| `toml` | `0.8` | `grindbot.toml` parsing. |
| `anyhow` | `1` | Error context. |
| `async-trait` | `0.1` | Async I/O traits. |

All required; no Cargo features defined by Grindbot.

## Dev/test (`[dev-dependencies]`)

| Crate | Role |
|---|---|
| `proptest` `1` | Property tests for planner/filters. |
| `tempfile` `3` | Temp repos and filesystem fixtures. |

Real `jj` and Bash are required by handoff/stop-hook tests. Supervisor integration tests use mock I/O and need no live services.

## Runtime tools

- **`gh`** — lists issues, fetches comments, posts comments (`src/io/github.rs`). Auth/permissions delegated to the user's `gh` install; repo and author allowlist in `[github]`.
- **`jj`** — workspaces, revisions, conflicts, rebase, bookmarks, push, handoff validation (`src/io/jj.rs`). Agents also run `jj` directly.
- **`polytoken`** (config: `binary`, default `polytoken`) — spawns sessions via `polytoken new --no-attach`; each session exposes an authenticated loopback HTTP API at `http://127.0.0.1:<port>` (`src/io/polytoken.rs`). Endpoints used: `/facet`, `/adventurous-handoff`, `/permission-monitor`, `/goal`, `/prompt`, `/state`, `/terminate`.
- **Bash** — generated stop hooks are Bash scripts using `cat`/`rm` and `$POLYTOKEN_PROJECT_DIR` (`src/prompt.rs`). Spawned by Polytoken, not the Rust supervisor.

## Filesystem

Implementer workspaces contain (`src/workspace.rs`, `src/handoff.rs`):

- `.grindbot/base_commit` — base revision for `handoff done` validation.
- `.grindbot/result.json` — agent completion/feedback.
- `.grindbot/stop_counter` — failed stop-attempt counter.
- `.polytoken/hooks.json` — stop hook config.
- `.polytoken/permissions.yaml` — command deny rules.

The workspaces directory (default `.grindbot-workspaces/`) is appended to the main repo's `.gitignore`. Supervisor state: `$HOME/.local/share/grindbot/{owner}/{repo}/state.json`, or `./.local/share/grindbot/{owner}/{repo}/state.json` when `HOME` is unset (`src/state_file.rs`).

## Source references

- `Cargo.toml`, `Cargo.lock`
- `src/io/{github,jj,polytoken}.rs`, `src/io/mod.rs`
- `src/workspace.rs`, `src/prompt.rs`, `src/state_file.rs`, `src/config.rs`
- `tests/common/`, `tests/handoff_done.rs`, `tests/stop_hook.rs`
