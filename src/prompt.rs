use crate::core::state::{Comment, Issue};

/// The implementer prompt template.
/// Loaded from `src/prompts/implementer.md` so the prompt is also visible in
/// the documentation (AGENT_PROMPTS.md includes the same file).
///
/// Placeholders: {number}, {title}, {url}, {body}, {recent_comments_section}, {grindbot_path}
pub const PROMPT_TEMPLATE: &str = include_str!("prompts/implementer.md");

/// Build the prompt for an implementer session.
pub fn build_prompt(issue: &Issue, github_url: &str, grindbot_path: &str) -> String {
    let recent_comments_section = if issue.comments.is_empty() {
        String::new()
    } else {
        let last_comments: Vec<&Comment> = issue.comments.iter().rev().take(5).collect();
        let mut section = String::from("## Recent Comments\n\n");
        for comment in last_comments.iter().rev() {
            section.push_str(&format!(
                "**{}** ({}):\n> {}\n\n",
                comment.author,
                jiff::fmt::strtime::format("%Y-%m-%d %H:%M UTC", comment.created_at)
                    .unwrap(),
                comment.body.replace('\n', "\n> ")
            ));
        }
        section
    };

    PROMPT_TEMPLATE
        .replace("{number}", &issue.number.to_string())
        .replace("{title}", &issue.title)
        .replace("{url}", github_url)
        .replace("{body}", &issue.body)
        .replace("{recent_comments_section}", &recent_comments_section)
        .replace("{grindbot_path}", grindbot_path)
}

/// The stop hook script that gates session end.
/// Allows stop when result file exists, prevents stop (returns `continue`)
/// when it doesn't, and allows stop after 3 consecutive attempts.
pub const STOP_HOOK_SCRIPT: &str = r#"#!/bin/bash
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
echo '{"outcome":"continue","reason":"You must call the handoff binary to end your session. Run: grindbot handoff done --manifest <path> OR grindbot handoff needs-feedback --message <text>"}'
exit 0
"#;

/// The stop hook that always allows stop (for conflict resolution agents).
pub const STOP_HOOK_ALWAYS_STOP: &str = r#"#!/bin/bash
echo '{"outcome":"stop"}'
exit 0
"#;

/// The permissions.yaml content for implementer workspaces.
pub const PERMISSIONS_YAML: &str = r#"version: 2
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
"#;

/// The hooks.json content for the stop hook.
pub fn hooks_json(stop_hook_script: &str) -> String {
    // We need to embed the script as a JSON string
    let escaped = serde_json::to_string(stop_hook_script).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"[
  {{
    "name": "grindbot-gate",
    "event": "stop",
    "handler": {{
      "bash": {}
    }}
  }}
]"#,
        escaped
    )
}

/// The hooks.json content for the always-stop hook (conflict resolution agents).
pub fn hooks_json_always_stop() -> String {
    let escaped =
        serde_json::to_string(STOP_HOOK_ALWAYS_STOP).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"[
  {{
    "name": "grindbot-gate",
    "event": "stop",
    "handler": {{
      "bash": {}
    }}
  }}
]"#,
        escaped
    )
}
