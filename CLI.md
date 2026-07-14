# CLI

Single executable, `grindbot` (`src/main.rs`). Three subcommands:

```text
grindbot supervise [--config <PATH>|-c <PATH>]
grindbot handoff done --commit <HASH>
grindbot handoff needs-feedback --message <TEXT>
```

No `--version` or other global flags. Clap provides `--help`/`-h`.

## `grindbot supervise`

Long-running daemon: gather GitHub/jj/Polytoken/filesystem state, plan, execute, save state, sleep `supervisor.poll_interval_secs` (`src/supervisor.rs::run`). Transient cycle errors are logged and retried; startup errors exit non-zero.

| Option | Short | Required | Default | Notes |
|---|---|---|---|---|
| `--config` | `-c` | No | `grindbot.toml` | Resolved relative to CWD (`main.rs:52`) |

`RUST_LOG` sets tracing verbosity, default `info` (`main.rs:41`). CWD is the repo root and default config location.

Prerequisites: a Jujutsu repo in CWD; `gh` and `jj` on `PATH` (hardcoded); Polytoken binary on `PATH` unless overridden by `polytoken.binary` (default `polytoken`). See [CONFIGURATION.md](CONFIGURATION.md).

## `grindbot handoff done`

Called by an implementer agent after committing an implementation.

```bash
grindbot handoff done --commit <commit_hash>
```

`--commit` required, no short form. Walks up to the nearest `.jj` ancestor, reads `.grindbot/base_commit`, verifies the revision exists and is strictly ahead of the base, then writes `.grindbot/result.json` and resets `.grindbot/stop_counter` (`src/handoff.rs::done`). Missing base, unknown revision, or a non-ahead commit fails without writing a result.

## `grindbot handoff needs-feedback`

Called by an implementer agent that cannot finish the task.

```bash
grindbot handoff needs-feedback --message "<explanation>"
```

`--message` required, no short form. Finds the `.jj` workspace, writes a needs-feedback result with UTC timestamp, and resets the stop counter (`src/handoff.rs::needs_feedback`). No commit/base validation.

## Result protocol

Both handoffs write `.grindbot/result.json`. The supervisor reads it on a later cycle (`src/supervisor.rs::gather_state`): **done** → rebase/merge, push, comment, clean up; **needs-feedback** → post the message to GitHub, clean up. The stop hook reads the same file to gate agent session end ([AGENT_PROMPTS.md](AGENT_PROMPTS.md); `src/prompt.rs::STOP_HOOK_SCRIPT`).

## Exits

Successful finite commands exit `0`. Parsing, config, filesystem, jj, and handoff errors propagate as non-zero. `supervise` runs unbounded and exits only on external termination or an uncaught startup error.

## Sources

- `src/main.rs` — CLI definition and dispatch
- `src/handoff.rs` — handoff validation and result-file writing
- `src/supervisor.rs` — daemon loop and runtime behavior
- `src/config.rs` — config schema and defaults
- `src/prompt.rs` — stop hook and permissions
