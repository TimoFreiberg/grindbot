# AGENTS.md

Instructions for coding agents working on this repository.

## Architecture

Grindbot is split into a **pure decision core** and an **I/O layer**:

- `src/core/` — Pure decision logic. No I/O. Fully property-testable.
  - `state.rs` — Data structures (SupervisorState, Issue, ImplementerState, etc.)
  - `actions.rs` — Action enum (what the planner can emit)
  - `planner.rs` — `plan(state) -> Vec<Action>` — the main decision function
  - `filters.rs` — Issue eligibility filtering
- `src/io/` — I/O traits and real implementations
  - `mod.rs` — Trait definitions (GithubClient, JjClient, PolytokenClient, Filesystem)
  - `github.rs` — GitHub via `gh` CLI
  - `jj.rs` — Jujutsu via `jj` CLI
  - `polytoken.rs` — Polytoken via HTTP API + CLI
  - `filesystem.rs` — Filesystem via `std::fs`
- `src/supervisor.rs` — Main loop: gather state, plan, execute, wait
- `src/workspace.rs` — Workspace setup (jj workspace, hooks, permissions)
- `src/handoff.rs` — Handoff binary (called by implementer agents)
- `src/prompt.rs` — Prompt template, stop hook script, permissions YAML
- `src/state_file.rs` — Persistent state file (active implementers, completed tasks)
- `src/config.rs` — Configuration parsing

## Testing Conventions

- **Pure core:** Use `proptest` for property-based tests. See `tests/core_planner.rs` and `tests/core_filters.rs`.
- **I/O layer:** Use mock trait implementations. See `tests/common/mod.rs` for shared mocks.
- **Handoff binary:** Use real jj repos in temp directories. See `tests/handoff_done.rs`.
- **Stop hook:** Test the bash script directly. See `tests/stop_hook.rs`.
- **Integration:** Use mock I/O to test supervisor flows. See `tests/integration_supervisor.rs`.

## Key Design Decisions

1. **Session completion:** File-based + stop hook. The `grindbot handoff` binary writes `.grindbot/result.json`. A stop hook gates session end.
2. **Ticket queue ordering:** FIFO (oldest first).
3. **Merge conflict escalation:** Throw away the newer implementation, re-queue the task.
4. **Permission mode:** Bypass+ with deny rules for dangerous commands.

## Version Control

This repo uses Jujutsu (jj). Always commit changes when done.
