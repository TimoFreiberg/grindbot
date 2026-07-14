# Agent Prompts

Grindbot spawns Polytoken agent sessions to do its work. This page documents the
**literal prompt templates** the supervisor sends to agents, the session
configuration each prompt runs under, and the stop-hook and permission controls
that gate agent behavior.

Two kinds of agents run:

- **Implementer agent** — starts on a new issue, plans + implements + reviews +
  hands off. Uses a gated stop hook.
- **Conflict resolution agent** — one-shot agent spawned when a rebase produces
  merge conflicts. Uses an always-stop hook (no gating).

These are the only prompts Grindbot sends to agent sessions. The supervisor also
posts messages back to GitHub issues at lifecycle points (merge success, needs
feedback, persistent conflict); those are plain formatted strings, not agent
prompts, and are listed at the end of this page.

---

## Implementer Prompt

The template below is sent to the implementer agent when the supervisor starts a
new task. It is the file `src/prompts/implementer.md`, loaded into the binary
with `include_str!` and substituted by `build_prompt()` in `src/prompt.rs`.
Placeholders are shown literally as `{name}`.

```markdown
You are implementing GitHub issue #{number}: {title}

Issue URL: {url}

## Issue Description

{body}

{recent_comments_section}

## Instructions

You are running in an autonomous supervised session. Your work will be reviewed
by reviewer subagents before it is accepted.

### 1. Investigate and plan

Investigate the issue and relevant codebase. Write a concrete implementation
plan. Run the existing `plan-reviewer` workflow, fixing or rebutting findings
until the plan-reviewer accepts the plan. Fix or rebut all findings and rerun
reviewers until the plan-reviewer accepts the plan. When the plan passes review,
hand it off, which will start plan execution.

### 2. Implement and test

Implement the accepted plan and add/update tests. Run the project checks. Then
run the repository's existing implementation-review workflow, fixing or
rebutting findings until the implementation reviewer accepts the result. Fix or
rebut all findings and keep reviewing until the implementation reviewer accepts
the implementation. Commit your work using jj. The existing reviewer
skills/facets define review behavior; do not invent a new review protocol.

### 3. Finish with structured evidence

Record an acceptance-criteria-to-test mapping and a test inventory/results in a
workspace-local JSON manifest (outside `.grindbot/`). Only after both review
stages accept, signal completion by running:

    {grindbot_path} handoff done --manifest <path>

The manifest must contain the approved outcome, commit, accepted plan-review
and implementation-review evidence, tests, mapping, unresolved-findings status,
summary, and timestamp. No operator approval is required for a clean,
fully-reviewed handoff; operator attention is for feedback requests or failures.
The commit must contain actual changes and be ahead of the recorded base.

### 4. Need Help?

`needs-feedback` is an intentional early exit. If you need more information from the issue author to proceed, run:

    {grindbot_path} handoff needs-feedback --message "<explanation>"

Do not write any code if you are requesting feedback. Explain clearly what
information you need and why, providing enough context for the issue author to
make a decision without reading the codebase.

### Important

- You MUST call one of the handoff commands to end your session.
- Your session will not end until you do.
- After 3 failed attempts to end without calling handoff, the session will
  be terminated and treated as a crash.
```


### Placeholders

| Placeholder | Substituted with |
|---|---|
| `{number}` | The GitHub issue number. |
| `{title}` | The issue title. |
| `{url}` | `https://github.com/{owner}/{repo}` — the repo root, with **no** `/issues/{number}` suffix. |
| `{body}` | The issue body. |
| `{recent_comments_section}` | The 5 most recent issue comments, oldest→newest. Each is formatted as `**author** (YYYY-MM-DD HH:MM UTC):\n> ` with the body quoted (`>` prefixed on each line). Rendered under a `## Recent Comments` heading. When the issue has no comments, this placeholder is replaced with an empty string and the heading is omitted entirely. |
| `{grindbot_path}` | Absolute path to the current grindbot executable (from `std::env::current_exe()`, falling back to the literal string `grindbot`). |

### Implementer session configuration

Set by `start_implementer()` in `src/supervisor.rs` before the prompt is sent:

1. Creates a jj workspace from the captured `main@origin` head commit.
2. Writes workspace files via `setup_workspace()`:
   - `.grindbot/base_commit` — the base commit the workspace started from.
   - `.polytoken/hooks.json` — the gated stop hook (see below).
   - `.polytoken/permissions.yaml` — `bypass_plus` with deny rules (see below).
3. Spawns a Polytoken session in the workspace.
4. Configures the session:
   - Facet: `plan` (reviewers gate the handoff to `execute`).
   - Adventurous handoff: **enabled**.
   - Permission mode: `bypass_plus`.
   - Goal: `Implement issue #{number}`.
5. Sends the prompt with `config.polytoken.max_tool_turns` turns.

---

## Conflict Resolution Prompt

When a rebase produces merge conflicts, the supervisor spawns a one-shot
conflict resolution agent. The prompt is the file
`src/prompts/conflict_resolution.md`, loaded with `include_str!`. It has **no
placeholders** — it is sent verbatim.

```text
Resolve the merge conflicts in this workspace. Use the jj-resolve-conflicts skill. Do not make any changes beyond what is needed to resolve the conflicts.
```

### Conflict resolution session configuration

Set by `resolve_conflict()` in `src/supervisor.rs`:

1. Checks the per-issue conflict retry count. If it has reached **3**, the
   supervisor posts the persistent-conflict message (see below), discards the
   workspace, and stops — no agent is spawned.
2. Otherwise, reuses the implementer's workspace and calls
   `setup_conflict_resolution_workspace()` to overwrite `.polytoken/hooks.json`
   with the always-stop hook.
3. Spawns a Polytoken session.
4. Configures the session:
   - Facet: `execute`.
   - Permission mode: `bypass_plus`.
   - Goal: `Resolve merge conflicts in workspace`.
   - Adventurous handoff is **not** enabled.
5. Sends the prompt with **50** max turns.
6. Polls for completion with a **30-minute** timeout, checking every 10 seconds.
   On timeout the session is terminated, the conflict retry count is
   incremented, and the workspace is discarded.

---

## Stop Hook Gate

The implementer workspace installs a `stop` hook (`grindbot-gate`) that gates
whether the agent session is allowed to end. The hook reads
`$POLYTOKEN_PROJECT_DIR/.grindbot/result.json`. The script `STOP_HOOK_SCRIPT`
in `src/prompt.rs`:

```bash
#!/bin/bash
set -e
PROJECT_DIR="$POLYTOKEN_PROJECT_DIR"
RESULT_FILE="$PROJECT_DIR/.grindbot/result.json"
COUNTER_FILE="$PROJECT_DIR/.grindbot/stop_counter"

# Result file exists → allow the session to stop.
if [ -f "$RESULT_FILE" ]; then
  echo '{"outcome":"stop"}'
  exit 0
fi

COUNT=$(cat "$COUNTER_FILE" 2>/dev/null || echo 0)
COUNT=$((COUNT + 1))
echo "$COUNT" > "$COUNTER_FILE"

# After 3 consecutive stops without a result file, allow stop (treated as crash).
if [ "$COUNT" -ge 3 ]; then
  rm -f "$COUNTER_FILE"
  echo '{"outcome":"stop"}'
  exit 0
fi

# No result file and under the counter limit → force the model back to work.
echo '{"outcome":"continue","reason":"You must call the handoff binary to end your session. Run: grindbot handoff done --commit <hash> OR grindbot handoff needs-feedback --message <text>"}'
exit 0
```

Behavior summary:

| Condition | Outcome |
|---|---|
| `.grindbot/result.json` present | `{"outcome":"stop"}` — session ends normally. |
| No result file, attempts 1–2 | `{"outcome":"continue","reason":...}` — the agent is sent back to work and reminded of the handoff commands. |
| No result file, attempt 3 | The counter is cleared and `{"outcome":"stop"}` is emitted; the supervisor classifies this as a crash. |

The `grindbot handoff` binary (invoked by the agent as `handoff done` or
`handoff needs-feedback`) writes `result.json` **and** removes the stop counter,
making the result file the durable completion signal. Until `result.json`
exists, the hook keeps the session alive.

---

## Always-Stop Hook

Conflict resolution agents use `STOP_HOOK_ALWAYS_STOP`, which unconditionally
allows the session to stop (no gating, since there is no handoff binary for the
resolver to call):

```bash
#!/bin/bash
echo '{"outcome":"stop"}'
exit 0
```

It is installed by `setup_conflict_resolution_workspace()`, overwriting the
implementer's gated hook in `.polytoken/hooks.json`.

---

## Permissions

Both implementer and conflict resolution workspaces use the same
`PERMISSIONS_YAML` (`src/prompt.rs`), applied under `bypass_plus`. It denies a
set of dangerous shell operations and protects Grindbot/Polytoken internals
from agent filesystem writes:

```yaml
version: 2
deny:
  - tool: shell_exec
    args: { executable: rm, flags_present: ["-rf"] }
  - tool: shell_exec
    args: { executable: rm, flags_present: ["-r", "-f"] }
  - tool: shell_exec
    args: { executable: rm, flags_present: ["-r"] }
  - tool: shell_exec
    args: { executable: rm, flags_present: ["--recursive"] }
  - tool: shell_exec
    args: { executable: git, subcommand: push }
  - tool: shell_exec
    args: { executable: jj, subcommand: git }
    message: "Use grindbot's merge flow instead of pushing directly."
  - tool: shell_exec
    args: { executable: jj, subcommand: abandon }
    message: "Do not abandon commits; let the supervisor manage workspace lifecycle."
filesystem:
  deny:
    - access: [write]
      path: .grindbot{,/**}
    - access: [write]
      path: .polytoken{,/**}
```

What each rule enforces:

| Rule | Why |
|---|---|
| `rm` with `-rf`, `-r -f`, `-r`, or `--recursive` | Block recursive deletion. |
| `git push` | The supervisor owns the merge/push flow. |
| `jj git` | The supervisor owns the merge flow; agents must not push directly. |
| `jj abandon` | The supervisor owns workspace/commit lifecycle. |
| Writes under `.grindbot/**` | Protect the result file, stop counter, and base-commit marker from agent tampering. |
| Writes under `.polytoken/**` | Protect hooks.json and permissions.yaml from agent tampering. |

---

## GitHub Lifecycle Messages

These are posted by the supervisor directly to GitHub issues as comments. They
are **not** agent prompts. Each begins with the hidden marker
`<!-- grindbot -->` so Grindbot can recognize its own comments.

### Merge success

Posted when an implementation rebases cleanly and is merged:

```
<!-- grindbot -->

Implementation complete. Commit `{commit}` has been merged to `{branch}`.
```

`{commit}` is the merged commit hash; `{branch}` is the configured
`supervisor.base_branch`.

### Merge success after conflict resolution

Posted when conflicts were resolved by a resolver agent and the result merged:

```
<!-- grindbot -->

Implementation complete (after conflict resolution). Commit `{commit}` has been merged to `{branch}`.
```

### Needs feedback

Posted when the implementer runs
`grindbot handoff needs-feedback --message "<message>"`:

```
<!-- grindbot -->

**Needs feedback:**

{message}
```

`{message}` is the explanation the implementer agent supplied.

### Persistent merge conflict

Posted when an issue has reached the conflict retry limit (3 failed resolution
attempts). The supervisor then discards the workspace and stops working the
issue:

```
<!-- grindbot -->

**Persistent merge conflict:**

The implementation for this issue has failed to merge after 3 conflict resolution attempts. The conflicts may indicate that the issue requires a different approach or manual intervention.

Please review and provide guidance.
```
