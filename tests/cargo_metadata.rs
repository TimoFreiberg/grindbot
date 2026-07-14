//! AC.1: Cargo.toml has repository field and binstall metadata.

#[test]
fn test_binstall_metadata() {
    let cargo_toml = include_str!("../Cargo.toml");
    let parsed: toml::Value = toml::from_str(cargo_toml).expect("Cargo.toml should parse");

    let package = parsed
        .get("package")
        .expect("Cargo.toml should have a [package] section");

    // repository field
    assert!(
        package.get("repository").is_some(),
        "Cargo.toml [package] should have a repository field"
    );

    // [package.metadata.binstall]
    let binstall = package
        .get("metadata")
        .and_then(|m| m.get("binstall"))
        .expect("Cargo.toml should have [package.metadata.binstall]");

    assert!(
        binstall.get("pkg-url").is_some(),
        "[package.metadata.binstall] should have pkg-url"
    );
    assert!(
        binstall.get("bin-dir").is_some(),
        "[package.metadata.binstall] should have bin-dir"
    );
    assert!(
        binstall.get("pkg-fmt").is_some(),
        "[package.metadata.binstall] should have pkg-fmt"
    );
}
