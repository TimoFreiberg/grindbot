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
- `src/supervisor.rs` — Main loop: gather state, plan, execute, wait. Graceful shutdown via SIGINT. Dry-run mode. Per-cycle summary logging.
- `src/workspace.rs` — Workspace setup (jj workspace, hooks, permissions)
- `src/handoff.rs` — Handoff binary (called by implementer agents)
- `src/prompt.rs` — Prompt template, stop hook script, permissions YAML
- `src/state_file.rs` — Persistent state file (active implementers, completed tasks). Per-repo path: `~/.local/share/grindbot/{owner}/{repo}/state.json`. `ActiveImplementer` stores full `SessionInfo` (port, bearer_token, credential_file) so sessions can be checked for liveness after restart.
- `src/config.rs` — Configuration parsing + validation (`Config::validate()`)
- `src/status.rs` — `grindbot status` command: shows active implementers (alive/dead), completed tasks, needs-feedback, conflict retries
- `src/doctor.rs` — `grindbot doctor` command: checks jj, gh (auth), polytoken, config validity, jj repo

## Testing Conventions

- **Pure core:** Use `proptest` for property-based tests. See `tests/core_planner.rs` and `tests/core_filters.rs`.
- **I/O layer:** Use mock trait implementations. See `tests/common/mod.rs` for shared mocks.
- **Handoff binary:** Use real jj repos in temp directories. See `tests/handoff_done.rs`.
- **Stop hook:** Test the bash script directly. See `tests/stop_hook.rs`.
- **Integration:** Use mock I/O to test supervisor flows. See `tests/integration_supervisor.rs`.
- **CLI:** Use `env!("CARGO_BIN_EXE_grindbot")` to test the binary. See `tests/cli_basics.rs`, `tests/status_command.rs`, `tests/doctor_command.rs`, `tests/dry_run.rs`.
- **Documentation:** See `tests/readme.rs` for README content checks.

## Key Design Decisions

1. **Session completion:** File-based + stop hook. The `grindbot handoff` binary writes `.grindbot/result.json`. A stop hook gates session end.
2. **Ticket queue ordering:** FIFO (oldest first).
3. **Merge conflict escalation:** Throw away the newer implementation and park the task after persistent conflicts.
4. **Permission mode:** Bypass+ with deny rules for dangerous commands.

## Documentation Maintenance

The repository-level surface documentation must stay synchronized with the implementation. When code changes affect the CLI, dependencies or integrations, configuration, agent prompts/controls, or the supervisor lifecycle, update the corresponding files in the same change:

- `CLI.md`
- `DEPENDENCIES.md`
- `CONFIGURATION.md`
- `AGENT_PROMPTS.md`
- `CORE_LOOP.md`

Verify documented claims against the code and keep these files concise.

## Version Control

This repo uses Jujutsu (jj). Always commit changes when done.
