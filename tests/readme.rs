//! AC.15: README contains required documentation.

#[test]
fn test_readme_contains_documentation() {
    let readme = include_str!("../README.md");

    // Correct Polytoken link
    assert!(
        readme.contains("docs.polytoken.dev"),
        "README should link to Polytoken docs site"
    );

    // Logging section
    assert!(
        readme.contains("## Logging"),
        "README should have a Logging section"
    );
    assert!(readme.contains("--quiet"), "README should document --quiet");
    assert!(
        readme.contains("--verbose"),
        "README should document --verbose"
    );
    assert!(
        readme.contains("RUST_LOG"),
        "README should document RUST_LOG"
    );

    // Commands section
    assert!(
        readme.contains("## Commands"),
        "README should have a Commands section"
    );
    assert!(
        readme.contains("supervise"),
        "README should list supervise command"
    );
    assert!(
        readme.contains("--dry-run"),
        "README should document --dry-run"
    );
    assert!(
        readme.contains("status"),
        "README should list status command"
    );
    assert!(
        readme.contains("doctor"),
        "README should list doctor command"
    );

    // Troubleshooting section
    assert!(
        readme.contains("## Troubleshooting"),
        "README should have a Troubleshooting section"
    );
    assert!(
        readme.contains("gh auth login"),
        "README troubleshooting should mention gh auth login"
    );
    assert!(
        readme.contains("state.json"),
        "README troubleshooting should mention state.json path"
    );

    // cargo-binstall installation
    assert!(
        readme.contains("cargo binstall grindbot"),
        "README should document cargo-binstall installation"
    );
}

#[test]
fn test_agent_prompt_documentation_includes_source_files() {
    let documentation_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("AGENT_PROMPTS.md");
    let documentation = std::fs::read_to_string(&documentation_path)
        .expect("AGENT_PROMPTS.md should be readable from the repository");
    let implementer = include_str!("../src/prompts/implementer.md");
    let conflict_resolution = include_str!("../src/prompts/conflict_resolution.md");

    let mut synchronized = documentation.clone();
    replace_generated_prompt(&mut synchronized, "IMPLEMENTER", implementer);
    replace_generated_prompt(
        &mut synchronized,
        "CONFLICT RESOLUTION",
        conflict_resolution,
    );

    if synchronized != documentation {
        std::fs::write(&documentation_path, synchronized)
            .expect("AGENT_PROMPTS.md should be writable when prompt documentation drifts");
        panic!(
            "AGENT_PROMPTS.md was out of sync and the working change was updated to make the markdown file match the prompt file; commit that change and run this test again"
        );
    }

    assert_eq!(
        generated_prompt(&documentation, "IMPLEMENTER"),
        implementer.trim_end_matches('\n'),
        "AGENT_PROMPTS.md must include the complete implementer prompt"
    );
    assert_eq!(
        generated_prompt(&documentation, "CONFLICT RESOLUTION"),
        conflict_resolution.trim_end_matches('\n'),
        "AGENT_PROMPTS.md must include the complete conflict-resolution prompt"
    );
    assert!(
        !documentation.contains("<polytoken-ref"),
        "AGENT_PROMPTS.md should use GitHub-compatible Markdown"
    );
}

fn generated_prompt<'a>(documentation: &'a str, name: &str) -> &'a str {
    let begin_marker = format!("<!-- BEGIN GENERATED {name} PROMPT -->");
    let end_marker = format!("<!-- END GENERATED {name} PROMPT -->");
    let content_start = documentation
        .find(&begin_marker)
        .unwrap_or_else(|| panic!("AGENT_PROMPTS.md is missing marker: {begin_marker}"))
        + begin_marker.len();
    let content_end = documentation[content_start..]
        .find(&end_marker)
        .map(|offset| content_start + offset)
        .unwrap_or_else(|| panic!("AGENT_PROMPTS.md is missing marker: {end_marker}"));

    documentation[content_start..content_end].trim()
}

fn replace_generated_prompt(documentation: &mut String, name: &str, prompt: &str) {
    let begin_marker = format!("<!-- BEGIN GENERATED {name} PROMPT -->");
    let end_marker = format!("<!-- END GENERATED {name} PROMPT -->");
    let content_start = documentation
        .find(&begin_marker)
        .unwrap_or_else(|| panic!("AGENT_PROMPTS.md is missing marker: {begin_marker}"))
        + begin_marker.len();
    let content_end = documentation[content_start..]
        .find(&end_marker)
        .map(|offset| content_start + offset)
        .unwrap_or_else(|| panic!("AGENT_PROMPTS.md is missing marker: {end_marker}"));

    documentation.replace_range(
        content_start..content_end,
        &format!("\n{}\n", prompt.trim_end_matches('\n')),
    );
}
