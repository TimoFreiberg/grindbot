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
    assert!(
        readme.contains("--quiet"),
        "README should document --quiet"
    );
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
}
