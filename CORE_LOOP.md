# Supervisor Core Loop

Grindbot is a pure decision core plus an I/O layer. The supervisor loop: gather state → plan → execute → save → sleep → repeat.

## Lifecycle

```mermaid
flowchart TD
    A[CLI: parse args, init tracing] --> B{Command}
    B -->|handoff done| HD[Find jj root, validate commit ahead of base]
    B -->|handoff needs-feedback| HF[Find jj root]
    HD --> R[Write .grindbot/result.json, reset stop counter]
    HF --> R
    B -->|supervise --dry-run| DR[Gather state once, plan, print actions, exit]
    B -->|supervise| C[Load grindbot.toml, build I/O layer]
    C --> D[Load state file]
    D --> PC[Pre-flight polytoken check]
    PC --> SC[Startup cleanup: reconcile dead/orphaned workspaces]
    SC --> SV[Save state]
    SV --> SIG[Install SIGINT handler]
    SIG --> H[Gather state]
    H --> I{Gather ok?}
    I -->|no| W[Log error, sleep poll_interval]
    W --> SIG2{SIGINT?}
    SIG2 -->|no| H
    I -->|yes| P[plan: pure decision function]
    P --> L[Log cycle summary]
    L --> X[Execute each action, save state after each]
    X --> T{SIGINT?}
    T -->|no| SLEEP[Sleep poll_interval_secs]
    SLEEP --> H
    T -->|yes| FIN[Finish current cycle, save state, exit]
    SIG2 -->|yes| FIN
```

### Startup

`main.rs` parses the CLI and inits tracing. For `supervise`: load TOML config, build the I/O layer, load the state file. The **dry-run** path gathers state once, plans, prints the planned actions, and exits — no side effects, no startup cleanup, no save. The normal path runs a pre-flight polytoken check, startup cleanup (reconcile dead/orphaned workspaces), saves state, then installs a SIGINT handler for graceful shutdown.

### Gather state

`gather_state` fetches open GitHub issues via `gh`, enriches allowlisted issues with comments, reads `main@origin` head, and builds implementer and workspace state from the state file plus live session liveness checks.

### Main loop

Each cycle: gather state → `plan` → log cycle summary → execute each action (saving state after each) → sleep `poll_interval_secs` (default 30). On SIGINT, the current cycle finishes, state is saved, and the process exits.

## Planner

The planner is a pure, deterministic function: `plan(state) -> Vec<Action>`. It performs **no I/O** and is property-tested in isolation. Decision order matters — earlier phases mark issues/workspaces as handled so later phases skip them.

```mermaid
flowchart TD
    START[plan: state snapshot in] --> S1
    S1["1. Crash cleanup<br/>workspace has session_id, daemon not alive, no result file<br/>→ CleanupWorkspace(SessionCrashed)"] --> S2
    S2["2. Finished sessions<br/>Done → MergeImplementation<br/>NeedsFeedback → PostComment + CleanupWorkspace(SessionFinished)"] --> S3
    S3["3. Crashed-with-result cleanup<br/>crashed implementer: check workspace result file<br/>→ CleanupWorkspace(SessionCrashed)"] --> S4
    S4["4. Orphan cleanup<br/>prefix-matching workspace, no session_id, no result file, not already handled<br/>→ CleanupWorkspace(OrphanedWorkspace)"] --> S5
    S5["5. Start new implementers<br/>active_count < max_parallelism?<br/>filter eligible, sort FIFO by creation date<br/>→ StartImplementer up to max_parallelism - active_count"] --> CHK
    CHK{Any actions emitted?} -->|no| NOOP[→ Noop]
    CHK -->|yes| OUT[→ Vec<Action>]
    NOOP --> OUT
```

### Eligibility

An issue is eligible for a new implementer when **all** hold:

- Issue author is on the configured allowlist.
- Last comment is not by the supervisor (supervisor comments carry the `<!-- grindbot -->` prefix).
- Issue is not currently active (not in the active-issues set).
- Issue is not in `completed_issues`.

Eligible issues are sorted FIFO (oldest first) by creation date.

## Implementer lifecycle

**Start:** create a jj workspace from the captured `main@origin` head → write `.grindbot/base_commit`, `.polytoken/hooks.json` (stop gate), `.polytoken/permissions.yaml` (bypass+ with deny rules) → spawn a Polytoken session → set facet `plan`, enable adventurous handoff, set permission mode `bypass_plus`, set goal → send the issue prompt.

**Agent session:** the stop hook gates session end. Stop is allowed when `.grindbot/result.json` exists; otherwise the hook forces `continue`. After 3 consecutive stop attempts with no result file, stop is allowed (the session is classified as a crash).

**Handoff:** `grindbot handoff done --manifest <path>` validates the versioned manifest evidence, commit existence, and commit-ahead-of-base fact, then writes `result.json` and resets the stop counter. `grindbot handoff needs-feedback --message <text>` (or `--message-file <path>`) writes the intentional early-exit result directly. The handoff binary walks up to the nearest `.jj` directory — the workspace root, not the main repo.

**Completion:** a `done` result triggers rebase onto `main@origin` → set bookmark → push → comment → record completed → reset conflict retries → cleanup workspace. A `needs-feedback` result posts the message, records it, and cleans up. Dead/crashed/malformed sessions are cleaned up via `CleanupWorkspace`.

## Conflict handling

A rebase conflict spawns a one-shot Polytoken agent: `execute` facet, 50 max turns, `bypass_plus` permissions, always-stop hook (no gating), **30-minute timeout**. It uses the `jj-resolve-conflicts` skill via prompt text.

- **Resolved** → retry the merge inline. Success completes the merge; a fresh conflict increments the retry counter and discards the workspace.
- **Unresolved / timeout / dead agent** → increment the retry counter, discard workspace.
- **Escalation (retry count ≥ 3)** → post a persistent-conflict comment (`<!-- grindbot -->` prefix), discard workspace. Because supervisor comments are detected by that prefix and `last_activity_by_supervisor` makes the issue ineligible, the issue is **parked pending human input** — it is not re-queued automatically.

## Design decisions

1. **Session completion:** file-based (`result.json`) plus a stop hook that gates session end.
2. **Ticket queue ordering:** FIFO — oldest eligible issue first.
3. **Merge conflict escalation:** discard the newer implementation; after persistent conflicts, park the task for human input.
4. **Permission mode:** `bypass_plus` with deny rules for dangerous commands.
