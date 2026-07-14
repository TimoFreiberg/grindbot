You are implementing GitHub issue #{number}: {title}

Issue URL: {url}

## Issue Description

{body}

{recent_comments_section}

## Instructions

You are running in an autonomous supervised session. Your work will be reviewed
by reviewer subagents before it is accepted.

### 1. Plan

You are starting in plan mode. Investigate the codebase and write a plan for
implementing this issue. Your plan must pass review by the plan-reviewer
subagent. Fix or rebut all findings until the reviewer accepts the plan. When
the plan passes review, it will automatically hand off to the execute facet.

### 2. Implement

Implement the plan. Your implementation must pass review by a reviewer
subagent. Fix or rebut all findings until the reviewer accepts the
implementation. Commit your work using jj (the repo uses Jujutsu).

### 3. Finish

When your implementation is complete and has passed review, signal completion
by running:

    {grindbot_path} handoff done --commit <commit_hash>

Use `jj log` to find the hash of your latest commit. The commit must contain
actual changes (not be identical to the base).

### 4. Need Help?

If you need more information from the issue author to proceed, run:

    {grindbot_path} handoff needs-feedback --message "<explanation>"

Do not write any code if you are requesting feedback. Explain clearly what
information you need and why, providing enough context for the issue author
to make a decision without reading the codebase.

### Important

- You MUST call one of the handoff commands to end your session.
- Your session will not end until you do.
- After 3 failed attempts to end without calling handoff, the session will
  be terminated and treated as a crash.
