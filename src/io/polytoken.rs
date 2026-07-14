use std::time::Duration;

use crate::io::{PolytokenClient, SessionInfo, SessionState};

/// Real Polytoken client using the HTTP API and CLI.
pub struct RealPolytokenClient {
    pub binary: String,
    pub http_client: reqwest::Client,
}

impl RealPolytokenClient {
    pub fn new(binary: &str) -> Self {
        Self {
            binary: binary.to_string(),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    fn base_url(&self, session: &SessionInfo) -> String {
        format!("http://127.0.0.1:{}", session.port)
    }

    async fn post_json<T: serde::Serialize>(
        &self,
        session: &SessionInfo,
        path: &str,
        body: &T,
    ) -> anyhow::Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url(session), path);
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&session.bearer_token)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to connect to polytoken daemon at {}: {}",
                    self.base_url(session),
                    e
                )
            })?;
        Ok(resp)
    }

    async fn get_json<R: serde::de::DeserializeOwned>(
        &self,
        session: &SessionInfo,
        path: &str,
    ) -> anyhow::Result<R> {
        let url = format!("{}{}", self.base_url(session), path);
        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(&session.bearer_token)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to connect to polytoken daemon at {}: {}",
                    self.base_url(session),
                    e
                )
            })?;
        let result = resp.json::<R>().await?;
        Ok(result)
    }
}

#[async_trait::async_trait]
impl PolytokenClient for RealPolytokenClient {
    async fn spawn_session(&self, workspace_dir: &str) -> anyhow::Result<SessionInfo> {
        // Step 1: Run `polytoken --working-dir <workspace> new --no-attach`
        let output = tokio::process::Command::new(&self.binary)
            .args(["--working-dir", workspace_dir, "new", "--no-attach"])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "polytoken new failed (binary: '{}', workspace: '{}'): {}\n--- stdout ---\n{}",
                self.binary,
                workspace_dir,
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse session ID and port from stdout.
        // Expected format: "Session ID: <id>\nPort: <port>\n"
        // or similar. We try multiple parsing strategies.
        let (session_id, port) = parse_session_output(&stdout)?;

        // Step 2: Find the credential file.
        // Try `polytoken sessions --format json` to find the credential_file_path.
        let credential_file = self.find_credential_file(&session_id).await?;

        // Step 3: Read the bearer token from the credential file.
        let cred_content = std::fs::read_to_string(&credential_file)?;
        #[derive(serde::Deserialize)]
        struct Credential {
            token: String,
        }
        let cred: Credential = serde_json::from_str(&cred_content)?;
        let bearer_token = cred.token;

        Ok(SessionInfo {
            session_id,
            port,
            credential_file,
            bearer_token,
        })
    }

    async fn set_facet(&self, session: &SessionInfo, facet: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct Body<'a> {
            facet: &'a str,
        }
        let resp = self.post_json(session, "/facet", &Body { facet }).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("set_facet failed (HTTP {}): {}", status, body);
        }
        Ok(())
    }

    async fn enable_adventurous_handoff(&self, session: &SessionInfo) -> anyhow::Result<()> {
        // First check if it's already enabled
        let url = format!("{}/adventurous-handoff", self.base_url(session));
        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(&session.bearer_token)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to connect to polytoken daemon at {}: {}",
                    self.base_url(session),
                    e
                )
            })?;

        if resp.status().is_success() {
            // Already enabled
            return Ok(());
        }

        // Enable it
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&session.bearer_token)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to connect to polytoken daemon at {}: {}",
                    self.base_url(session),
                    e
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "enable_adventurous_handoff failed (HTTP {}): {}",
                status,
                body
            );
        }
        Ok(())
    }

    async fn set_permission_mode(&self, session: &SessionInfo, mode: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct Body<'a> {
            mode: &'a str,
        }
        let resp = self
            .post_json(session, "/permission-monitor", &Body { mode })
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("set_permission_mode failed (HTTP {}): {}", status, body);
        }
        Ok(())
    }

    async fn set_goal(&self, session: &SessionInfo, summary: &str) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct Body<'a> {
            summary: &'a str,
        }
        let resp = self.post_json(session, "/goal", &Body { summary }).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("set_goal failed (HTTP {}): {}", status, body);
        }
        Ok(())
    }

    async fn send_prompt(
        &self,
        session: &SessionInfo,
        content: &str,
        max_turns: u32,
    ) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct Body<'a> {
            content: &'a str,
            max_tool_turns: u32,
        }
        let resp = self
            .post_json(
                session,
                "/prompt",
                &Body {
                    content,
                    max_tool_turns: max_turns,
                },
            )
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("send_prompt failed (HTTP {}): {}", status, body);
        }
        Ok(())
    }

    async fn get_state(&self, session: &SessionInfo) -> anyhow::Result<SessionState> {
        #[derive(serde::Deserialize)]
        struct ContextUsage {
            #[serde(default)]
            used_tokens: Option<u32>,
            #[serde(default)]
            limit_tokens: Option<u32>,
        }

        #[derive(serde::Deserialize)]
        struct StateResponse {
            #[serde(rename = "turn_in_flight")]
            turn_in_flight: bool,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            context_usage: Option<ContextUsage>,
            #[serde(default)]
            most_recent_assistant_text: Option<String>,
        }
        let state: StateResponse = self.get_json(session, "/state").await?;
        Ok(SessionState {
            turn_in_flight: state.turn_in_flight,
            cwd: state.cwd,
            used_tokens: state.context_usage.as_ref().and_then(|c| c.used_tokens),
            limit_tokens: state.context_usage.as_ref().and_then(|c| c.limit_tokens),
            most_recent_assistant_text: state.most_recent_assistant_text,
        })
    }

    async fn terminate(&self, session: &SessionInfo) -> anyhow::Result<()> {
        let url = format!("{}/terminate", self.base_url(session));
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&session.bearer_token)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to connect to polytoken daemon at {}: {}",
                    self.base_url(session),
                    e
                )
            })?;
        if !resp.status().is_success() {
            tracing::warn!("terminate returned non-success: {}", resp.status());
        }
        Ok(())
    }

    async fn is_alive(&self, session: &SessionInfo) -> bool {
        match self.get_state(session).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl RealPolytokenClient {
    async fn find_credential_file(&self, session_id: &str) -> anyhow::Result<String> {
        let output = tokio::process::Command::new(&self.binary)
            .args(["sessions", "--format", "json"])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "polytoken sessions failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[derive(serde::Deserialize)]
        struct SessionRecord {
            #[serde(rename = "session_id")]
            id: String,
            #[serde(default)]
            credential_file_path: Option<String>,
        }

        let sessions: Vec<SessionRecord> = serde_json::from_slice(&output.stdout)?;

        let record = sessions
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| anyhow::anyhow!("session {} not found in sessions list", session_id))?;

        record
            .credential_file_path
            .ok_or_else(|| anyhow::anyhow!("no credential_file_path for session {}", session_id))
    }
}

/// Parse session ID and port from `polytoken new --no-attach` stdout.
/// Tries multiple formats.
fn parse_session_output(stdout: &str) -> anyhow::Result<(String, u16)> {
    // Try to find session ID and port in the output.
    // Common patterns:
    //   "Session ID: abc123"
    //   "Port: 12345"
    //   or JSON-like output

    // Try JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        let session_id = json
            .get("session_id")
            .or_else(|| json.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("could not find session ID in JSON output: {stdout}")
            })?;
        let port = json
            .get("port")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                anyhow::anyhow!("could not find port in JSON output: {stdout}")
            })?;
        return Ok((session_id.to_string(), port as u16));
    }

    // Try line-by-line parsing
    let mut session_id = None;
    let mut port = None;

    for line in stdout.lines() {
        let lower = line.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix("session id:") {
            session_id = Some(rest.trim().to_string());
        } else if let Some(rest) = lower.strip_prefix("session:") {
            session_id = Some(rest.trim().to_string());
        } else if let Some(rest) = lower.strip_prefix("port:") {
            if let Ok(p) = rest.trim().parse::<u16>() {
                port = Some(p);
            }
        } else {
            // Try token-by-token parsing for key=value pairs on a single line,
            // e.g. "session_id=abc123 port=8080"
            for token in lower.split_whitespace() {
                if let Some(rest) = token.strip_prefix("session_id=") {
                    session_id = Some(rest.trim().to_string());
                } else if let Some(rest) = token.strip_prefix("port=") {
                    if let Ok(p) = rest.trim().parse::<u16>() {
                        port = Some(p);
                    }
                }
            }
        }
    }

    match (session_id, port) {
        (Some(id), Some(p)) => Ok((id, p)),
        _ => {
            anyhow::bail!(
                "could not parse session ID and port from polytoken output: {stdout}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_output_key_value() {
        let stdout = "Session ID: abc123def456\nPort: 8080\n";
        let (id, port) = parse_session_output(stdout).unwrap();
        assert_eq!(id, "abc123def456");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_parse_session_output_json() {
        let stdout = r#"{"session_id":"abc123","port":9090}"#;
        let (id, port) = parse_session_output(stdout).unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(port, 9090);
    }

    #[test]
    fn test_parse_session_output_error_includes_raw() {
        // AC.9 / AC.13: unparseable input should include the raw output in the error
        let stdout = "garbage output with no session info\nrandom text\n";
        let err = parse_session_output(stdout).unwrap_err().to_string();
        assert!(
            err.contains(stdout),
            "error should contain raw output; got: {err}"
        );
    }

    #[test]
    fn test_parse_session_output_json_error_includes_raw() {
        // JSON with missing fields should include raw output
        let stdout = r#"{"foo":"bar"}"#;
        let err = parse_session_output(stdout).unwrap_err().to_string();
        assert!(
            err.contains(stdout),
            "error should contain raw output; got: {err}"
        );
    }

    #[test]
    fn test_parse_session_output_equals_format() {
        // polytoken new --no-attach emits "session_id=xxx port=yyy"
        let stdout = "session_id=05z7gk-gush port=49633\n";
        let (id, port) = parse_session_output(stdout).unwrap();
        assert_eq!(id, "05z7gk-gush");
        assert_eq!(port, 49633);
    }

    #[test]
    fn test_parse_session_output_equals_format_on_one_line() {
        // Also handles space-separated key=value on a single line
        let stdout = "session_id=abc123 port=8080\n";
        let (id, port) = parse_session_output(stdout).unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_session_state_deserialization() {
        // AC.1: SessionState captures used_tokens, limit_tokens, and most_recent_assistant_text
        let json = r#"{
            "turn_in_flight": true,
            "cwd": "/path/to/workspace",
            "context_usage": { "used_tokens": 12000, "limit_tokens": 200000 },
            "most_recent_assistant_text": "Reading src/main.rs..."
        }"#;

        #[derive(serde::Deserialize)]
        struct ContextUsage {
            #[serde(default)]
            used_tokens: Option<u32>,
            #[serde(default)]
            limit_tokens: Option<u32>,
        }

        #[derive(serde::Deserialize)]
        struct StateResponse {
            #[serde(rename = "turn_in_flight")]
            turn_in_flight: bool,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            context_usage: Option<ContextUsage>,
            #[serde(default)]
            most_recent_assistant_text: Option<String>,
        }

        let resp: StateResponse = serde_json::from_str(json).unwrap();
        assert!(resp.turn_in_flight);
        assert_eq!(resp.cwd.as_deref(), Some("/path/to/workspace"));
        let ctx = resp.context_usage.expect("context_usage");
        assert_eq!(ctx.used_tokens, Some(12000));
        assert_eq!(ctx.limit_tokens, Some(200000));
        assert_eq!(
            resp.most_recent_assistant_text.as_deref(),
            Some("Reading src/main.rs...")
        );
    }

    #[test]
    fn test_session_state_deserialization_missing_context_usage() {
        // When context_usage and most_recent_assistant_text are absent, fields default to None
        let json = r#"{"turn_in_flight": false, "cwd": "/path"}"#;

        #[derive(serde::Deserialize)]
        struct ContextUsage {
            #[serde(default)]
            used_tokens: Option<u32>,
            #[serde(default)]
            limit_tokens: Option<u32>,
        }

        #[derive(serde::Deserialize)]
        struct StateResponse {
            #[serde(rename = "turn_in_flight")]
            turn_in_flight: bool,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            context_usage: Option<ContextUsage>,
            #[serde(default)]
            most_recent_assistant_text: Option<String>,
        }

        let resp: StateResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.turn_in_flight);
        assert!(resp.context_usage.is_none());
        assert!(resp.most_recent_assistant_text.is_none());
    }
}
