# Grindbot

Autonomous issue implementation supervisor. Grindbot pulls issues from GitHub, spawns Polytoken coding agents in isolated jj workspaces at configurable parallelism, and merges completed work back to main — all without human intervention.

## Installation

```sh
cargo install --path .
```

## Configuration

Create a `grindbot.toml` in your repo root (see `config.example.toml`):

```toml
[github]
owner = "your-org"
repo = "your-repo"
allowlist = ["alice", "bob"]

[supervisor]
max_parallelism = 2
poll_interval_secs = 30
base_branch = "main"

[polytoken]
binary = "polytoken"
max_tool_turns = 200

[workspace]
prefix = "grindbot"
workspaces_dir = ".grindbot-workspaces"
```

## Usage

```sh
# Run the supervisor daemon
grindbot supervise --config grindbot.toml

# Signal completion (called by implementer agents inside workspaces)
grindbot handoff done --commit <hash>
grindbot handoff needs-feedback --message "Need more info about X"
```

## Logging

Grindbot uses `tracing` for structured logs. Control verbosity with:

- `--quiet` / `-q` — warnings only
- `--verbose` / `-v` — debug output
- `--verbose` / `-vv` — trace output (everything)
- `RUST_LOG=grindbot=debug` — env var override (takes precedence)

## Commands

```sh
grindbot supervise --config grindbot.toml    # Run the supervisor daemon
grindbot supervise --dry-run                 # Preview actions without executing
grindbot status --config grindbot.toml       # Show current state
grindbot doctor --config grindbot.toml       # Check dependencies
grindbot handoff done --commit <hash>        # Signal completion
grindbot handoff needs-feedback --message "..."  # Request feedback
grindbot --version                           # Print version
```

## How It Works

```
┌─────────────────────────────────────────────────────┐
│                   Supervisor Loop                    │
│                                                      │
│  1. Gather state (gh, jj, polytoken, filesystem)     │
│  2. core::plan(state) -> Vec<Action>                 │
│  3. Execute actions via I/O traits                   │
│  4. Wait for next poll cycle                         │
│  └──────────────────────────────────────────────►    │
└─────────────────────────────────────────────────────┘
```

### Architecture: Pure Core + I/O Layer

The codebase is split into a **pure decision core** (no I/O, fully property-testable) and an **I/O layer** (traits with real implementations and test mocks). The supervisor loop gathers state from I/O, feeds it to the core, and executes the returned actions.

### Issue Lifecycle

1. **Eligible:** Issue author is on the allowlist, last activity was by a human, not currently being implemented, not already completed.
2. **In progress:** Supervisor creates a jj workspace, spawns a Polytoken session in plan mode, and sends the issue as a prompt.
3. **Done:** Implementer calls `grindbot handoff done --commit <hash>`. Supervisor rebases onto main, resolves conflicts if needed, pushes, posts a comment, and cleans up.
4. **Needs feedback:** Implementer calls `grindbot handoff needs-feedback --message <text>`. Supervisor posts the feedback as a comment and cleans up.
5. **Crash:** If the daemon dies without a result file, the supervisor cleans up the workspace and the task remains eligible.

### Merge Conflict Resolution

When a rebase produces conflicts, the supervisor spawns a one-shot conflict resolution agent with the `jj-resolve-conflicts` skill. If resolution fails after 3 attempts, the supervisor posts a comment explaining the persistent conflict.

## Requirements

- [Polytoken](https://docs.polytoken.dev/introduction/) 0.5.0+
- [Jujutsu](https://github.com/martinvonz/jj) (jj)
- [GitHub CLI](https://cli.github.com/) (gh)

## Troubleshooting

- **`gh issue list failed`**: Run `gh auth login` to authenticate, verify `gh repo view {owner}/{repo}` works.
- **`jj workspace add failed`**: Ensure you're in a jj repo (`jj log` works), and the workspace directory doesn't already exist.
- **`polytoken new failed`**: Verify the polytoken binary path in config, run `polytoken --version`.
- **Sessions immediately marked crashed**: Ensure the supervisor can reach the Polytoken daemon's HTTP port (check firewall rules).
- **State file not found**: The state file is at `~/.local/share/grindbot/{owner}/{repo}/state.json`. The directory is created automatically on first run.
