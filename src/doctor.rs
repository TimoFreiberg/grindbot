use crate::config::Config;

/// Entry point for the CLI.
/// If config is provided, checks config validity too.
/// If config is None, runs only binary checks.
pub async fn run(config: Option<&Config>) -> anyhow::Result<()> {
    println!("Checking grindbot dependencies...");
    println!();

    let mut all_passed = true;

    // jj
    let jj_ok = check_binary("jj", &["--version"]).await;
    all_passed &= jj_ok;

    // gh (version + auth)
    let gh_ok = check_gh().await;
    all_passed &= gh_ok;

    // polytoken (from config if available)
    let pt_ok = if let Some(cfg) = config {
        check_binary(&cfg.polytoken.binary, &["--version"]).await
    } else {
        // Try default binary name
        check_binary("polytoken", &["--version"]).await
    };
    all_passed &= pt_ok;

    // config validity
    let cfg_ok = if let Some(cfg) = config {
        match cfg.validate() {
            Ok(()) => {
                println!("  ✓ config      valid");
                true
            }
            Err(e) => {
                println!("  ✗ config      invalid: {}", e);
                false
            }
        }
    } else {
        println!("  - config      (no config file found)");
        true // not a failure, just skipped
    };
    all_passed &= cfg_ok;

    // jj repo
    let cwd = std::env::current_dir().unwrap_or_default();
    let jj_repo_ok = if cwd.join(".jj").exists() {
        println!("  ✓ jj repo     initialized");
        true
    } else {
        println!("  ✗ jj repo     not found (run 'jj git init --colocate' in this directory)");
        false
    };
    all_passed &= jj_repo_ok;

    println!();
    if all_passed {
        println!("All checks passed.");
        Ok(())
    } else {
        println!("Some checks failed. See above for details.");
        std::process::exit(1);
    }
}

async fn check_binary(name: &str, args: &[&str]) -> bool {
    tracing::debug!(command = name, args = ?args, "running external command");
    match tokio::process::Command::new(name).args(args).output().await {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.lines().next().unwrap_or("").trim();
            if version_str.is_empty() {
                println!("  ✓ {:<11} found", name);
            } else {
                println!("  ✓ {:<11} {}", name, version_str);
            }
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_str = stderr.lines().next().unwrap_or("").trim();
            println!("  ✗ {:<11} error: {}", name, stderr_str);
            false
        }
        Err(_) => {
            println!(
                "  ✗ {:<11} not found. Install or add to PATH.",
                name
            );
            false
        }
    }
}

async fn check_gh() -> bool {
    // Check version
    let version_ok = check_binary("gh", &["--version"]).await;
    if !version_ok {
        return false;
    }

    // Check auth status
    tracing::debug!(command = "gh auth status", "running external command");
    match tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            // Try to extract the authenticated user from stderr (gh prints to stderr)
            let stderr = String::from_utf8_lossy(&output.stderr);
            if let Some(line) = stderr.lines().find(|l| l.contains("Logged in to")) {
                // Try to extract the account name
                let account = line
                    .split("account")
                    .nth(1)
                    .and_then(|s| s.trim_start_matches(|c: char| c.is_whitespace() || c == ':').strip_prefix(' '))
                    .and_then(|s| s.split_whitespace().next())
                    .unwrap_or("");
                if !account.is_empty() {
                    println!("  ✓ {:<11} authenticated as {}", "gh", account);
                } else {
                    println!("  ✓ {:<11} authenticated", "gh");
                }
            } else {
                println!("  ✓ {:<11} authenticated", "gh");
            }
            true
        }
        _ => {
            println!("  ✗ {:<11} not authenticated (run 'gh auth login')", "gh");
            false
        }
    }
}
