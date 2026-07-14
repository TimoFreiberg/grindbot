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

After both review stages accept, signal completion by running:

    {grindbot_path} handoff done --commit <rev> \
      --plan-review '<accepted plan review>' \
      --implementation-review '<accepted implementation review>' \
      --all-tests-passed \
      --summary '<short summary>'

Run `{grindbot_path} handoff done --help` to see all available arguments.
These text values are your self-reported review/test evidence and audit notes;
the supervisor records them but does not independently verify them. The command
writes the result file itself; no manifest file is needed. No operator approval
is required for a clean, fully-reviewed handoff; operator attention is for
feedback requests or failures. The commit must contain actual changes and be
ahead of the recorded base.

### 4. Need Help?

`needs-feedback` is an intentional early exit. Its message is sent to the human
operator/issue author verbatim and posted as the feedback request. If you need
more information to proceed, run:

    {grindbot_path} handoff needs-feedback --message "<explanation>"

Do not write any code if you are requesting feedback. Explain clearly what
information you need and why, providing enough context for the human operator
to make a decision without reading the codebase. Do not include private scratch
notes or instructions intended only for yourself in this message.

### Important

- You MUST call one of the handoff commands to end your session.
- Your session will not end until you do.
- After 3 failed attempts to end without calling handoff, the session will
  be terminated and treated as a crash.
- Run `{grindbot_path} handoff --help` or `{grindbot_path} handoff <subcommand> --help`
  to discover available commands and arguments.
