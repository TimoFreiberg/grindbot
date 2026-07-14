# Configuration

Grindbot reads a TOML config for the supervisor, GitHub repo, Polytoken executable, and workspace layout.

## Selecting the file

```bash
grindbot supervise                         # ./grindbot.toml
grindbot supervise --config path/to/file.toml
grindbot supervise -c path/to/file.toml
```

Paths resolve from the current working directory. Config is loaded once at startup; restart to apply changes. Template: [`config.example.toml`](config.example.toml).

## Schema

```toml
[github]
owner = "your-org"
repo = "your-repo"
# GitHub usernames whose issues may be implemented (not email addresses)
allowlist = ["alice", "bob"]

[supervisor]
max_parallelism = 2
poll_interval_secs = 30
base_branch = "main"
merge_lock_timeout_secs = 1800
final_check_command = "cargo test"
stall_threshold_cycles = 5
log_interval_secs = 300

[polytoken]
binary = "polytoken"
max_tool_turns = 200

[workspace]
prefix = "grindbot"
workspaces_dir = ".grindbot-workspaces"
```

### `[github]`

| Key | Type | Required | Description |
|---|---|---:|---|
| `owner` | string | Yes | GitHub org or user. |
| `repo` | string | Yes | GitHub repository name. |
| `allowlist` | array of strings | Yes | GitHub usernames whose issues may be implemented. |

Issues are listed/fetched via `gh`; eligible only when the author is allowlisted and planner filters pass.

### `[supervisor]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `max_parallelism` | integer | `2` | Max concurrent implementer sessions. |
| `poll_interval_secs` | integer | `30` | Delay between poll cycles. |
| `base_branch` | string | `"main"` | Jujutsu bookmark used as the merge target. |
| `merge_lock_timeout_secs` | integer | `1800` | Age threshold used when recovering an inactive stale `.grindbot/merge.lock`. |
| `final_check_command` | string | absent | Optional command run in the implementation workspace before pushing main. |
| `stall_threshold_cycles` | integer | `5` | Consecutive poll cycles with no token growth before a stuck warning is emitted. Effective wall-clock time depends on `poll_interval_secs`. |
| `log_interval_secs` | integer | `300` | Minimum seconds between routine info-level cycle summaries and progress logs. Stall warnings still fire every cycle. |

### `[polytoken]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `binary` | string | `"polytoken"` | Executable name/path. |
| `max_tool_turns` | integer | `200` | Turn limit for implementer sessions. Conflict resolvers use a fixed 50. |

### `[workspace]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `prefix` | string | `"grindbot"` | Managed jj workspace name prefix. |
| `workspaces_dir` | string | `".grindbot-workspaces"` | Directory for managed workspaces. Added to `.gitignore` if absent. |

The configured workspace directory is ignored in the main repository, but each JJ workspace is a separate working-tree root with its own `.gitignore`. Add the generated runtime paths to every managed workspace's `.gitignore` manually:

```gitignore
.grindbot/
.polytoken/
```

`grindbot doctor` warns when it finds an existing managed workspace without both entries. This is advisory only. Grindbot does not create or edit per-workspace `.gitignore` files and does not commit ignore-rule changes.

## Environment

- `RUST_LOG` — tracing filter; defaults to `info` when unset. Can be overridden by `--quiet`/`--verbose` CLI flags.
- `HOME` — determines the state file path `$HOME/.local/share/grindbot/{owner}/{repo}/state.json`; falls back to `./.local/share/grindbot/{owner}/{repo}/state.json` when unset.
- `POLYTOKEN_PROJECT_DIR` — consumed by the generated stop hook to locate workspace result/counter files (set by Polytoken from the session working directory).
- CWD — repo context, default config location, and handoff workspace-root discovery (walks up to the nearest `.jj/`).

## Generated and persistent files

Per managed workspace:

- `.grindbot/base_commit` — revision the implementer must produce a commit ahead of.
- `.grindbot/result.json` — versioned approved manifest evidence or `needs-feedback` result.
- `.grindbot/merge.lock` — atomically acquired supervisor merge ownership metadata; released after handled merge paths.
- `.grindbot/stop_counter` — failed stop-hook attempts (reset on handoff).
- `.polytoken/hooks.json` — stop hook (gated for implementers; always-stop for conflict resolvers).
- `.polytoken/permissions.yaml` — deny rules (`rm -r*`, `git push`, `jj git`, `jj abandon`; filesystem write-denied under `.grindbot/` and `.polytoken/`).

Supervisor state at the `HOME`-derived path above. Missing/malformed/version-mismatched files are discarded and replaced with fresh state (version `1`); saves are atomic (temp + rename).

## Validation

`Config::validate()` enforces the following. Any failure aborts startup with a descriptive error.

- `owner` must not be empty.
- `repo` must not be empty.
- `allowlist` must contain at least one username.
- `max_parallelism` must be >= 1.
- `poll_interval_secs` must be >= 1.
- `base_branch` must not be empty.
- `stall_threshold_cycles` must be >= 1.
- `log_interval_secs` must be >= 1.
- `workspace.prefix` must not be empty.

The supervisor does not merge or push from the implementer prompt; the base bookmark is managed by the supervisor's jj flow.
