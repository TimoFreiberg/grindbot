# Agent Prompts and Controls

Agent-facing behavior lives in `src/prompt.rs`; orchestration in `src/supervisor.rs` (`start_implementer`), with workspace setup in `src/workspace.rs` and the handoff binary in `src/handoff.rs`.

## Implementer prompt

Template: `PROMPT_TEMPLATE` (`src/prompt.rs`). `build_prompt()` substitutes:

| Placeholder | Value |
|---|---|
| `{number}` | GitHub issue number |
| `{title}` | Issue title |
| `{url}` | `https://github.com/{owner}/{repo}` (no `/issues/{number}` suffix — built from `config.github.owner/repo`) |
| `{body}` | Issue body |
| `{recent_comments_section}` | Up to 5 most recent comments, oldest→newest, `**author** (YYYY-MM-DD HH:MM UTC):\n> ` quoted; empty (omitted) when no comments |
| `{grindbot_path}` | Current Grindbot executable path |

The template instructs the agent to: start in `plan` facet and pass plan review → implement and pass implementation review → commit with jj → end with exactly one handoff command — `grindbot handoff done --commit <hash>` or `grindbot handoff needs-feedback --message "<text>"`. No handoff ⇒ the stop hook forces continuation.

## Implementer session config

`start_implementer` (`src/supervisor.rs:420`) creates a jj workspace, writes control files, spawns a Polytoken session, then sets facet `plan`, enables adventurous handoff, sets permission mode `bypass_plus`, sets goal `Implement issue #<number>`, and sends the prompt with `config.polytoken.max_tool_turns`. The workspace gets the gated stop hook and `PERMISSIONS_YAML` (`src/workspace.rs:setup_workspace`).

## Stop-hook gate

`STOP_HOOK_SCRIPT` (`src/prompt.rs`) reads `POLYTOKEN_PROJECT_DIR/.grindbot/result.json`:

- Result file present → `{"outcome":"stop"}`.
- No result, attempts 1–2 → `{"outcome":"continue","reason":...}` reminding the agent of the handoff commands.
- Attempt 3 → clears the counter, emits `{"outcome":"stop"}` (supervisor classifies the session as crashed).

`grindbot handoff` (`src/handoff.rs:write_result`) writes `result.json` and removes the stop counter, making the file the durable completion signal — not process exit.

## Conflict-resolution prompt

On a merge/rebase conflict (`src/supervisor.rs:resolve_conflict`), if `conflict_retry_count < 3`, the supervisor spawns a one-shot resolver in the same workspace: facet `execute`, permission `bypass_plus`, always-stop hook (`setup_conflict_resolution_workspace`), goal `Resolve merge conflicts in workspace`, literal prompt:

> Resolve the merge conflicts in this workspace. Use the jj-resolve-conflicts skill. Do not make any changes beyond what is needed to resolve the conflicts.

50-turn limit, polled up to 600 s (10 min) at 10 s intervals. After it finishes the supervisor retries the merge; failure increments the retry count. At 3 retries it posts a GitHub comment and discards the workspace. The resolver does not receive implementer handoff instructions.

## Permissions

`PERMISSIONS_YAML` (`src/prompt.rs`), shared by implementer and resolver workspaces, denies under `bypass_plus`:

- shell `rm` with `-rf`, `-r -f`, `-r`, or `--recursive`
- shell `git push`
- shell `jj git` (supervisor owns the merge flow)
- shell `jj abandon` (supervisor owns workspace lifecycle)
- filesystem writes under `.grindbot/**` and `.polytoken/**`

## GitHub lifecycle messages

Posted by the supervisor/planner (not session prompts):

- merge success: `<!-- grindbot -->\n\nImplementation complete. Commit \`{commit}\` has been merged to \`{branch}\`.`
- merge success after conflict resolution: `...Implementation complete (after conflict resolution)...`
- needs feedback: `<!-- grindbot -->\n\n**Needs feedback:**\n\n{message}`
- persistent conflict (3 retries): `<!-- grindbot -->\n\n**Persistent merge conflict:**\n\n...`

## Source references

- `src/prompt.rs` — prompt template, stop hooks, permissions YAML
- `src/supervisor.rs` — `start_implementer`, `resolve_conflict`, `process_result`
- `src/workspace.rs` — `setup_workspace`, `setup_conflict_resolution_workspace`
- `src/handoff.rs` — `done`, `needs_feedback`, `write_result`
- `src/core/planner.rs` — lifecycle comment bodies
- Tests: `tests/stop_hook.rs`, `tests/handoff_done.rs`, `tests/integration_supervisor.rs`, `tests/integration_workspace_setup.rs`
