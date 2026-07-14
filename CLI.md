# CLI

Single executable, `grindbot` (`src/main.rs`).

```text
grindbot [global flags] <subcommand> [options]
```

## Global flags

| Flag | Short | Description |
|---|---|---|
| `--verbose` | `-v` | Increase log verbosity (repeatable: `-v` = debug, `-vv` = trace). |
| `--quiet` | `-q` | Reduce log verbosity to `warn`. |
| `--version` | | Print version and exit (clap `version` attribute). |

`--verbose` and `--quiet` override the `RUST_LOG` environment variable for the default filter. When `RUST_LOG` is set explicitly, it takes precedence over both flags.

## Subcommands

```text
grindbot supervise   [--config <PATH>|-c <PATH>] [--dry-run]
grindbot status      [--config <PATH>|-c <PATH>]
grindbot doctor      [--config <PATH>|-c <PATH>]
grindbot handoff done             --commit <REVISION> [evidence args]
grindbot handoff needs-feedback   --message <TEXT> | --message-file <PATH>
```

Clap provides `--help`/`-h` on every subcommand.

## `grindbot supervise`

Long-running daemon: gather GitHub/jj/Polytoken/filesystem state, plan, execute, save state, sleep `supervisor.poll_interval_secs`, repeat (`src/supervisor.rs::run`). Transient cycle errors are logged and retried; startup errors exit non-zero.

The supervisor installs a SIGINT handler (`tokio::signal::ctrl_c`). On SIGINT it finishes the current cycle and exits gracefully — it does not kill the process mid-cycle.

| Option | Short | Required | Default | Notes |
|---|---|---|---|---|
| `--config` | `-c` | No | `grindbot.toml` | Resolved relative to CWD. |
| `--dry-run` | | No | off | Gather state and plan once, print the planned actions, and exit with no side effects. Does not run startup cleanup, does not start implementer sessions, and does not save state. |

`RUST_LOG` sets tracing verbosity, default `info`. CWD is the repo root and default config location.

Prerequisites: a Jujutsu repo in CWD; `gh` and `jj` on `PATH` (hardcoded); Polytoken binary on `PATH` unless overridden by `polytoken.binary` (default `polytoken`). See [CONFIGURATION.md](CONFIGURATION.md).

## `grindbot status`

Show current supervisor state. Loads the state file (at the `HOME`-derived path), gathers live state via the real I/O layer, and prints a human-readable summary of active implementers, completed tasks, and needs-feedback tasks. Does not modify state. Requires a valid config.

| Option | Short | Required | Default | Notes |
|---|---|---|---|---|
| `--config` | `-c` | No | `grindbot.toml` | Resolved relative to CWD. |

## `grindbot doctor`

Check dependencies and configuration. Verifies that `jj`, `gh`, and the Polytoken binary are on `PATH` and runnable, and (if a config is loaded) that the config validates.

The `--config` option is **optional** and behaves differently from `supervise`/`status`: if omitted, doctor tries `./grindbot.toml` and loads it if present; if absent, it runs binary checks only without config validation.

| Option | Short | Required | Default | Notes |
|---|---|---|---|---|
| `--config` | `-c` | No | `grindbot.toml` if it exists | If absent, runs dependency checks only. |

## `grindbot handoff done`

Called by an implementer agent after committing an implementation.

```bash
grindbot handoff done \\
  --commit REVISION_ID \\
  --plan-review 'accepted after planning review' \\
  --implementation-review 'accepted after implementation review' \\
  --all-tests-passed \\
  --summary 'Short description of completed work'
```

| Option | Required | Repeatable | Notes |
|---|---|---|---|
| `--commit` | Yes | No | jj revision, strictly ahead of base |
| `--plan-review` | Yes | No | Non-empty attestation |
| `--implementation-review` | Yes | No | Non-empty attestation |
| `--all-tests-passed` | Yes (runtime) | No | Boolean flag; required for successful handoff |
| `--summary` | No | No | Defaults to empty |
| `--issue` | No | No | Issue number |
| `--unresolved-findings` | No | No | Must not be supplied for a successful handoff |

Run `grindbot handoff done --help` to see the full argument list and an example.

Walks up to the nearest `.jj` ancestor, validates the evidence arguments and that the revision exists and is strictly ahead of `.grindbot/base_commit`, then writes `.grindbot/result.json` and resets `.grindbot/stop_counter` (`src/handoff.rs::done`). No input manifest file is required.

## `grindbot handoff needs-feedback`

Called by an implementer agent that cannot finish the task.

```bash
grindbot handoff needs-feedback --message "<explanation>"
grindbot handoff needs-feedback --message-file path/to/message.txt
```

| Option | Short | Required | Default |
|---|---|---|---|
| `--message` | | One of the two | — |
| `--message-file` | | One of the two | — |

Exactly one of `--message` or `--message-file` must be provided; they conflict with each other (`conflicts_with`). The message is sent to the human operator/issue author verbatim and posted as the feedback request. `--message-file` reads the message from a file (trimmed). Finds the `.jj` workspace, writes a needs-feedback result with UTC timestamp, and resets the stop counter (`src/handoff.rs::needs_feedback`). No commit/base validation.

## Result protocol

Both handoffs write `.grindbot/result.json`. The supervisor reads it on a later cycle (`src/supervisor.rs::gather_state`): **done** → rebase/merge, push, comment, clean up; **needs-feedback** → post the message to GitHub, clean up. The stop hook reads the same file to gate agent session end ([AGENT_PROMPTS.md](AGENT_PROMPTS.md); `src/prompt.rs::STOP_HOOK_SCRIPT`).

## Exits

Successful finite commands exit `0`. Parsing, config, filesystem, jj, and handoff errors propagate as non-zero. `supervise` runs until it receives SIGINT (graceful shutdown after the current cycle) or hits an uncaught startup error.
